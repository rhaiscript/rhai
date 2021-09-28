//! Main module defining the lexer and parser.

use crate::engine::{
    Precedence, KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL,
    KEYWORD_FN_PTR_CURRY, KEYWORD_IS_DEF_VAR, KEYWORD_PRINT, KEYWORD_THIS, KEYWORD_TYPE_OF,
};
use crate::{Engine, LexError, StaticVec, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    borrow::Cow,
    cell::Cell,
    char, fmt,
    iter::{FusedIterator, Peekable},
    num::NonZeroUsize,
    ops::{Add, AddAssign},
    rc::Rc,
    str::{Chars, FromStr},
};

#[cfg(not(feature = "no_float"))]
use crate::{ast::FloatWrapper, FLOAT};

#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

#[cfg(not(feature = "no_function"))]
use crate::engine::KEYWORD_IS_DEF_FN;

/// _(internals)_ A type containing commands to control the tokenizer.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Copy, Default)]
pub struct TokenizerControlBlock {
    /// Is the current tokenizer position within an interpolated text string?
    /// This flag allows switching the tokenizer back to _text_ parsing after an interpolation stream.
    pub is_within_text: bool,
}

/// _(internals)_ A shared object that allows control of the tokenizer from outside.
pub type TokenizerControl = Rc<Cell<TokenizerControlBlock>>;

type LERR = LexError;

/// Separator character for numbers.
const NUMBER_SEPARATOR: char = '_';

/// A stream of tokens.
pub type TokenStream<'a> = Peekable<TokenIterator<'a>>;

/// A location (line number + character position) in the input script.
///
/// # Limitations
///
/// In order to keep footprint small, both line number and character position have 16-bit resolution,
/// meaning they go up to a maximum of 65,535 lines and 65,535 characters per line.
///
/// Advancing beyond the maximum line length or maximum number of lines is not an error but has no effect.
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct Position {
    /// Line number - 0 = none
    #[cfg(not(feature = "no_position"))]
    line: u16,
    /// Character position - 0 = BOL
    #[cfg(not(feature = "no_position"))]
    pos: u16,
}

impl Position {
    /// A [`Position`] representing no position.
    pub const NONE: Self = Self {
        #[cfg(not(feature = "no_position"))]
        line: 0,
        #[cfg(not(feature = "no_position"))]
        pos: 0,
    };
    /// A [`Position`] representing the first position.
    pub const START: Self = Self {
        #[cfg(not(feature = "no_position"))]
        line: 1,
        #[cfg(not(feature = "no_position"))]
        pos: 0,
    };

    /// Create a new [`Position`].
    ///
    /// `line` must not be zero.
    ///
    /// If `position` is zero, then it is at the beginning of a line.
    ///
    /// # Panics
    ///
    /// Panics if `line` is zero.
    #[inline(always)]
    #[must_use]
    pub fn new(line: u16, position: u16) -> Self {
        assert!(line != 0, "line cannot be zero");

        let _pos = position;

        Self {
            #[cfg(not(feature = "no_position"))]
            line,
            #[cfg(not(feature = "no_position"))]
            pos: _pos,
        }
    }
    /// Create a new [`Position`].
    ///
    /// If `line` is zero, then [`None`] is returned.
    ///
    /// If `position` is zero, then it is at the beginning of a line.
    #[inline(always)]
    #[must_use]
    pub const fn new_const(line: u16, position: u16) -> Option<Self> {
        if line == 0 {
            return None;
        }
        let _pos = position;

        Some(Self {
            #[cfg(not(feature = "no_position"))]
            line,
            #[cfg(not(feature = "no_position"))]
            pos: _pos,
        })
    }
    /// Get the line number (1-based), or [`None`] if there is no position.
    #[inline(always)]
    #[must_use]
    pub const fn line(self) -> Option<usize> {
        #[cfg(not(feature = "no_position"))]
        return if self.is_none() {
            None
        } else {
            Some(self.line as usize)
        };

        #[cfg(feature = "no_position")]
        return None;
    }
    /// Get the character position (1-based), or [`None`] if at beginning of a line.
    #[inline(always)]
    #[must_use]
    pub const fn position(self) -> Option<usize> {
        #[cfg(not(feature = "no_position"))]
        return if self.is_none() || self.pos == 0 {
            None
        } else {
            Some(self.pos as usize)
        };

        #[cfg(feature = "no_position")]
        return None;
    }
    /// Advance by one character position.
    #[inline(always)]
    pub(crate) fn advance(&mut self) {
        #[cfg(not(feature = "no_position"))]
        {
            assert!(!self.is_none(), "cannot advance Position::none");

            // Advance up to maximum position
            if self.pos < u16::MAX {
                self.pos += 1;
            }
        }
    }
    /// Go backwards by one character position.
    ///
    /// # Panics
    ///
    /// Panics if already at beginning of a line - cannot rewind to a previous line.
    #[inline(always)]
    pub(crate) fn rewind(&mut self) {
        #[cfg(not(feature = "no_position"))]
        {
            assert!(!self.is_none(), "cannot rewind Position::none");
            assert!(self.pos > 0, "cannot rewind at position 0");
            self.pos -= 1;
        }
    }
    /// Advance to the next line.
    #[inline(always)]
    pub(crate) fn new_line(&mut self) {
        #[cfg(not(feature = "no_position"))]
        {
            assert!(!self.is_none(), "cannot advance Position::none");

            // Advance up to maximum position
            if self.line < u16::MAX {
                self.line += 1;
                self.pos = 0;
            }
        }
    }
    /// Is this [`Position`] at the beginning of a line?
    #[inline(always)]
    #[must_use]
    pub const fn is_beginning_of_line(self) -> bool {
        #[cfg(not(feature = "no_position"))]
        return self.pos == 0 && !self.is_none();
        #[cfg(feature = "no_position")]
        return false;
    }
    /// Is there no [`Position`]?
    #[inline(always)]
    #[must_use]
    pub const fn is_none(self) -> bool {
        #[cfg(not(feature = "no_position"))]
        return self.line == 0 && self.pos == 0;
        #[cfg(feature = "no_position")]
        return true;
    }
    /// Print this [`Position`] for debug purposes.
    #[inline(always)]
    pub(crate) fn debug_print(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_position"))]
        if !self.is_none() {
            write!(_f, " @ {:?}", self)?;
        }

        Ok(())
    }
}

impl Default for Position {
    #[inline(always)]
    fn default() -> Self {
        Self::START
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() {
            write!(f, "none")?;
        } else {
            #[cfg(not(feature = "no_position"))]
            write!(f, "line {}, position {}", self.line, self.pos)?;
            #[cfg(feature = "no_position")]
            unreachable!();
        }

        Ok(())
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_position"))]
        write!(f, "{}:{}", self.line, self.pos)?;
        #[cfg(feature = "no_position")]
        f.write_str("none")?;

        Ok(())
    }
}

impl Add for Position {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        if rhs.is_none() {
            self
        } else {
            #[cfg(not(feature = "no_position"))]
            return Self {
                line: self.line + rhs.line - 1,
                pos: if rhs.is_beginning_of_line() {
                    self.pos
                } else {
                    self.pos + rhs.pos - 1
                },
            };
            #[cfg(feature = "no_position")]
            unreachable!();
        }
    }
}

