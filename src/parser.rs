//! Main module defining the lexer and parser.

use crate::any::{Any, AnyExt, Dynamic};
use crate::engine::{Engine, FunctionsLib};
use crate::error::{LexError, ParseError, ParseErrorType};
use crate::scope::{EntryType as ScopeEntryType, Scope};

#[cfg(not(feature = "no_optimize"))]
use crate::optimize::optimize_into_ast;

use crate::stdlib::{
    borrow::Cow,
    boxed::Box,
    char,
    collections::HashMap,
    fmt, format,
    iter::Peekable,
    ops::Add,
    rc::Rc,
    str::Chars,
    str::FromStr,
    string::{String, ToString},
    sync::Arc,
    usize, vec,
    vec::Vec,
};

/// The system integer type.
///
/// If the `only_i32` feature is enabled, this will be `i32` instead.
#[cfg(not(feature = "only_i32"))]
pub type INT = i64;

/// The system integer type.
///
/// If the `only_i32` feature is not enabled, this will be `i64` instead.
#[cfg(feature = "only_i32")]
pub type INT = i32;

/// The system floating-point type.
///
/// Not available under the `no_float` feature.
#[cfg(not(feature = "no_float"))]
pub type FLOAT = f64;

type LERR = LexError;
type PERR = ParseErrorType;

/// A location (line number + character position) in the input script.
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct Position {
    /// Line number - 0 = none, MAX = EOF
    line: usize,
    /// Character position - 0 = BOL, MAX = EOF
    pos: usize,
}

impl Position {
    /// Create a new `Position`.
    pub fn new(line: usize, position: usize) -> Self {
        assert!(line != 0, "line cannot be zero");
        assert!(
            line != usize::MAX || position != usize::MAX,
            "invalid position"
        );

        Self {
            line,
            pos: position,
        }
    }

    /// Get the line number (1-based), or `None` if no position or EOF.
    pub fn line(&self) -> Option<usize> {
        if self.is_none() || self.is_eof() {
            None
        } else {
            Some(self.line)
        }
    }

    /// Get the character position (1-based), or `None` if at beginning of a line.
    pub fn position(&self) -> Option<usize> {
        if self.is_none() || self.is_eof() || self.pos == 0 {
            None
        } else {
            Some(self.pos)
        }
    }

    /// Advance by one character position.
    pub(crate) fn advance(&mut self) {
        self.pos += 1;
    }

    /// Go backwards by one character position.
    ///
    /// # Panics
    ///
    /// Panics if already at beginning of a line - cannot rewind to a previous line.
    ///
    pub(crate) fn rewind(&mut self) {
        assert!(self.pos > 0, "cannot rewind at position 0");
        self.pos -= 1;
    }

    /// Advance to the next line.
    pub(crate) fn new_line(&mut self) {
        self.line += 1;
        self.pos = 0;
    }

    /// Create a `Position` representing no position.
    pub(crate) fn none() -> Self {
        Self { line: 0, pos: 0 }
    }

    /// Create a `Position` at EOF.
    pub(crate) fn eof() -> Self {
        Self {
            line: usize::MAX,
            pos: usize::MAX,
        }
    }

    /// Is there no `Position`?
    pub fn is_none(&self) -> bool {
        self.line == 0 && self.pos == 0
    }

    /// Is the `Position` at EOF?
    pub fn is_eof(&self) -> bool {
        self.line == usize::MAX && self.pos == usize::MAX
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new(1, 0)
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_eof() {
            write!(f, "EOF")
        } else if self.is_none() {
            write!(f, "none")
        } else {
            write!(f, "line {}, position {}", self.line, self.pos)
        }
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_eof() {
            write!(f, "(EOF)")
        } else {
            write!(f, "({}:{})", self.line, self.pos)
        }
    }
}

/// Compiled AST (abstract syntax tree) of a Rhai script.
///
/// Currently, `AST` is neither `Send` nor `Sync`. Turn on the `sync` feature to make it `Send + Sync`.
#[derive(Debug, Clone)]
pub struct AST(
    pub(crate) Vec<Stmt>,
    #[cfg(feature = "sync")] pub(crate) Arc<FunctionsLib>,
    #[cfg(not(feature = "sync"))] pub(crate) Rc<FunctionsLib>,
);

impl AST {
    /// Create a new `AST`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Merge two `AST` into one.  Both `AST`'s are untouched and a new, merged, version
    /// is returned.
    ///
    /// The second `AST` is simply appended to the end of the first _without any processing_.
    /// Thus, the return value of the first `AST` (if using expression-statement syntax) is buried.
    /// Of course, if the first `AST` uses a `return` statement at the end, then
    /// the second `AST` will essentially be dead code.
    ///
    /// All script-defined functions in the second `AST` overwrite similarly-named functions
    /// in the first `AST` with the same number of parameters.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), rhai::EvalAltResult> {
    /// # #[cfg(not(feature = "no_function"))]
    /// # {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// let ast1 = engine.compile(r#"fn foo(x) { 42 + x } foo(1)"#)?;
    /// let ast2 = engine.compile(r#"fn foo(n) { "hello" + n } foo("!")"#)?;
    ///
    /// let ast = ast1.merge(&ast2);    // Merge 'ast2' into 'ast1'
    ///
    /// // Notice that using the '+' operator also works:
    /// // let ast = &ast1 + &ast2;
    ///
    /// // 'ast' is essentially:
    /// //
    /// //    fn foo(n) { "hello" + n } // <- definition of first 'foo' is overwritten
    /// //    foo(1)                    // <- notice this will be "hello1" instead of 43,
    /// //                              //    but it is no longer the return value
    /// //    foo("!")                  // returns "hello!"
    ///
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast::<String>(&ast)?, "hello!");
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    pub fn merge(&self, other: &Self) -> Self {
        let Self(statements, functions) = self;

        let ast = match (statements.is_empty(), other.0.is_empty()) {
            (false, false) => {
                let mut statements = statements.clone();
                statements.extend(other.0.iter().cloned());
                statements
            }
            (false, true) => statements.clone(),
            (true, false) => other.0.clone(),
            (true, true) => vec![],
        };

        #[cfg(feature = "sync")]
        {
            Self(ast, Arc::new(functions.merge(other.1.as_ref())))
        }
        #[cfg(not(feature = "sync"))]
        {
            Self(ast, Rc::new(functions.merge(other.1.as_ref())))
        }
    }

    /// Clear all function definitions in the `AST`.
    pub fn clear_functions(&mut self) {
        #[cfg(feature = "sync")]
        {
            self.1 = Arc::new(FunctionsLib::new());
        }
        #[cfg(not(feature = "sync"))]
        {
            self.1 = Rc::new(FunctionsLib::new());
        }
    }

    /// Clear all statements in the `AST`, leaving only function definitions.
    pub fn retain_functions(&mut self) {
        self.0 = vec![];
    }
}

impl Default for AST {
    fn default() -> Self {
        #[cfg(feature = "sync")]
        {
            Self(vec![], Arc::new(FunctionsLib::new()))
        }
        #[cfg(not(feature = "sync"))]
        {
            Self(vec![], Rc::new(FunctionsLib::new()))
        }
    }
}
impl Add<Self> for &AST {
    type Output = AST;

    fn add(self, rhs: Self) -> Self::Output {
        self.merge(rhs)
    }
}

/// A script-function definition.
#[derive(Debug, Clone)]
pub struct FnDef {
    /// Function name.
    pub name: String,
    /// Names of function parameters.
    pub params: Vec<String>,
    /// Function body.
    pub body: Stmt,
    /// Position of the function definition.
    pub pos: Position,
}

/// `return`/`throw` statement.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ReturnType {
    /// `return` statement.
    Return,
    /// `throw` statement.
    Exception,
}

/// A statement.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// No-op.
    Noop(Position),
    /// if expr { stmt } else { stmt }
    IfThenElse(Box<Expr>, Box<Stmt>, Option<Box<Stmt>>),
    /// while expr { stmt }
    While(Box<Expr>, Box<Stmt>),
    /// loop { stmt }
    Loop(Box<Stmt>),
    /// for id in expr { stmt }
    For(String, Box<Expr>, Box<Stmt>),
    /// let id = expr
    Let(String, Option<Box<Expr>>, Position),
    /// const id = expr
    Const(String, Box<Expr>, Position),
    /// { stmt; ... }
    Block(Vec<Stmt>, Position),
    /// { stmt }
    Expr(Box<Expr>),
    /// continue
    Continue(Position),
    /// break
    Break(Position),
    /// `return`/`throw`
    ReturnWithVal(Option<Box<Expr>>, ReturnType, Position),
}

impl Stmt {
    /// Get the `Position` of this statement.
    pub fn position(&self) -> Position {
        match self {
            Stmt::Noop(pos)
            | Stmt::Let(_, _, pos)
            | Stmt::Const(_, _, pos)
            | Stmt::Block(_, pos)
            | Stmt::Continue(pos)
            | Stmt::Break(pos)
            | Stmt::ReturnWithVal(_, _, pos) => *pos,
            Stmt::IfThenElse(expr, _, _) | Stmt::Expr(expr) => expr.position(),
            Stmt::While(_, stmt) | Stmt::Loop(stmt) | Stmt::For(_, _, stmt) => stmt.position(),
        }
    }

