//! Main module defining the lexer and parser.

use crate::engine::{
    Engine, KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY,
    KEYWORD_SHARED, KEYWORD_PRINT, KEYWORD_THIS, KEYWORD_TYPE_OF,
};

use crate::error::LexError;
use crate::parser::INT;
use crate::utils::StaticVec;

#[cfg(not(feature = "no_float"))]
use crate::parser::FLOAT;

use crate::stdlib::{
    borrow::Cow,
    boxed::Box,
    char,
    collections::HashMap,
    fmt, format,
    iter::Peekable,
    str::{Chars, FromStr},
    string::{String, ToString},
    vec::Vec,
};

type LERR = LexError;

pub type TokenStream<'a, 't> = Peekable<TokenIterator<'a, 't>>;

/// A location (line number + character position) in the input script.
///
/// In order to keep footprint small, both line number and character position have 16-bit unsigned resolution,
/// meaning they go up to a maximum of 65,535 lines and characters per line.
/// Advancing beyond the maximum line length or maximum number of lines is not an error but has no effect.
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct Position {
    /// Line number - 0 = none
    line: u16,
    /// Character position - 0 = BOL
    pos: u16,
}

impl Position {
    /// Create a new `Position`.
    pub fn new(line: u16, position: u16) -> Self {
        assert!(line != 0, "line cannot be zero");

        Self {
            line,
            pos: position,
        }
    }

    /// Get the line number (1-based), or `None` if no position.
    pub fn line(&self) -> Option<usize> {
        if self.is_none() {
            None
        } else {
            Some(self.line as usize)
        }
    }

    /// Get the character position (1-based), or `None` if at beginning of a line.
    pub fn position(&self) -> Option<usize> {
        if self.is_none() || self.pos == 0 {
            None
        } else {
            Some(self.pos as usize)
        }
    }

    /// Advance by one character position.
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
    ///
    pub(crate) fn rewind(&mut self) {
        assert!(!self.is_none(), "cannot rewind Position::none");
        assert!(self.pos > 0, "cannot rewind at position 0");
        self.pos -= 1;
    }

    /// Advance to the next line.
    pub(crate) fn new_line(&mut self) {
        assert!(!self.is_none(), "cannot advance Position::none");

        // Advance up to maximum position
        if self.line < u16::MAX {
            self.line += 1;
            self.pos = 0;
        }
    }

    /// Create a `Position` representing no position.
    pub(crate) fn none() -> Self {
        Self { line: 0, pos: 0 }
    }

    /// Is there no `Position`?
    pub fn is_none(&self) -> bool {
        self.line == 0 && self.pos == 0
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new(1, 0)
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() {
            write!(f, "none")
        } else {
            write!(f, "line {}, position {}", self.line, self.pos)
        }
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.pos)
    }
}

