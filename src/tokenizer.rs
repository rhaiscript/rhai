//! Main module defining the lexer and parser.

use crate::engine::Precedence;
use crate::func::native::OnParseTokenCallback;
use crate::{Engine, Identifier, LexError, Position, SmartString, StaticVec, INT, UNSIGNED_INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    cell::RefCell,
    char, fmt,
    iter::{FusedIterator, Peekable},
    rc::Rc,
    str::{Chars, FromStr},
};

/// _(internals)_ A type containing commands to control the tokenizer.
#[derive(Debug, Clone, Eq, PartialEq, Default, Hash)]
pub struct TokenizerControlBlock {
    /// Is the current tokenizer position within an interpolated text string?
    ///
    /// This flag allows switching the tokenizer back to _text_ parsing after an interpolation stream.
    pub is_within_text: bool,
    /// Global comments.
    #[cfg(feature = "metadata")]
    pub global_comments: String,
    /// Whitespace-compressed version of the script (if any).
    ///
    /// Set to `Some` in order to collect a compressed script.
    pub compressed: Option<String>,
}

impl TokenizerControlBlock {
    /// Create a new `TokenizerControlBlock`.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            is_within_text: false,
            #[cfg(feature = "metadata")]
            global_comments: String::new(),
            compressed: None,
        }
    }
}

/// _(internals)_ A shared object that allows control of the tokenizer from outside.
pub type TokenizerControl = Rc<RefCell<TokenizerControlBlock>>;

type LERR = LexError;

/// Separator character for numbers.
const NUMBER_SEPARATOR: char = '_';

/// A stream of tokens.
pub type TokenStream<'a> = Peekable<TokenIterator<'a>>;

/// _(internals)_ A Rhai language token.
/// Exported under the `internals` feature only.
#[derive(Debug, PartialEq, Clone, Hash)]
#[non_exhaustive]
pub enum Token {
    /// An `INT` constant.
    IntegerConstant(INT),
    /// A `FLOAT` constant, including its text representation.
    ///
    /// Reserved under the `no_float` feature.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(Box<(crate::types::FloatWrapper<crate::FLOAT>, Identifier)>),
    /// A [`Decimal`][rust_decimal::Decimal] constant.
    ///
    /// Requires the `decimal` feature, including its text representation.
    #[cfg(feature = "decimal")]
    DecimalConstant(Box<(rust_decimal::Decimal, Identifier)>),
    /// An identifier.
    Identifier(Box<Identifier>),
    /// A character constant.
    CharConstant(char),
    /// A string constant.
    StringConstant(Box<SmartString>),
    /// An interpolated string.
    InterpolatedString(Box<SmartString>),
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
    /// `()`
    Unit,
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
    /// `?.`
    ///
    /// Reserved under the `no_object` feature.
    #[cfg(not(feature = "no_object"))]
    Elvis,
    /// `??`
    DoubleQuestion,
    /// `?[`
    ///
    /// Reserved under the `no_object` feature.
    #[cfg(not(feature = "no_index"))]
    QuestionBracket,
    /// `..`
    ExclusiveRange,
    /// `..=`
    InclusiveRange,
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
    /// `!in`
    NotIn,
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
    LexError(Box<LexError>),
    /// A comment block.
    Comment(Box<String>),
    /// A reserved symbol.
    Reserved(Box<Identifier>),
    /// A custom keyword.
    ///
    /// Not available under `no_custom_syntax`.
    #[cfg(not(feature = "no_custom_syntax"))]
    Custom(Box<Identifier>),
    /// End of the input stream.
    /// Used as a placeholder for the end of input.
    EOF,
}

impl fmt::Display for Token {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        match self {
            IntegerConstant(i) => write!(f, "{i}"),
            #[cfg(not(feature = "no_float"))]
            FloatConstant(v) => write!(f, "{}", v.0),
            #[cfg(feature = "decimal")]
            DecimalConstant(d) => write!(f, "{}", d.0),
            StringConstant(s) => write!(f, r#""{s}""#),
            InterpolatedString(..) => f.write_str("string"),
            CharConstant(c) => write!(f, "{c}"),
            Identifier(s) => f.write_str(s),
            Reserved(s) => f.write_str(s),
            #[cfg(not(feature = "no_custom_syntax"))]
            Custom(s) => f.write_str(s),
            LexError(err) => write!(f, "{err}"),
            Comment(s) => f.write_str(s),

            EOF => f.write_str("{EOF}"),

            token => f.write_str(token.literal_syntax()),
        }
    }
}

// Table-driven keyword recognizer generated by GNU `gperf` on the file `tools/keywords.txt`.
//
// When adding new keywords, make sure to update `tools/keywords.txt` and re-generate this.

const MIN_KEYWORD_LEN: usize = 1;
const MAX_KEYWORD_LEN: usize = 8;
const MIN_KEYWORD_HASH_VALUE: usize = 1;
const MAX_KEYWORD_HASH_VALUE: usize = 152;

