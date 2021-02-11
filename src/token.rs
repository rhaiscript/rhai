//! Main module defining the lexer and parser.

use crate::engine::{
    KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY,
    KEYWORD_PRINT, KEYWORD_THIS, KEYWORD_TYPE_OF,
};
use crate::stdlib::{
    borrow::Cow,
    char, fmt, format,
    iter::Peekable,
    num::NonZeroUsize,
    str::{Chars, FromStr},
    string::{String, ToString},
};
use crate::{Engine, LexError, StaticVec, INT};

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

type LERR = LexError;

pub type TokenStream<'a, 't> = Peekable<TokenIterator<'a, 't>>;

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
    line: u16,
    /// Character position - 0 = BOL
    pos: u16,
}

impl Position {
    /// A [`Position`] representing no position.
    pub const NONE: Self = Self { line: 0, pos: 0 };
    /// A [`Position`] representing the first position.
    pub const START: Self = Self { line: 1, pos: 0 };

    /// Create a new [`Position`].
    ///
    /// `line` must not be zero.
    /// If [`Position`] is zero, then it is at the beginning of a line.
    ///
    /// # Panics
    ///
    /// Panics if `line` is zero.
    #[inline(always)]
    pub fn new(line: u16, position: u16) -> Self {
        assert!(line != 0, "line cannot be zero");

        Self {
            line,
            pos: position,
        }
    }
    /// Get the line number (1-based), or [`None`] if there is no position.
    #[inline(always)]
    pub fn line(self) -> Option<usize> {
        if self.is_none() {
            None
        } else {
            Some(self.line as usize)
        }
    }
    /// Get the character position (1-based), or [`None`] if at beginning of a line.
    #[inline(always)]
    pub fn position(self) -> Option<usize> {
        if self.is_none() || self.pos == 0 {
            None
        } else {
            Some(self.pos as usize)
        }
    }
    /// Advance by one character position.
    #[inline(always)]
    pub(crate) fn advance(&mut self) {
        assert!(!self.is_none(), "cannot advance Position::none");

        // Advance up to maximum position
        if self.pos < u16::MAX {
            self.pos += 1;
        }
    }
    /// Go backwards by one character position.
    ///
    /// # Panics
    ///
    /// Panics if already at beginning of a line - cannot rewind to a previous line.
    #[inline(always)]
    pub(crate) fn rewind(&mut self) {
        assert!(!self.is_none(), "cannot rewind Position::none");
        assert!(self.pos > 0, "cannot rewind at position 0");
        self.pos -= 1;
    }
    /// Advance to the next line.
    #[inline(always)]
    pub(crate) fn new_line(&mut self) {
        assert!(!self.is_none(), "cannot advance Position::none");

        // Advance up to maximum position
        if self.line < u16::MAX {
            self.line += 1;
            self.pos = 0;
        }
    }
    /// Is this [`Position`] at the beginning of a line?
    #[inline(always)]
    pub fn is_beginning_of_line(self) -> bool {
        self.line == 0 && !self.is_none()
    }
    /// Is there no [`Position`]?
    #[inline(always)]
    pub fn is_none(self) -> bool {
        self == Self::NONE
    }
}

impl Default for Position {
    #[inline(always)]
    fn default() -> Self {
        Self::START
    }
}

impl fmt::Display for Position {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() {
            write!(f, "none")
        } else {
            write!(f, "line {}, position {}", self.line, self.pos)
        }
    }
}

impl fmt::Debug for Position {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.pos)
    }
}