    /// Is this statement self-terminated (i.e. no need for a semicolon terminator)?
    pub fn is_self_terminated(&self) -> bool {
        match self {
            Stmt::IfThenElse(_, _, _)
            | Stmt::While(_, _)
            | Stmt::Loop(_)
            | Stmt::For(_, _, _)
            | Stmt::Block(_, _) => true,

            // A No-op requires a semicolon in order to know it is an empty statement!
            Stmt::Noop(_) => false,

            Stmt::Let(_, _, _)
            | Stmt::Const(_, _, _)
            | Stmt::Expr(_)
            | Stmt::Continue(_)
            | Stmt::Break(_)
            | Stmt::ReturnWithVal(_, _, _) => false,
        }
    }

    /// Is this statement _pure_?
    pub fn is_pure(&self) -> bool {
        match self {
            Stmt::Noop(_) => true,
            Stmt::Expr(expr) => expr.is_pure(),
            Stmt::IfThenElse(guard, if_block, Some(else_block)) => {
                guard.is_pure() && if_block.is_pure() && else_block.is_pure()
            }
            Stmt::IfThenElse(guard, block, None) | Stmt::While(guard, block) => {
                guard.is_pure() && block.is_pure()
            }
            Stmt::Loop(block) => block.is_pure(),
            Stmt::For(_, range, block) => range.is_pure() && block.is_pure(),
            Stmt::Let(_, _, _) | Stmt::Const(_, _, _) => false,
            Stmt::Block(statements, _) => statements.iter().all(Stmt::is_pure),
            Stmt::Continue(_) | Stmt::Break(_) | Stmt::ReturnWithVal(_, _, _) => false,
        }
    }
}

/// An expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer constant.
    IntegerConstant(INT, Position),
    /// Floating-point constant.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(FLOAT, Position),
    /// Character constant.
    CharConstant(char, Position),
    /// String constant.
    StringConstant(String, Position),
    /// Variable access.
    Variable(String, Position),
    /// Property access.
    Property(String, Position),
    /// { stmt }
    Stmt(Box<Stmt>, Position),
    /// func(expr, ... )
    FunctionCall(String, Vec<Expr>, Option<Dynamic>, Position),
    /// expr = expr
    Assignment(Box<Expr>, Box<Expr>, Position),
    /// lhs.rhs
    #[cfg(not(feature = "no_object"))]
    Dot(Box<Expr>, Box<Expr>, Position),
    /// expr[expr]
    #[cfg(not(feature = "no_index"))]
    Index(Box<Expr>, Box<Expr>, Position),
    #[cfg(not(feature = "no_index"))]
    /// [ expr, ... ]
    Array(Vec<Expr>, Position),
    #[cfg(not(feature = "no_object"))]
    /// #{ name:expr, ... }
    Map(Vec<(String, Expr, Position)>, Position),
    /// lhs && rhs
    And(Box<Expr>, Box<Expr>),
    /// lhs || rhs
    Or(Box<Expr>, Box<Expr>),
    /// true
    True(Position),
    /// false
    False(Position),
    /// ()
    Unit(Position),
}

impl Expr {
    /// Get the `Dynamic` value of a constant expression.
    ///
    /// # Panics
    ///
    /// Panics when the expression is not constant.
    pub fn get_constant_value(&self) -> Dynamic {
        match self {
            Expr::IntegerConstant(i, _) => i.into_dynamic(),
            Expr::CharConstant(c, _) => c.into_dynamic(),
            Expr::StringConstant(s, _) => s.into_dynamic(),
            Expr::True(_) => true.into_dynamic(),
            Expr::False(_) => false.into_dynamic(),
            Expr::Unit(_) => ().into_dynamic(),

            #[cfg(not(feature = "no_index"))]
            Expr::Array(items, _) if items.iter().all(Expr::is_constant) => items
                .iter()
                .map(Expr::get_constant_value)
                .collect::<Vec<_>>()
                .into_dynamic(),

            #[cfg(not(feature = "no_object"))]
            Expr::Map(items, _) if items.iter().all(|(_, v, _)| v.is_constant()) => items
                .iter()
                .map(|(k, v, _)| (k.clone(), v.get_constant_value()))
                .collect::<HashMap<_, _>>()
                .into_dynamic(),

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(f, _) => f.into_dynamic(),

            _ => panic!("cannot get value of non-constant expression"),
        }
    }

    /// Get the display value of a constant expression.
    ///
    /// # Panics
    ///
    /// Panics when the expression is not constant.
    pub fn get_constant_str(&self) -> String {
        match self {
            Expr::IntegerConstant(i, _) => i.to_string(),
            Expr::CharConstant(c, _) => c.to_string(),
            Expr::StringConstant(_, _) => "string".to_string(),
            Expr::True(_) => "true".to_string(),
            Expr::False(_) => "false".to_string(),
            Expr::Unit(_) => "()".to_string(),

            #[cfg(not(feature = "no_index"))]
            Expr::Array(items, _) if items.iter().all(Expr::is_constant) => "array".to_string(),

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(f, _) => f.to_string(),

            _ => panic!("cannot get value of non-constant expression"),
        }
    }

    /// Get the `Position` of the expression.
    pub fn position(&self) -> Position {
        match self {
            Expr::IntegerConstant(_, pos)
            | Expr::CharConstant(_, pos)
            | Expr::StringConstant(_, pos)
            | Expr::Variable(_, pos)
            | Expr::Property(_, pos)
            | Expr::Stmt(_, pos)
            | Expr::FunctionCall(_, _, _, pos)
            | Expr::True(pos)
            | Expr::False(pos)
            | Expr::Unit(pos) => *pos,

            Expr::Assignment(expr, _, _) | Expr::And(expr, _) | Expr::Or(expr, _) => {
                expr.position()
            }

            #[cfg(not(feature = "no_object"))]
            Expr::Dot(expr, _, _) => expr.position(),

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_, pos) => *pos,

            #[cfg(not(feature = "no_index"))]
            Expr::Array(_, pos) => *pos,

            #[cfg(not(feature = "no_object"))]
            Expr::Map(_, pos) => *pos,

            #[cfg(not(feature = "no_index"))]
            Expr::Index(expr, _, _) => expr.position(),
        }
    }

    /// Is the expression pure?
    ///
    /// A pure expression has no side effects.
    pub fn is_pure(&self) -> bool {
        match self {
            #[cfg(not(feature = "no_index"))]
            Expr::Array(expressions, _) => expressions.iter().all(Expr::is_pure),

            #[cfg(not(feature = "no_index"))]
            Expr::Index(x, y, _) => x.is_pure() && y.is_pure(),

            Expr::And(x, y) | Expr::Or(x, y) => x.is_pure() && y.is_pure(),

            Expr::Stmt(stmt, _) => stmt.is_pure(),

            expr => expr.is_constant() || matches!(expr, Expr::Variable(_, _)),
        }
    }

    /// Is the expression a constant?
    pub fn is_constant(&self) -> bool {
        match self {
            Expr::IntegerConstant(_, _)
            | Expr::CharConstant(_, _)
            | Expr::StringConstant(_, _)
            | Expr::True(_)
            | Expr::False(_)
            | Expr::Unit(_) => true,

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_, _) => true,

            // An array literal is constant if all items are constant
            #[cfg(not(feature = "no_index"))]
            Expr::Array(expressions, _) => expressions.iter().all(Expr::is_constant),

            _ => false,
        }
    }
}

/// Tokens.
#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    IntegerConstant(INT),
    #[cfg(not(feature = "no_float"))]
    FloatConstant(FLOAT),
    Identifier(String),
    CharConstant(char),
    StringConst(String),
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    #[cfg(not(feature = "no_index"))]
    LeftBracket,
    #[cfg(not(feature = "no_index"))]
    RightBracket,
    Plus,
    UnaryPlus,
    Minus,
    UnaryMinus,
    Multiply,
    Divide,
    Modulo,
    PowerOf,
    LeftShift,
    RightShift,
    SemiColon,
    Colon,
    Comma,
    Period,
    #[cfg(not(feature = "no_object"))]
    MapStart,
    Equals,
    True,
    False,
    Let,
    Const,
    If,
    Else,
    While,
    Loop,
    For,
    In,
    LessThan,
    GreaterThan,
    LessThanEqualsTo,
    GreaterThanEqualsTo,
    EqualsTo,
    NotEqualsTo,
    Bang,
    Pipe,
    Or,
    XOr,
    Ampersand,
    And,
    #[cfg(not(feature = "no_function"))]
    Fn,
    Continue,
    Break,
    Return,
    Throw,
    PlusAssign,
    MinusAssign,
    MultiplyAssign,
    DivideAssign,
    LeftShiftAssign,
    RightShiftAssign,
    AndAssign,
    OrAssign,
    XOrAssign,
    ModuloAssign,
    PowerOfAssign,
    LexError(Box<LexError>),
}

impl Token {
    /// Get the syntax of the token.
    pub fn syntax(&self) -> Cow<str> {
        use self::Token::*;

        match self {
            IntegerConstant(i) => i.to_string().into(),
            #[cfg(not(feature = "no_float"))]
            FloatConstant(f) => f.to_string().into(),
            Identifier(s) => s.into(),
            CharConstant(c) => c.to_string().into(),
            LexError(err) => err.to_string().into(),

            token => (match token {
                StringConst(_) => "string",
                LeftBrace => "{",
                RightBrace => "}",
                LeftParen => "(",
                RightParen => ")",
                #[cfg(not(feature = "no_index"))]
                LeftBracket => "[",
                #[cfg(not(feature = "no_index"))]
                RightBracket => "]",
                Plus => "+",
                UnaryPlus => "+",
                Minus => "-",
                UnaryMinus => "-",
                Multiply => "*",
                Divide => "/",
                SemiColon => ";",
                Colon => ":",
                Comma => ",",
                Period => ".",
                #[cfg(not(feature = "no_object"))]
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
                #[cfg(not(feature = "no_function"))]
                Fn => "fn",
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
                For => "for",
                In => "in",
                _ => panic!("operator should be match in outer scope"),
            })
            .into(),
        }
    }