static KEYWORD_ASSOC_VALUES: [u8; 257] = [
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 115, 153, 100, 153, 110,
    105, 40, 80, 2, 20, 25, 125, 95, 15, 40, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 55,
    35, 10, 5, 0, 30, 110, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 120, 105, 100, 85, 90, 153, 125, 5,
    0, 125, 35, 10, 100, 153, 20, 0, 153, 10, 0, 45, 55, 0, 153, 50, 55, 5, 0, 153, 0, 0, 35, 153,
    45, 50, 30, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153, 153,
    153,
];
static KEYWORDS_LIST: [(&str, Token); 153] = [
    ("", Token::EOF),
    (">", Token::GreaterThan),
    (">=", Token::GreaterThanEqualsTo),
    (")", Token::RightParen),
    ("", Token::EOF),
    ("const", Token::Const),
    ("=", Token::Equals),
    ("==", Token::EqualsTo),
    ("continue", Token::Continue),
    ("", Token::EOF),
    ("catch", Token::Catch),
    ("<", Token::LessThan),
    ("<=", Token::LessThanEqualsTo),
    ("for", Token::For),
    ("loop", Token::Loop),
    ("", Token::EOF),
    (".", Token::Period),
    ("<<", Token::LeftShift),
    ("<<=", Token::LeftShiftAssign),
    ("", Token::EOF),
    ("false", Token::False),
    ("*", Token::Multiply),
    ("*=", Token::MultiplyAssign),
    ("let", Token::Let),
    ("", Token::EOF),
    ("while", Token::While),
    ("+", Token::Plus),
    ("+=", Token::PlusAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("throw", Token::Throw),
    ("}", Token::RightBrace),
    (">>", Token::RightShift),
    (">>=", Token::RightShiftAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    (";", Token::SemiColon),
    ("=>", Token::DoubleArrow),
    ("", Token::EOF),
    ("else", Token::Else),
    ("", Token::EOF),
    ("/", Token::Divide),
    ("/=", Token::DivideAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("{", Token::LeftBrace),
    ("**", Token::PowerOf),
    ("**=", Token::PowerOfAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("|", Token::Pipe),
    ("|=", Token::OrAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    (":", Token::Colon),
    ("..", Token::ExclusiveRange),
    ("..=", Token::InclusiveRange),
    ("", Token::EOF),
    ("until", Token::Until),
    ("switch", Token::Switch),
    #[cfg(not(feature = "no_function"))]
    ("private", Token::Private),
    #[cfg(feature = "no_function")]
    ("", Token::EOF),
    ("try", Token::Try),
    ("true", Token::True),
    ("break", Token::Break),
    ("return", Token::Return),
    #[cfg(not(feature = "no_function"))]
    ("fn", Token::Fn),
    #[cfg(feature = "no_function")]
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    #[cfg(not(feature = "no_module"))]
    ("import", Token::Import),
    #[cfg(feature = "no_module")]
    ("", Token::EOF),
    #[cfg(not(feature = "no_object"))]
    ("?.", Token::Elvis),
    #[cfg(feature = "no_object")]
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    #[cfg(not(feature = "no_module"))]
    ("export", Token::Export),
    #[cfg(feature = "no_module")]
    ("", Token::EOF),
    ("in", Token::In),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("(", Token::LeftParen),
    ("||", Token::Or),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("^", Token::XOr),
    ("^=", Token::XOrAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("_", Token::Underscore),
    ("::", Token::DoubleColon),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("-", Token::Minus),
    ("-=", Token::MinusAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("]", Token::RightBracket),
    ("()", Token::Unit),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("&", Token::Ampersand),
    ("&=", Token::AndAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("%", Token::Modulo),
    ("%=", Token::ModuloAssign),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("!", Token::Bang),
    ("!=", Token::NotEqualsTo),
    ("!in", Token::NotIn),
    ("", Token::EOF),
    ("", Token::EOF),
    ("[", Token::LeftBracket),
    ("if", Token::If),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    (",", Token::Comma),
    ("do", Token::Do),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    #[cfg(not(feature = "no_module"))]
    ("as", Token::As),
    #[cfg(feature = "no_module")]
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    #[cfg(not(feature = "no_index"))]
    ("?[", Token::QuestionBracket),
    #[cfg(feature = "no_index")]
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("??", Token::DoubleQuestion),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("&&", Token::And),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("", Token::EOF),
    ("#{", Token::MapStart),
];

// Table-driven reserved symbol recognizer generated by GNU `gperf` on the file `tools/reserved.txt`.
//
// When adding new reserved symbols, make sure to update `tools/reserved.txt` and re-generate this.

const MIN_RESERVED_LEN: usize = 1;
const MAX_RESERVED_LEN: usize = 10;
const MIN_RESERVED_HASH_VALUE: usize = 1;
const MAX_RESERVED_HASH_VALUE: usize = 149;

static RESERVED_ASSOC_VALUES: [u8; 256] = [
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 10, 150, 5, 35, 150, 150,
    150, 45, 35, 30, 30, 150, 20, 15, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 35,
    30, 15, 5, 25, 0, 25, 150, 150, 150, 150, 150, 65, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 40, 150, 150, 150, 150, 150, 0, 150, 0,
    0, 0, 15, 45, 10, 15, 150, 150, 35, 25, 10, 50, 0, 150, 5, 0, 15, 0, 5, 25, 45, 15, 150, 150,
    25, 150, 20, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
    150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150, 150,
];
static RESERVED_LIST: [(&str, bool, bool, bool); 150] = [
    ("", false, false, false),
    ("?", true, false, false),
    ("as", cfg!(feature = "no_module"), false, false),
    ("use", true, false, false),
    ("case", true, false, false),
    ("async", true, false, false),
    ("public", true, false, false),
    ("package", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("super", true, false, false),
    ("#", true, false, false),
    ("private", cfg!(feature = "no_function"), false, false),
    ("var", true, false, false),
    ("protected", true, false, false),
    ("spawn", true, false, false),
    ("shared", true, false, false),
    ("is", true, false, false),
    ("===", true, false, false),
    ("sync", true, false, false),
    ("curry", true, true, true),
    ("static", true, false, false),
    ("default", true, false, false),
    ("!==", true, false, false),
    ("is_shared", cfg!(not(feature = "no_closure")), true, true),
    ("print", true, true, false),
    ("", false, false, false),
    ("#!", true, false, false),
    ("", false, false, false),
    ("this", true, false, false),
    ("is_def_var", true, true, false),
    ("thread", true, false, false),
    ("?.", cfg!(feature = "no_object"), false, false),
    ("", false, false, false),
    ("is_def_fn", cfg!(not(feature = "no_function")), true, false),
    ("yield", true, false, false),
    ("", false, false, false),
    ("fn", cfg!(feature = "no_function"), false, false),
    ("new", true, false, false),
    ("call", true, true, true),
    ("match", true, false, false),
    ("~", true, false, false),
    ("!.", true, false, false),
    ("", false, false, false),
    ("eval", true, true, false),
    ("await", true, false, false),
    ("", false, false, false),
    (":=", true, false, false),
    ("...", true, false, false),
    ("null", true, false, false),
    ("debug", true, true, false),
    ("@", true, false, false),
    ("type_of", true, true, true),
    ("", false, false, false),
    ("with", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("<-", true, false, false),
    ("", false, false, false),
    ("void", true, false, false),
    ("", false, false, false),
    ("import", cfg!(feature = "no_module"), false, false),
    ("--", true, false, false),
    ("nil", true, false, false),
    ("exit", false, false, false),
    ("", false, false, false),
    ("export", cfg!(feature = "no_module"), false, false),
    ("<|", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("$", true, false, false),
    ("->", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("|>", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("module", true, false, false),
    ("?[", cfg!(feature = "no_index"), false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("Fn", true, true, false),
    ("::<", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("++", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    (":;", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("*)", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("(*", true, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("", false, false, false),
    ("go", true, false, false),
    ("", false, false, false),
    ("goto", true, false, false),
];

impl Token {
    /// Is the token a literal symbol?
    #[must_use]
    pub const fn is_literal(&self) -> bool {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        match self {
            IntegerConstant(..) => false,
            #[cfg(not(feature = "no_float"))]
            FloatConstant(..) => false,
            #[cfg(feature = "decimal")]
            DecimalConstant(..) => false,
            StringConstant(..)
            | InterpolatedString(..)
            | CharConstant(..)
            | Identifier(..)
            | Reserved(..) => false,
            #[cfg(not(feature = "no_custom_syntax"))]
            Custom(..) => false,
            LexError(..) | Comment(..) => false,

            EOF => false,

            _ => true,
        }
    }
    /// Get the literal syntax of the token.
    ///
    /// # Panics
    ///
    /// Panics if the token is not a literal symbol.
    #[must_use]
    pub const fn literal_syntax(&self) -> &'static str {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        match self {
            LeftBrace => "{",
            RightBrace => "}",
            LeftParen => "(",
            RightParen => ")",
            LeftBracket => "[",
            RightBracket => "]",
            Unit => "()",
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
            #[cfg(not(feature = "no_object"))]
            Elvis => "?.",
            DoubleQuestion => "??",
            #[cfg(not(feature = "no_index"))]
            QuestionBracket => "?[",
            ExclusiveRange => "..",
            InclusiveRange => "..=",
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
            NotIn => "!in",
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

            _ => panic!("token is not a literal symbol"),
        }
    }

    /// Is this token an op-assignment operator?
    #[inline]
    #[must_use]
    pub const fn is_op_assignment(&self) -> bool {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        matches!(
            self,
            PlusAssign
                | MinusAssign
                | MultiplyAssign
                | DivideAssign
                | LeftShiftAssign
                | RightShiftAssign
                | ModuloAssign
                | PowerOfAssign
                | AndAssign
                | OrAssign
                | XOrAssign
        )
    }

    /// Get the corresponding operator of the token if it is an op-assignment operator.
    #[must_use]
    pub const fn get_base_op_from_assignment(&self) -> Option<Self> {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        Some(match self {
            PlusAssign => Plus,
            MinusAssign => Minus,
            MultiplyAssign => Multiply,
            DivideAssign => Divide,
            LeftShiftAssign => LeftShift,
            RightShiftAssign => RightShift,
            ModuloAssign => Modulo,
            PowerOfAssign => PowerOf,
            AndAssign => Ampersand,
            OrAssign => Pipe,
            XOrAssign => XOr,
            _ => return None,
        })
    }

    /// Has this token a corresponding op-assignment operator?
    #[inline]
    #[must_use]
    pub const fn has_op_assignment(&self) -> bool {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        matches!(
            self,
            Plus | Minus
                | Multiply
                | Divide
                | LeftShift
                | RightShift
                | Modulo
                | PowerOf
                | Ampersand
                | Pipe
                | XOr
        )
    }

    /// Get the corresponding op-assignment operator of the token.
    #[must_use]
    pub const fn convert_to_op_assignment(&self) -> Option<Self> {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        Some(match self {
            Plus => PlusAssign,
            Minus => MinusAssign,
            Multiply => MultiplyAssign,
            Divide => DivideAssign,
            LeftShift => LeftShiftAssign,
            RightShift => RightShiftAssign,
            Modulo => ModuloAssign,
            PowerOf => PowerOfAssign,
            Ampersand => AndAssign,
            Pipe => OrAssign,
            XOr => XOrAssign,
            _ => return None,
        })
    }

    /// Reverse lookup a symbol token from a piece of syntax.
    #[inline]
    #[must_use]
    pub fn lookup_symbol_from_syntax(syntax: &str) -> Option<Self> {
        // This implementation is based upon a pre-calculated table generated
        // by GNU `gperf` on the list of keywords.
        let utf8 = syntax.as_bytes();
        let len = utf8.len();

        if !(MIN_KEYWORD_LEN..=MAX_KEYWORD_LEN).contains(&len) {
            return None;
        }

        let mut hash_val = len;

        match len {
            1 => (),
            _ => hash_val += KEYWORD_ASSOC_VALUES[(utf8[1] as usize) + 1] as usize,
        }
        hash_val += KEYWORD_ASSOC_VALUES[utf8[0] as usize] as usize;

        if !(MIN_KEYWORD_HASH_VALUE..=MAX_KEYWORD_HASH_VALUE).contains(&hash_val) {
            return None;
        }

        match KEYWORDS_LIST[hash_val] {
            (_, Self::EOF) => None,
            // Fail early to avoid calling memcmp().
            // Since we are already working with bytes, mind as well check the first one.
            (s, ref t) if s.len() == len && s.as_bytes()[0] == utf8[0] && s == syntax => {
                Some(t.clone())
            }
            _ => None,
        }
    }

    /// If another operator is after these, it's probably a unary operator
    /// (not sure about `fn` name).
    #[must_use]
    pub const fn is_next_unary(&self) -> bool {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        match self {
            SemiColon        | // ; - is unary
            Colon            | // #{ foo: - is unary
            Comma            | // ( ... , -expr ) - is unary
            //Period         |
            //Elvis          |
            DoubleQuestion   | // ?? - is unary
            ExclusiveRange   | // .. - is unary
            InclusiveRange   | // ..= - is unary
            LeftBrace        | // { -expr } - is unary
            // RightBrace    | // { expr } - expr not unary & is closing
            LeftParen        | // ( -expr ) - is unary
            // RightParen    | // ( expr ) - expr not unary & is closing
            LeftBracket      | // [ -expr ] - is unary
            // RightBracket  | // [ expr ] - expr not unary & is closing
            Plus             |
            PlusAssign       |
            UnaryPlus        |
            Minus            |
            MinusAssign      |
            UnaryMinus       |
            Multiply         |
            MultiplyAssign   |
            Divide           |
            DivideAssign     |
            Modulo           |
            ModuloAssign     |
            PowerOf          |
            PowerOfAssign    |
            LeftShift        |
            LeftShiftAssign  |
            RightShift       |
            RightShiftAssign |
            Equals           |
            EqualsTo         |
            NotEqualsTo      |
            LessThan         |
            GreaterThan      |
            Bang             |
            LessThanEqualsTo |
            GreaterThanEqualsTo |
            Pipe             |
            Ampersand        |
            If               |
            //Do             |
            While            |
            Until            |
            In               |
            NotIn            |
            And              |
            AndAssign        |
            Or               |
            OrAssign         |
            XOr              |
            XOrAssign        |
            Return           |
            Throw               => true,

            #[cfg(not(feature = "no_index"))]
            QuestionBracket     => true,    // ?[ - is unary

            LexError(..)        => true,

            _                   => false,
        }
    }

    /// Get the precedence number of the token.
    #[must_use]
    pub const fn precedence(&self) -> Option<Precedence> {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        Precedence::new(match self {
            Or | XOr | Pipe => 30,

            And | Ampersand => 60,

            EqualsTo | NotEqualsTo => 90,

            In | NotIn => 110,

            LessThan | LessThanEqualsTo | GreaterThan | GreaterThanEqualsTo => 130,

            DoubleQuestion => 135,

            ExclusiveRange | InclusiveRange => 140,

            Plus | Minus => 150,

            Divide | Multiply | Modulo => 180,

            PowerOf => 190,

            LeftShift | RightShift => 210,

            _ => 0,
        })
    }

    /// Does an expression bind to the right (instead of left)?
    #[must_use]
    pub const fn is_bind_right(&self) -> bool {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        match self {
            // Exponentiation binds to the right
            PowerOf => true,

            _ => false,
        }
    }

    /// Is this token a standard symbol used in the language?
    #[must_use]
    pub const fn is_standard_symbol(&self) -> bool {
        #[allow(clippy::enum_glob_use)]
        use Token::*;

        match self {
            LeftBrace | RightBrace | LeftParen | RightParen | LeftBracket | RightBracket | Plus
            | UnaryPlus | Minus | UnaryMinus | Multiply | Divide | Modulo | PowerOf | LeftShift
            | RightShift | SemiColon | Colon | DoubleColon | Comma | Period | DoubleQuestion
            | ExclusiveRange | InclusiveRange | MapStart | Equals | LessThan | GreaterThan
            | LessThanEqualsTo | GreaterThanEqualsTo | EqualsTo | NotEqualsTo | Bang | Pipe
            | Or | XOr | Ampersand | And | PlusAssign | MinusAssign | MultiplyAssign
            | DivideAssign | LeftShiftAssign | RightShiftAssign | AndAssign | OrAssign
            | XOrAssign | ModuloAssign | PowerOfAssign => true,

            #[cfg(not(feature = "no_object"))]
            Elvis => true,

            #[cfg(not(feature = "no_index"))]
            QuestionBracket => true,

            _ => false,
        }
    }

    /// Is this token a standard keyword?
    #[inline]
    #[must_use]
    pub const fn is_standard_keyword(&self) -> bool {
        #[allow(clippy::enum_glob_use)]
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

    /// Is this token a reserved keyword or symbol?
    #[inline(always)]
    #[must_use]
    pub const fn is_reserved(&self) -> bool {
        matches!(self, Self::Reserved(..))
    }

    /// Is this token a custom keyword?
    #[cfg(not(feature = "no_custom_syntax"))]
    #[inline(always)]
    #[must_use]
    pub const fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(..))
    }
}

impl From<Token> for String {
    #[inline(always)]
    fn from(token: Token) -> Self {
        token.to_string()
    }
}

/// _(internals)_ State of the tokenizer.
/// Exported under the `internals` feature only.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct TokenizeState {
    /// Maximum length of a string.
    ///
    /// Not available under `unchecked`.
    #[cfg(not(feature = "unchecked"))]
    pub max_string_len: Option<std::num::NonZeroUsize>,
    /// Can the next token be a unary operator?
    pub next_token_cannot_be_unary: bool,
    /// Shared object to allow controlling the tokenizer externally.
    pub tokenizer_control: TokenizerControl,
    /// Is the tokenizer currently inside a block comment?
    pub comment_level: usize,
    /// Include comments?
    pub include_comments: bool,
    /// Is the current tokenizer position within the text stream of an interpolated string?
    pub is_within_text_terminated_by: Option<char>,
    /// Textual syntax of the current token, if any.
    ///
    /// Set to `Some` to begin tracking this information.
    pub last_token: Option<SmartString>,
}

/// _(internals)_ Trait that encapsulates a peekable character input stream.
/// Exported under the `internals` feature only.
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

    /// Consume the next character.
    #[inline(always)]
    fn eat_next_and_advance(&mut self, pos: &mut Position) -> Option<char> {
        pos.advance();
        self.get_next()
    }
}

/// _(internals)_ Parse a string literal ended by a specified termination character.
/// Exported under the `internals` feature only.
///
/// Returns the parsed string and a boolean indicating whether the string is
/// terminated by an interpolation `${`.
///
/// # Returns
///
/// | Type                            | Return Value               |`state.is_within_text_terminated_by`|
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
pub fn parse_string_literal(
    stream: &mut (impl InputStream + ?Sized),
    state: &mut TokenizeState,
    pos: &mut Position,
    termination_char: char,
    verbatim: bool,
    allow_line_continuation: bool,
    allow_interpolation: bool,
) -> Result<(SmartString, bool, Position), (LexError, Position)> {
    let mut result = SmartString::new_const();
    let mut escape = SmartString::new_const();

    let start = *pos;
    let mut first_char = Position::NONE;
    let mut interpolated = false;
    #[cfg(not(feature = "no_position"))]
    let mut skip_space_until = 0;

    state.is_within_text_terminated_by = Some(termination_char);
    if let Some(ref mut last) = state.last_token {
        last.clear();
        last.push(termination_char);
    }

    loop {
        debug_assert!(
            !verbatim || escape.is_empty(),
            "verbatim strings should not have any escapes"
        );

        let next_char = match stream.get_next() {
            Some(ch) => {
                pos.advance();
                ch
            }
            None if verbatim => {
                debug_assert_eq!(escape, "", "verbatim strings should not have any escapes");
                pos.advance();
                break;
            }
            None if allow_line_continuation && !escape.is_empty() => {
                debug_assert_eq!(escape, "\\", "unexpected escape {escape} at end of line");
                pos.advance();
                break;
            }
            None => {
                pos.advance();
                state.is_within_text_terminated_by = None;
                return Err((LERR::UnterminatedString, start));
            }
        };

        if let Some(ref mut last) = state.last_token {
            last.push(next_char);
        }

        // String interpolation?
        if allow_interpolation
            && next_char == '$'
            && escape.is_empty()
            && stream.peek_next().map_or(false, |ch| ch == '{')
        {
            interpolated = true;
            state.is_within_text_terminated_by = None;
            break;
        }

        // Check string length
        #[cfg(not(feature = "unchecked"))]
        if let Some(max) = state.max_string_len {
            if result.len() > max.get() {
                return Err((LexError::StringTooLong(max.get()), start));
            }
        }

        // Close wrapper
        if termination_char == next_char && escape.is_empty() {
            // Double wrapper
            if stream.peek_next().map_or(false, |c| c == termination_char) {
                stream.eat_next_and_advance(pos);
                if let Some(ref mut last) = state.last_token {
                    last.push(termination_char);
                }
            } else {
                state.is_within_text_terminated_by = None;
                break;
            }
        }

        if first_char.is_none() {
            first_char = *pos;
        }

        match next_char {
            // \r - ignore if followed by \n
            '\r' if stream.peek_next().map_or(false, |ch| ch == '\n') => (),
            // \r
            'r' if !escape.is_empty() => {
                escape.clear();
                result.push('\r');
            }
            // \n
            'n' if !escape.is_empty() => {
                escape.clear();
                result.push('\n');
            }
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
            // \x??, \u????, \U????????
            ch @ ('x' | 'u' | 'U') if !escape.is_empty() => {
                let mut seq = escape.clone();
                escape.clear();
                seq.push(ch);

                let mut out_val: u32 = 0;
                let len = match ch {
                    'x' => 2,
                    'u' => 4,
                    'U' => 8,
                    c => unreachable!("x or u or U expected but gets '{}'", c),
                };

                for _ in 0..len {
                    let c = stream
                        .get_next()
                        .ok_or_else(|| (LERR::MalformedEscapeSequence(seq.to_string()), *pos))?;

                    pos.advance();
                    seq.push(c);
                    if let Some(ref mut last) = state.last_token {
                        last.push(c);
                    }

                    out_val *= 16;
                    out_val += c
                        .to_digit(16)
                        .ok_or_else(|| (LERR::MalformedEscapeSequence(seq.to_string()), *pos))?;
                }

                result.push(
                    char::from_u32(out_val)
                        .ok_or_else(|| (LERR::MalformedEscapeSequence(seq.to_string()), *pos))?,
                );
            }

            // LF - Verbatim
            '\n' if verbatim => {
                debug_assert_eq!(escape, "", "verbatim strings should not have any escapes");
                pos.new_line();
                result.push('\n');
            }

            // LF - Line continuation
            '\n' if allow_line_continuation && !escape.is_empty() => {
                debug_assert_eq!(escape, "\\", "unexpected escape {escape} at end of line");
                escape.clear();
                pos.new_line();

                #[cfg(not(feature = "no_position"))]
                {
                    let start_position = start.position().unwrap();
                    skip_space_until = start_position + 1;
                }
            }

            // LF - Unterminated string
            '\n' => {
                pos.rewind();
                state.is_within_text_terminated_by = None;
                return Err((LERR::UnterminatedString, start));
            }

            // \{termination_char} - escaped termination character
            ch if termination_char == ch && !escape.is_empty() => {
                escape.clear();
                result.push(termination_char);
            }

            // Unknown escape sequence
            ch if !escape.is_empty() => {
                escape.push(ch);

                return Err((LERR::MalformedEscapeSequence(escape.to_string()), *pos));
            }

            // Whitespace to skip
            #[cfg(not(feature = "no_position"))]
            ch if ch.is_whitespace() && pos.position().unwrap() < skip_space_until => (),

            // All other characters
            ch => {
                escape.clear();
                result.push(ch);

                #[cfg(not(feature = "no_position"))]
                {
                    skip_space_until = 0;
                }
            }
        }
    }

    // Check string length
    #[cfg(not(feature = "unchecked"))]
    if let Some(max) = state.max_string_len {
        if result.len() > max.get() {
            return Err((LexError::StringTooLong(max.get()), start));
        }
    }

    Ok((result, interpolated, first_char))
}

/// Scan for a block comment until the end.
fn scan_block_comment(
    stream: &mut (impl InputStream + ?Sized),
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
                if let Some(c2) = stream.peek_next().filter(|&ch| ch == '*') {
                    stream.eat_next_and_advance(pos);
                    if let Some(comment) = comment.as_mut() {
                        comment.push(c2);
                    }
                    level += 1;
                }
            }
            '*' => {
                if let Some(c2) = stream.peek_next().filter(|&ch| ch == '/') {
                    stream.eat_next_and_advance(pos);
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

/// Test if the given character is a hex character.
#[inline(always)]
const fn is_hex_digit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// Test if the given character is a numeric digit (i.e. 0-9).
#[inline(always)]
const fn is_numeric_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// Test if the given character is an octal digit (i.e. 0-7).
#[inline(always)]
const fn is_octal_digit(c: char) -> bool {
    matches!(c, '0'..='7')
}

/// Test if the given character is a binary digit (i.e. 0 or 1).
#[inline(always)]
const fn is_binary_digit(c: char) -> bool {
    c == '0' || c == '1'
}

/// Test if the comment block is a doc-comment.
#[cfg(not(feature = "no_function"))]
#[cfg(feature = "metadata")]
#[inline]
#[must_use]
pub fn is_doc_comment(comment: &str) -> bool {
    (comment.starts_with("///") && !comment.starts_with("////"))
        || (comment.starts_with("/**") && !comment.starts_with("/***"))
}

/// _(internals)_ Get the next token from the input stream.
/// Exported under the `internals` feature only.
#[inline(always)]
#[must_use]
pub fn get_next_token(
    stream: &mut (impl InputStream + ?Sized),
    state: &mut TokenizeState,
    pos: &mut Position,
) -> (Token, Position) {
    let result = get_next_token_inner(stream, state, pos);

    // Save the last token's state
    state.next_token_cannot_be_unary = !result.0.is_next_unary();

    result
}

/// Get the next token.
#[must_use]
fn get_next_token_inner(
    stream: &mut (impl InputStream + ?Sized),
    state: &mut TokenizeState,
    pos: &mut Position,
) -> (Token, Position) {
    state.last_token.as_mut().map(SmartString::clear);

    // Still inside a comment?
    if state.comment_level > 0 {
        let start_pos = *pos;
        let mut comment = String::new();
        let comment_buf = state.include_comments.then_some(&mut comment);

        state.comment_level = scan_block_comment(stream, state.comment_level, pos, comment_buf);

        let return_comment = state.include_comments;

        #[cfg(not(feature = "no_function"))]
        #[cfg(feature = "metadata")]
        let return_comment = return_comment || is_doc_comment(&comment);

        if return_comment {
            return (Token::Comment(comment.into()), start_pos);
        }

        // Reached EOF without ending comment block?
        if state.comment_level > 0 {
            return (Token::EOF, *pos);
        }
    }

    // Within text?
    if let Some(ch) = state.is_within_text_terminated_by.take() {
        return parse_string_literal(stream, state, pos, ch, true, false, true).map_or_else(
            |(err, err_pos)| (Token::LexError(err.into()), err_pos),
            |(result, interpolated, start_pos)| {
                if interpolated {
                    (Token::InterpolatedString(result.into()), start_pos)
                } else {
                    (Token::StringConstant(result.into()), start_pos)
                }
            },
        );
    }

    let mut negated: Option<Position> = None;

    while let Some(c) = stream.get_next() {
        pos.advance();

        let start_pos = *pos;
        let cc = stream.peek_next().unwrap_or('\0');

        // Identifiers and strings that can have non-ASCII characters
        match (c, cc) {
            // digit ...
            ('0'..='9', ..) => {
                let mut result = SmartString::new_const();
                let mut radix_base: Option<u32> = None;
                let mut valid: fn(char) -> bool = is_numeric_digit;
                let mut _has_period = false;
                let mut _has_e = false;

                result.push(c);

                while let Some(next_char) = stream.peek_next() {
                    match next_char {
                        NUMBER_SEPARATOR => {
                            stream.eat_next_and_advance(pos);
                        }
                        ch if valid(ch) => {
                            result.push(ch);
                            stream.eat_next_and_advance(pos);
                        }
                        #[cfg(any(not(feature = "no_float"), feature = "decimal"))]
                        '.' if !_has_period && radix_base.is_none() => {
                            stream.get_next().unwrap();

                            // Check if followed by digits or something that cannot start a property name
                            match stream.peek_next() {
                                // digits after period - accept the period
                                Some('0'..='9') => {
                                    result.push('.');
                                    pos.advance();
                                    _has_period = true;
                                }
                                // _ - cannot follow a decimal point
                                Some(NUMBER_SEPARATOR) => {
                                    stream.unget('.');
                                    break;
                                }
                                // .. - reserved symbol, not a floating-point number
                                Some('.') => {
                                    stream.unget('.');
                                    break;
                                }
                                // symbol after period - probably a float
                                Some(ch) if !is_id_first_alphabetic(ch) => {
                                    result.push('.');
                                    pos.advance();
                                    result.push('0');
                                    _has_period = true;
                                }
                                // Not a floating-point number
                                _ => {
                                    stream.unget('.');
                                    break;
                                }
                            }
                        }
                        #[cfg(not(feature = "no_float"))]
                        'e' if !_has_e && radix_base.is_none() => {
                            stream.get_next().unwrap();

                            // Check if followed by digits or +/-
                            match stream.peek_next() {
                                // digits after e - accept the e (no decimal points allowed)
                                Some('0'..='9') => {
                                    result.push('e');
                                    pos.advance();
                                    _has_e = true;
                                    _has_period = true;
                                }
                                // +/- after e - accept the e and the sign (no decimal points allowed)
                                Some('+' | '-') => {
                                    result.push('e');
                                    pos.advance();
                                    result.push(stream.get_next().unwrap());
                                    pos.advance();
                                    _has_e = true;
                                    _has_period = true;
                                }
                                // Not a floating-point number
                                _ => {
                                    stream.unget('e');
                                    break;
                                }
                            }
                        }
                        // 0x????, 0o????, 0b???? at beginning
                        ch @ ('x' | 'o' | 'b' | 'X' | 'O' | 'B')
                            if c == '0' && result.len() <= 1 =>
                        {
                            result.push(ch);
                            stream.eat_next_and_advance(pos);

                            valid = match ch {
                                'x' | 'X' => is_hex_digit,
                                'o' | 'O' => is_octal_digit,
                                'b' | 'B' => is_binary_digit,
                                c => unreachable!("x/X or o/O or b/B expected but gets '{}'", c),
                            };

                            radix_base = Some(match ch {
                                'x' | 'X' => 16,
                                'o' | 'O' => 8,
                                'b' | 'B' => 2,
                                c => unreachable!("x/X or o/O or b/B expected but gets '{}'", c),
                            });
                        }

                        _ => break,
                    }
                }

                let num_pos = negated.map_or(start_pos, |negated_pos| {
                    result.insert(0, '-');
                    negated_pos
                });

                if let Some(ref mut last) = state.last_token {
                    *last = result.clone();
                }

                // Parse number
                let token = if let Some(radix) = radix_base {
                    let result = &result[2..];

                    UNSIGNED_INT::from_str_radix(result, radix)
                        .map(|v| v as INT)
                        .map_or_else(
                            |_| Token::LexError(LERR::MalformedNumber(result.to_string()).into()),
                            Token::IntegerConstant,
                        )
                } else {
                    (|| {
                        let num = INT::from_str(&result).map(Token::IntegerConstant);

                        // If integer parsing is unnecessary, try float instead
                        #[cfg(not(feature = "no_float"))]
                        if num.is_err() {
                            if let Ok(v) = crate::types::FloatWrapper::from_str(&result) {
                                return Token::FloatConstant((v, result).into());
                            }
                        }

                        // Then try decimal
                        #[cfg(feature = "decimal")]
                        if num.is_err() {
                            if let Ok(v) = rust_decimal::Decimal::from_str(&result) {
                                return Token::DecimalConstant((v, result).into());
                            }
                        }

                        // Then try decimal in scientific notation
                        #[cfg(feature = "decimal")]
                        if num.is_err() {
                            if let Ok(v) = rust_decimal::Decimal::from_scientific(&result) {
                                return Token::DecimalConstant((v, result).into());
                            }
                        }

                        num.unwrap_or_else(|_| {
                            Token::LexError(LERR::MalformedNumber(result.to_string()).into())
                        })
                    })()
                };

                return (token, num_pos);
            }

            // " - string literal
            ('"', ..) => {
                return parse_string_literal(stream, state, pos, c, false, true, false)
                    .map_or_else(
                        |(err, err_pos)| (Token::LexError(err.into()), err_pos),
                        |(result, ..)| (Token::StringConstant(result.into()), start_pos),
                    );
            }
            // ` - string literal
            ('`', ..) => {
                // Start from the next line if at the end of line
                match stream.peek_next() {
                    // `\r - start from next line
                    Some('\r') => {
                        stream.eat_next_and_advance(pos);
                        // `\r\n
                        if stream.peek_next() == Some('\n') {
                            stream.eat_next_and_advance(pos);
                        }
                        pos.new_line();
                    }
                    // `\n - start from next line
                    Some('\n') => {
                        stream.eat_next_and_advance(pos);
                        pos.new_line();
                    }
                    _ => (),
                }

                return parse_string_literal(stream, state, pos, c, true, false, true).map_or_else(
                    |(err, err_pos)| (Token::LexError(err.into()), err_pos),
                    |(result, interpolated, ..)| {
                        if interpolated {
                            (Token::InterpolatedString(result.into()), start_pos)
                        } else {
                            (Token::StringConstant(result.into()), start_pos)
                        }
                    },
                );
            }

            // ' - character literal
            ('\'', '\'') => {
                return (
                    Token::LexError(LERR::MalformedChar(String::new()).into()),
                    start_pos,
                )
            }
            ('\'', ..) => {
                return parse_string_literal(stream, state, pos, c, false, false, false)
                    .map_or_else(
                        |(err, err_pos)| (Token::LexError(err.into()), err_pos),
                        |(result, ..)| {
                            let mut chars = result.chars();
                            let first = chars.next().unwrap();

                            if chars.next().is_some() {
                                (
                                    Token::LexError(LERR::MalformedChar(result.to_string()).into()),
                                    start_pos,
                                )
                            } else {
                                (Token::CharConstant(first), start_pos)
                            }
                        },
                    )
            }

            // Braces
            ('{', ..) => return (Token::LeftBrace, start_pos),
            ('}', ..) => return (Token::RightBrace, start_pos),

            // Unit
            ('(', ')') => {
                stream.eat_next_and_advance(pos);
                return (Token::Unit, start_pos);
            }

            // Parentheses
            ('(', '*') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("(*".into())), start_pos);
            }
            ('(', ..) => return (Token::LeftParen, start_pos),
            (')', ..) => return (Token::RightParen, start_pos),

            // Indexing
            ('[', ..) => return (Token::LeftBracket, start_pos),
            (']', ..) => return (Token::RightBracket, start_pos),

            // Map literal
            #[cfg(not(feature = "no_object"))]
            ('#', '{') => {
                stream.eat_next_and_advance(pos);
                return (Token::MapStart, start_pos);
            }
            // Shebang
            ('#', '!') => return (Token::Reserved(Box::new("#!".into())), start_pos),

            ('#', ' ') => {
                stream.eat_next_and_advance(pos);
                let token = if stream.peek_next() == Some('{') {
                    stream.eat_next_and_advance(pos);
                    "# {"
                } else {
                    "#"
                };
                return (Token::Reserved(Box::new(token.into())), start_pos);
            }

            ('#', ..) => return (Token::Reserved(Box::new("#".into())), start_pos),

            // Operators
            ('+', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::PlusAssign, start_pos);
            }
            ('+', '+') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("++".into())), start_pos);
            }
            ('+', ..) if !state.next_token_cannot_be_unary => return (Token::UnaryPlus, start_pos),
            ('+', ..) => return (Token::Plus, start_pos),

            ('-', '0'..='9') if !state.next_token_cannot_be_unary => negated = Some(start_pos),
            ('-', '0'..='9') => return (Token::Minus, start_pos),
            ('-', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::MinusAssign, start_pos);
            }
            ('-', '>') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("->".into())), start_pos);
            }
            ('-', '-') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("--".into())), start_pos);
            }
            ('-', ..) if !state.next_token_cannot_be_unary => {
                return (Token::UnaryMinus, start_pos)
            }
            ('-', ..) => return (Token::Minus, start_pos),

            ('*', ')') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("*)".into())), start_pos);
            }
            ('*', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::MultiplyAssign, start_pos);
            }
            ('*', '*') => {
                stream.eat_next_and_advance(pos);

                return (
                    if stream.peek_next() == Some('=') {
                        stream.eat_next_and_advance(pos);
                        Token::PowerOfAssign
                    } else {
                        Token::PowerOf
                    },
                    start_pos,
                );
            }
            ('*', ..) => return (Token::Multiply, start_pos),

            // Comments
            ('/', '/') => {
                stream.eat_next_and_advance(pos);

                let mut comment: Option<String> = match stream.peek_next() {
                    #[cfg(not(feature = "no_function"))]
                    #[cfg(feature = "metadata")]
                    Some('/') => {
                        stream.eat_next_and_advance(pos);

                        // Long streams of `///...` are not doc-comments
                        match stream.peek_next() {
                            Some('/') => None,
                            _ => Some("///".into()),
                        }
                    }
                    #[cfg(feature = "metadata")]
                    Some('!') => {
                        stream.eat_next_and_advance(pos);
                        Some("//!".into())
                    }
                    _ if state.include_comments => Some("//".into()),
                    _ => None,
                };

                while let Some(c) = stream.get_next() {
                    if c == '\r' {
                        // \r\n
                        if stream.peek_next() == Some('\n') {
                            stream.eat_next_and_advance(pos);
                        }
                        pos.new_line();
                        break;
                    }
                    if c == '\n' {
                        pos.new_line();
                        break;
                    }
                    if let Some(comment) = comment.as_mut() {
                        comment.push(c);
                    }
                    pos.advance();
                }

                match comment {
                    #[cfg(feature = "metadata")]
                    Some(comment) if comment.starts_with("//!") => {
                        let g = &mut state.tokenizer_control.borrow_mut().global_comments;
                        if !g.is_empty() {
                            g.push('\n');
                        }
                        g.push_str(&comment);
                    }
                    Some(comment) => return (Token::Comment(comment.into()), start_pos),
                    None => (),
                }
            }
            ('/', '*') => {
                state.comment_level += 1;
                stream.eat_next_and_advance(pos);

                let mut comment: Option<String> = match stream.peek_next() {
                    #[cfg(not(feature = "no_function"))]
                    #[cfg(feature = "metadata")]
                    Some('*') => {
                        stream.eat_next_and_advance(pos);

                        // Long streams of `/****...` are not doc-comments
                        match stream.peek_next() {
                            Some('*') => None,
                            _ => Some("/**".into()),
                        }
                    }
                    _ if state.include_comments => Some("/*".into()),
                    _ => None,
                };

                state.comment_level =
                    scan_block_comment(stream, state.comment_level, pos, comment.as_mut());

                if let Some(comment) = comment {
                    return (Token::Comment(comment.into()), start_pos);
                }
            }

            ('/', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::DivideAssign, start_pos);
            }
            ('/', ..) => return (Token::Divide, start_pos),

            (';', ..) => return (Token::SemiColon, start_pos),
            (',', ..) => return (Token::Comma, start_pos),

            ('.', '.') => {
                stream.eat_next_and_advance(pos);
                return (
                    match stream.peek_next() {
                        Some('.') => {
                            stream.eat_next_and_advance(pos);
                            Token::Reserved(Box::new("...".into()))
                        }
                        Some('=') => {
                            stream.eat_next_and_advance(pos);
                            Token::InclusiveRange
                        }
                        _ => Token::ExclusiveRange,
                    },
                    start_pos,
                );
            }
            ('.', ..) => return (Token::Period, start_pos),

            ('=', '=') => {
                stream.eat_next_and_advance(pos);

                if stream.peek_next() == Some('=') {
                    stream.eat_next_and_advance(pos);
                    return (Token::Reserved(Box::new("===".into())), start_pos);
                }

                return (Token::EqualsTo, start_pos);
            }
            ('=', '>') => {
                stream.eat_next_and_advance(pos);
                return (Token::DoubleArrow, start_pos);
            }
            ('=', ..) => return (Token::Equals, start_pos),

            #[cfg(not(feature = "no_module"))]
            (':', ':') => {
                stream.eat_next_and_advance(pos);

                if stream.peek_next() == Some('<') {
                    stream.eat_next_and_advance(pos);
                    return (Token::Reserved(Box::new("::<".into())), start_pos);
                }

                return (Token::DoubleColon, start_pos);
            }
            (':', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new(":=".into())), start_pos);
            }
            (':', ';') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new(":;".into())), start_pos);
            }
            (':', ..) => return (Token::Colon, start_pos),

            ('<', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::LessThanEqualsTo, start_pos);
            }
            ('<', '-') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("<-".into())), start_pos);
            }
            ('<', '<') => {
                stream.eat_next_and_advance(pos);

                return (
                    if stream.peek_next() == Some('=') {
                        stream.eat_next_and_advance(pos);
                        Token::LeftShiftAssign
                    } else {
                        Token::LeftShift
                    },
                    start_pos,
                );
            }
            ('<', '|') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("<|".into())), start_pos);
            }
            ('<', ..) => return (Token::LessThan, start_pos),

            ('>', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::GreaterThanEqualsTo, start_pos);
            }
            ('>', '>') => {
                stream.eat_next_and_advance(pos);

                return (
                    if stream.peek_next() == Some('=') {
                        stream.eat_next_and_advance(pos);
                        Token::RightShiftAssign
                    } else {
                        Token::RightShift
                    },
                    start_pos,
                );
            }
            ('>', ..) => return (Token::GreaterThan, start_pos),

            ('!', 'i') => {
                stream.get_next().unwrap();
                if stream.peek_next() == Some('n') {
                    stream.get_next().unwrap();
                    match stream.peek_next() {
                        Some(c) if is_id_continue(c) => {
                            stream.unget('n');
                            stream.unget('i');
                            return (Token::Bang, start_pos);
                        }
                        _ => {
                            pos.advance();
                            pos.advance();
                            return (Token::NotIn, start_pos);
                        }
                    }
                }

                stream.unget('i');
                return (Token::Bang, start_pos);
            }
            ('!', '=') => {
                stream.eat_next_and_advance(pos);

                if stream.peek_next() == Some('=') {
                    stream.eat_next_and_advance(pos);
                    return (Token::Reserved(Box::new("!==".into())), start_pos);
                }

                return (Token::NotEqualsTo, start_pos);
            }
            ('!', '.') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("!.".into())), start_pos);
            }
            ('!', ..) => return (Token::Bang, start_pos),

            ('|', '|') => {
                stream.eat_next_and_advance(pos);
                return (Token::Or, start_pos);
            }
            ('|', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::OrAssign, start_pos);
            }
            ('|', '>') => {
                stream.eat_next_and_advance(pos);
                return (Token::Reserved(Box::new("|>".into())), start_pos);
            }
            ('|', ..) => return (Token::Pipe, start_pos),

            ('&', '&') => {
                stream.eat_next_and_advance(pos);
                return (Token::And, start_pos);
            }
            ('&', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::AndAssign, start_pos);
            }
            ('&', ..) => return (Token::Ampersand, start_pos),

            ('^', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::XOrAssign, start_pos);
            }
            ('^', ..) => return (Token::XOr, start_pos),

            ('~', ..) => return (Token::Reserved(Box::new("~".into())), start_pos),

            ('%', '=') => {
                stream.eat_next_and_advance(pos);
                return (Token::ModuloAssign, start_pos);
            }
            ('%', ..) => return (Token::Modulo, start_pos),

            ('@', ..) => return (Token::Reserved(Box::new("@".into())), start_pos),

            ('$', ..) => return (Token::Reserved(Box::new("$".into())), start_pos),

            ('?', '.') => {
                stream.eat_next_and_advance(pos);
                return (
                    #[cfg(not(feature = "no_object"))]
                    Token::Elvis,
                    #[cfg(feature = "no_object")]
                    Token::Reserved(Box::new("?.".into())),
                    start_pos,
                );
            }
            ('?', '?') => {
                stream.eat_next_and_advance(pos);
                return (Token::DoubleQuestion, start_pos);
            }
            ('?', '[') => {
                stream.eat_next_and_advance(pos);
                return (
                    #[cfg(not(feature = "no_index"))]
                    Token::QuestionBracket,
                    #[cfg(feature = "no_index")]
                    Token::Reserved(Box::new("?[".into())),
                    start_pos,
                );
            }
            ('?', ..) => return (Token::Reserved(Box::new("?".into())), start_pos),

            // letter or underscore ...
            _ if is_id_first_alphabetic(c) || c == '_' => {
                return parse_identifier_token(stream, state, pos, start_pos, c);
            }

            // \n
            ('\n', ..) => pos.new_line(),

            // Whitespace - follows Rust's SPACE, TAB, CR, LF, FF which is the same as WhatWG.
            (ch, ..) if ch.is_ascii_whitespace() => (),

            _ => {
                return (
                    Token::LexError(LERR::UnexpectedInput(c.to_string()).into()),
                    start_pos,
                )
            }
        }
    }

    pos.advance();

    (Token::EOF, *pos)
}