/// _(INTERNALS)_ A Rhai language token.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    /// An `INT` constant.
    IntegerConstant(INT),
    /// A `FLOAT` constant.
    ///
    /// Reserved under the `no_float` feature.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(FLOAT),
    /// An identifier.
    Identifier(String),
    /// A character constant.
    CharConstant(char),
    /// A string constant.
    StringConstant(String),
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
    /// `e`
    Scinot,
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
    /// Get the syntax of the token.
    pub fn syntax(&self) -> Cow<'static, str> {
        use Token::*;

        match self {
            IntegerConstant(i) => i.to_string().into(),
            #[cfg(not(feature = "no_float"))]
            FloatConstant(f) => f.to_string().into(),
            StringConstant(_) => "string".into(),
            CharConstant(c) => c.to_string().into(),
            Identifier(s) => s.clone().into(),
            Reserved(s) => s.clone().into(),
            Custom(s) => s.clone().into(),
            LexError(err) => err.to_string().into(),
            Comment(s) => s.clone().into(),

            token => match token {
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
                Scinot => "e",
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
                EOF => "{EOF}",
                _ => unreachable!("operator should be matched in outer scope"),
            }
            .into(),
        }
    }

    /// Reverse lookup a token from a piece of syntax.
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
            "e" => Scinot,
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

            "===" | "!==" | "->" | "<-" | ":=" | "~" | "::<" | "(*" | "*)" | "#" | "public"
            | "new" | "use" | "module" | "package" | "var" | "static" | "begin" | "end"
            | "shared" | "with" | "each" | "then" | "goto" | "unless" | "exit" | "match"
            | "case" | "default" | "void" | "null" | "nil" | "spawn" | "thread" | "go" | "sync"
            | "async" | "await" | "yield" => Reserved(syntax.into()),

            KEYWORD_PRINT | KEYWORD_DEBUG | KEYWORD_TYPE_OF | KEYWORD_EVAL | KEYWORD_FN_PTR
            | KEYWORD_FN_PTR_CALL | KEYWORD_FN_PTR_CURRY | KEYWORD_THIS => Reserved(syntax.into()),

            _ => return None,
        })
    }

    // Is this token [`EOF`][Token::EOF]?
    #[inline(always)]
    pub fn is_eof(&self) -> bool {
        use Token::*;

        match self {
            EOF => true,
            _ => false,
        }
    }

    // If another operator is after these, it's probably an unary operator
    // (not sure about `fn` name).
    pub fn is_next_unary(&self) -> bool {
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
            Scinot           |
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
    pub fn precedence(&self) -> u8 {
        use Token::*;

        match self {
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

            PowerOf | Scinot => 190,

            LeftShift | RightShift => 210,

            Period => 240,

            _ => 0,
        }
    }

    /// Does an expression bind to the right (instead of left)?
    pub fn is_bind_right(&self) -> bool {
        use Token::*;

        match self {
            // Assignments bind to the right
            Equals | PlusAssign | MinusAssign | MultiplyAssign | DivideAssign | PowerOfAssign
            | LeftShiftAssign | RightShiftAssign | AndAssign | OrAssign | XOrAssign
            | ModuloAssign => true,

            // Property access binds to the right
            Period => true,

            // Exponentiation binds to the right
            PowerOf | Scinot => true,

            _ => false,
        }
    }

    /// Is this token a standard symbol used in the language?
    pub fn is_symbol(&self) -> bool {
        use Token::*;

        match self {
            LeftBrace | RightBrace | LeftParen | RightParen | LeftBracket | RightBracket | Plus
            | UnaryPlus | Minus | UnaryMinus | Multiply | Divide | Modulo | PowerOf | Scinot | LeftShift
            | RightShift | SemiColon | Colon | DoubleColon | Comma | Period | MapStart | Equals
            | LessThan | GreaterThan | LessThanEqualsTo | GreaterThanEqualsTo | EqualsTo
            | NotEqualsTo | Bang | Pipe | Or | XOr | Ampersand | And | PlusAssign | MinusAssign
            | MultiplyAssign | DivideAssign | LeftShiftAssign | RightShiftAssign | AndAssign
            | OrAssign | XOrAssign | ModuloAssign | PowerOfAssign => true,

            _ => false,
        }
    }

    /// Is this token an active standard keyword?
    pub fn is_keyword(&self) -> bool {
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
    pub fn is_reserved(&self) -> bool {
        match self {
            Self::Reserved(_) => true,
            _ => false,
        }
    }

    /// Convert a token into a function name, if possible.
    #[cfg(not(feature = "no_function"))]
    pub(crate) fn into_function_name_for_override(self) -> Result<String, Self> {
        match self {
            Self::Reserved(s) if can_override_keyword(&s) => Ok(s),
            Self::Custom(s) | Self::Identifier(s) if is_valid_identifier(s.chars()) => Ok(s),
            _ => Err(self),
        }
    }

    /// Is this token a custom keyword?
    #[inline(always)]
    pub fn is_custom(&self) -> bool {
        match self {
            Self::Custom(_) => true,
            _ => false,
        }
    }
}

impl From<Token> for String {
    #[inline(always)]
    fn from(token: Token) -> Self {
        token.syntax().into()
    }
}

/// _(INTERNALS)_ State of the tokenizer.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct TokenizeState {
    /// Maximum length of a string (0 = unlimited).
    pub max_string_size: Option<NonZeroUsize>,
    /// Can the next token be a unary operator?
    pub non_unary: bool,
    /// Is the tokenizer currently inside a block comment?
    pub comment_level: usize,
    /// Return [`None`] at the end of the stream instead of [`Some(Token::EOF)`][Token::EOF]?
    pub end_with_none: bool,
    /// Include comments?
    pub include_comments: bool,
    /// Disable doc-comments?
    pub disable_doc_comments: bool,
}