impl AddAssign for Position {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

/// _(internals)_ A Rhai language token.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, PartialEq, Clone, Hash)]
pub enum Token {
    /// An `INT` constant.
    IntegerConstant(INT),
    /// A `FLOAT` constant.
    ///
    /// Reserved under the `no_float` feature.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(FloatWrapper<FLOAT>),
    /// A [`Decimal`] constant.
    ///
    /// Requires the `decimal` feature.
    #[cfg(feature = "decimal")]
    DecimalConstant(Decimal),
    /// An identifier.
    Identifier(String),
    /// A character constant.
    CharConstant(char),
    /// A string constant.
    StringConstant(String),
    /// An interpolated string.
    InterpolatedString(String),
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `+`
    Plus,
    /// `+` (unary)
    UnaryPlus,
    /// `-`
    Minus,
    /// `-` (unary)
    UnaryMinus,
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `%`
    Modulo,
    /// `**`
    PowerOf,
    /// `<<`
    LeftShift,
    /// `>>`
    RightShift,
    /// `;`
    SemiColon,
    /// `:`
    Colon,
    /// `::`
    DoubleColon,
    /// `=>`
    DoubleArrow,
    /// `_`
    Underscore,
    /// `,`
    Comma,
    /// `.`
    Period,
    /// `#{`
    MapStart,
    /// `=`
    Equals,
    /// `true`
    True,
    /// `false`
    False,
    /// `let`
    Let,
    /// `const`
    Const,
    /// `if`
    If,
    /// `else`
    Else,
    /// `switch`
    Switch,
    /// `do`
    Do,
    /// `while`
    While,
    /// `until`
    Until,
    /// `loop`
    Loop,
    /// `for`
    For,
    /// `in`
    In,
    /// `<`
    LessThan,
    /// `>`
    GreaterThan,
    /// `<=`
    LessThanEqualsTo,
    /// `>=`
    GreaterThanEqualsTo,
    /// `==`
    EqualsTo,
    /// `!=`
    NotEqualsTo,
    /// `!`
    Bang,
    /// `|`
    Pipe,
    /// `||`
    Or,
    /// `^`
    XOr,
    /// `&`
    Ampersand,
    /// `&&`
    And,
    /// `fn`
    ///
    /// Reserved under the `no_function` feature.
    #[cfg(not(feature = "no_function"))]
    Fn,
    /// `continue`
    Continue,
    /// `break`
    Break,
    /// `return`
    Return,
    /// `throw`
    Throw,
    /// `try`
    Try,
    /// `catch`
    Catch,
    /// `+=`
    PlusAssign,
    /// `-=`
    MinusAssign,
    /// `*=`
    MultiplyAssign,
    /// `/=`
    DivideAssign,
    /// `<<=`
    LeftShiftAssign,
    /// `>>=`
    RightShiftAssign,
    /// `&=`
    AndAssign,
    /// `|=`
    OrAssign,
    /// `^=`
    XOrAssign,
    /// `%=`
    ModuloAssign,
    /// `**=`
    PowerOfAssign,
    /// `private`
    ///
    /// Reserved under the `no_function` feature.
    #[cfg(not(feature = "no_function"))]
    Private,
    /// `import`
    ///
    /// Reserved under the `no_module` feature.
    #[cfg(not(feature = "no_module"))]
    Import,
    /// `export`
    ///
    /// Reserved under the `no_module` feature.
    #[cfg(not(feature = "no_module"))]
    Export,
    /// `as`
    ///
    /// Reserved under the `no_module` feature.
    #[cfg(not(feature = "no_module"))]
    As,
    /// A lexer error.
    LexError(LexError),
    /// A comment block.
    Comment(String),
    /// A reserved symbol.
    Reserved(String),
    /// A custom keyword.
    Custom(String),
    /// End of the input stream.
    EOF,
}

impl Token {
    /// Get the syntax of the token if it is a keyword.
    #[must_use]
    pub const fn keyword_syntax(&self) -> &'static str {
        use Token::*;

        match self {
            LeftBrace => "{",
            RightBrace => "}",
            LeftParen => "(",
            RightParen => ")",
            LeftBracket => "[",
            RightBracket => "]",
            Plus => "+",
            UnaryPlus => "+",
            Minus => "-",
            UnaryMinus => "-",
            Multiply => "*",
            Divide => "/",
            SemiColon => ";",
            Colon => ":",
            DoubleColon => "::",
            DoubleArrow => "=>",
            Underscore => "_",
            Comma => ",",
            Period => ".",
            MapStart => "#{",
            Equals => "=",
            True => "true",
            False => "false",
            Let => "let",
            Const => "const",
            If => "if",
            Else => "else",
            Switch => "switch",
            Do => "do",
            While => "while",
            Until => "until",
            Loop => "loop",
            For => "for",
            In => "in",
            LessThan => "<",
            GreaterThan => ">",
            Bang => "!",
            LessThanEqualsTo => "<=",
            GreaterThanEqualsTo => ">=",
            EqualsTo => "==",
            NotEqualsTo => "!=",
            Pipe => "|",
            Or => "||",
            Ampersand => "&",
            And => "&&",
            Continue => "continue",
            Break => "break",
            Return => "return",
            Throw => "throw",
            Try => "try",
            Catch => "catch",
            PlusAssign => "+=",
            MinusAssign => "-=",
            MultiplyAssign => "*=",
            DivideAssign => "/=",
            LeftShiftAssign => "<<=",
            RightShiftAssign => ">>=",
            AndAssign => "&=",
            OrAssign => "|=",
            XOrAssign => "^=",
            LeftShift => "<<",
            RightShift => ">>",
            XOr => "^",
            Modulo => "%",
            ModuloAssign => "%=",
            PowerOf => "**",
            PowerOfAssign => "**=",

            #[cfg(not(feature = "no_function"))]
            Fn => "fn",
            #[cfg(not(feature = "no_function"))]
            Private => "private",

            #[cfg(not(feature = "no_module"))]
            Import => "import",
            #[cfg(not(feature = "no_module"))]
            Export => "export",
            #[cfg(not(feature = "no_module"))]
            As => "as",

            _ => "ERROR: NOT A KEYWORD",
        }
    }

    /// Get the syntax of the token.
    #[must_use]
    pub fn syntax(&self) -> Cow<'static, str> {
        use Token::*;