/// Get the next token, parsing it as an identifier.
fn parse_identifier_token(
    stream: &mut (impl InputStream + ?Sized),
    state: &mut TokenizeState,
    pos: &mut Position,
    start_pos: Position,
    first_char: char,
) -> (Token, Position) {
    let mut identifier = SmartString::new_const();
    identifier.push(first_char);
    if let Some(ref mut last) = state.last_token {
        last.clear();
        last.push(first_char);
    }

    while let Some(next_char) = stream.peek_next() {
        match next_char {
            x if is_id_continue(x) => {
                stream.eat_next_and_advance(pos);
                identifier.push(x);
                if let Some(ref mut last) = state.last_token {
                    last.push(x);
                }
            }
            _ => break,
        }
    }

    if let Some(token) = Token::lookup_symbol_from_syntax(&identifier) {
        return (token, start_pos);
    }

    if is_reserved_keyword_or_symbol(&identifier).0 {
        return (Token::Reserved(Box::new(identifier)), start_pos);
    }

    if !is_valid_identifier(&identifier) {
        return (
            Token::LexError(LERR::MalformedIdentifier(identifier.to_string()).into()),
            start_pos,
        );
    }

    (Token::Identifier(identifier.into()), start_pos)
}

/// _(internals)_ Is a text string a valid identifier?
/// Exported under the `internals` feature only.
#[must_use]
pub fn is_valid_identifier(name: &str) -> bool {
    let mut first_alphabetic = false;

    for ch in name.chars() {
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

/// _(internals)_ Is a text string a valid script-defined function name?
/// Exported under the `internals` feature only.
#[inline(always)]
#[must_use]
pub fn is_valid_function_name(name: &str) -> bool {
    is_valid_identifier(name)
        && !is_reserved_keyword_or_symbol(name).0
        && Token::lookup_symbol_from_syntax(name).is_none()
}

/// Is a character valid to start an identifier?
#[inline(always)]
#[must_use]
#[allow(clippy::missing_const_for_fn)]
pub fn is_id_first_alphabetic(x: char) -> bool {
    #[cfg(feature = "unicode-xid-ident")]
    return unicode_xid::UnicodeXID::is_xid_start(x);
    #[cfg(not(feature = "unicode-xid-ident"))]
    return x.is_ascii_alphabetic();
}

/// Is a character valid for an identifier?
#[inline(always)]
#[must_use]
#[allow(clippy::missing_const_for_fn)]
pub fn is_id_continue(x: char) -> bool {
    #[cfg(feature = "unicode-xid-ident")]
    return unicode_xid::UnicodeXID::is_xid_continue(x);
    #[cfg(not(feature = "unicode-xid-ident"))]
    return x.is_ascii_alphanumeric() || x == '_';
}

/// Is a piece of syntax a reserved keyword or reserved symbol?
///
/// # Return values
///
/// The first `bool` indicates whether it is a reserved keyword or symbol.
///
/// The second `bool` indicates whether the keyword can be called normally as a function.
/// `false` if it is not a reserved keyword.
///
/// The third `bool` indicates whether the keyword can be called in method-call style.
/// `false` if it is not a reserved keyword or it cannot be called as a function.
#[inline]
#[must_use]
pub fn is_reserved_keyword_or_symbol(syntax: &str) -> (bool, bool, bool) {
    // This implementation is based upon a pre-calculated table generated
    // by GNU `gperf` on the list of keywords.
    let utf8 = syntax.as_bytes();
    let len = utf8.len();

    if !(MIN_RESERVED_LEN..=MAX_RESERVED_LEN).contains(&len) {
        return (false, false, false);
    }

    let mut hash_val = len;

    match len {
        1 => (),
        _ => hash_val += RESERVED_ASSOC_VALUES[utf8[1] as usize] as usize,
    }
    hash_val += RESERVED_ASSOC_VALUES[utf8[0] as usize] as usize;
    hash_val += RESERVED_ASSOC_VALUES[utf8[len - 1] as usize] as usize;

    if !(MIN_RESERVED_HASH_VALUE..=MAX_RESERVED_HASH_VALUE).contains(&hash_val) {
        return (false, false, false);
    }

    match RESERVED_LIST[hash_val] {
        ("", ..) => (false, false, false),
        (s, true, a, b) => {
            // Fail early to avoid calling memcmp().
            // Since we are already working with bytes, mind as well check the first one.
            let is_reserved = s.len() == len && s.as_bytes()[0] == utf8[0] && s == syntax;
            (is_reserved, is_reserved && a, is_reserved && a && b)
        }
        _ => (false, false, false),
    }
}

/// _(internals)_ A type that implements the [`InputStream`] trait.
/// Exported under the `internals` feature only.
///
/// Multiple character streams are jointed together to form one single stream.
pub struct MultiInputsStream<'a> {
    /// Buffered characters, if any.
    pub buf: [Option<char>; 2],
    /// The current stream index.
    pub index: usize,
    /// Input character streams.
    pub streams: StaticVec<Peekable<Chars<'a>>>,
}

impl InputStream for MultiInputsStream<'_> {
    #[inline]
    fn unget(&mut self, ch: char) {
        match self.buf {
            [None, ..] => self.buf[0] = Some(ch),
            [_, None] => self.buf[1] = Some(ch),
            _ => unreachable!("cannot unget more than 2 characters!"),
        }
    }
    fn get_next(&mut self) -> Option<char> {
        match self.buf {
            [None, ..] => (),
            [ch @ Some(_), None] => {
                self.buf[0] = None;
                return ch;
            }
            [_, ch @ Some(_)] => {
                self.buf[1] = None;
                return ch;
            }
        }

        loop {
            if self.index >= self.streams.len() {
                // No more streams
                return None;
            }
            if let Some(ch) = self.streams[self.index].next() {
                // Next character in main stream
                return Some(ch);
            }
            // Jump to the next stream
            self.index += 1;
        }
    }
    fn peek_next(&mut self) -> Option<char> {
        match self.buf {
            [None, ..] => (),
            [ch @ Some(_), None] => return ch,
            [_, ch @ Some(_)] => return ch,
        }

        loop {
            if self.index >= self.streams.len() {
                // No more streams
                return None;
            }
            if let Some(&ch) = self.streams[self.index].peek() {
                // Next character in main stream
                return Some(ch);
            }
            // Jump to the next stream
            self.index += 1;
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
    /// Input character stream.
    pub stream: MultiInputsStream<'a>,
    /// A processor function that maps a token to another.
    pub token_mapper: Option<&'a OnParseTokenCallback>,
}

impl<'a> Iterator for TokenIterator<'a> {
    type Item = (Token, Position);

    fn next(&mut self) -> Option<Self::Item> {
        let (within_interpolated, compress_script) = {
            let control = &mut *self.state.tokenizer_control.borrow_mut();

            if control.is_within_text {
                // Switch to text mode terminated by back-tick
                self.state.is_within_text_terminated_by = Some('`');
                // Reset it
                control.is_within_text = false;
            }

            (
                self.state.is_within_text_terminated_by.is_some(),
                control.compressed.is_some(),
            )
        };

        let (token, pos) = match get_next_token(&mut self.stream, &mut self.state, &mut self.pos) {
            // {EOF}
            r @ (Token::EOF, _) => return Some(r),
            // {EOF} after unterminated string.
            // The only case where `TokenizeState.is_within_text_terminated_by` is set is when
            // a verbatim string or a string with continuation encounters {EOF}.
            // This is necessary to handle such cases for line-by-line parsing, but for an entire
            // script it is a syntax error.
            (Token::StringConstant(..), pos) if self.state.is_within_text_terminated_by.is_some() => {
                self.state.is_within_text_terminated_by = None;
                return Some((Token::LexError(LERR::UnterminatedString.into()), pos));
            }
            // Reserved keyword/symbol
            (Token::Reserved(s), pos) => (match
                (s.as_str(),
                    #[cfg(not(feature = "no_custom_syntax"))]
                    self.engine.custom_keywords.contains_key(&*s),
                    #[cfg(feature = "no_custom_syntax")]
                    false
                )
            {
                ("===", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "'===' is not a valid operator. This is not JavaScript! Should it be '=='?".to_string(),
                ).into()),
                ("!==", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "'!==' is not a valid operator. This is not JavaScript! Should it be '!='?".to_string(),
                ).into()),
                ("->", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "'->' is not a valid symbol. This is not C or C++!".to_string()).into()),
                ("<-", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "'<-' is not a valid symbol. This is not Go! Should it be '<='?".to_string(),
                ).into()),
                (":=", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "':=' is not a valid assignment operator. This is not Go or Pascal! Should it be simply '='?".to_string(),
                ).into()),
                (":;", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "':;' is not a valid symbol. Should it be '::'?".to_string(),
                ).into()),
                ("::<", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "'::<>' is not a valid symbol. This is not Rust! Should it be '::'?".to_string(),
                ).into()),
                ("(*" | "*)", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "'(* .. *)' is not a valid comment format. This is not Pascal! Should it be '/* .. */'?".to_string(),
                ).into()),
                ("# {", false) => Token::LexError(LERR::ImproperSymbol(s.to_string(),
                    "'#' is not a valid symbol. Should it be '#{'?".to_string(),
                ).into()),
                // Reserved keyword/operator that is custom.
                #[cfg(not(feature = "no_custom_syntax"))]
                (.., true) => Token::Custom(s),
                #[cfg(feature = "no_custom_syntax")]
                (.., true) => unreachable!("no custom operators"),
                // Reserved keyword that is not custom and disabled.
                (token, false) if self.engine.is_symbol_disabled(token) => {
                    let msg = format!("reserved {} '{token}' is disabled", if is_valid_identifier(token) { "keyword"} else {"symbol"});
                    Token::LexError(LERR::ImproperSymbol(s.to_string(), msg).into())
                },
                // Reserved keyword/operator that is not custom.
                (.., false) => Token::Reserved(s),
            }, pos),
            // Custom keyword
            #[cfg(not(feature = "no_custom_syntax"))]
            (Token::Identifier(s), pos) if self.engine.custom_keywords.contains_key(&*s) => {
                (Token::Custom(s), pos)
            }
            // Custom keyword/symbol - must be disabled
            #[cfg(not(feature = "no_custom_syntax"))]
            (token, pos) if token.is_literal() && self.engine.custom_keywords.contains_key(token.literal_syntax()) => {
                // Active standard keyword should never be a custom keyword!
                debug_assert!(self.engine.is_symbol_disabled(token.literal_syntax()), "{:?} is an active keyword", token);

                (Token::Custom(Box::new(token.literal_syntax().into())), pos)
            }
            // Disabled symbol
            (token, pos) if token.is_literal() && self.engine.is_symbol_disabled(token.literal_syntax()) => {
                (Token::Reserved(Box::new(token.literal_syntax().into())), pos)
            }
            // Normal symbol
            r => r,
        };

        // Run the mapper, if any
        let token = match self.token_mapper {
            Some(func) => func(token, pos, &self.state),
            None => token,
        };

        // Collect the compressed script, if needed
        if compress_script {
            let control = &mut *self.state.tokenizer_control.borrow_mut();

            if token != Token::EOF {
                if let Some(ref mut compressed) = control.compressed {
                    use std::fmt::Write;

                    let last_token = self.state.last_token.as_ref().unwrap();
                    let mut buf = SmartString::new_const();

                    if last_token.is_empty() {
                        write!(buf, "{token}").unwrap();
                    } else if within_interpolated
                        && matches!(
                            token,
                            Token::StringConstant(..) | Token::InterpolatedString(..)
                        )
                    {
                        compressed.push_str(&last_token[1..]);
                    } else {
                        buf = last_token.clone();
                    }

                    if !buf.is_empty() && !compressed.is_empty() {
                        let cur = buf.chars().next().unwrap();

                        if cur == '_' || is_id_first_alphabetic(cur) || is_id_continue(cur) {
                            let prev = compressed.chars().last().unwrap();

                            if prev == '_' || is_id_first_alphabetic(prev) || is_id_continue(prev) {
                                compressed.push(' ');
                            }
                        }
                    }

                    compressed.push_str(&buf);
                }
            }
        }

        Some((token, pos))
    }
}