    // If another operator is after these, it's probably an unary operator
    // (not sure about fn name).
    pub fn is_next_unary(&self) -> bool {
        use self::Token::*;

        match self {
            LexError(_)      |
            LeftBrace        | // (+expr) - is unary
            // RightBrace    | {expr} - expr not unary & is closing
            LeftParen        | // {-expr} - is unary
            // RightParen    | (expr) - expr not unary & is closing
            Plus             |
            UnaryPlus        |
            Minus            |
            UnaryMinus       |
            Multiply         |
            Divide           |
            Colon            |
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
            PowerOfAssign => true,

            #[cfg(not(feature = "no_index"))]
            LeftBracket => true, // [-expr] - is unary
            // RightBracket  | [expr] - expr not unary & is closing

            _ => false,
        }
    }

    /// Get the precedence number of the token.
    pub fn precedence(&self) -> u8 {
        match self {
            Self::Equals
            | Self::PlusAssign
            | Self::MinusAssign
            | Self::MultiplyAssign
            | Self::DivideAssign
            | Self::LeftShiftAssign
            | Self::RightShiftAssign
            | Self::AndAssign
            | Self::OrAssign
            | Self::XOrAssign
            | Self::ModuloAssign
            | Self::PowerOfAssign => 10,

            Self::Or | Self::XOr | Self::Pipe => 50,

            Self::And | Self::Ampersand => 60,

            Self::LessThan
            | Self::LessThanEqualsTo
            | Self::GreaterThan
            | Self::GreaterThanEqualsTo
            | Self::EqualsTo
            | Self::NotEqualsTo => 70,

            Self::Plus | Self::Minus => 80,

            Self::Divide | Self::Multiply | Self::PowerOf => 90,

            Self::LeftShift | Self::RightShift => 100,

            Self::Modulo => 110,

            Self::Period => 120,

            _ => 0,
        }
    }

    /// Does an expression bind to the right (instead of left)?
    pub fn is_bind_right(&self) -> bool {
        match self {
            // Assignments bind to the right
            Self::Equals
            | Self::PlusAssign
            | Self::MinusAssign
            | Self::MultiplyAssign
            | Self::DivideAssign
            | Self::LeftShiftAssign
            | Self::RightShiftAssign
            | Self::AndAssign
            | Self::OrAssign
            | Self::XOrAssign
            | Self::ModuloAssign
            | Self::PowerOfAssign => true,

            // Property access binds to the right
            Self::Period => true,

            _ => false,
        }
    }
}

/// An iterator on a `Token` stream.
pub struct TokenIterator<'a> {
    /// Can the next token be a unary operator?
    can_be_unary: bool,
    /// Current position.
    pos: Position,
    /// The input characters stream.
    stream: Peekable<Chars<'a>>,
}

impl<'a> TokenIterator<'a> {
    /// Consume the next character.
    fn eat_next(&mut self) {
        self.stream.next();
        self.advance();
    }
    /// Move the current position one character ahead.
    fn advance(&mut self) {
        self.pos.advance();
    }
    /// Move the current position back one character.
    ///
    /// # Panics
    ///
    /// Panics if already at the beginning of a line - cannot rewind to the previous line.
    fn rewind(&mut self) {
        self.pos.rewind();
    }
    /// Move the current position to the next line.
    fn new_line(&mut self) {
        self.pos.new_line()
    }

    /// Parse a string literal wrapped by `enclosing_char`.
    pub fn parse_string_literal(
        &mut self,
        enclosing_char: char,
    ) -> Result<String, (LexError, Position)> {
        let mut result = Vec::new();
        let mut escape = String::with_capacity(12);

        loop {
            let next_char = self.stream.next();
            self.advance();

            match next_char.ok_or((LERR::UnterminatedString, Position::eof()))? {
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
                // \"
                '"' if !escape.is_empty() => {
                    escape.clear();
                    result.push('"');
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
                        _ => panic!("should be 'x', 'u' or 'U'"),
                    };

                    for _ in 0..len {
                        let c = self.stream.next().ok_or_else(|| {
                            (LERR::MalformedEscapeSequence(seq.to_string()), self.pos)
                        })?;

                        seq.push(c);
                        self.advance();

                        out_val *= 16;
                        out_val += c.to_digit(16).ok_or_else(|| {
                            (LERR::MalformedEscapeSequence(seq.to_string()), self.pos)
                        })?;
                    }

                    result.push(
                        char::from_u32(out_val)
                            .ok_or_else(|| (LERR::MalformedEscapeSequence(seq), self.pos))?,
                    );
                }

                // \{enclosing_char} - escaped
                ch if enclosing_char == ch && !escape.is_empty() => result.push(ch),

                // Close wrapper
                ch if enclosing_char == ch && escape.is_empty() => break,

                // Unknown escape sequence
                _ if !escape.is_empty() => {
                    return Err((LERR::MalformedEscapeSequence(escape), self.pos))
                }

                // Cannot have new-lines inside string literals
                '\n' => {
                    self.rewind();
                    return Err((LERR::UnterminatedString, self.pos));
                }

                // All other characters
                ch => {
                    escape.clear();
                    result.push(ch);
                }
            }
        }