        match self {
            IntegerConstant(i) => i.to_string().into(),
            #[cfg(not(feature = "no_float"))]
            FloatConstant(f) => f.to_string().into(),
            #[cfg(feature = "decimal")]
            DecimalConstant(d) => d.to_string().into(),
            StringConstant(_) => "string".into(),
            InterpolatedString(_) => "string".into(),
            CharConstant(c) => c.to_string().into(),
            Identifier(s) => s.clone().into(),
            Reserved(s) => s.clone().into(),
            Custom(s) => s.clone().into(),
            LexError(err) => err.to_string().into(),
            Comment(s) => s.clone().into(),

            EOF => "{EOF}".into(),

            token => token.keyword_syntax().into(),
        }
    }

    /// Is this token an op-assignment operator?
    #[inline]
    #[must_use]
    pub const fn is_op_assignment(&self) -> bool {
        matches!(
            self,
            Self::PlusAssign
                | Self::MinusAssign
                | Self::MultiplyAssign
                | Self::DivideAssign
                | Self::LeftShiftAssign
                | Self::RightShiftAssign
                | Self::ModuloAssign
                | Self::PowerOfAssign
                | Self::AndAssign
                | Self::OrAssign
                | Self::XOrAssign
        )
    }

    /// Get the corresponding operator of the token if it is an op-assignment operator.
    #[must_use]
    pub const fn map_op_assignment(&self) -> Option<Self> {
        Some(match self {
            Self::PlusAssign => Self::Plus,
            Self::MinusAssign => Self::Minus,
            Self::MultiplyAssign => Self::Multiply,
            Self::DivideAssign => Self::Divide,
            Self::LeftShiftAssign => Self::LeftShift,
            Self::RightShiftAssign => Self::RightShift,
            Self::ModuloAssign => Self::Modulo,
            Self::PowerOfAssign => Self::PowerOf,
            Self::AndAssign => Self::Ampersand,
            Self::OrAssign => Self::Pipe,
            Self::XOrAssign => Self::XOr,
            _ => return None,
        })
    }

    /// Has this token a corresponding op-assignment operator?
    #[inline]
    #[must_use]
    pub const fn has_op_assignment(&self) -> bool {
        matches!(
            self,
            Self::Plus
                | Self::Minus
                | Self::Multiply
                | Self::Divide
                | Self::LeftShift
                | Self::RightShift
                | Self::Modulo
                | Self::PowerOf
                | Self::Ampersand
                | Self::Pipe
                | Self::XOr
        )
    }

    /// Get the corresponding op-assignment operator of the token.
    #[must_use]
    pub const fn make_op_assignment(&self) -> Option<Self> {
        Some(match self {
            Self::Plus => Self::PlusAssign,
            Self::Minus => Self::MinusAssign,
            Self::Multiply => Self::MultiplyAssign,
            Self::Divide => Self::DivideAssign,
            Self::LeftShift => Self::LeftShiftAssign,
            Self::RightShift => Self::RightShiftAssign,
            Self::Modulo => Self::ModuloAssign,
            Self::PowerOf => Self::PowerOfAssign,
            Self::Ampersand => Self::AndAssign,
            Self::Pipe => Self::OrAssign,
            Self::XOr => Self::XOrAssign,
            _ => return None,
        })
    }

    /// Reverse lookup a token from a piece of syntax.
    #[must_use]
    pub fn lookup_from_syntax(syntax: &str) -> Option<Self> {
        use Token::*;

        Some(match syntax {
            "{" => LeftBrace,
            "}" => RightBrace,
            "(" => LeftParen,
            ")" => RightParen,
            "[" => LeftBracket,
            "]" => RightBracket,
            "+" => Plus,
            "-" => Minus,
            "*" => Multiply,
            "/" => Divide,
            ";" => SemiColon,
            ":" => Colon,
            "::" => DoubleColon,
            "=>" => DoubleArrow,
            "_" => Underscore,
            "," => Comma,
            "." => Period,
            "#{" => MapStart,
            "=" => Equals,
            "true" => True,
            "false" => False,
            "let" => Let,
            "const" => Const,
            "if" => If,
            "else" => Else,
            "switch" => Switch,
            "do" => Do,
            "while" => While,
            "until" => Until,
            "loop" => Loop,
            "for" => For,
            "in" => In,
            "<" => LessThan,
            ">" => GreaterThan,
            "!" => Bang,
            "<=" => LessThanEqualsTo,
            ">=" => GreaterThanEqualsTo,
            "==" => EqualsTo,
            "!=" => NotEqualsTo,
            "|" => Pipe,
            "||" => Or,
            "&" => Ampersand,
            "&&" => And,
            "continue" => Continue,
            "break" => Break,
            "return" => Return,
            "throw" => Throw,
            "try" => Try,
            "catch" => Catch,
            "+=" => PlusAssign,
            "-=" => MinusAssign,
            "*=" => MultiplyAssign,
            "/=" => DivideAssign,
            "<<=" => LeftShiftAssign,
            ">>=" => RightShiftAssign,
            "&=" => AndAssign,
            "|=" => OrAssign,
            "^=" => XOrAssign,
            "<<" => LeftShift,
            ">>" => RightShift,
            "^" => XOr,
            "%" => Modulo,
            "%=" => ModuloAssign,
            "**" => PowerOf,
            "**=" => PowerOfAssign,

            #[cfg(not(feature = "no_function"))]
            "fn" => Fn,
            #[cfg(not(feature = "no_function"))]
            "private" => Private,

            #[cfg(not(feature = "no_module"))]
            "import" => Import,
            #[cfg(not(feature = "no_module"))]
            "export" => Export,
            #[cfg(not(feature = "no_module"))]
            "as" => As,

            #[cfg(feature = "no_function")]
            "fn" | "private" => Reserved(syntax.into()),

            #[cfg(feature = "no_module")]
            "import" | "export" | "as" => Reserved(syntax.into()),

            "===" | "!==" | "->" | "<-" | ":=" | "~" | "::<" | "(*" | "*)" | "#" | "#!"
            | "public" | "protected" | "super" | "new" | "use" | "module" | "package" | "var"
            | "static" | "shared" | "with" | "goto" | "exit" | "match" | "case" | "default"
            | "void" | "null" | "nil" | "spawn" | "thread" | "go" | "sync" | "async" | "await"
            | "yield" => Reserved(syntax.into()),

            KEYWORD_PRINT | KEYWORD_DEBUG | KEYWORD_TYPE_OF | KEYWORD_EVAL | KEYWORD_FN_PTR
            | KEYWORD_FN_PTR_CALL | KEYWORD_FN_PTR_CURRY | KEYWORD_THIS | KEYWORD_IS_DEF_VAR => {
                Reserved(syntax.into())
            }

            #[cfg(not(feature = "no_function"))]
            KEYWORD_IS_DEF_FN => Reserved(syntax.into()),

            _ => return None,
        })
    }

    // Is this token [`EOF`][Token::EOF]?
    #[inline(always)]
    #[must_use]
    pub const fn is_eof(&self) -> bool {
        matches!(self, Self::EOF)
    }

    // If another operator is after these, it's probably an unary operator
    // (not sure about `fn` name).
    #[must_use]
    pub const fn is_next_unary(&self) -> bool {
        use Token::*;

        match self {
            LexError(_)      |
            LeftBrace        | // {+expr} - is unary
            // RightBrace    | {expr} - expr not unary & is closing
            LeftParen        | // (-expr) - is unary
            // RightParen    | (expr) - expr not unary & is closing
            LeftBracket      | // [-expr] - is unary
            // RightBracket  | [expr] - expr not unary & is closing
            Plus             |
            UnaryPlus        |
            Minus            |
            UnaryMinus       |
            Multiply         |
            Divide           |
            Comma            |
            Period           |
            Equals           |
            LessThan         |
            GreaterThan      |
            Bang             |
            LessThanEqualsTo |
            GreaterThanEqualsTo |
            EqualsTo         |
            NotEqualsTo      |
            Pipe             |
            Or               |
            Ampersand        |
            And              |
            If               |
            Do               |
            While            |
            Until            |
            PlusAssign       |
            MinusAssign      |
            MultiplyAssign   |
            DivideAssign     |
            LeftShiftAssign  |
            RightShiftAssign |
            PowerOf          |
            PowerOfAssign    |
            AndAssign        |
            OrAssign         |
            XOrAssign        |
            LeftShift        |
            RightShift       |
            XOr              |
            Modulo           |
            ModuloAssign     |
            Return           |
            Throw            |
            In               => true,

            _ => false,
        }
    }

    /// Get the precedence number of the token.
    #[must_use]
    pub const fn precedence(&self) -> Option<Precedence> {
        use Token::*;

        Precedence::new(match self {
            // Assignments are not considered expressions - set to zero
            Equals | PlusAssign | MinusAssign | MultiplyAssign | DivideAssign | PowerOfAssign
            | LeftShiftAssign | RightShiftAssign | AndAssign | OrAssign | XOrAssign
            | ModuloAssign => 0,

            Or | XOr | Pipe => 30,

            And | Ampersand => 60,

            EqualsTo | NotEqualsTo => 90,

            In => 110,

            LessThan | LessThanEqualsTo | GreaterThan | GreaterThanEqualsTo => 130,

            Plus | Minus => 150,

            Divide | Multiply | Modulo => 180,

            PowerOf => 190,

            LeftShift | RightShift => 210,

            Period => 240,

            _ => 0,
        })
    }

    /// Does an expression bind to the right (instead of left)?
    #[must_use]
    pub const fn is_bind_right(&self) -> bool {
        use Token::*;

        match self {
            // Assignments bind to the right
            Equals | PlusAssign | MinusAssign | MultiplyAssign | DivideAssign | PowerOfAssign
            | LeftShiftAssign | RightShiftAssign | AndAssign | OrAssign | XOrAssign
            | ModuloAssign => true,

            // Property access binds to the right
            Period => true,

            // Exponentiation binds to the right
            PowerOf => true,

            _ => false,
        }
    }

    /// Is this token a standard symbol used in the language?
    #[must_use]
    pub const fn is_symbol(&self) -> bool {
        use Token::*;

        match self {
            LeftBrace | RightBrace | LeftParen | RightParen | LeftBracket | RightBracket | Plus
            | UnaryPlus | Minus | UnaryMinus | Multiply | Divide | Modulo | PowerOf | LeftShift
            | RightShift | SemiColon | Colon | DoubleColon | Comma | Period | MapStart | Equals
            | LessThan | GreaterThan | LessThanEqualsTo | GreaterThanEqualsTo | EqualsTo
            | NotEqualsTo | Bang | Pipe | Or | XOr | Ampersand | And | PlusAssign | MinusAssign
            | MultiplyAssign | DivideAssign | LeftShiftAssign | RightShiftAssign | AndAssign
            | OrAssign | XOrAssign | ModuloAssign | PowerOfAssign => true,

            _ => false,
        }
    }

    /// Is this token an active standard keyword?
    #[inline]
    #[must_use]
    pub const fn is_keyword(&self) -> bool {
        use Token::*;

        match self {
            #[cfg(not(feature = "no_function"))]
            Fn | Private => true,

            #[cfg(not(feature = "no_module"))]
            Import | Export | As => true,

            True | False | Let | Const | If | Else | Do | While | Until | Loop | For | In
            | Continue | Break | Return | Throw | Try | Catch => true,

            _ => false,
        }
    }

    /// Is this token a reserved symbol?
    #[inline(always)]
    #[must_use]
    pub const fn is_reserved(&self) -> bool {
        matches!(self, Self::Reserved(_))
    }

    /// Convert a token into a function name, if possible.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub(crate) fn into_function_name_for_override(self) -> Result<String, Self> {
        match self {
            Self::Custom(s) | Self::Identifier(s) if is_valid_identifier(s.chars()) => Ok(s),
            _ => Err(self),
        }
    }

    /// Is this token a custom keyword?
    #[inline(always)]
    #[must_use]
    pub const fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }
}