/// [INTERNALS] A Rhai language token.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    /// An `INT` constant.
    IntegerConstant(INT),
    /// A `FLOAT` constaint.
    ///
    /// Never appears under the `no_float` feature.
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
    /// `~`
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
    /// `while`
    While,
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
    /// Never appears under the `no_function` feature.
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
    /// `~=`
    PowerOfAssign,
    /// `private`
    ///
    /// Never appears under the `no_function` feature.
    #[cfg(not(feature = "no_function"))]
    Private,
    /// `import`
    ///
    /// Never appears under the `no_module` feature.
    #[cfg(not(feature = "no_module"))]
    Import,
    /// `export`
    ///
    /// Never appears under the `no_module` feature.
    #[cfg(not(feature = "no_module"))]
    Export,
    /// `as`
    ///
    /// Never appears under the `no_module` feature.
    #[cfg(not(feature = "no_module"))]
    As,
    /// A lexer error.
    LexError(Box<LexError>),
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
                While => "while",
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
                PowerOf => "~",
                PowerOfAssign => "~=",

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
                _ => unreachable!("operator should be match in outer scope"),
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
            "while" => While,
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
            "~" => PowerOf,
            "~=" => PowerOfAssign,

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

            "===" | "!==" | "->" | "<-" | "=>" | ":=" | "::<" | "(*" | "*)" | "#" => {
                Reserved(syntax.into())
            }
            KEYWORD_PRINT | KEYWORD_DEBUG | KEYWORD_TYPE_OF | KEYWORD_EVAL | KEYWORD_FN_PTR
            | KEYWORD_FN_PTR_CALL | KEYWORD_FN_PTR_CURRY | KEYWORD_SHARED | KEYWORD_THIS => Reserved(syntax.into()),

            _ => return None,
        })
    }

    // Is this token EOF?
    pub fn is_eof(&self) -> bool {
        use Token::*;

        match self {
            EOF => true,
            _ => false,
        }
    }

    // If another operator is after these, it's probably an unary operator
    // (not sure about fn name).
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
            While            |
            PlusAssign       |
            MinusAssign      |
            MultiplyAssign   |
            DivideAssign     |
            LeftShiftAssign  |
            RightShiftAssign |
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
            PowerOf          |
            In               |
            PowerOfAssign    => true,

            _ => false,
        }
    }

    /// Get the precedence number of the token.
    pub fn precedence(&self, custom: Option<&HashMap<String, u8>>) -> u8 {
        use Token::*;

        match self {
            // Assignments are not considered expressions - set to zero
            Equals | PlusAssign | MinusAssign | MultiplyAssign | DivideAssign | LeftShiftAssign
            | RightShiftAssign | AndAssign | OrAssign | XOrAssign | ModuloAssign
            | PowerOfAssign => 0,

            Or | XOr | Pipe => 30,

            And | Ampersand => 60,

            EqualsTo | NotEqualsTo => 90,

            LessThan | LessThanEqualsTo | GreaterThan | GreaterThanEqualsTo => 110,

            In => 130,

            Plus | Minus => 150,

            Divide | Multiply | PowerOf | Modulo => 180,

            LeftShift | RightShift => 210,

            Period => 240,

            // Custom operators
            Custom(s) => custom.map_or(0, |c| *c.get(s).unwrap()),

            _ => 0,
        }
    }

    /// Does an expression bind to the right (instead of left)?
    pub fn is_bind_right(&self) -> bool {
        use Token::*;

        match self {
            // Assignments bind to the right
            Equals | PlusAssign | MinusAssign | MultiplyAssign | DivideAssign | LeftShiftAssign
            | RightShiftAssign | AndAssign | OrAssign | XOrAssign | ModuloAssign
            | PowerOfAssign => true,

            // Property access binds to the right
            Period => true,

            _ => false,
        }
    }

    /// Is this token an operator?
    pub fn is_operator(&self) -> bool {
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

    /// Is this token a standard keyword?
    pub fn is_keyword(&self) -> bool {
        use Token::*;

        match self {
            #[cfg(not(feature = "no_function"))]
            Fn | Private => true,

            #[cfg(not(feature = "no_module"))]
            Import | Export | As => true,

            True | False | Let | Const | If | Else | While | Loop | For | In | Continue | Break
            | Return | Throw => true,

            _ => false,
        }
    }

    /// Is this token a reserved symbol?
    pub fn is_reserved(&self) -> bool {
        match self {
            Self::Reserved(_) => true,
            _ => false,
        }
    }

    /// Is this token a custom keyword?
    pub fn is_custom(&self) -> bool {
        match self {
            Self::Custom(_) => true,
            _ => false,
        }
    }
}

impl From<Token> for String {
    fn from(token: Token) -> Self {
        token.syntax().into()
    }
}

/// [INTERNALS] State of the tokenizer.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct TokenizeState {
    /// Maximum length of a string (0 = unlimited).
    pub max_string_size: usize,
    /// Can the next token be a unary operator?
    pub non_unary: bool,
    /// Is the tokenizer currently inside a block comment?
    pub comment_level: usize,
    /// Return `None` at the end of the stream instead of `Some(Token::EOF)`?
    pub end_with_none: bool,
    /// Include comments?
    pub include_comments: bool,
}

/// [INTERNALS] Trait that encapsulates a peekable character input stream.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This trait is volatile and may change.
pub trait InputStream {
    /// Get the next character
    fn get_next(&mut self) -> Option<char>;
    /// Peek the next character
    fn peek_next(&mut self) -> Option<char>;
}

pub fn is_valid_identifier(name: impl Iterator<Item = char>) -> bool {
    let mut first_alphabetic = false;

    for ch in name {
        match ch {
            '_' => (),
            _ if char::is_ascii_alphabetic(&ch) => first_alphabetic = true,
            _ if !first_alphabetic => return false,
            _ if char::is_ascii_alphanumeric(&ch) => (),
            _ => return false,
        }
    }

    first_alphabetic
}