        Ok(result.iter().collect())
    }

    /// Get the next token.
    fn inner_next(&mut self) -> Option<(Token, Position)> {
        let mut negated = false;

        while let Some(c) = self.stream.next() {
            self.advance();

            let pos = self.pos;

            match (c, self.stream.peek().copied().unwrap_or('\0')) {
                // \n
                ('\n', _) => self.new_line(),

                // digit ...
                ('0'..='9', _) => {
                    let mut result = Vec::new();
                    let mut radix_base: Option<u32> = None;
                    result.push(c);

                    while let Some(&next_char) = self.stream.peek() {
                        match next_char {
                            '0'..='9' | '_' => {
                                result.push(next_char);
                                self.eat_next();
                            }
                            #[cfg(not(feature = "no_float"))]
                            '.' => {
                                result.push(next_char);
                                self.eat_next();
                                while let Some(&next_char_in_float) = self.stream.peek() {
                                    match next_char_in_float {
                                        '0'..='9' | '_' => {
                                            result.push(next_char_in_float);
                                            self.eat_next();
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
                                self.eat_next();

                                let valid = match ch {
                                    'x' | 'X' => [
                                        'a', 'b', 'c', 'd', 'e', 'f', 'A', 'B', 'C', 'D', 'E', 'F',
                                        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '_',
                                    ],
                                    'o' | 'O' => [
                                        '0', '1', '2', '3', '4', '5', '6', '7', '_', '_', '_', '_',
                                        '_', '_', '_', '_', '_', '_', '_', '_', '_', '_', '_',
                                    ],
                                    'b' | 'B' => [
                                        '0', '1', '_', '_', '_', '_', '_', '_', '_', '_', '_', '_',
                                        '_', '_', '_', '_', '_', '_', '_', '_', '_', '_', '_',
                                    ],
                                    _ => panic!("unexpected character {}", ch),
                                };

                                radix_base = Some(match ch {
                                    'x' | 'X' => 16,
                                    'o' | 'O' => 8,
                                    'b' | 'B' => 2,
                                    _ => panic!("unexpected character {}", ch),
                                });

                                while let Some(&next_char_in_hex) = self.stream.peek() {
                                    if !valid.contains(&next_char_in_hex) {
                                        break;
                                    }

                                    result.push(next_char_in_hex);
                                    self.eat_next();
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
                                        result.iter().collect(),
                                    )))
                                }),
                            pos,
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
                                    result.iter().collect(),
                                )))
                            }),
                            pos,
                        ));
                    }
                }

                // letter or underscore ...
                ('A'..='Z', _) | ('a'..='z', _) | ('_', _) => {
                    let mut result = Vec::new();
                    result.push(c);

                    while let Some(&next_char) = self.stream.peek() {
                        match next_char {
                            x if x.is_ascii_alphanumeric() || x == '_' => {
                                result.push(x);
                                self.eat_next();
                            }
                            _ => break,
                        }
                    }

                    let is_valid_identifier = result
                        .iter()
                        .find(|&ch| char::is_ascii_alphanumeric(ch)) // first alpha-numeric character
                        .map(char::is_ascii_alphabetic) // is a letter
                        .unwrap_or(false); // if no alpha-numeric at all - syntax error

                    let identifier: String = result.iter().collect();

                    if !is_valid_identifier {
                        return Some((
                            Token::LexError(Box::new(LERR::MalformedIdentifier(identifier))),
                            pos,
                        ));
                    }

                    return Some((
                        match identifier.as_str() {
                            "true" => Token::True,
                            "false" => Token::False,
                            "let" => Token::Let,
                            "const" => Token::Const,
                            "if" => Token::If,
                            "else" => Token::Else,
                            "while" => Token::While,
                            "loop" => Token::Loop,
                            "continue" => Token::Continue,
                            "break" => Token::Break,
                            "return" => Token::Return,
                            "throw" => Token::Throw,
                            "for" => Token::For,
                            "in" => Token::In,

                            #[cfg(not(feature = "no_function"))]
                            "fn" => Token::Fn,

                            _ => Token::Identifier(identifier),
                        },
                        pos,
                    ));
                }

                // " - string literal
                ('"', _) => {
                    return self.parse_string_literal('"').map_or_else(
                        |err| Some((Token::LexError(Box::new(err.0)), err.1)),
                        |out| Some((Token::StringConst(out), pos)),
                    );
                }

                // ' - character literal
                ('\'', '\'') => {
                    return Some((
                        Token::LexError(Box::new(LERR::MalformedChar("".to_string()))),
                        pos,
                    ));
                }
                ('\'', _) => {
                    return Some(self.parse_string_literal('\'').map_or_else(
                        |err| (Token::LexError(Box::new(err.0)), err.1),
                        |result| {
                            let mut chars = result.chars();
                            let first = chars.next();

                            if chars.next().is_some() {
                                (Token::LexError(Box::new(LERR::MalformedChar(result))), pos)
                            } else {
                                (Token::CharConstant(first.expect("should be Some")), pos)
                            }
                        },
                    ));
                }

                // Braces
                ('{', _) => return Some((Token::LeftBrace, pos)),
                ('}', _) => return Some((Token::RightBrace, pos)),

                // Parentheses
                ('(', _) => return Some((Token::LeftParen, pos)),
                (')', _) => return Some((Token::RightParen, pos)),

                // Indexing
                #[cfg(not(feature = "no_index"))]
                ('[', _) => return Some((Token::LeftBracket, pos)),
                #[cfg(not(feature = "no_index"))]
                (']', _) => return Some((Token::RightBracket, pos)),

                // Map literal
                #[cfg(not(feature = "no_object"))]
                ('#', '{') => {
                    self.eat_next();
                    return Some((Token::MapStart, pos));
                }

                // Operators
                ('+', '=') => {
                    self.eat_next();
                    return Some((Token::PlusAssign, pos));
                }
                ('+', _) if self.can_be_unary => return Some((Token::UnaryPlus, pos)),
                ('+', _) => return Some((Token::Plus, pos)),

                ('-', '0'..='9') if self.can_be_unary => negated = true,
                ('-', '0'..='9') => return Some((Token::Minus, pos)),
                ('-', '=') => {
                    self.eat_next();
                    return Some((Token::MinusAssign, pos));
                }
                ('-', _) if self.can_be_unary => return Some((Token::UnaryMinus, pos)),
                ('-', _) => return Some((Token::Minus, pos)),

                ('*', '=') => {
                    self.eat_next();
                    return Some((Token::MultiplyAssign, pos));
                }
                ('*', _) => return Some((Token::Multiply, pos)),

                // Comments
                ('/', '/') => {
                    self.eat_next();

                    while let Some(c) = self.stream.next() {
                        if c == '\n' {
                            self.new_line();
                            break;
                        }

                        self.advance();
                    }
                }
                ('/', '*') => {
                    let mut level = 1;

                    self.eat_next();

                    while let Some(c) = self.stream.next() {
                        self.advance();

                        match c {
                            '/' => {
                                if self.stream.next() == Some('*') {
                                    level += 1;
                                }
                                self.advance();
                            }
                            '*' => {
                                if self.stream.next() == Some('/') {
                                    level -= 1;
                                }
                                self.advance();
                            }
                            '\n' => self.new_line(),
                            _ => (),
                        }

                        if level == 0 {
                            break;
                        }
                    }
                }

                ('/', '=') => {
                    self.eat_next();
                    return Some((Token::DivideAssign, pos));
                }
                ('/', _) => return Some((Token::Divide, pos)),

                (';', _) => return Some((Token::SemiColon, pos)),
                (':', _) => return Some((Token::Colon, pos)),
                (',', _) => return Some((Token::Comma, pos)),
                ('.', _) => return Some((Token::Period, pos)),

                ('=', '=') => {
                    self.eat_next();
                    return Some((Token::EqualsTo, pos));
                }
                ('=', _) => return Some((Token::Equals, pos)),

                ('<', '=') => {
                    self.eat_next();
                    return Some((Token::LessThanEqualsTo, pos));
                }
                ('<', '<') => {
                    self.eat_next();

                    return Some((
                        if self.stream.peek() == Some(&'=') {
                            self.eat_next();
                            Token::LeftShiftAssign
                        } else {
                            Token::LeftShift
                        },
                        pos,
                    ));
                }
                ('<', _) => return Some((Token::LessThan, pos)),

                ('>', '=') => {
                    self.eat_next();
                    return Some((Token::GreaterThanEqualsTo, pos));
                }
                ('>', '>') => {
                    self.eat_next();

                    return Some((
                        if self.stream.peek() == Some(&'=') {
                            self.eat_next();
                            Token::RightShiftAssign
                        } else {
                            Token::RightShift
                        },
                        pos,
                    ));
                }
                ('>', _) => return Some((Token::GreaterThan, pos)),

                ('!', '=') => {
                    self.eat_next();
                    return Some((Token::NotEqualsTo, pos));
                }
                ('!', _) => return Some((Token::Bang, pos)),

                ('|', '|') => {
                    self.eat_next();
                    return Some((Token::Or, pos));
                }
                ('|', '=') => {
                    self.eat_next();
                    return Some((Token::OrAssign, pos));
                }
                ('|', _) => return Some((Token::Pipe, pos)),

                ('&', '&') => {
                    self.eat_next();
                    return Some((Token::And, pos));
                }
                ('&', '=') => {
                    self.eat_next();
                    return Some((Token::AndAssign, pos));
                }
                ('&', _) => return Some((Token::Ampersand, pos)),

                ('^', '=') => {
                    self.eat_next();
                    return Some((Token::XOrAssign, pos));
                }
                ('^', _) => return Some((Token::XOr, pos)),

                ('%', '=') => {
                    self.eat_next();
                    return Some((Token::ModuloAssign, pos));
                }
                ('%', _) => return Some((Token::Modulo, pos)),

                ('~', '=') => {
                    self.eat_next();
                    return Some((Token::PowerOfAssign, pos));
                }
                ('~', _) => return Some((Token::PowerOf, pos)),

                (ch, _) if ch.is_whitespace() => (),
                (ch, _) => return Some((Token::LexError(Box::new(LERR::UnexpectedChar(ch))), pos)),
            }
        }

        None
    }
}

impl<'a> Iterator for TokenIterator<'a> {
    type Item = (Token, Position);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner_next().map(|x| {
            // Save the last token
            self.can_be_unary = x.0.is_next_unary();
            x
        })
    }
}

/// Tokenize an input text stream.
pub fn lex(input: &str) -> TokenIterator<'_> {
    TokenIterator {
        can_be_unary: true,
        pos: Position::new(1, 0),
        stream: input.chars().peekable(),
    }
}

/// Parse ( expr )
fn parse_paren_expr<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    begin: Position,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    if matches!(input.peek(), Some((Token::RightParen, _))) {
        input.next();
        return Ok(Expr::Unit(begin));
    }

    let expr = parse_expr(input, allow_stmt_expr)?;

    match input.next() {
        // ( xxx )
        Some((Token::RightParen, _)) => Ok(expr),
        // ( xxx ???
        Some((_, pos)) => Err(PERR::MissingToken(
            ")".into(),
            "for a matching ( in this expression".into(),
        )
        .into_err(pos)),
        // ( xxx
        None => Err(
            PERR::MissingToken(")".into(), "for a matching ( in this expression".into())
                .into_err_eof(),
        ),
    }
}

/// Parse a function call.
fn parse_call_expr<'a>(
    id: String,
    input: &mut Peekable<TokenIterator<'a>>,
    begin: Position,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    let mut args_expr_list = Vec::new();

    // id()
    if let (Token::RightParen, _) = input.peek().ok_or_else(|| {
        PERR::MissingToken(
            ")".into(),
            format!("to close the arguments list of this function call '{}'", id),
        )
        .into_err_eof()
    })? {
        input.next();
        return Ok(Expr::FunctionCall(id, args_expr_list, None, begin));
    }

    loop {
        args_expr_list.push(parse_expr(input, allow_stmt_expr)?);

        match input.peek().ok_or_else(|| {
            PERR::MissingToken(
                ")".into(),
                format!("to close the arguments list of this function call '{}'", id),
            )
            .into_err_eof()
        })? {
            (Token::RightParen, _) => {
                input.next();
                return Ok(Expr::FunctionCall(id, args_expr_list, None, begin));
            }
            (Token::Comma, _) => (),
            (_, pos) => {
                return Err(PERR::MissingToken(
                    ",".into(),
                    format!("to separate the arguments to function call '{}'", id),
                )
                .into_err(*pos))
            }
        }

        input.next();
    }
}