impl From<Token> for String {
    #[inline(always)]
    fn from(token: Token) -> Self {
        token.syntax().into()
    }
}

/// _(internals)_ State of the tokenizer.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct TokenizeState {
    /// Maximum length of a string.
    pub max_string_size: Option<NonZeroUsize>,
    /// Can the next token be a unary operator?
    pub non_unary: bool,
    /// Is the tokenizer currently inside a block comment?
    pub comment_level: usize,
    /// Include comments?
    pub include_comments: bool,
    /// Is the current tokenizer position within the text stream of an interpolated string?
    pub is_within_text_terminated_by: Option<char>,
}

/// _(internals)_ Trait that encapsulates a peekable character input stream.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This trait is volatile and may change.
pub trait InputStream {
    /// Un-get a character back into the `InputStream`.
    /// The next [`get_next`][InputStream::get_next] or [`peek_next`][InputStream::peek_next]
    /// will return this character instead.
    fn unget(&mut self, ch: char);
    /// Get the next character from the `InputStream`.
    fn get_next(&mut self) -> Option<char>;
    /// Peek the next character in the `InputStream`.
    #[must_use]
    fn peek_next(&mut self) -> Option<char>;
}

/// _(internals)_ Parse a string literal ended by `termination_char`.
/// Exported under the `internals` feature only.
///
/// Returns the parsed string and a boolean indicating whether the string is
/// terminated by an interpolation `${`.
///
/// # Returns
///
/// |Type                             |Return Value                |`state.is_within_text_terminated_by`|
/// |---------------------------------|:--------------------------:|:----------------------------------:|
/// |`"hello"`                        |`StringConstant("hello")`   |`None`                              |
/// |`"hello`_{LF}_ or _{EOF}_        |`LexError`                  |`None`                              |
/// |`"hello\`_{EOF}_ or _{LF}{EOF}_  |`StringConstant("hello")`   |`Some('"')`                         |
/// |`` `hello``_{EOF}_               |`StringConstant("hello")`   |``Some('`')``                       |
/// |`` `hello``_{LF}{EOF}_           |`StringConstant("hello\n")` |``Some('`')``                       |
/// |`` `hello ${``                   |`InterpolatedString("hello ")`<br/>next token is `{`|`None`      |
/// |`` } hello` ``                   |`StringConstant(" hello")`  |`None`                              |
/// |`} hello`_{EOF}_                 |`StringConstant(" hello")`  |``Some('`')``                       |
///
/// This function does not throw a `LexError` for the following conditions:
///
/// * Unterminated literal string at _{EOF}_
///
/// * Unterminated normal string with continuation at _{EOF}_
///
/// This is to facilitate using this function to parse a script line-by-line, where the end of the
/// line (i.e. _{EOF}_) is not necessarily the end of the script.
///
/// Any time a [`StringConstant`][`Token::StringConstant`] is returned with
/// `state.is_within_text_terminated_by` set to `Some(_)` is one of the above conditions.
///
/// # Volatile API
///
/// This function is volatile and may change.
pub fn parse_string_literal(
    stream: &mut impl InputStream,
    state: &mut TokenizeState,
    pos: &mut Position,
    termination_char: char,
    continuation: bool,
    verbatim: bool,
    allow_interpolation: bool,
) -> Result<(String, bool), (LexError, Position)> {
    let mut result = String::with_capacity(12);
    let mut escape = String::with_capacity(12);

    let start = *pos;
    let mut interpolated = false;
    #[cfg(not(feature = "no_position"))]
    let mut skip_whitespace_until = 0;

    state.is_within_text_terminated_by = Some(termination_char);

    loop {
        assert!(
            !verbatim || escape.is_empty(),
            "verbatim strings should not have any escapes"
        );

        let next_char = match stream.get_next() {
            Some(ch) => {
                pos.advance();
                ch
            }
            None if verbatim => {
                assert_eq!(escape, "", "verbatim strings should not have any escapes");
                pos.advance();
                break;
            }
            None if continuation && !escape.is_empty() => {
                assert_eq!(escape, "\\", "unexpected escape {} at end of line", escape);
                pos.advance();
                break;
            }
            None => {
                result += &escape;
                pos.advance();
                state.is_within_text_terminated_by = None;
                return Err((LERR::UnterminatedString, start));
            }
        };

        // String interpolation?
        if allow_interpolation
            && next_char == '$'
            && escape.is_empty()
            && stream.peek_next().map(|ch| ch == '{').unwrap_or(false)
        {
            interpolated = true;
            state.is_within_text_terminated_by = None;
            break;
        }

        if let Some(max) = state.max_string_size {
            if result.len() > max.get() {
                return Err((LexError::StringTooLong(max.get()), *pos));
            }
        }

        match next_char {
            // \r - ignore if followed by \n
            '\r' if stream.peek_next().map(|ch| ch == '\n').unwrap_or(false) => (),
            // \...
            '\\' if !verbatim && escape.is_empty() => {
                escape.push('\\');
            }
            // \\
            '\\' if !escape.is_empty() => {
                escape.clear();
                result.push('\\');
            }
            // \t
            't' if !escape.is_empty() => {
                escape.clear();
                result.push('\t');
            }
            // \n
            'n' if !escape.is_empty() => {
                escape.clear();
                result.push('\n');
            }
            // \r
            'r' if !escape.is_empty() => {
                escape.clear();
                result.push('\r');
            }
            // \x??, \u????, \U????????
            ch @ 'x' | ch @ 'u' | ch @ 'U' if !escape.is_empty() => {
                let mut seq = escape.clone();
                escape.clear();
                seq.push(ch);

                let mut out_val: u32 = 0;
                let len = match ch {
                    'x' => 2,
                    'u' => 4,
                    'U' => 8,
                    _ => unreachable!(),
                };

                for _ in 0..len {
                    let c = stream
                        .get_next()
                        .ok_or_else(|| (LERR::MalformedEscapeSequence(seq.to_string()), *pos))?;

                    seq.push(c);
                    pos.advance();

                    out_val *= 16;
                    out_val += c
                        .to_digit(16)
                        .ok_or_else(|| (LERR::MalformedEscapeSequence(seq.to_string()), *pos))?;
                }

                result.push(
                    char::from_u32(out_val)
                        .ok_or_else(|| (LERR::MalformedEscapeSequence(seq), *pos))?,
                );
            }

            // \{termination_char} - escaped
            _ if termination_char == next_char && !escape.is_empty() => {
                escape.clear();
                result.push(next_char)
            }

            // Close wrapper
            _ if termination_char == next_char && escape.is_empty() => {
                state.is_within_text_terminated_by = None;
                break;
            }

            // Verbatim
            '\n' if verbatim => {
                assert_eq!(escape, "", "verbatim strings should not have any escapes");
                pos.new_line();
                result.push(next_char);
            }

            // Line continuation
            '\n' if continuation && !escape.is_empty() => {
                assert_eq!(escape, "\\", "unexpected escape {} at end of line", escape);
                escape.clear();
                pos.new_line();

                #[cfg(not(feature = "no_position"))]
                {
                    let start_position = start
                        .position()
                        .expect("never fails because the string must have a starting position");
                    skip_whitespace_until = start_position + 1;
                }
            }

            // Unterminated string
            '\n' => {
                pos.rewind();
                state.is_within_text_terminated_by = None;
                return Err((LERR::UnterminatedString, start));
            }

            // Unknown escape sequence
            _ if !escape.is_empty() => {
                escape.push(next_char);

                return Err((LERR::MalformedEscapeSequence(escape), *pos));
            }

            // Whitespace to skip
            #[cfg(not(feature = "no_position"))]
            _ if next_char.is_whitespace()
                && pos
                    .position()
                    .expect("never fails because a character must have a position")
                    < skip_whitespace_until => {}

            // All other characters
            _ => {
                escape.clear();
                result.push(next_char);

                #[cfg(not(feature = "no_position"))]
                {
                    skip_whitespace_until = 0;
                }
            }
        }
    }

    if let Some(max) = state.max_string_size {
        if result.len() > max.get() {
            return Err((LexError::StringTooLong(max.get()), *pos));
        }
    }

    Ok((result, interpolated))
}