impl FusedIterator for TokenIterator<'_> {}

impl Engine {
    /// _(internals)_ Tokenize an input text stream.
    /// Exported under the `internals` feature only.
    ///
    /// # Panics
    ///
    /// Panics if there are no input streams.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn lex<'a>(
        &'a self,
        inputs: impl IntoIterator<Item = &'a (impl AsRef<str> + 'a)>,
    ) -> (TokenIterator<'a>, TokenizerControl) {
        lex_raw(self, inputs, None)
    }
    /// _(internals)_ Tokenize an input text stream with a mapping function.
    /// Exported under the `internals` feature only.
    ///
    /// # Panics
    ///
    /// Panics if there are no input streams.
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn lex_with_map<'a>(
        &'a self,
        inputs: impl IntoIterator<Item = &'a (impl AsRef<str> + 'a)>,
        token_mapper: &'a OnParseTokenCallback,
    ) -> (TokenIterator<'a>, TokenizerControl) {
        lex_raw(self, inputs, Some(token_mapper))
    }
}

/// Tokenize an input text stream with an optional mapping function.
///
/// # Panics
///
/// Panics if there are no input streams.
#[inline]
#[must_use]
pub fn lex_raw<'a>(
    engine: &'a Engine,
    inputs: impl IntoIterator<Item = &'a (impl AsRef<str> + 'a)>,
    token_mapper: Option<&'a OnParseTokenCallback>,
) -> (TokenIterator<'a>, TokenizerControl) {
    let buffer: TokenizerControl = RefCell::new(TokenizerControlBlock::new()).into();
    let buffer2 = buffer.clone();

    (
        TokenIterator {
            engine,
            state: TokenizeState {
                #[cfg(not(feature = "unchecked"))]
                max_string_len: std::num::NonZeroUsize::new(engine.max_string_size()),
                next_token_cannot_be_unary: false,
                tokenizer_control: buffer,
                comment_level: 0,
                include_comments: false,
                is_within_text_terminated_by: None,
                last_token: None,
            },
            pos: Position::new(1, 0),
            stream: MultiInputsStream {
                buf: [None, None],
                streams: inputs
                    .into_iter()
                    .map(|s| s.as_ref().chars().peekable())
                    .collect(),
                index: 0,
            },
            token_mapper,
        },
        buffer2,
    )
}