/// Parse an indexing expression.
#[cfg(not(feature = "no_index"))]
fn parse_index_expr<'a>(
    lhs: Box<Expr>,
    input: &mut Peekable<TokenIterator<'a>>,
    pos: Position,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    let idx_expr = parse_expr(input, allow_stmt_expr)?;

    // Check type of indexing - must be integer or string
    match &idx_expr {
        // lhs[int]
        Expr::IntegerConstant(i, pos) if *i < 0 => {
            return Err(PERR::MalformedIndexExpr(format!(
                "Array access expects non-negative index: {} < 0",
                i
            ))
            .into_err(*pos))
        }
        Expr::IntegerConstant(_, pos) => match *lhs {
            Expr::Array(_, _) | Expr::StringConstant(_, _) => (),

            #[cfg(not(feature = "no_object"))]
            Expr::Map(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Object map access expects string index, not a number".into(),
                )
                .into_err(*pos))
            }

            Expr::FloatConstant(_, pos)
            | Expr::CharConstant(_, pos)
            | Expr::Assignment(_, _, pos)
            | Expr::Unit(pos)
            | Expr::True(pos)
            | Expr::False(pos) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(pos))
            }

            Expr::And(lhs, _) | Expr::Or(lhs, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            _ => (),
        },

        // lhs[string]
        Expr::StringConstant(_, pos) => match *lhs {
            #[cfg(not(feature = "no_object"))]
            Expr::Map(_, _) => (),

            Expr::Array(_, _) | Expr::StringConstant(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Array or string expects numeric index, not a string".into(),
                )
                .into_err(*pos))
            }
            Expr::FloatConstant(_, pos)
            | Expr::CharConstant(_, pos)
            | Expr::Assignment(_, _, pos)
            | Expr::Unit(pos)
            | Expr::True(pos)
            | Expr::False(pos) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(pos))
            }

            Expr::And(lhs, _) | Expr::Or(lhs, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            _ => (),
        },

        // lhs[float]
        #[cfg(not(feature = "no_float"))]
        Expr::FloatConstant(_, pos) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a float".into(),
            )
            .into_err(*pos))
        }
        // lhs[char]
        Expr::CharConstant(_, pos) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a character".into(),
            )
            .into_err(*pos))
        }
        // lhs[??? = ??? ], lhs[()]
        Expr::Assignment(_, _, pos) | Expr::Unit(pos) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not ()".into(),
            )
            .into_err(*pos))
        }
        // lhs[??? && ???], lhs[??? || ???]
        Expr::And(lhs, _) | Expr::Or(lhs, _) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a boolean".into(),
            )
            .into_err(lhs.position()))
        }
        // lhs[true], lhs[false]
        Expr::True(pos) | Expr::False(pos) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a boolean".into(),
            )
            .into_err(*pos))
        }
        // All other expressions
        _ => (),
    }

    // Check if there is a closing bracket
    match input.peek().ok_or_else(|| {
        PERR::MissingToken(
            "]".into(),
            "for a matching [ in this index expression".into(),
        )
        .into_err_eof()
    })? {
        (Token::RightBracket, _) => {
            input.next();
            Ok(Expr::Index(lhs, Box::new(idx_expr), pos))
        }
        (_, pos) => Err(PERR::MissingToken(
            "]".into(),
            "for a matching [ in this index expression".into(),
        )
        .into_err(*pos)),
    }
}

/// Parse an expression that begins with an identifier.
fn parse_ident_expr<'a>(
    id: String,
    input: &mut Peekable<TokenIterator<'a>>,
    begin: Position,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    match input.peek() {
        // id(...) - function call
        Some((Token::LeftParen, _)) => {
            input.next();
            parse_call_expr(id, input, begin, allow_stmt_expr)
        }
        // id[...] - indexing
        #[cfg(not(feature = "no_index"))]
        Some((Token::LeftBracket, pos)) => {
            let pos = *pos;
            input.next();
            parse_index_expr(
                Box::new(Expr::Variable(id, begin)),
                input,
                pos,
                allow_stmt_expr,
            )
        }
        // id - variable
        Some(_) => Ok(Expr::Variable(id, begin)),
        // EOF
        None => Ok(Expr::Variable(id, begin)),
    }
}

/// Parse an array literal.
#[cfg(not(feature = "no_index"))]
fn parse_array_literal<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    begin: Position,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    let mut arr = Vec::new();

    if !matches!(input.peek(), Some((Token::RightBracket, _))) {
        while input.peek().is_some() {
            arr.push(parse_expr(input, allow_stmt_expr)?);

            match input.peek().ok_or_else(|| {
                PERR::MissingToken("]".into(), "to end this array literal".into()).into_err_eof()
            })? {
                (Token::Comma, _) => {
                    input.next();
                }
                (Token::RightBracket, _) => break,
                (_, pos) => {
                    return Err(PERR::MissingToken(
                        ",".into(),
                        "to separate the items of this array literal".into(),
                    )
                    .into_err(*pos))
                }
            }
        }
    }

    match input.peek().ok_or_else(|| {
        PERR::MissingToken("]".into(), "to end this array literal".into()).into_err_eof()
    })? {
        (Token::RightBracket, _) => {
            input.next();
            Ok(Expr::Array(arr, begin))
        }
        (_, pos) => {
            Err(PERR::MissingToken("]".into(), "to end this array literal".into()).into_err(*pos))
        }
    }
}

/// Parse a map literal.
#[cfg(not(feature = "no_object"))]
fn parse_map_literal<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    begin: Position,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    let mut map = Vec::new();

    if !matches!(input.peek(), Some((Token::RightBrace, _))) {
        while input.peek().is_some() {
            let (name, pos) = match input.next().ok_or_else(|| {
                PERR::MissingToken("}".into(), "to end this object map literal".into())
                    .into_err_eof()
            })? {
                (Token::Identifier(s), pos) => (s, pos),
                (Token::StringConst(s), pos) => (s, pos),
                (_, pos) if map.is_empty() => {
                    return Err(PERR::MissingToken(
                        "}".into(),
                        "to end this object map literal".into(),
                    )
                    .into_err(pos))
                }
                (_, pos) => return Err(PERR::PropertyExpected.into_err(pos)),
            };

            match input.next().ok_or_else(|| {
                PERR::MissingToken(
                    ":".into(),
                    format!(
                        "to follow the property '{}' in this object map literal",
                        name
                    ),
                )
                .into_err_eof()
            })? {
                (Token::Colon, _) => (),
                (_, pos) => {
                    return Err(PERR::MissingToken(
                        ":".into(),
                        format!(
                            "to follow the property '{}' in this object map literal",
                            name
                        ),
                    )
                    .into_err(pos))
                }
            };

            let expr = parse_expr(input, allow_stmt_expr)?;

            map.push((name, expr, pos));

            match input.peek().ok_or_else(|| {
                PERR::MissingToken("}".into(), "to end this object map literal".into())
                    .into_err_eof()
            })? {
                (Token::Comma, _) => {
                    input.next();
                }
                (Token::RightBrace, _) => break,
                (Token::Identifier(_), pos) => {
                    return Err(PERR::MissingToken(
                        ",".into(),
                        "to separate the items of this object map literal".into(),
                    )
                    .into_err(*pos))
                }
                (_, pos) => {
                    return Err(PERR::MissingToken(
                        "}".into(),
                        "to end this object map literal".into(),
                    )
                    .into_err(*pos))
                }
            }
        }
    }

    // Check for duplicating properties
    map.iter()
        .enumerate()
        .try_for_each(|(i, (k1, _, _))| {
            map.iter()
                .skip(i + 1)
                .find(|(k2, _, _)| k2 == k1)
                .map_or_else(|| Ok(()), |(k2, _, pos)| Err((k2, *pos)))
        })
        .map_err(|(key, pos)| PERR::DuplicatedProperty(key.to_string()).into_err(pos))?;

    // Ending brace
    match input.peek().ok_or_else(|| {
        PERR::MissingToken("}".into(), "to end this object map literal".into()).into_err_eof()
    })? {
        (Token::RightBrace, _) => {
            input.next();
            Ok(Expr::Map(map, begin))
        }
        (_, pos) => Err(
            PERR::MissingToken("]".into(), "to end this object map literal".into()).into_err(*pos),
        ),
    }
}

/// Parse a primary expression.
fn parse_primary<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    let token = match input
        .peek()
        .ok_or_else(|| PERR::UnexpectedEOF.into_err_eof())?
    {
        // { - block statement as expression
        (Token::LeftBrace, pos) if allow_stmt_expr => {
            let pos = *pos;
            return parse_block(input, false, allow_stmt_expr)
                .map(|block| Expr::Stmt(Box::new(block), pos));
        }
        _ => input.next().expect("should be a token"),
    };

    let mut can_be_indexed = false;

    let mut root_expr = match token {
        #[cfg(not(feature = "no_float"))]
        (Token::FloatConstant(x), pos) => Ok(Expr::FloatConstant(x, pos)),

        (Token::IntegerConstant(x), pos) => Ok(Expr::IntegerConstant(x, pos)),
        (Token::CharConstant(c), pos) => Ok(Expr::CharConstant(c, pos)),
        (Token::StringConst(s), pos) => {
            can_be_indexed = true;
            Ok(Expr::StringConstant(s, pos))
        }
        (Token::Identifier(s), pos) => {
            can_be_indexed = true;
            parse_ident_expr(s, input, pos, allow_stmt_expr)
        }
        (Token::LeftParen, pos) => {
            can_be_indexed = true;
            parse_paren_expr(input, pos, allow_stmt_expr)
        }
        #[cfg(not(feature = "no_index"))]
        (Token::LeftBracket, pos) => {
            can_be_indexed = true;
            parse_array_literal(input, pos, allow_stmt_expr)
        }
        #[cfg(not(feature = "no_object"))]
        (Token::MapStart, pos) => {
            can_be_indexed = true;
            parse_map_literal(input, pos, allow_stmt_expr)
        }
        (Token::True, pos) => Ok(Expr::True(pos)),
        (Token::False, pos) => Ok(Expr::False(pos)),
        (Token::LexError(err), pos) => Err(PERR::BadInput(err.to_string()).into_err(pos)),
        (token, pos) => {
            Err(PERR::BadInput(format!("Unexpected '{}'", token.syntax())).into_err(pos))
        }
    }?;

    if can_be_indexed {
        // Tail processing all possible indexing
        #[cfg(not(feature = "no_index"))]
        while let Some((Token::LeftBracket, pos)) = input.peek() {
            let pos = *pos;
            input.next();
            root_expr = parse_index_expr(Box::new(root_expr), input, pos, allow_stmt_expr)?;
        }
    }

    Ok(root_expr)
}