/// Consume the next character.
#[inline(always)]
fn eat_next(stream: &mut impl InputStream, pos: &mut Position) -> Option<char> {
    pos.advance();
    stream.get_next()
}

/// Scan for a block comment until the end.
fn scan_block_comment(
    stream: &mut impl InputStream,
    level: usize,
    pos: &mut Position,
    comment: Option<&mut String>,
) -> usize {
    let mut level = level;
    let mut comment = comment;

    while let Some(c) = stream.get_next() {
        pos.advance();

        if let Some(comment) = comment.as_mut() {
            comment.push(c);
        }

        match c {
            '/' => {
                if let Some(c2) = stream.peek_next().filter(|&c2| c2 == '*') {
                    eat_next(stream, pos);
                    if let Some(comment) = comment.as_mut() {
                        comment.push(c2);
                    }
                    level += 1;
                }
            }
            '*' => {
                if let Some(c2) = stream.peek_next().filter(|&c2| c2 == '/') {
                    eat_next(stream, pos);
                    if let Some(comment) = comment.as_mut() {
                        comment.push(c2);
                    }
                    level -= 1;
                }
            }
            '\n' => pos.new_line(),
            _ => (),
        }

        if level == 0 {
            break;
        }
    }

    level
}

/// _(internals)_ Get the next token from the `stream`.
/// Exported under the `internals` feature only.
///
/// # Volatile API
///
/// This function is volatile and may change.
#[inline]
#[must_use]
pub fn get_next_token(
    stream: &mut impl InputStream,
    state: &mut TokenizeState,
    pos: &mut Position,
) -> Option<(Token, Position)> {
    let result = get_next_token_inner(stream, state, pos);

    // Save the last token's state
    if let Some((ref token, _)) = result {
        state.non_unary = !token.is_next_unary();
    }

    result
}

/// Test if the given character is a hex character.
#[inline(always)]
fn is_hex_digit(c: char) -> bool {
    matches!(c, 'a'..='f' | 'A'..='F' | '0'..='9')
}

/// Test if the given character is a numeric digit.
#[inline(always)]
fn is_numeric_digit(c: char) -> bool {
    matches!(c, '0'..='9')
}

/// Test if the comment block is a doc-comment.
#[cfg(not(feature = "no_function"))]
#[cfg(feature = "metadata")]
#[inline(always)]
#[must_use]
pub fn is_doc_comment(comment: &str) -> bool {
    (comment.starts_with("///") && !comment.starts_with("////"))
        || (comment.starts_with("/**") && !comment.starts_with("/***"))
}