/// _(INTERNALS)_ Trait that encapsulates a peekable character input stream.
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
    fn peek_next(&mut self) -> Option<char>;
}

/// _(INTERNALS)_ Parse a string literal wrapped by `enclosing_char`.
/// Exported under the `internals` feature only.
///
/// # Volatile API
///
/// This function is volatile and may change.
pub fn parse_string_literal(
    stream: &mut impl InputStream,
    state: &mut TokenizeState,
    pos: &mut Position,
    enclosing_char: char,
) -> Result<String, (LexError, Position)> {
    let mut result: StaticVec<char> = Default::default();
    let mut escape: StaticVec<char> = Default::default();

    let start = *pos;

    loop {
        let next_char = stream.get_next().ok_or((LERR::UnterminatedString, start))?;

        pos.advance();

        if let Some(max) = state.max_string_size {
            if result.len() > max.get() {
                return Err((LexError::StringTooLong(max.get()), *pos));
            }
        }

        match next_char {
            // \...
            '\\' if escape.is_empty() => {
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
                    _ => unreachable!("expecting 'x', 'u' or 'U', but gets {}", ch),
                };

                for _ in 0..len {
                    let c = stream.get_next().ok_or_else(|| {
                        (
                            LERR::MalformedEscapeSequence(seq.iter().cloned().collect()),
                            *pos,
                        )
                    })?;

                    seq.push(c);
                    pos.advance();

                    out_val *= 16;
                    out_val += c.to_digit(16).ok_or_else(|| {
                        (
                            LERR::MalformedEscapeSequence(seq.iter().cloned().collect()),
                            *pos,
                        )
                    })?;
                }

                result.push(char::from_u32(out_val).ok_or_else(|| {
                    (
                        LERR::MalformedEscapeSequence(seq.into_iter().collect()),
                        *pos,
                    )
                })?);
            }

            // \{enclosing_char} - escaped
            ch if enclosing_char == ch && !escape.is_empty() => {
                escape.clear();
                result.push(ch)
            }

            // Close wrapper
            ch if enclosing_char == ch && escape.is_empty() => break,

            // Unknown escape sequence
            ch if !escape.is_empty() => {
                escape.push(ch);

                return Err((
                    LERR::MalformedEscapeSequence(escape.into_iter().collect()),
                    *pos,
                ));
            }

            // Cannot have new-lines inside string literals
            '\n' => {
                pos.rewind();
                return Err((LERR::UnterminatedString, start));
            }

            // All other characters
            ch => {
                escape.clear();
                result.push(ch);
            }
        }
    }

    let s = result.iter().collect::<String>();

    if let Some(max) = state.max_string_size {
        if s.len() > max.get() {
            return Err((LexError::StringTooLong(max.get()), *pos));
        }
    }

    Ok(s)
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
    mut level: usize,
    pos: &mut Position,
    comment: &mut Option<String>,
) -> usize {
    while let Some(c) = stream.get_next() {
        pos.advance();

        if let Some(ref mut comment) = comment {
            comment.push(c);
        }

        match c {
            '/' => {
                if let Some(c2) = stream.peek_next() {
                    if c2 == '*' {
                        eat_next(stream, pos);
                        if let Some(ref mut comment) = comment {
                            comment.push(c2);
                        }
                        level += 1;
                    }
                }
            }
            '*' => {
                if let Some(c2) = stream.peek_next() {
                    if c2 == '/' {
                        eat_next(stream, pos);
                        if let Some(ref mut comment) = comment {
                            comment.push(c2);
                        }
                        level -= 1;
                    }
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

/// _(INTERNALS)_ Get the next token from the `stream`.
/// Exported under the `internals` feature only.
///
/// # Volatile API
///
/// This function is volatile and may change.
#[inline]
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
fn is_hex_char(c: char) -> bool {
    match c {
        'a'..='f' => true,
        'A'..='F' => true,
        '0'..='9' => true,
        _ => false,
    }
}

/// Test if the given character is an octal character.
#[inline(always)]
fn is_octal_char(c: char) -> bool {
    match c {
        '0'..='7' => true,
        _ => false,
    }
}

/// Test if the given character is a binary character.
#[inline(always)]
fn is_binary_char(c: char) -> bool {
    match c {
        '0' | '1' => true,
        _ => false,
    }
}

/// Test if the comment block is a doc-comment.
#[inline(always)]
pub fn is_doc_comment(comment: &str) -> bool {
    (comment.starts_with("///") && !comment.starts_with("////"))
        || (comment.starts_with("/**") && !comment.starts_with("/***"))
}

/// Get the next token.
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

        state.comment_level = scan_block_comment(stream, state.comment_level, pos, &mut comment);

        if state.include_comments
            || (!state.disable_doc_comments && is_doc_comment(comment.as_ref().unwrap()))
        {
            return Some((Token::Comment(comment.unwrap()), start_pos));
        }
    }

    let mut negated = false;

    while let Some(c) = stream.get_next() {
        pos.advance();

        let start_pos = *pos;

        match (c, stream.peek_next().unwrap_or('\0')) {
            // \n
            ('\n', _) => pos.new_line(),

            // digit ...
            ('0'..='9', _) => {
                let mut result: StaticVec<char> = Default::default();
                let mut radix_base: Option<u32> = None;
                result.push(c);

                while let Some(next_char) = stream.peek_next() {
                    match next_char {
                        '0'..='9' | '_' => {
                            result.push(next_char);
                            eat_next(stream, pos);
                        }
                        #[cfg(not(feature = "no_float"))]
                        '.' => {
                            stream.get_next().unwrap();

                            // Check if followed by digits or something that cannot start a property name
                            match stream.peek_next().unwrap_or('\0') {
                                // digits after period - accept the period
                                '0'..='9' => {
                                    result.push(next_char);
                                    pos.advance()
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
                                ch @ _ if !is_id_first_alphabetic(ch) => {
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

                            while let Some(next_char_in_float) = stream.peek_next() {
                                match next_char_in_float {
                                    '0'..='9' | '_' => {
                                        result.push(next_char_in_float);
                                        eat_next(stream, pos);
                                    }
                                    _ => break,
                                }
                            }
                        }
                        // 0x????, 0o????, 0b????
                        ch @ 'x' | ch @ 'o' | ch @ 'b' | ch @ 'X' | ch @ 'O' | ch @ 'B'
                            if c == '0' =>
                        {
                            result.push(next_char);
                            eat_next(stream, pos);

                            let valid = match ch {
                                'x' | 'X' => is_hex_char,
                                'o' | 'O' => is_octal_char,
                                'b' | 'B' => is_binary_char,
                                _ => unreachable!("expecting 'x', 'o' or 'b', but gets {}", ch),
                            };

                            radix_base = Some(match ch {
                                'x' | 'X' => 16,
                                'o' | 'O' => 8,
                                'b' | 'B' => 2,
                                _ => unreachable!("expecting 'x', 'o' or 'b', but gets {}", ch),
                            });

                            while let Some(next_char_in_escape_seq) = stream.peek_next() {
                                if !valid(next_char_in_escape_seq) {
                                    break;
                                }

                                result.push(next_char_in_escape_seq);
                                eat_next(stream, pos);
                            }
                        }

                        _ => break,
                    }
                }

                if negated {
                    result.insert(0, '-');
                }

                // Parse number
                if let Some(radix) = radix_base {
                    let out: String = result.iter().skip(2).filter(|&&c| c != '_').collect();

                    return Some((
                        INT::from_str_radix(&out, radix)
                            .map(Token::IntegerConstant)
                            .unwrap_or_else(|_| {
                                Token::LexError(LERR::MalformedNumber(result.into_iter().collect()))
                            }),
                        start_pos,
                    ));
                } else {
                    let out: String = result.iter().filter(|&&c| c != '_').collect();
                    let num = INT::from_str(&out).map(Token::IntegerConstant);

                    // If integer parsing is unnecessary, try float instead
                    #[cfg(not(feature = "no_float"))]
                    let num = num.or_else(|_| FLOAT::from_str(&out).map(Token::FloatConstant));

                    return Some((
                        num.unwrap_or_else(|_| {
                            Token::LexError(LERR::MalformedNumber(result.into_iter().collect()))
                        }),
                        start_pos,
                    ));
                }
            }
            ('e', _) => return Some((Token::Scinot, start_pos)),

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
                return parse_string_literal(stream, state, pos, '"').map_or_else(
                    |err| Some((Token::LexError(err.0), err.1)),
                    |out| Some((Token::StringConstant(out), start_pos)),
                )
            }

            // ' - character literal
            ('\'', '\'') => {
                return Some((
                    Token::LexError(LERR::MalformedChar("".to_string())),
                    start_pos,
                ))
            }
            ('\'', _) => {
                return Some(parse_string_literal(stream, state, pos, '\'').map_or_else(
                    |err| (Token::LexError(err.0), err.1),
                    |result| {
                        let mut chars = result.chars();
                        let first = chars.next().unwrap();

                        if chars.next().is_some() {
                            (Token::LexError(LERR::MalformedChar(result)), start_pos)
                        } else {
                            (Token::CharConstant(first), start_pos)
                        }
                    },
                ))
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

            ('-', '0'..='9') if !state.non_unary => negated = true,
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
                    Some('/') if !state.disable_doc_comments => {
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
                    if let Some(ref mut comment) = comment {
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
                    Some('*') if !state.disable_doc_comments => {
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
                    scan_block_comment(stream, state.comment_level, pos, &mut comment);

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

    if state.end_with_none {
        None
    } else {
        Some((Token::EOF, *pos))
    }
}

/// Get the next identifier.
fn get_identifier(
    stream: &mut impl InputStream,
    pos: &mut Position,
    start_pos: Position,
    first_char: char,
) -> Option<(Token, Position)> {
    let mut result: StaticVec<_> = Default::default();
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

    return Some((Token::Identifier(identifier), start_pos));
}

/// Is this keyword allowed as a function?
#[inline(always)]
pub fn is_keyword_function(name: &str) -> bool {
    match name {
        KEYWORD_PRINT | KEYWORD_DEBUG | KEYWORD_TYPE_OF | KEYWORD_EVAL | KEYWORD_FN_PTR
        | KEYWORD_FN_PTR_CALL | KEYWORD_FN_PTR_CURRY => true,
        _ => false,
    }
}

/// Can this keyword be overridden as a function?
#[cfg(not(feature = "no_function"))]
#[inline(always)]
pub fn can_override_keyword(name: &str) -> bool {
    match name {
        KEYWORD_PRINT | KEYWORD_DEBUG | KEYWORD_TYPE_OF | KEYWORD_EVAL | KEYWORD_FN_PTR => true,
        _ => false,
    }
}

/// Is a text string a valid identifier?
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

#[cfg(feature = "unicode-xid-ident")]
#[inline(always)]
fn is_id_first_alphabetic(x: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_start(x)
}

#[cfg(feature = "unicode-xid-ident")]
#[inline(always)]
fn is_id_continue(x: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_continue(x)
}

#[cfg(not(feature = "unicode-xid-ident"))]
#[inline(always)]
fn is_id_first_alphabetic(x: char) -> bool {
    x.is_ascii_alphabetic()
}

#[cfg(not(feature = "unicode-xid-ident"))]
#[inline(always)]
fn is_id_continue(x: char) -> bool {
    x.is_ascii_alphanumeric() || x == '_'
}

/// A type that implements the [`InputStream`] trait.
/// Multiple character streams are jointed together to form one single stream.
pub struct MultiInputsStream<'a> {
    /// Buffered character, if any.
    buf: Option<char>,
    /// The input character streams.
    streams: StaticVec<Peekable<Chars<'a>>>,
    /// The current stream index.
    index: usize,
}

impl InputStream for MultiInputsStream<'_> {
    #[inline(always)]
    fn unget(&mut self, ch: char) {
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

/// An iterator on a [`Token`] stream.
pub struct TokenIterator<'a, 'e> {
    /// Reference to the scripting `Engine`.
    engine: &'e Engine,
    /// Current state.
    state: TokenizeState,
    /// Current position.
    pos: Position,
    /// Input character stream.
    stream: MultiInputsStream<'a>,
    /// A processor function that maps a token to another.
    map: Option<fn(Token) -> Token>,
}

impl<'a> Iterator for TokenIterator<'a, '_> {
    type Item = (Token, Position);

    fn next(&mut self) -> Option<Self::Item> {
        let (token, pos) = match get_next_token(&mut self.stream, &mut self.state, &mut self.pos) {
            // {EOF}
            None => return None,
            // Reserved keyword/symbol
            Some((Token::Reserved(s), pos)) => (match
                (s.as_str(), self.engine.custom_keywords.contains_key(&s))
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
                ("(*", false) | ("*)", false) | ("begin", false) | ("end", false) => Token::LexError(LERR::ImproperSymbol(s,
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
            Some((Token::Identifier(s), pos)) if self.engine.custom_keywords.contains_key(&s) => {
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
        let token = if let Some(map) = self.map {
            map(token)
        } else {
            token
        };

        Some((token, pos))
    }
}

impl Engine {
    /// Tokenize an input text stream.
    #[inline(always)]
    pub fn lex<'a, 'e>(
        &'e self,
        input: impl IntoIterator<Item = &'a &'a str>,
    ) -> TokenIterator<'a, 'e> {
        self.lex_raw(input, None)
    }
    /// Tokenize an input text stream with a mapping function.
    #[inline(always)]
    pub fn lex_with_map<'a, 'e>(
        &'e self,
        input: impl IntoIterator<Item = &'a &'a str>,
        map: fn(Token) -> Token,
    ) -> TokenIterator<'a, 'e> {
        self.lex_raw(input, Some(map))
    }
    /// Tokenize an input text stream with an optional mapping function.
    #[inline]
    fn lex_raw<'a, 'e>(
        &'e self,
        input: impl IntoIterator<Item = &'a &'a str>,
        map: Option<fn(Token) -> Token>,
    ) -> TokenIterator<'a, 'e> {
        TokenIterator {
            engine: self,
            state: TokenizeState {
                #[cfg(not(feature = "unchecked"))]
                max_string_size: self.limits.max_string_size,
                #[cfg(feature = "unchecked")]
                max_string_size: None,
                non_unary: false,
                comment_level: 0,
                end_with_none: false,
                include_comments: false,
                disable_doc_comments: self.disable_doc_comments,
            },
            pos: Position::new(1, 0),
            stream: MultiInputsStream {
                buf: None,
                streams: input.into_iter().map(|s| s.chars().peekable()).collect(),
                index: 0,
            },
            map,
        }
    }
}