/// [INTERNALS] Parse a string literal wrapped by `enclosing_char`.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
pub fn parse_string_literal(
    stream: &mut impl InputStream,
    state: &mut TokenizeState,
    pos: &mut Position,
    enclosing_char: char,
) -> Result<String, (LexError, Position)> {
    let mut result = Vec::new();
    let mut escape = String::with_capacity(12);

    loop {
        let next_char = stream.get_next().ok_or((LERR::UnterminatedString, *pos))?;

        pos.advance();

        if state.max_string_size > 0 && result.len() > state.max_string_size {
            return Err((LexError::StringTooLong(state.max_string_size), *pos));
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
                seq.push(ch);
                escape.clear();

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

            // \{enclosing_char} - escaped
            ch if enclosing_char == ch && !escape.is_empty() => {
                escape.clear();
                result.push(ch)
            }

            // Close wrapper
            ch if enclosing_char == ch && escape.is_empty() => break,

            // Unknown escape sequence
            _ if !escape.is_empty() => return Err((LERR::MalformedEscapeSequence(escape), *pos)),

            // Cannot have new-lines inside string literals
            '\n' => {
                pos.rewind();
                return Err((LERR::UnterminatedString, *pos));
            }

            // All other characters
            ch => {
                escape.clear();
                result.push(ch);
            }
        }
    }

    let s = result.iter().collect::<String>();

    if state.max_string_size > 0 && s.len() > state.max_string_size {
        return Err((LexError::StringTooLong(state.max_string_size), *pos));
    }

    Ok(s)
}

/// Consume the next character.
fn eat_next(stream: &mut impl InputStream, pos: &mut Position) -> Option<char> {
    pos.advance();
    stream.get_next()
}

/// Scan for a block comment until the end.
fn scan_comment(
    stream: &mut impl InputStream,
    state: &mut TokenizeState,
    pos: &mut Position,
    comment: &mut String,
) {
    while let Some(c) = stream.get_next() {
        pos.advance();

        if state.include_comments {
            comment.push(c);
        }

        match c {
            '/' => {
                if let Some(c2) = stream.get_next() {
                    if state.include_comments {
                        comment.push(c2);
                    }
                    if c2 == '*' {
                        state.comment_level += 1;
                    }
                }
                pos.advance();
            }
            '*' => {
                if let Some(c2) = stream.get_next() {
                    if state.include_comments {
                        comment.push(c2);
                    }
                    if c2 == '/' {
                        state.comment_level -= 1;
                    }
                }
                pos.advance();
            }
            '\n' => pos.new_line(),
            _ => (),
        }

        if state.comment_level == 0 {
            break;
        }
    }
}

/// [INTERNALS] Get the next token from the `InputStream`.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
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
fn is_hex_char(c: char) -> bool {
    match c {
        'a'..='f' => true,
        'A'..='F' => true,
        '0'..='9' => true,
        _ => false,
    }
}

/// Test if the given character is an octal character.
fn is_octal_char(c: char) -> bool {
    match c {
        '0'..='7' => true,
        _ => false,
    }
}