/// Get the next token.
#[must_use]
fn get_next_token_inner(
    stream: &mut impl InputStream,
    state: &mut TokenizeState,
    pos: &mut Position,
) -> Option<(Token, Position)> {
    // Still inside a comment?
    if state.comment_level > 0 {
        let start_pos = *pos;
        let mut comment = if state.include_comments {
            Some(String::new())
        } else {
            None
        };

        state.comment_level =
            scan_block_comment(stream, state.comment_level, pos, comment.as_mut());

        let return_comment = state.include_comments;

        #[cfg(not(feature = "no_function"))]
        #[cfg(feature = "metadata")]
        let return_comment = return_comment
            || is_doc_comment(
                comment
                    .as_ref()
                    .expect("never fails because `include_comments` is true"),
            );

        if return_comment {
            return Some((
                Token::Comment(comment.expect("never fails because `return_comment` is true")),
                start_pos,
            ));
        }
        if state.comment_level > 0 {
            // Reached EOF without ending comment block
            return None;
        }
    }

    // Within text?
    if let Some(ch) = state.is_within_text_terminated_by.take() {
        let start_pos = *pos;

        return parse_string_literal(stream, state, pos, ch, false, true, true).map_or_else(
            |(err, err_pos)| Some((Token::LexError(err), err_pos)),
            |(result, interpolated)| {
                if interpolated {
                    Some((Token::InterpolatedString(result), start_pos))
                } else {
                    Some((Token::StringConstant(result), start_pos))
                }
            },
        );
    }

    let mut negated: Option<Position> = None;

    while let Some(c) = stream.get_next() {
        pos.advance();

        let start_pos = *pos;

        match (c, stream.peek_next().unwrap_or('\0')) {
            // \n
            ('\n', _) => pos.new_line(),

            // digit ...
            ('0'..='9', _) => {
                let mut result: smallvec::SmallVec<[char; 16]> = Default::default();
                let mut radix_base: Option<u32> = None;
                let mut valid: fn(char) -> bool = is_numeric_digit;
                result.push(c);

                while let Some(next_char) = stream.peek_next() {
                    match next_char {
                        ch if valid(ch) || ch == NUMBER_SEPARATOR => {
                            result.push(next_char);
                            eat_next(stream, pos);
                        }
                        #[cfg(any(not(feature = "no_float"), feature = "decimal"))]
                        '.' => {
                            stream.get_next().expect("never fails because it is `.`");

                            // Check if followed by digits or something that cannot start a property name
                            match stream.peek_next().unwrap_or('\0') {
                                // digits after period - accept the period
                                '0'..='9' => {
                                    result.push(next_char);
                                    pos.advance();
                                }
                                // _ - cannot follow a decimal point
                                '_' => {
                                    stream.unget(next_char);
                                    break;
                                }
                                // .. - reserved symbol, not a floating-point number
                                '.' => {
                                    stream.unget(next_char);
                                    break;
                                }
                                // symbol after period - probably a float
                                ch if !is_id_first_alphabetic(ch) => {
                                    result.push(next_char);
                                    pos.advance();
                                    result.push('0');
                                }
                                // Not a floating-point number
                                _ => {
                                    stream.unget(next_char);
                                    break;
                                }
                            }
                        }
                        #[cfg(not(feature = "no_float"))]
                        'e' => {
                            stream.get_next().expect("never fails it is `e`");

                            // Check if followed by digits or +/-
                            match stream.peek_next().unwrap_or('\0') {
                                // digits after e - accept the e
                                '0'..='9' => {
                                    result.push(next_char);
                                    pos.advance();
                                }
                                // +/- after e - accept the e and the sign
                                '+' | '-' => {
                                    result.push(next_char);
                                    pos.advance();
                                    result.push(
                                        stream
                                            .get_next()
                                            .expect("never fails because it is `+` or `-`"),
                                    );
                                    pos.advance();
                                }
                                // Not a floating-point number
                                _ => {
                                    stream.unget(next_char);
                                    break;
                                }
                            }
                        }
                        // 0x????, 0o????, 0b???? at beginning
                        ch @ 'x' | ch @ 'o' | ch @ 'b' | ch @ 'X' | ch @ 'O' | ch @ 'B'
                            if c == '0' && result.len() <= 1 =>
                        {
                            result.push(next_char);
                            eat_next(stream, pos);

                            valid = match ch {
                                'x' | 'X' => is_hex_digit,
                                'o' | 'O' => is_numeric_digit,
                                'b' | 'B' => is_numeric_digit,
                                _ => unreachable!(),
                            };

                            radix_base = Some(match ch {
                                'x' | 'X' => 16,
                                'o' | 'O' => 8,
                                'b' | 'B' => 2,
                                _ => unreachable!(),
                            });
                        }

                        _ => break,
                    }
                }

                let num_pos = negated.map_or(start_pos, |negated_pos| {
                    result.insert(0, '-');
                    negated_pos
                });

                // Parse number
                return Some((
                    if let Some(radix) = radix_base {
                        let out: String = result
                            .iter()
                            .skip(2)
                            .filter(|&&c| c != NUMBER_SEPARATOR)
                            .collect();

                        INT::from_str_radix(&out, radix)
                            .map(Token::IntegerConstant)
                            .unwrap_or_else(|_| {
                                Token::LexError(LERR::MalformedNumber(result.into_iter().collect()))
                            })
                    } else {
                        let out: String =
                            result.iter().filter(|&&c| c != NUMBER_SEPARATOR).collect();
                        let num = INT::from_str(&out).map(Token::IntegerConstant);

                        // If integer parsing is unnecessary, try float instead
                        #[cfg(not(feature = "no_float"))]
                        let num =
                            num.or_else(|_| FloatWrapper::from_str(&out).map(Token::FloatConstant));

                        // Then try decimal
                        #[cfg(feature = "decimal")]
                        let num =
                            num.or_else(|_| Decimal::from_str(&out).map(Token::DecimalConstant));

                        // Then try decimal in scientific notation
                        #[cfg(feature = "decimal")]
                        let num = num.or_else(|_| {
                            Decimal::from_scientific(&out).map(Token::DecimalConstant)
                        });

                        num.unwrap_or_else(|_| {
                            Token::LexError(LERR::MalformedNumber(result.into_iter().collect()))
                        })
                    },
                    num_pos,
                ));
            }

            // letter or underscore ...
            #[cfg(not(feature = "unicode-xid-ident"))]
            ('a'..='z', _) | ('_', _) | ('A'..='Z', _) => {
                return get_identifier(stream, pos, start_pos, c);
            }
            #[cfg(feature = "unicode-xid-ident")]
            (ch, _) if unicode_xid::UnicodeXID::is_xid_start(ch) || ch == '_' => {
                return get_identifier(stream, pos, start_pos, c);
            }

            // " - string literal
            ('"', _) => {
                return parse_string_literal(stream, state, pos, c, true, false, false)
                    .map_or_else(
                        |(err, err_pos)| Some((Token::LexError(err), err_pos)),
                        |(result, _)| Some((Token::StringConstant(result), start_pos)),
                    );
            }
            // ` - string literal
            ('`', _) => {
                // Start from the next line if at the end of line
                match stream.peek_next() {
                    // `\r - start from next line
                    Some('\r') => {
                        eat_next(stream, pos);
                        pos.new_line();
                        // `\r\n
                        if stream.peek_next().map(|ch| ch == '\n').unwrap_or(false) {
                            eat_next(stream, pos);
                        }
                    }
                    // `\n - start from next line
                    Some('\n') => {
                        eat_next(stream, pos);
                        pos.new_line();
                    }
                    _ => (),
                }

                return parse_string_literal(stream, state, pos, c, false, true, true).map_or_else(
                    |(err, err_pos)| Some((Token::LexError(err), err_pos)),
                    |(result, interpolated)| {
                        if interpolated {
                            Some((Token::InterpolatedString(result), start_pos))
                        } else {
                            Some((Token::StringConstant(result), start_pos))
                        }
                    },
                );
            }

            // ' - character literal
            ('\'', '\'') => {
                return Some((
                    Token::LexError(LERR::MalformedChar("".to_string())),
                    start_pos,
                ))
            }
            ('\'', _) => {
                return Some(
                    parse_string_literal(stream, state, pos, c, false, false, false).map_or_else(
                        |(err, err_pos)| (Token::LexError(err), err_pos),
                        |(result, _)| {
                            let mut chars = result.chars();
                            let first = chars.next().unwrap();

                            if chars.next().is_some() {
                                (Token::LexError(LERR::MalformedChar(result)), start_pos)
                            } else {
                                (Token::CharConstant(first), start_pos)
                            }
                        },
                    ),
                )
            }

            // Braces
            ('{', _) => return Some((Token::LeftBrace, start_pos)),
            ('}', _) => return Some((Token::RightBrace, start_pos)),

            // Parentheses
            ('(', '*') => {
                eat_next(stream, pos);
                return Some((Token::Reserved("(*".into()), start_pos));
            }
            ('(', _) => return Some((Token::LeftParen, start_pos)),
            (')', _) => return Some((Token::RightParen, start_pos)),

            // Indexing
            ('[', _) => return Some((Token::LeftBracket, start_pos)),
            (']', _) => return Some((Token::RightBracket, start_pos)),

            // Map literal
            #[cfg(not(feature = "no_object"))]
            ('#', '{') => {
                eat_next(stream, pos);
                return Some((Token::MapStart, start_pos));
            }
            // Shebang
            ('#', '!') => return Some((Token::Reserved("#!".into()), start_pos)),

            ('#', _) => return Some((Token::Reserved("#".into()), start_pos)),

            // Operators
            ('+', '=') => {
                eat_next(stream, pos);
                return Some((Token::PlusAssign, start_pos));
            }
            ('+', '+') => {
                eat_next(stream, pos);
                return Some((Token::Reserved("++".into()), start_pos));
            }
            ('+', _) if !state.non_unary => return Some((Token::UnaryPlus, start_pos)),
            ('+', _) => return Some((Token::Plus, start_pos)),

            ('-', '0'..='9') if !state.non_unary => negated = Some(start_pos),
            ('-', '0'..='9') => return Some((Token::Minus, start_pos)),
            ('-', '=') => {
                eat_next(stream, pos);
                return Some((Token::MinusAssign, start_pos));
            }
            ('-', '>') => {
                eat_next(stream, pos);
                return Some((Token::Reserved("->".into()), start_pos));
            }
            ('-', '-') => {
                eat_next(stream, pos);
                return Some((Token::Reserved("--".into()), start_pos));
            }
            ('-', _) if !state.non_unary => return Some((Token::UnaryMinus, start_pos)),
            ('-', _) => return Some((Token::Minus, start_pos)),

            ('*', ')') => {
                eat_next(stream, pos);
                return Some((Token::Reserved("*)".into()), start_pos));
            }
            ('*', '=') => {
                eat_next(stream, pos);
                return Some((Token::MultiplyAssign, start_pos));
            }
            ('*', '*') => {
                eat_next(stream, pos);

                return Some((
                    if stream.peek_next() == Some('=') {
                        eat_next(stream, pos);
                        Token::PowerOfAssign
                    } else {
                        Token::PowerOf
                    },
                    start_pos,
                ));
            }
            ('*', _) => return Some((Token::Multiply, start_pos)),

            // Comments
            ('/', '/') => {
                eat_next(stream, pos);

                let mut comment = match stream.peek_next() {
                    #[cfg(not(feature = "no_function"))]
                    #[cfg(feature = "metadata")]
                    Some('/') => {
                        eat_next(stream, pos);

                        // Long streams of `///...` are not doc-comments
                        match stream.peek_next() {
                            Some('/') => None,
                            _ => Some("///".to_string()),
                        }
                    }
                    _ if state.include_comments => Some("//".to_string()),
                    _ => None,
                };

                while let Some(c) = stream.get_next() {
                    if c == '\n' {
                        pos.new_line();
                        break;
                    }
                    if let Some(comment) = comment.as_mut() {
                        comment.push(c);
                    }
                    pos.advance();
                }

                if let Some(comment) = comment {
                    return Some((Token::Comment(comment), start_pos));
                }
            }
            ('/', '*') => {
                state.comment_level = 1;
                eat_next(stream, pos);

                let mut comment = match stream.peek_next() {
                    #[cfg(not(feature = "no_function"))]
                    #[cfg(feature = "metadata")]
                    Some('*') => {
                        eat_next(stream, pos);

                        // Long streams of `/****...` are not doc-comments
                        match stream.peek_next() {
                            Some('*') => None,
                            _ => Some("/**".to_string()),
                        }
                    }
                    _ if state.include_comments => Some("/*".to_string()),
                    _ => None,
                };

                state.comment_level =
                    scan_block_comment(stream, state.comment_level, pos, comment.as_mut());

                if let Some(comment) = comment {
                    return Some((Token::Comment(comment), start_pos));
                }
            }

            ('/', '=') => {
                eat_next(stream, pos);
                return Some((Token::DivideAssign, start_pos));
            }
            ('/', _) => return Some((Token::Divide, start_pos)),

            (';', _) => return Some((Token::SemiColon, start_pos)),
            (',', _) => return Some((Token::Comma, start_pos)),

            ('.', '.') => {
                eat_next(stream, pos);

                if stream.peek_next() == Some('.') {
                    eat_next(stream, pos);
                    return Some((Token::Reserved("...".into()), start_pos));
                } else {
                    return Some((Token::Reserved("..".into()), start_pos));
                }
            }
            ('.', _) => return Some((Token::Period, start_pos)),

            ('=', '=') => {
                eat_next(stream, pos);

                if stream.peek_next() == Some('=') {
                    eat_next(stream, pos);
                    return Some((Token::Reserved("===".into()), start_pos));
                }

                return Some((Token::EqualsTo, start_pos));
            }
            ('=', '>') => {
                eat_next(stream, pos);
                return Some((Token::DoubleArrow, start_pos));
            }
            ('=', _) => return Some((Token::Equals, start_pos)),

            (':', ':') => {
                eat_next(stream, pos);

                if stream.peek_next() == Some('<') {
                    eat_next(stream, pos);
                    return Some((Token::Reserved("::<".into()), start_pos));
                }

                return Some((Token::DoubleColon, start_pos));
            }
            (':', '=') => {
                eat_next(stream, pos);
                return Some((Token::Reserved(":=".into()), start_pos));
            }
            (':', _) => return Some((Token::Colon, start_pos)),

            ('<', '=') => {
                eat_next(stream, pos);
                return Some((Token::LessThanEqualsTo, start_pos));
            }
            ('<', '-') => {
                eat_next(stream, pos);
                return Some((Token::Reserved("<-".into()), start_pos));
            }
            ('<', '<') => {
                eat_next(stream, pos);

                return Some((
                    if stream.peek_next() == Some('=') {
                        eat_next(stream, pos);
                        Token::LeftShiftAssign
                    } else {
                        Token::LeftShift
                    },
                    start_pos,
                ));
            }
            ('<', _) => return Some((Token::LessThan, start_pos)),

            ('>', '=') => {
                eat_next(stream, pos);
                return Some((Token::GreaterThanEqualsTo, start_pos));
            }
            ('>', '>') => {
                eat_next(stream, pos);

                return Some((
                    if stream.peek_next() == Some('=') {
                        eat_next(stream, pos);
                        Token::RightShiftAssign
                    } else {
                        Token::RightShift
                    },
                    start_pos,
                ));
            }
            ('>', _) => return Some((Token::GreaterThan, start_pos)),

            ('!', '=') => {
                eat_next(stream, pos);

                if stream.peek_next() == Some('=') {
                    eat_next(stream, pos);
                    return Some((Token::Reserved("!==".into()), start_pos));
                }

                return Some((Token::NotEqualsTo, start_pos));
            }
            ('!', _) => return Some((Token::Bang, start_pos)),

            ('|', '|') => {
                eat_next(stream, pos);
                return Some((Token::Or, start_pos));
            }
            ('|', '=') => {
                eat_next(stream, pos);
                return Some((Token::OrAssign, start_pos));
            }
            ('|', _) => return Some((Token::Pipe, start_pos)),

            ('&', '&') => {
                eat_next(stream, pos);
                return Some((Token::And, start_pos));
            }
            ('&', '=') => {
                eat_next(stream, pos);
                return Some((Token::AndAssign, start_pos));
            }
            ('&', _) => return Some((Token::Ampersand, start_pos)),

            ('^', '=') => {
                eat_next(stream, pos);
                return Some((Token::XOrAssign, start_pos));
            }
            ('^', _) => return Some((Token::XOr, start_pos)),

            ('~', _) => return Some((Token::Reserved("~".into()), start_pos)),

            ('%', '=') => {
                eat_next(stream, pos);
                return Some((Token::ModuloAssign, start_pos));
            }
            ('%', _) => return Some((Token::Modulo, start_pos)),

            ('@', _) => return Some((Token::Reserved("@".into()), start_pos)),

            ('$', _) => return Some((Token::Reserved("$".into()), start_pos)),

            (ch, _) if ch.is_whitespace() => (),

            (ch, _) => {
                return Some((
                    Token::LexError(LERR::UnexpectedInput(ch.to_string())),
                    start_pos,
                ))
            }
        }
    }

    pos.advance();

    Some((Token::EOF, *pos))
}