/// Parse a potential unary operator.
fn parse_unary<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    match input
        .peek()
        .ok_or_else(|| PERR::UnexpectedEOF.into_err_eof())?
    {
        // If statement is allowed to act as expressions
        (Token::If, pos) => {
            let pos = *pos;
            Ok(Expr::Stmt(
                Box::new(parse_if(input, false, allow_stmt_expr)?),
                pos,
            ))
        }
        // -expr
        (Token::UnaryMinus, pos) => {
            let pos = *pos;

            input.next();

            match parse_unary(input, allow_stmt_expr)? {
                // Negative integer
                Expr::IntegerConstant(i, _) => i
                    .checked_neg()
                    .map(|x| Expr::IntegerConstant(x, pos))
                    .or_else(|| {
                        #[cfg(not(feature = "no_float"))]
                        return Some(Expr::FloatConstant(-(i as FLOAT), pos));

                        #[cfg(feature = "no_float")]
                        return None;
                    })
                    .ok_or_else(|| {
                        PERR::BadInput(LERR::MalformedNumber(format!("-{}", i)).to_string())
                            .into_err(pos)
                    }),

                // Negative float
                #[cfg(not(feature = "no_float"))]
                Expr::FloatConstant(f, pos) => Ok(Expr::FloatConstant(-f, pos)),

                // Call negative function
                expr => Ok(Expr::FunctionCall("-".into(), vec![expr], None, pos)),
            }
        }
        // +expr
        (Token::UnaryPlus, _) => {
            input.next();
            parse_unary(input, allow_stmt_expr)
        }
        // !expr
        (Token::Bang, pos) => {
            let pos = *pos;

            input.next();

            Ok(Expr::FunctionCall(
                "!".into(),
                vec![parse_primary(input, allow_stmt_expr)?],
                Some(Box::new(false)), // NOT operator, when operating on invalid operand, defaults to false
                pos,
            ))
        }
        // All other tokens
        _ => parse_primary(input, allow_stmt_expr),
    }
}

/// Parse an assignment.
fn parse_assignment(lhs: Expr, rhs: Expr, pos: Position) -> Result<Expr, ParseError> {
    // Is the LHS in a valid format for an assignment target?
    fn valid_assignment_chain(expr: &Expr, is_top: bool) -> Option<ParseError> {
        match expr {
            // var
            Expr::Variable(_, _) => {
                assert!(is_top, "property expected but gets variable");
                None
            }
            // property
            Expr::Property(_, _) => {
                assert!(!is_top, "variable expected but gets property");
                None
            }

            // var[...]
            #[cfg(not(feature = "no_index"))]
            Expr::Index(idx_lhs, _, _) if matches!(idx_lhs.as_ref(), &Expr::Variable(_, _)) => {
                assert!(is_top, "property expected but gets variable");
                None
            }
            // property[...]
            #[cfg(not(feature = "no_index"))]
            Expr::Index(idx_lhs, _, _) if matches!(idx_lhs.as_ref(), &Expr::Property(_, _)) => {
                assert!(!is_top, "variable expected but gets property");
                None
            }

            // idx_lhs[...]
            #[cfg(not(feature = "no_index"))]
            Expr::Index(idx_lhs, _, pos) => match idx_lhs.as_ref() {
                Expr::Index(_, _, _) => Some(ParseErrorType::AssignmentToCopy.into_err(*pos)),
                _ => Some(ParseErrorType::AssignmentToInvalidLHS.into_err(*pos)),
            },

            // dot_lhs.dot_rhs
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(dot_lhs, dot_rhs, _) => match dot_lhs.as_ref() {
                // var.dot_rhs
                Expr::Variable(_, _) if is_top => valid_assignment_chain(dot_rhs, false),
                // property.dot_rhs
                Expr::Property(_, _) if !is_top => valid_assignment_chain(dot_rhs, false),
                // var[...]
                #[cfg(not(feature = "no_index"))]
                Expr::Index(idx_lhs, _, _)
                    if matches!(idx_lhs.as_ref(), &Expr::Variable(_, _)) && is_top =>
                {
                    valid_assignment_chain(dot_rhs, false)
                }
                // property[...]
                #[cfg(not(feature = "no_index"))]
                Expr::Index(idx_lhs, _, _)
                    if matches!(idx_lhs.as_ref(), &Expr::Property(_, _)) && !is_top =>
                {
                    valid_assignment_chain(dot_rhs, false)
                }
                // idx_lhs[...]
                #[cfg(not(feature = "no_index"))]
                Expr::Index(idx_lhs, _, _) => {
                    Some(ParseErrorType::AssignmentToCopy.into_err(idx_lhs.position()))
                }

                expr => panic!("unexpected dot expression {:#?}", expr),
            },

            _ => Some(ParseErrorType::AssignmentToInvalidLHS.into_err(expr.position())),
        }
    }

    match valid_assignment_chain(&lhs, true) {
        None => Ok(Expr::Assignment(Box::new(lhs), Box::new(rhs), pos)),
        Some(err) => Err(err),
    }
}

/// Parse an operator-assignment expression.
fn parse_op_assignment(op: &str, lhs: Expr, rhs: Expr, pos: Position) -> Result<Expr, ParseError> {
    let lhs_copy = lhs.clone();

    // lhs op= rhs -> lhs = op(lhs, rhs)
    parse_assignment(
        lhs,
        Expr::FunctionCall(op.into(), vec![lhs_copy, rhs], None, pos),
        pos,
    )
}

/// Parse a binary expression.
fn parse_binary_op<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    parent_precedence: u8,
    lhs: Expr,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    let mut current_lhs = lhs;

    loop {
        let (current_precedence, bind_right) = if let Some((current_op, _)) = input.peek() {
            (current_op.precedence(), current_op.is_bind_right())
        } else {
            (0, false)
        };

        // Bind left to the parent lhs expression if precedence is higher
        // If same precedence, then check if the operator binds right
        if current_precedence < parent_precedence
            || (current_precedence == parent_precedence && !bind_right)
        {
            return Ok(current_lhs);
        }

        if let Some((op_token, pos)) = input.next() {
            input.peek();

            let rhs = parse_unary(input, allow_stmt_expr)?;

            let next_precedence = if let Some((next_op, _)) = input.peek() {
                next_op.precedence()
            } else {
                0
            };

            // Bind to right if the next operator has higher precedence
            // If same precedence, then check if the operator binds right
            let rhs = if (current_precedence == next_precedence && bind_right)
                || current_precedence < next_precedence
            {
                parse_binary_op(input, current_precedence, rhs, allow_stmt_expr)?
            } else {
                // Otherwise bind to left (even if next operator has the same precedence)
                rhs
            };

            current_lhs = match op_token {
                Token::Plus => Expr::FunctionCall("+".into(), vec![current_lhs, rhs], None, pos),
                Token::Minus => Expr::FunctionCall("-".into(), vec![current_lhs, rhs], None, pos),
                Token::Multiply => {
                    Expr::FunctionCall("*".into(), vec![current_lhs, rhs], None, pos)
                }
                Token::Divide => Expr::FunctionCall("/".into(), vec![current_lhs, rhs], None, pos),

                Token::Equals => parse_assignment(current_lhs, rhs, pos)?,
                Token::PlusAssign => parse_op_assignment("+", current_lhs, rhs, pos)?,
                Token::MinusAssign => parse_op_assignment("-", current_lhs, rhs, pos)?,

                #[cfg(not(feature = "no_object"))]
                Token::Period => {
                    fn check_property(expr: Expr) -> Result<Expr, ParseError> {
                        match expr {
                            // xxx.lhs.rhs
                            Expr::Dot(lhs, rhs, pos) => Ok(Expr::Dot(
                                Box::new(check_property(*lhs)?),
                                Box::new(check_property(*rhs)?),
                                pos,
                            )),
                            // xxx.lhs[idx]
                            #[cfg(not(feature = "no_index"))]
                            Expr::Index(lhs, idx, pos) => {
                                Ok(Expr::Index(Box::new(check_property(*lhs)?), idx, pos))
                            }
                            // xxx.id
                            Expr::Variable(id, pos) => Ok(Expr::Property(id, pos)),
                            // xxx.prop
                            expr @ Expr::Property(_, _) => Ok(expr),
                            // xxx.fn()
                            expr @ Expr::FunctionCall(_, _, _, _) => Ok(expr),
                            expr => Err(PERR::PropertyExpected.into_err(expr.position())),
                        }
                    }

                    Expr::Dot(Box::new(current_lhs), Box::new(check_property(rhs)?), pos)
                }

                // Comparison operators default to false when passed invalid operands
                Token::EqualsTo => Expr::FunctionCall(
                    "==".into(),
                    vec![current_lhs, rhs],
                    Some(Box::new(false)),
                    pos,
                ),
                Token::NotEqualsTo => Expr::FunctionCall(
                    "!=".into(),
                    vec![current_lhs, rhs],
                    Some(Box::new(false)),
                    pos,
                ),
                Token::LessThan => Expr::FunctionCall(
                    "<".into(),
                    vec![current_lhs, rhs],
                    Some(Box::new(false)),
                    pos,
                ),
                Token::LessThanEqualsTo => Expr::FunctionCall(
                    "<=".into(),
                    vec![current_lhs, rhs],
                    Some(Box::new(false)),
                    pos,
                ),
                Token::GreaterThan => Expr::FunctionCall(
                    ">".into(),
                    vec![current_lhs, rhs],
                    Some(Box::new(false)),
                    pos,
                ),
                Token::GreaterThanEqualsTo => Expr::FunctionCall(
                    ">=".into(),
                    vec![current_lhs, rhs],
                    Some(Box::new(false)),
                    pos,
                ),

                Token::Or => Expr::Or(Box::new(current_lhs), Box::new(rhs)),
                Token::And => Expr::And(Box::new(current_lhs), Box::new(rhs)),
                Token::XOr => Expr::FunctionCall("^".into(), vec![current_lhs, rhs], None, pos),
                Token::OrAssign => parse_op_assignment("|", current_lhs, rhs, pos)?,
                Token::AndAssign => parse_op_assignment("&", current_lhs, rhs, pos)?,
                Token::XOrAssign => parse_op_assignment("^", current_lhs, rhs, pos)?,
                Token::MultiplyAssign => parse_op_assignment("*", current_lhs, rhs, pos)?,
                Token::DivideAssign => parse_op_assignment("/", current_lhs, rhs, pos)?,
                Token::Pipe => Expr::FunctionCall("|".into(), vec![current_lhs, rhs], None, pos),
                Token::LeftShift => {
                    Expr::FunctionCall("<<".into(), vec![current_lhs, rhs], None, pos)
                }
                Token::RightShift => {
                    Expr::FunctionCall(">>".into(), vec![current_lhs, rhs], None, pos)
                }
                Token::LeftShiftAssign => parse_op_assignment("<<", current_lhs, rhs, pos)?,
                Token::RightShiftAssign => parse_op_assignment(">>", current_lhs, rhs, pos)?,
                Token::Ampersand => {
                    Expr::FunctionCall("&".into(), vec![current_lhs, rhs], None, pos)
                }
                Token::Modulo => Expr::FunctionCall("%".into(), vec![current_lhs, rhs], None, pos),
                Token::ModuloAssign => parse_op_assignment("%", current_lhs, rhs, pos)?,
                Token::PowerOf => Expr::FunctionCall("~".into(), vec![current_lhs, rhs], None, pos),
                Token::PowerOfAssign => parse_op_assignment("~", current_lhs, rhs, pos)?,
                token => return Err(PERR::UnknownOperator(token.syntax().into()).into_err(pos)),
            };
        }
    }
}