/// Test if the given character is a binary character.
fn is_binary_char(c: char) -> bool {
    match c {
        '0' | '1' => true,
        _ => false,
    }
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
        let mut comment = String::new();
        scan_comment(stream, state, pos, &mut comment);

        if state.include_comments {
            return Some((Token::Comment(comment), start_pos));
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
                let mut result = Vec::new();
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
                            result.push(next_char);
                            eat_next(stream, pos);
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
                        ch @ 'x' | ch @ 'X' | ch @ 'o' | ch @ 'O' | ch @ 'b' | ch @ 'B'
                            if c == '0' =>
                        {
                            result.push(next_char);
                            eat_next(stream, pos);

                            let valid = match ch {
                                'x' | 'X' => is_hex_char,
                                'o' | 'O' => is_octal_char,
                                'b' | 'B' => is_binary_char,
                                _ => unreachable!(),
                            };

                            radix_base = Some(match ch {
                                'x' | 'X' => 16,
                                'o' | 'O' => 8,
                                'b' | 'B' => 2,
                                _ => unreachable!(),
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
                                Token::LexError(Box::new(LERR::MalformedNumber(
                                    result.into_iter().collect(),
                                )))
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
                            Token::LexError(Box::new(LERR::MalformedNumber(
                                result.into_iter().collect(),
                            )))
                        }),
                        start_pos,
                    ));
                }
            }

            // letter or underscore ...
            ('A'..='Z', _) | ('a'..='z', _) | ('_', _) => {
                let mut result = Vec::new();
                result.push(c);

                while let Some(next_char) = stream.peek_next() {
                    match next_char {
                        x if x.is_ascii_alphanumeric() || x == '_' => {
                            result.push(x);
                            eat_next(stream, pos);
                        }
                        _ => break,
                    }
                }

                let is_valid_identifier = is_valid_identifier(result.iter().cloned());

                let identifier: String = result.into_iter().collect();

                if !is_valid_identifier {
                    return Some((
                        Token::LexError(Box::new(LERR::MalformedIdentifier(identifier))),
                        start_pos,
                    ));
                }

                return Some((
                    Token::lookup_from_syntax(&identifier)
                        .unwrap_or_else(|| Token::Identifier(identifier)),
                    start_pos,
                ));
            }

            // " - string literal
            ('"', _) => {
                return parse_string_literal(stream, state, pos, '"').map_or_else(
                    |err| Some((Token::LexError(Box::new(err.0)), err.1)),
                    |out| Some((Token::StringConstant(out), start_pos)),
                )
            }

            // ' - character literal
            ('\'', '\'') => {
                return Some((
                    Token::LexError(Box::new(LERR::MalformedChar("".to_string()))),
                    start_pos,
                ))
            }
            ('\'', _) => {
                return Some(parse_string_literal(stream, state, pos, '\'').map_or_else(
                    |err| (Token::LexError(Box::new(err.0)), err.1),
                    |result| {
                        let mut chars = result.chars();
                        let first = chars.next();

                        if chars.next().is_some() {
                            (
                                Token::LexError(Box::new(LERR::MalformedChar(result))),
                                start_pos,
                            )
                        } else {
                            (
                                Token::CharConstant(first.expect("should be Some")),
                                start_pos,
                            )
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
            ('*', _) => return Some((Token::Multiply, start_pos)),

            // Comments
            ('/', '/') => {
                eat_next(stream, pos);

                let mut comment = if state.include_comments {
                    "//".to_string()
                } else {
                    String::new()
                };

                while let Some(c) = stream.get_next() {
                    if c == '\n' {
                        pos.new_line();
                        break;
                    }

                    if state.include_comments {
                        comment.push(c);
                    }
                    pos.advance();
                }

                if state.include_comments {
                    return Some((Token::Comment(comment), start_pos));
                }
            }
            ('/', '*') => {
                state.comment_level = 1;

                eat_next(stream, pos);

                let mut comment = if state.include_comments {
                    "/*".to_string()
                } else {
                    String::new()
                };
                scan_comment(stream, state, pos, &mut comment);

                if state.include_comments {
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
            ('.', _) => return Some((Token::Period, start_pos)),

            ('=', '=') => {
                eat_next(stream, pos);

                // Warn against `===`
                if stream.peek_next() == Some('=') {
                    eat_next(stream, pos);
                    return Some((Token::Reserved("===".into()), start_pos));
                }

                return Some((Token::EqualsTo, start_pos));
            }
            ('=', '>') => {
                eat_next(stream, pos);
                return Some((Token::Reserved("=>".into()), start_pos));
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

            ('%', '=') => {
                eat_next(stream, pos);
                return Some((Token::ModuloAssign, start_pos));
            }
            ('%', _) => return Some((Token::Modulo, start_pos)),

            ('~', '=') => {
                eat_next(stream, pos);
                return Some((Token::PowerOfAssign, start_pos));
            }
            ('~', _) => return Some((Token::PowerOf, start_pos)),

            ('@', _) => return Some((Token::Reserved("@".into()), start_pos)),

            ('\0', _) => unreachable!(),

            (ch, _) if ch.is_whitespace() => (),
            (ch, _) => {
                return Some((
                    Token::LexError(Box::new(LERR::UnexpectedInput(ch.to_string()))),
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

/// A type that implements the `InputStream` trait.
/// Multiple character streams are jointed together to form one single stream.
pub struct MultiInputsStream<'a> {
    /// The input character streams.
    streams: StaticVec<Peekable<Chars<'a>>>,
    /// The current stream index.
    index: usize,
}

impl InputStream for MultiInputsStream<'_> {
    /// Get the next character
    fn get_next(&mut self) -> Option<char> {
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
    /// Peek the next character
    fn peek_next(&mut self) -> Option<char> {
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

/// An iterator on a `Token` stream.
pub struct TokenIterator<'a, 'e> {
    /// Reference to the scripting `Engine`.
    engine: &'e Engine,
    /// Current state.
    state: TokenizeState,
    /// Current position.
    pos: Position,
    /// Input character stream.
    stream: MultiInputsStream<'a>,
}

impl<'a> Iterator for TokenIterator<'a, '_> {
    type Item = (Token, Position);

    fn next(&mut self) -> Option<Self::Item> {
        match (
            get_next_token(&mut self.stream, &mut self.state, &mut self.pos),
            self.engine.disabled_symbols.as_ref(),
            self.engine.custom_keywords.as_ref(),
        ) {
            // {EOF}
            (None, _, _) => None,
            // Reserved keyword/symbol
            (Some((Token::Reserved(s), pos)), disabled, custom) => Some((match
                (s.as_str(), custom.map(|c| c.contains_key(&s)).unwrap_or(false))
            {
                ("===", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'===' is not a valid operator. This is not JavaScript! Should it be '=='?".to_string(),
                ))),
                ("!==", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'!==' is not a valid operator. This is not JavaScript! Should it be '!='?".to_string(),
                ))),
                ("->", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'->' is not a valid symbol. This is not C or C++!".to_string()))),
                ("<-", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'<-' is not a valid symbol. This is not Go! Should it be '<='?".to_string(),
                ))),
                ("=>", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'=>' is not a valid symbol. This is not Rust! Should it be '>='?".to_string(),
                ))),
                (":=", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "':=' is not a valid assignment operator. This is not Go! Should it be simply '='?".to_string(),
                ))),
                ("::<", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'::<>' is not a valid symbol. This is not Rust! Should it be '::'?".to_string(),
                ))),
                ("(*", false) | ("*)", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'(* .. *)' is not a valid comment format. This is not Pascal! Should it be '/* .. */'?".to_string(),
                ))),
                ("#", false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    "'#' is not a valid symbol. Should it be '#{'?".to_string(),
                ))),
                // Reserved keyword/operator that is custom.
                (_, true) => Token::Custom(s),
                // Reserved operator that is not custom.
                (token, false) if !is_valid_identifier(token.chars()) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    format!("'{}' is a reserved symbol", token)
                ))),
                // Reserved keyword that is not custom and disabled.
                (token, false) if disabled.map(|d| d.contains(token)).unwrap_or(false) => Token::LexError(Box::new(LERR::ImproperSymbol(
                    format!("reserved symbol '{}' is disabled", token)
                ))),
                // Reserved keyword/operator that is not custom.
                (_, false) => Token::Reserved(s),
            }, pos)),
            // Custom keyword
            (Some((Token::Identifier(s), pos)), _, Some(custom)) if custom.contains_key(&s) => {
                Some((Token::Custom(s), pos))
            }
            // Custom standard keyword - must be disabled
            (Some((token, pos)), Some(disabled), Some(custom))
                if token.is_keyword() && custom.contains_key(token.syntax().as_ref()) =>
            {
                if disabled.contains(token.syntax().as_ref()) {
                    // Disabled standard keyword
                    Some((Token::Custom(token.syntax().into()), pos))
                } else {
                    // Active standard keyword - should never be a custom keyword!
                    unreachable!()
                }
            }
            // Disabled operator
            (Some((token, pos)), Some(disabled), _)
                if token.is_operator() && disabled.contains(token.syntax().as_ref()) =>
            {
                Some((
                    Token::LexError(Box::new(LexError::UnexpectedInput(token.syntax().into()))),
                    pos,
                ))
            }
            // Disabled standard keyword
            (Some((token, pos)), Some(disabled), _)
                if token.is_keyword() && disabled.contains(token.syntax().as_ref()) =>
            {
                Some((Token::Reserved(token.syntax().into()), pos))
            }
            (r, _, _) => r,
        }
    }
}

/// Tokenize an input text stream.
pub fn lex<'a, 'e>(input: &'a [&'a str], engine: &'e Engine) -> TokenIterator<'a, 'e> {
    TokenIterator {
        engine,
        state: TokenizeState {
            #[cfg(not(feature = "unchecked"))]
            max_string_size: engine.limits.max_string_size,
            #[cfg(feature = "unchecked")]
            max_string_size: 0,
            non_unary: false,
            comment_level: 0,
            end_with_none: false,
            include_comments: false,
        },
        pos: Position::new(1, 0),
        stream: MultiInputsStream {
            streams: input.iter().map(|s| s.chars().peekable()).collect(),
            index: 0,
        },
    }
}