/// Get the next identifier.
fn get_identifier(
    stream: &mut impl InputStream,
    pos: &mut Position,
    start_pos: Position,
    first_char: char,
) -> Option<(Token, Position)> {
    let mut result: smallvec::SmallVec<[char; 8]> = Default::default();
    result.push(first_char);

    while let Some(next_char) = stream.peek_next() {
        match next_char {
            x if is_id_continue(x) => {
                result.push(x);
                eat_next(stream, pos);
            }
            _ => break,
        }
    }

    let is_valid_identifier = is_valid_identifier(result.iter().cloned());

    let identifier: String = result.into_iter().collect();

    if let Some(token) = Token::lookup_from_syntax(&identifier) {
        return Some((token, start_pos));
    }

    if !is_valid_identifier {
        return Some((
            Token::LexError(LERR::MalformedIdentifier(identifier)),
            start_pos,
        ));
    }

    Some((Token::Identifier(identifier), start_pos))
}

/// Is this keyword allowed as a function?
#[inline]
#[must_use]
pub fn is_keyword_function(name: &str) -> bool {
    match name {
        KEYWORD_PRINT | KEYWORD_DEBUG | KEYWORD_TYPE_OF | KEYWORD_EVAL | KEYWORD_FN_PTR
        | KEYWORD_FN_PTR_CALL | KEYWORD_FN_PTR_CURRY | KEYWORD_IS_DEF_VAR => true,

        #[cfg(not(feature = "no_function"))]
        KEYWORD_IS_DEF_FN => true,

        _ => false,
    }
}

/// Is a text string a valid identifier?
#[must_use]
pub fn is_valid_identifier(name: impl Iterator<Item = char>) -> bool {
    let mut first_alphabetic = false;

    for ch in name {
        match ch {
            '_' => (),
            _ if is_id_first_alphabetic(ch) => first_alphabetic = true,
            _ if !first_alphabetic => return false,
            _ if char::is_ascii_alphanumeric(&ch) => (),
            _ => return false,
        }
    }

    first_alphabetic
}

/// Is a character valid to start an identifier?
#[cfg(feature = "unicode-xid-ident")]
#[inline(always)]
#[must_use]
pub fn is_id_first_alphabetic(x: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_start(x)
}

/// Is a character valid for an identifier?
#[cfg(feature = "unicode-xid-ident")]
#[inline(always)]
#[must_use]
pub fn is_id_continue(x: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_continue(x)
}

/// Is a character valid to start an identifier?
#[cfg(not(feature = "unicode-xid-ident"))]
#[inline(always)]
#[must_use]
pub fn is_id_first_alphabetic(x: char) -> bool {
    x.is_ascii_alphabetic()
}