/// Parse an expression.
fn parse_expr<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<Expr, ParseError> {
    // Parse a real expression
    let lhs = parse_unary(input, allow_stmt_expr)?;
    parse_binary_op(input, 1, lhs, allow_stmt_expr)
}

/// Make sure that the expression is not a statement expression (i.e. wrapped in {})
fn ensure_not_statement_expr<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    type_name: &str,
) -> Result<(), ParseError> {
    match input
        .peek()
        .ok_or_else(|| PERR::ExprExpected(type_name.to_string()).into_err_eof())?
    {
        // Disallow statement expressions
        (Token::LeftBrace, pos) => Err(PERR::ExprExpected(type_name.to_string()).into_err(*pos)),
        // No need to check for others at this time - leave it for the expr parser
        _ => Ok(()),
    }
}

/// Parse an if statement.
fn parse_if<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    breakable: bool,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    // if ...
    input.next();

    // if guard { if_body }
    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, allow_stmt_expr)?;
    let if_body = parse_block(input, breakable, allow_stmt_expr)?;

    // if guard { if_body } else ...
    let else_body = if matches!(input.peek(), Some((Token::Else, _))) {
        input.next();

        Some(Box::new(if matches!(input.peek(), Some((Token::If, _))) {
            // if guard { if_body } else if ...
            parse_if(input, breakable, allow_stmt_expr)?
        } else {
            // if guard { if_body } else { else-body }
            parse_block(input, breakable, allow_stmt_expr)?
        }))
    } else {
        None
    };

    Ok(Stmt::IfThenElse(
        Box::new(guard),
        Box::new(if_body),
        else_body,
    ))
}

/// Parse a while loop.
fn parse_while<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    // while ...
    input.next();

    // while guard { body }
    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, allow_stmt_expr)?;
    let body = parse_block(input, true, allow_stmt_expr)?;

    Ok(Stmt::While(Box::new(guard), Box::new(body)))
}

/// Parse a loop statement.
fn parse_loop<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    // loop ...
    input.next();

    // loop { body }
    let body = parse_block(input, true, allow_stmt_expr)?;

    Ok(Stmt::Loop(Box::new(body)))
}

/// Parse a for loop.
fn parse_for<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    // for ...
    input.next();

    // for name ...
    let name = match input
        .next()
        .ok_or_else(|| PERR::VariableExpected.into_err_eof())?
    {
        // Variable name
        (Token::Identifier(s), _) => s,
        // Bad identifier
        (Token::LexError(err), pos) => return Err(PERR::BadInput(err.to_string()).into_err(pos)),
        // Not a variable name
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    // for name in ...
    match input.next().ok_or_else(|| {
        PERR::MissingToken("in".into(), "after the iteration variable".into()).into_err_eof()
    })? {
        (Token::In, _) => (),
        (_, pos) => {
            return Err(
                PERR::MissingToken("in".into(), "after the iteration variable".into())
                    .into_err(pos),
            )
        }
    }

    // for name in expr { body }
    ensure_not_statement_expr(input, "a boolean")?;
    let expr = parse_expr(input, allow_stmt_expr)?;
    let body = parse_block(input, true, allow_stmt_expr)?;

    Ok(Stmt::For(name, Box::new(expr), Box::new(body)))
}

/// Parse a variable definition statement.
fn parse_let<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    var_type: ScopeEntryType,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    // let/const... (specified in `var_type`)
    input.next();

    // let name ...
    let (name, pos) = match input
        .next()
        .ok_or_else(|| PERR::VariableExpected.into_err_eof())?
    {
        (Token::Identifier(s), pos) => (s, pos),
        (Token::LexError(err), pos) => return Err(PERR::BadInput(err.to_string()).into_err(pos)),
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    // let name = ...
    if matches!(input.peek(), Some((Token::Equals, _))) {
        input.next();

        // let name = expr
        let init_value = parse_expr(input, allow_stmt_expr)?;

        match var_type {
            // let name = expr
            ScopeEntryType::Normal => Ok(Stmt::Let(name, Some(Box::new(init_value)), pos)),
            // const name = { expr:constant }
            ScopeEntryType::Constant if init_value.is_constant() => {
                Ok(Stmt::Const(name, Box::new(init_value), pos))
            }
            // const name = expr - error
            ScopeEntryType::Constant => {
                Err(PERR::ForbiddenConstantExpr(name).into_err(init_value.position()))
            }
        }
    } else {
        // let name
        Ok(Stmt::Let(name, None, pos))
    }
}

/// Parse a statement block.
fn parse_block<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    breakable: bool,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    // Must start with {
    let pos = match input
        .next()
        .ok_or_else(|| PERR::UnexpectedEOF.into_err_eof())?
    {
        (Token::LeftBrace, pos) => pos,
        (_, pos) => {
            return Err(
                PERR::MissingToken("{".into(), "to start a statement block".into()).into_err(pos),
            )
        }
    };

    let mut statements = Vec::new();

    while !matches!(input.peek(), Some((Token::RightBrace, _))) {
        // Parse statements inside the block
        let stmt = parse_stmt(input, breakable, allow_stmt_expr)?;

        // See if it needs a terminating semicolon
        let need_semicolon = !stmt.is_self_terminated();

        statements.push(stmt);

        match input.peek() {
            // EOF
            None => break,
            // { ... stmt }
            Some((Token::RightBrace, _)) => break,
            // { ... stmt;
            Some((Token::SemiColon, _)) if need_semicolon => {
                input.next();
            }
            // { ... { stmt } ;
            Some((Token::SemiColon, _)) if !need_semicolon => (),
            // { ... { stmt } ???
            Some((_, _)) if !need_semicolon => (),
            // { ... stmt ??? - error
            Some((_, pos)) => {
                // Semicolons are not optional between statements
                return Err(
                    PERR::MissingToken(";".into(), "to terminate this statement".into())
                        .into_err(*pos),
                );
            }
        }
    }

    match input.peek().ok_or_else(|| {
        PERR::MissingToken("}".into(), "to end this statement block".into()).into_err_eof()
    })? {
        (Token::RightBrace, _) => {
            input.next();
            Ok(Stmt::Block(statements, pos))
        }
        (_, pos) => {
            Err(PERR::MissingToken("}".into(), "to end this statement block".into()).into_err(*pos))
        }
    }
}

/// Parse an expression as a statement.
fn parse_expr_stmt<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    Ok(Stmt::Expr(Box::new(parse_expr(input, allow_stmt_expr)?)))
}

/// Parse a single statement.
fn parse_stmt<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    breakable: bool,
    allow_stmt_expr: bool,
) -> Result<Stmt, ParseError> {
    let token = match input.peek() {
        Some(token) => token,
        None => return Ok(Stmt::Noop(Position::eof())),
    };

    match token {
        // Semicolon - empty statement
        (Token::SemiColon, pos) => Ok(Stmt::Noop(*pos)),

        (Token::LeftBrace, _) => parse_block(input, breakable, allow_stmt_expr),

        // fn ...
        #[cfg(not(feature = "no_function"))]
        (Token::Fn, pos) => Err(PERR::WrongFnDefinition.into_err(*pos)),

        (Token::If, _) => parse_if(input, breakable, allow_stmt_expr),
        (Token::While, _) => parse_while(input, allow_stmt_expr),
        (Token::Loop, _) => parse_loop(input, allow_stmt_expr),
        (Token::For, _) => parse_for(input, allow_stmt_expr),

        (Token::Continue, pos) if breakable => {
            let pos = *pos;
            input.next();
            Ok(Stmt::Continue(pos))
        }
        (Token::Break, pos) if breakable => {
            let pos = *pos;
            input.next();
            Ok(Stmt::Break(pos))
        }
        (Token::Continue, pos) | (Token::Break, pos) => Err(PERR::LoopBreak.into_err(*pos)),

        (token @ Token::Return, pos) | (token @ Token::Throw, pos) => {
            let return_type = match token {
                Token::Return => ReturnType::Return,
                Token::Throw => ReturnType::Exception,
                _ => panic!("token should be return or throw"),
            };

            let pos = *pos;
            input.next();

            match input.peek() {
                // `return`/`throw` at EOF
                None => Ok(Stmt::ReturnWithVal(None, return_type, Position::eof())),
                // `return;` or `throw;`
                Some((Token::SemiColon, _)) => Ok(Stmt::ReturnWithVal(None, return_type, pos)),
                // `return` or `throw` with expression
                Some((_, _)) => {
                    let expr = parse_expr(input, allow_stmt_expr)?;
                    let pos = expr.position();
                    Ok(Stmt::ReturnWithVal(Some(Box::new(expr)), return_type, pos))
                }
            }
        }

        (Token::Let, _) => parse_let(input, ScopeEntryType::Normal, allow_stmt_expr),
        (Token::Const, _) => parse_let(input, ScopeEntryType::Constant, allow_stmt_expr),

        _ => parse_expr_stmt(input, allow_stmt_expr),
    }
}

/// Parse a function definition.
#[cfg(not(feature = "no_function"))]
fn parse_fn<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
    allow_stmt_expr: bool,
) -> Result<FnDef, ParseError> {
    let pos = input.next().expect("should be fn").1;

    let name = match input
        .next()
        .ok_or_else(|| PERR::FnMissingName.into_err_eof())?
    {
        (Token::Identifier(s), _) => s,
        (_, pos) => return Err(PERR::FnMissingName.into_err(pos)),
    };

    match input
        .peek()
        .ok_or_else(|| PERR::FnMissingParams(name.clone()).into_err_eof())?
    {
        (Token::LeftParen, _) => {
            input.next();
        }
        (_, pos) => return Err(PERR::FnMissingParams(name).into_err(*pos)),
    }

    let mut params = Vec::new();

    if matches!(input.peek(), Some((Token::RightParen, _))) {
        input.next();
    } else {
        let end_err = format!("to close the parameters list of function '{}'", name);
        let sep_err = format!("to separate the parameters of function '{}'", name);

        loop {
            match input
                .next()
                .ok_or_else(|| PERR::MissingToken(")".into(), end_err.to_string()).into_err_eof())?
            {
                (Token::Identifier(s), pos) => {
                    params.push((s, pos));
                }
                (_, pos) => return Err(PERR::MissingToken(")".into(), end_err).into_err(pos)),
            }

            match input
                .next()
                .ok_or_else(|| PERR::MissingToken(")".into(), end_err.to_string()).into_err_eof())?
            {
                (Token::RightParen, _) => break,
                (Token::Comma, _) => (),
                (Token::Identifier(_), pos) => {
                    return Err(PERR::MissingToken(",".into(), sep_err).into_err(pos))
                }
                (_, pos) => return Err(PERR::MissingToken(",".into(), sep_err).into_err(pos)),
            }
        }
    }

    // Check for duplicating parameters
    params
        .iter()
        .enumerate()
        .try_for_each(|(i, (p1, _))| {
            params
                .iter()
                .skip(i + 1)
                .find(|(p2, _)| p2 == p1)
                .map_or_else(|| Ok(()), |(p2, pos)| Err((p2, *pos)))
        })
        .map_err(|(p, pos)| {
            PERR::FnDuplicatedParam(name.to_string(), p.to_string()).into_err(pos)
        })?;

    // Parse function body
    let body = match input.peek() {
        Some((Token::LeftBrace, _)) => parse_block(input, false, allow_stmt_expr)?,
        Some((_, pos)) => return Err(PERR::FnMissingBody(name).into_err(*pos)),
        None => return Err(PERR::FnMissingBody(name).into_err_eof()),
    };

    Ok(FnDef {
        name,
        params: params.into_iter().map(|(p, _)| p).collect(),
        body,
        pos,
    })
}

pub fn parse_global_expr<'a, 'e>(
    input: &mut Peekable<TokenIterator<'a>>,
    engine: &Engine<'e>,
    scope: &Scope,
) -> Result<AST, ParseError> {
    let expr = parse_expr(input, false)?;

    if let Some((token, pos)) = input.peek() {
        // Return error if the expression doesn't end
        return Err(PERR::BadInput(format!("Unexpected '{}'", token.syntax())).into_err(*pos));
    }

    Ok(
        // Optimize AST
        #[cfg(not(feature = "no_optimize"))]
        optimize_into_ast(engine, scope, vec![Stmt::Expr(Box::new(expr))], vec![]),
        //
        // Do not optimize AST if `no_optimize`
        #[cfg(feature = "no_optimize")]
        AST(
            vec![Stmt::Expr(Box::new(expr))],
            #[cfg(feature = "sync")]
            {
                Arc::new(FunctionsLib::new())
            },
            #[cfg(not(feature = "sync"))]
            {
                Rc::new(FunctionsLib::new())
            },
        ),
    )
}

/// Parse the global level statements.
fn parse_global_level<'a>(
    input: &mut Peekable<TokenIterator<'a>>,
) -> Result<(Vec<Stmt>, Vec<FnDef>), ParseError> {
    let mut statements = Vec::<Stmt>::new();
    let mut functions = Vec::<FnDef>::new();

    while input.peek().is_some() {
        #[cfg(not(feature = "no_function"))]
        {
            // Collect all the function definitions
            if matches!(input.peek().expect("should not be None"), (Token::Fn, _)) {
                let f = parse_fn(input, true)?;

                // Ensure list is sorted
                match functions.binary_search_by(|fn_def| fn_def.compare(&f.name, f.params.len())) {
                    Ok(n) => functions[n] = f,        // Override previous definition
                    Err(n) => functions.insert(n, f), // New function definition
                }

                continue;
            }
        }

        // Actual statement
        let stmt = parse_stmt(input, false, true)?;

        let need_semicolon = !stmt.is_self_terminated();

        statements.push(stmt);

        match input.peek() {
            // EOF
            None => break,
            // stmt ;
            Some((Token::SemiColon, _)) if need_semicolon => {
                input.next();
            }
            // stmt ;
            Some((Token::SemiColon, _)) if !need_semicolon => (),
            // { stmt } ???
            Some((_, _)) if !need_semicolon => (),
            // stmt ??? - error
            Some((_, pos)) => {
                // Semicolons are not optional between statements
                return Err(
                    PERR::MissingToken(";".into(), "to terminate this statement".into())
                        .into_err(*pos),
                );
            }
        }
    }

    Ok((statements, functions))
}

/// Run the parser on an input stream, returning an AST.
pub fn parse<'a, 'e>(
    input: &mut Peekable<TokenIterator<'a>>,
    engine: &Engine<'e>,
    scope: &Scope,
) -> Result<AST, ParseError> {
    let (statements, functions) = parse_global_level(input)?;

    Ok(
        // Optimize AST
        #[cfg(not(feature = "no_optimize"))]
        optimize_into_ast(engine, scope, statements, functions),
        //
        // Do not optimize AST if `no_optimize`
        #[cfg(feature = "no_optimize")]
        AST(statements, Arc::new(FunctionsLib::from_vec(functions))),
    )
}

/// Map a `Dynamic` value to an expression.
///
/// Returns Some(expression) if conversion is successful.  Otherwise None.
pub fn map_dynamic_to_expr(value: Dynamic, pos: Position) -> Option<Expr> {
    if value.is::<INT>() {
        Some(Expr::IntegerConstant(value.cast(), pos))
    } else if value.is::<char>() {
        Some(Expr::CharConstant(value.cast(), pos))
    } else if value.is::<String>() {
        Some(Expr::StringConstant(value.cast(), pos))
    } else if value.is::<bool>() {
        Some(if value.cast::<bool>() {
            Expr::True(pos)
        } else {
            Expr::False(pos)
        })
    } else {
        #[cfg(not(feature = "no_float"))]
        {
            if value.is::<FLOAT>() {
                return Some(Expr::FloatConstant(value.cast(), pos));
            }
        }

        None
    }
}