/// Is a character valid for an identifier?
#[cfg(not(feature = "unicode-xid-ident"))]
#[inline(always)]
#[must_use]
pub fn is_id_continue(x: char) -> bool {
    x.is_ascii_alphanumeric() || x == '_'
}

/// _(internals)_ A type that implements the [`InputStream`] trait.
/// Exported under the `internals` feature only.
///
/// Multiple character streams are jointed together to form one single stream.
pub struct MultiInputsStream<'a> {
    /// Buffered character, if any.
    pub buf: Option<char>,
    /// The current stream index.
    pub index: usize,
    /// The input character streams.
    pub streams: StaticVec<Peekable<Chars<'a>>>,
}

impl InputStream for MultiInputsStream<'_> {
    #[inline]
    fn unget(&mut self, ch: char) {
        if self.buf.is_some() {
            panic!("cannot unget two characters in a row");
        }

        self.buf = Some(ch);
    }
    fn get_next(&mut self) -> Option<char> {
        if let Some(ch) = self.buf.take() {
            return Some(ch);
        }

        loop {
            if self.index >= self.streams.len() {
                // No more streams
                return None;
            } else if let Some(ch) = self.streams[self.index].next() {
                // Next character in current stream
                return Some(ch);
            } else {
                // Jump to the next stream
                self.index += 1;
            }
        }
    }
    fn peek_next(&mut self) -> Option<char> {
        if let Some(ch) = self.buf {
            return Some(ch);
        }

        loop {
            if self.index >= self.streams.len() {
                // No more streams
                return None;
            } else if let Some(&ch) = self.streams[self.index].peek() {
                // Next character in current stream
                return Some(ch);
            } else {
                // Jump to the next stream
                self.index += 1;
            }
        }
    }
}

/// _(internals)_ An iterator on a [`Token`] stream.
/// Exported under the `internals` feature only.
pub struct TokenIterator<'a> {
    /// Reference to the scripting `Engine`.
    pub engine: &'a Engine,
    /// Current state.
    pub state: TokenizeState,
    /// Current position.
    pub pos: Position,
    /// External buffer containing the next character to read, if any.
    pub tokenizer_control: TokenizerControl,
    /// Input character stream.
    pub stream: MultiInputsStream<'a>,
    /// A processor function that maps a token to another.
    pub map: Option<fn(Token) -> Token>,
}

impl<'a> Iterator for TokenIterator<'a> {
    type Item = (Token, Position);

    fn next(&mut self) -> Option<Self::Item> {
        let mut control = self.tokenizer_control.get();

        if control.is_within_text {
            // Switch to text mode terminated by back-tick
            self.state.is_within_text_terminated_by = Some('`');
            // Reset it
            control.is_within_text = false;
            self.tokenizer_control.set(control);
        }

        let (token, pos) = match get_next_token(&mut self.stream, &mut self.state, &mut self.pos) {
            // {EOF}
            None => return None,
            // {EOF} after unterminated string.
            // The only case where `TokenizeState.is_within_text_terminated_by` is set is when
            // a verbatim string or a string with continuation encounters {EOF}.
            // This is necessary to handle such cases for line-by-line parsing, but for an entire
            // script it is a syntax error.
            Some((Token::StringConstant(_), pos)) if self.state.is_within_text_terminated_by.is_some() => {
                self.state.is_within_text_terminated_by = None;
                return Some((Token::LexError(LERR::UnterminatedString), pos));
            }
            // Reserved keyword/symbol
            Some((Token::Reserved(s), pos)) => (match
                (s.as_str(), self.engine.custom_keywords.contains_key(s.as_str()))
            {
                ("===", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "'===' is not a valid operator. This is not JavaScript! Should it be '=='?".to_string(),
                )),
                ("!==", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "'!==' is not a valid operator. This is not JavaScript! Should it be '!='?".to_string(),
                )),
                ("->", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "'->' is not a valid symbol. This is not C or C++!".to_string())),
                ("<-", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "'<-' is not a valid symbol. This is not Go! Should it be '<='?".to_string(),
                )),
                (":=", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "':=' is not a valid assignment operator. This is not Go or Pascal! Should it be simply '='?".to_string(),
                )),
                ("::<", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "'::<>' is not a valid symbol. This is not Rust! Should it be '::'?".to_string(),
                )),
                ("(*", false) | ("*)", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "'(* .. *)' is not a valid comment format. This is not Pascal! Should it be '/* .. */'?".to_string(),
                )),
                ("#", false) => Token::LexError(LERR::ImproperSymbol(s,
                    "'#' is not a valid symbol. Should it be '#{'?".to_string(),
                )),
                // Reserved keyword/operator that is custom.
                (_, true) => Token::Custom(s),
                // Reserved operator that is not custom.
                (token, false) if !is_valid_identifier(token.chars()) => {
                    let msg = format!("'{}' is a reserved symbol", token);
                    Token::LexError(LERR::ImproperSymbol(s, msg))
                },
                // Reserved keyword that is not custom and disabled.
                (token, false) if self.engine.disabled_symbols.contains(token) => {
                    let msg = format!("reserved symbol '{}' is disabled", token);
                    Token::LexError(LERR::ImproperSymbol(s, msg))
                },
                // Reserved keyword/operator that is not custom.
                (_, false) => Token::Reserved(s),
            }, pos),
            // Custom keyword
            Some((Token::Identifier(s), pos)) if self.engine.custom_keywords.contains_key(s.as_str()) => {
                (Token::Custom(s), pos)
            }
            // Custom standard keyword/symbol - must be disabled
            Some((token, pos)) if self.engine.custom_keywords.contains_key(token.syntax().as_ref()) => {
                if self.engine.disabled_symbols.contains(token.syntax().as_ref()) {
                    // Disabled standard keyword/symbol
                    (Token::Custom(token.syntax().into()), pos)
                } else {
                    // Active standard keyword - should never be a custom keyword!
                    unreachable!("{:?} is an active keyword", token)
                }
            }
            // Disabled symbol
            Some((token, pos)) if self.engine.disabled_symbols.contains(token.syntax().as_ref()) => {
                (Token::Reserved(token.syntax().into()), pos)
            }
            // Normal symbol
            Some(r) => r,
        };

        // Run the mapper, if any
        let token = if let Some(map_func) = self.map {
            map_func(token)
        } else {
            token
        };

        Some((token, pos))
    }
}

impl FusedIterator for TokenIterator<'_> {}

impl Engine {
    /// _(internals)_ Tokenize an input text stream.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn lex<'a>(
        &'a self,
        input: impl IntoIterator<Item = &'a &'a str>,
    ) -> (TokenIterator<'a>, TokenizerControl) {
        self.lex_raw(input, None)
    }
    /// _(internals)_ Tokenize an input text stream with a mapping function.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn lex_with_map<'a>(
        &'a self,
        input: impl IntoIterator<Item = &'a &'a str>,
        map: fn(Token) -> Token,
    ) -> (TokenIterator<'a>, TokenizerControl) {
        self.lex_raw(input, Some(map))
    }
    /// Tokenize an input text stream with an optional mapping function.
    #[inline]
    #[must_use]
    pub(crate) fn lex_raw<'a>(
        &'a self,
        input: impl IntoIterator<Item = &'a &'a str>,
        map: Option<fn(Token) -> Token>,
    ) -> (TokenIterator<'a>, TokenizerControl) {
        let buffer: TokenizerControl = Default::default();
        let buffer2 = buffer.clone();

        (
            TokenIterator {
                engine: self,
                state: TokenizeState {
                    #[cfg(not(feature = "unchecked"))]
                    max_string_size: self.limits.max_string_size,
                    #[cfg(feature = "unchecked")]
                    max_string_size: None,
                    non_unary: false,
                    comment_level: 0,
                    include_comments: false,
                    is_within_text_terminated_by: None,
                },
                pos: Position::new(1, 0),
                tokenizer_control: buffer,
                stream: MultiInputsStream {
                    buf: None,
                    streams: input.into_iter().map(|s| s.chars().peekable()).collect(),
                    index: 0,
                },
                map,
            },
            buffer2,
        )
    }
}
