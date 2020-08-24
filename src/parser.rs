//! Main module defining the lexer and parser.

use crate::any::{Dynamic, Union};
use crate::calc_fn_hash;
use crate::engine::{Engine, KEYWORD_THIS, MARKER_BLOCK, MARKER_EXPR, MARKER_IDENT};
use crate::error::{LexError, ParseError, ParseErrorType};
use crate::fn_native::{FnPtr, Shared};
use crate::module::{Module, ModuleRef};
use crate::optimize::{optimize_into_ast, OptimizationLevel};
use crate::scope::{EntryType as ScopeEntryType, Scope};
use crate::syntax::FnCustomSyntaxEval;
use crate::token::{is_keyword_function, is_valid_identifier, Position, Token, TokenStream};
use crate::utils::{StaticVec, StraightHasherBuilder};

#[cfg(not(feature = "no_function"))]
use crate::engine::{FN_ANONYMOUS, KEYWORD_FN_PTR_CURRY};

#[cfg(not(feature = "no_object"))]
use crate::engine::{make_getter, make_setter};

use crate::stdlib::{
    borrow::Cow,
    boxed::Box,
    char,
    collections::HashMap,
    fmt, format,
    hash::{Hash, Hasher},
    iter::empty,
    num::NonZeroUsize,
    ops::Add,
    string::{String, ToString},
    vec,
    vec::Vec,
};

#[cfg(not(feature = "no_std"))]
#[cfg(not(feature = "no_function"))]
use crate::stdlib::collections::hash_map::DefaultHasher;

#[cfg(not(feature = "no_closure"))]
use crate::stdlib::collections::HashSet;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_function"))]
use ahash::AHasher;

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

#[cfg(feature = "decimal")]
use rust_decimal::prelude::*;

type PERR = ParseErrorType;

pub use crate::utils::ImmutableString;

type FunctionsLib = HashMap<u64, ScriptFnDef, StraightHasherBuilder>;

/// Compiled AST (abstract syntax tree) of a Rhai script.
///
/// Currently, `AST` is neither `Send` nor `Sync`. Turn on the `sync` feature to make it `Send + Sync`.
#[derive(Debug, Clone, Default)]
pub struct AST(
    /// Global statements.
    Vec<Stmt>,
    /// Script-defined functions.
    Module,
);

impl AST {
    /// Create a new `AST`.
    pub fn new(statements: Vec<Stmt>, lib: Module) -> Self {
        Self(statements, lib)
    }

    /// Get the statements.
    #[cfg(not(feature = "internals"))]
    pub(crate) fn statements(&self) -> &[Stmt] {
        &self.0
    }

    /// [INTERNALS] Get the statements.
    #[cfg(feature = "internals")]
    #[deprecated(note = "this method is volatile and may change")]
    pub fn statements(&self) -> &[Stmt] {
        &self.0
    }

    /// Get a mutable reference to the statements.
    pub(crate) fn statements_mut(&mut self) -> &mut Vec<Stmt> {
        &mut self.0
    }

    /// Get the internal `Module` containing all script-defined functions.
    #[cfg(not(feature = "internals"))]
    pub(crate) fn lib(&self) -> &Module {
        &self.1
    }

    /// [INTERNALS] Get the internal `Module` containing all script-defined functions.
    #[cfg(feature = "internals")]
    #[deprecated(note = "this method is volatile and may change")]
    pub fn lib(&self) -> &Module {
        &self.1
    }

    /// Clone the `AST`'s functions into a new `AST`.
    /// No statements are cloned.
    ///
    /// This operation is cheap because functions are shared.
    pub fn clone_functions_only(&self) -> Self {
        self.clone_functions_only_filtered(|_, _, _| true)
    }

    /// Clone the `AST`'s functions into a new `AST` based on a filter predicate.
    /// No statements are cloned.
    ///
    /// This operation is cheap because functions are shared.
    pub fn clone_functions_only_filtered(
        &self,
        filter: impl Fn(FnAccess, &str, usize) -> bool,
    ) -> Self {
        let mut functions: Module = Default::default();
        functions.merge_filtered(&self.1, filter);
        Self(Default::default(), functions)
    }

    /// Clone the `AST`'s script statements into a new `AST`.
    /// No functions are cloned.
    pub fn clone_statements_only(&self) -> Self {
        Self(self.0.clone(), Default::default())
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
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_function"))]
    /// # {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// let ast1 = engine.compile(r#"
    ///                 fn foo(x) { 42 + x }
    ///                 foo(1)
    ///             "#)?;
    ///
    /// let ast2 = engine.compile(r#"
    ///                 fn foo(n) { "hello" + n }
    ///                 foo("!")
    ///             "#)?;
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
        self.merge_filtered(other, |_, _, _| true)
    }

    /// Merge two `AST` into one.  Both `AST`'s are untouched and a new, merged, version
    /// is returned.
    ///
    /// The second `AST` is simply appended to the end of the first _without any processing_.
    /// Thus, the return value of the first `AST` (if using expression-statement syntax) is buried.
    /// Of course, if the first `AST` uses a `return` statement at the end, then
    /// the second `AST` will essentially be dead code.
    ///
    /// All script-defined functions in the second `AST` are first selected based on a filter
    /// predicate, then overwrite similarly-named functions in the first `AST` with the
    /// same number of parameters.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_function"))]
    /// # {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// let ast1 = engine.compile(r#"
    ///                 fn foo(x) { 42 + x }
    ///                 foo(1)
    ///             "#)?;
    ///
    /// let ast2 = engine.compile(r#"
    ///                 fn foo(n) { "hello" + n }
    ///                 fn error() { 0 }
    ///                 foo("!")
    ///             "#)?;
    ///
    /// // Merge 'ast2', picking only 'error()' but not 'foo(_)', into 'ast1'
    /// let ast = ast1.merge_filtered(&ast2, |_, name, params| name == "error" && params == 0);
    ///
    /// // 'ast' is essentially:
    /// //
    /// //    fn foo(n) { 42 + n }      // <- definition of 'ast1::foo' is not overwritten
    /// //                              //    because 'ast2::foo' is filtered away
    /// //    foo(1)                    // <- notice this will be 43 instead of "hello1",
    /// //                              //    but it is no longer the return value
    /// //    fn error() { 0 }          // <- this function passes the filter and is merged
    /// //    foo("!")                  // <- returns "42!"
    ///
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast::<String>(&ast)?, "42!");
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    pub fn merge_filtered(
        &self,
        other: &Self,
        filter: impl Fn(FnAccess, &str, usize) -> bool,
    ) -> Self {
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

        let mut functions = functions.clone();
        functions.merge_filtered(&other.1, filter);

        Self::new(ast, functions)
    }

    /// Filter out the functions, retaining only some based on a filter predicate.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_function"))]
    /// # {
    /// use rhai::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// let mut ast = engine.compile(r#"
    ///                         fn foo(n) { n + 1 }
    ///                         fn bar() { print("hello"); }
    ///                     "#)?;
    ///
    /// // Remove all functions except 'foo(_)'
    /// ast.retain_functions(|_, name, params| name == "foo" && params == 1);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_function"))]
    pub fn retain_functions(&mut self, filter: impl Fn(FnAccess, &str, usize) -> bool) {
        self.1.retain_functions(filter);
    }

    /// Clear all function definitions in the `AST`.
    #[cfg(not(feature = "no_function"))]
    pub fn clear_functions(&mut self) {
        self.1 = Default::default();
    }

    /// Clear all statements in the `AST`, leaving only function definitions.
    pub fn clear_statements(&mut self) {
        self.0 = vec![];
    }
}

impl Add<Self> for &AST {
    type Output = AST;

    fn add(self, rhs: Self) -> Self::Output {
        self.merge(rhs)
    }
}

impl AsRef<[Stmt]> for AST {
    fn as_ref(&self) -> &[Stmt] {
        self.statements()
    }
}

impl AsRef<Module> for AST {
    fn as_ref(&self) -> &Module {
        self.lib()
    }
}

/// A type representing the access mode of a scripted function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum FnAccess {
    /// Private function.
    Private,
    /// Public function.
    Public,
}

impl fmt::Display for FnAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Private => write!(f, "private"),
            Self::Public => write!(f, "public"),
        }
    }
}

/// [INTERNALS] A type containing information on a scripted function.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone)]
pub struct ScriptFnDef {
    /// Function name.
    pub name: ImmutableString,
    /// Function access mode.
    pub access: FnAccess,
    /// Names of function parameters.
    pub params: StaticVec<String>,
    /// Access to external variables.
    #[cfg(not(feature = "no_closure"))]
    pub externals: HashSet<String>,
    /// Function body.
    pub body: Stmt,
    /// Position of the function definition.
    pub pos: Position,
}

impl fmt::Display for ScriptFnDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}({})",
            match self.access {
                FnAccess::Public => "",
                FnAccess::Private => "private ",
            },
            self.name,
            self.params
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

/// [INTERNALS] A type encapsulating the mode of a `return`/`throw` statement.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub enum ReturnType {
    /// `return` statement.
    Return,
    /// `throw` statement.
    Exception,
}

#[derive(Clone)]
struct ParseState<'e> {
    /// Reference to the scripting `Engine`.
    engine: &'e Engine,
    /// Encapsulates a local stack with variable names to simulate an actual runtime scope.
    stack: Vec<(String, ScopeEntryType)>,
    /// Tracks a list of external variables (variables that are not explicitly declared in the scope).
    #[cfg(not(feature = "no_closure"))]
    externals: HashMap<String, Position>,
    /// An indicator that disables variable capturing into externals one single time.
    /// If set to false the next call to `access_var` will not capture the variable.
    /// All consequent calls to `access_var` will not be affected
    #[cfg(not(feature = "no_closure"))]
    allow_capture: bool,
    /// Encapsulates a local stack with variable names to simulate an actual runtime scope.
    modules: Vec<String>,
    /// Maximum levels of expression nesting.
    #[cfg(not(feature = "unchecked"))]
    max_expr_depth: usize,
    /// Maximum levels of expression nesting in functions.
    #[cfg(not(feature = "unchecked"))]
    max_function_expr_depth: usize,
}

impl<'e> ParseState<'e> {
    /// Create a new `ParseState`.
    pub fn new(
        engine: &'e Engine,
        #[cfg(not(feature = "unchecked"))] max_expr_depth: usize,
        #[cfg(not(feature = "unchecked"))] max_function_expr_depth: usize,
    ) -> Self {
        Self {
            engine,
            #[cfg(not(feature = "unchecked"))]
            max_expr_depth,
            #[cfg(not(feature = "unchecked"))]
            max_function_expr_depth,
            #[cfg(not(feature = "no_closure"))]
            externals: Default::default(),
            #[cfg(not(feature = "no_closure"))]
            allow_capture: true,
            stack: Default::default(),
            modules: Default::default(),
        }
    }

    /// Find explicitly declared variable by name in the `ParseState`, searching in reverse order.
    ///
    /// If the variable is not present in the scope adds it to the list of external variables
    ///
    /// The return value is the offset to be deducted from `Stack::len`,
    /// i.e. the top element of the `ParseState` is offset 1.
    /// Return `None` when the variable name is not found in the `stack`.
    fn access_var(&mut self, name: &str, _pos: Position) -> Option<NonZeroUsize> {
        let index = self
            .stack
            .iter()
            .rev()
            .enumerate()
            .find(|(_, (n, _))| *n == name)
            .and_then(|(i, _)| NonZeroUsize::new(i + 1));

        #[cfg(not(feature = "no_closure"))]
        if self.allow_capture {
            if index.is_none() && !self.externals.contains_key(name) {
                self.externals.insert(name.to_string(), _pos);
            }
        } else {
            self.allow_capture = true
        }

        index
    }

    /// Find a module by name in the `ParseState`, searching in reverse.
    /// The return value is the offset to be deducted from `Stack::len`,
    /// i.e. the top element of the `ParseState` is offset 1.
    /// Return `None` when the variable name is not found in the `ParseState`.
    pub fn find_module(&self, name: &str) -> Option<NonZeroUsize> {
        self.modules
            .iter()
            .rev()
            .enumerate()
            .find(|(_, n)| *n == name)
            .and_then(|(i, _)| NonZeroUsize::new(i + 1))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
/// A type that encapsulates all the settings for a particular parsing function.
struct ParseSettings {
    /// Current position.
    pos: Position,
    /// Is the construct being parsed located at global level?
    is_global: bool,
    /// Is the construct being parsed located at function definition level?
    is_function_scope: bool,
    /// Is the current position inside a loop?
    is_breakable: bool,
    /// Is anonymous function allowed?
    allow_anonymous_fn: bool,
    /// Is if-expression allowed?
    allow_if_expr: bool,
    /// Is statement-expression allowed?
    allow_stmt_expr: bool,
    /// Current expression nesting level.
    level: usize,
}

impl ParseSettings {
    /// Create a new `ParseSettings` with one higher expression level.
    pub fn level_up(&self) -> Self {
        Self {
            level: self.level + 1,
            ..*self
        }
    }
    /// Make sure that the current level of expression nesting is within the maximum limit.
    #[cfg(not(feature = "unchecked"))]
    pub fn ensure_level_within_max_limit(&self, limit: usize) -> Result<(), ParseError> {
        if limit == 0 {
            Ok(())
        } else if self.level > limit {
            Err(PERR::ExprTooDeep.into_err(self.pos))
        } else {
            Ok(())
        }
    }
}

/// [INTERNALS] A Rhai statement.
/// Exported under the `internals` feature only.
///
/// Each variant is at most one pointer in size (for speed),
/// with everything being allocated together in one single tuple.
#[derive(Debug, Clone, Hash)]
pub enum Stmt {
    /// No-op.
    Noop(Position),
    /// if expr { stmt } else { stmt }
    IfThenElse(Box<(Expr, Stmt, Option<Stmt>, Position)>),
    /// while expr { stmt }
    While(Box<(Expr, Stmt, Position)>),
    /// loop { stmt }
    Loop(Box<(Stmt, Position)>),
    /// for id in expr { stmt }
    For(Box<(String, Expr, Stmt, Position)>),
    /// let id = expr
    Let(Box<((String, Position), Option<Expr>, Position)>),
    /// const id = expr
    Const(Box<((String, Position), Expr, Position)>),
    /// { stmt; ... }
    Block(Box<(StaticVec<Stmt>, Position)>),
    /// expr
    Expr(Box<Expr>),
    /// continue
    Continue(Position),
    /// break
    Break(Position),
    /// return/throw
    ReturnWithVal(Box<((ReturnType, Position), Option<Expr>, Position)>),
    /// import expr as module
    #[cfg(not(feature = "no_module"))]
    Import(Box<(Expr, Option<(String, Position)>, Position)>),
    /// expr id as name, ...
    #[cfg(not(feature = "no_module"))]
    Export(
        Box<(
            StaticVec<((String, Position), Option<(String, Position)>)>,
            Position,
        )>,
    ),
    /// Convert a variable to shared.
    #[cfg(not(feature = "no_closure"))]
    Share(Box<(String, Position)>),
}

impl Default for Stmt {
    fn default() -> Self {
        Self::Noop(Default::default())
    }
}

impl Stmt {
    /// Get the `Position` of this statement.
    pub fn position(&self) -> Position {
        match self {
            Stmt::Noop(pos) | Stmt::Continue(pos) | Stmt::Break(pos) => *pos,
            Stmt::Let(x) => (x.0).1,
            Stmt::Const(x) => (x.0).1,
            Stmt::ReturnWithVal(x) => (x.0).1,
            Stmt::Block(x) => x.1,
            Stmt::IfThenElse(x) => x.3,
            Stmt::Expr(x) => x.position(),
            Stmt::While(x) => x.2,
            Stmt::Loop(x) => x.1,
            Stmt::For(x) => x.3,

            #[cfg(not(feature = "no_module"))]
            Stmt::Import(x) => x.2,
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(x) => x.1,

            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(x) => x.1,
        }
    }

    /// Override the `Position` of this statement.
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            Stmt::Noop(pos) | Stmt::Continue(pos) | Stmt::Break(pos) => *pos = new_pos,
            Stmt::Let(x) => (x.0).1 = new_pos,
            Stmt::Const(x) => (x.0).1 = new_pos,
            Stmt::ReturnWithVal(x) => (x.0).1 = new_pos,
            Stmt::Block(x) => x.1 = new_pos,
            Stmt::IfThenElse(x) => x.3 = new_pos,
            Stmt::Expr(x) => {
                x.set_position(new_pos);
            }
            Stmt::While(x) => x.2 = new_pos,
            Stmt::Loop(x) => x.1 = new_pos,
            Stmt::For(x) => x.3 = new_pos,

            #[cfg(not(feature = "no_module"))]
            Stmt::Import(x) => x.2 = new_pos,
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(x) => x.1 = new_pos,

            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(x) => x.1 = new_pos,
        }

        self
    }

    /// Is this statement self-terminated (i.e. no need for a semicolon terminator)?
    pub fn is_self_terminated(&self) -> bool {
        match self {
            Stmt::IfThenElse(_)
            | Stmt::While(_)
            | Stmt::Loop(_)
            | Stmt::For(_)
            | Stmt::Block(_) => true,

            // A No-op requires a semicolon in order to know it is an empty statement!
            Stmt::Noop(_) => false,

            Stmt::Let(_)
            | Stmt::Const(_)
            | Stmt::Expr(_)
            | Stmt::Continue(_)
            | Stmt::Break(_)
            | Stmt::ReturnWithVal(_) => false,

            #[cfg(not(feature = "no_module"))]
            Stmt::Import(_) | Stmt::Export(_) => false,

            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(_) => false,
        }
    }

    /// Is this statement _pure_?
    pub fn is_pure(&self) -> bool {
        match self {
            Stmt::Noop(_) => true,
            Stmt::Expr(expr) => expr.is_pure(),
            Stmt::IfThenElse(x) if x.2.is_some() => {
                x.0.is_pure() && x.1.is_pure() && x.2.as_ref().unwrap().is_pure()
            }
            Stmt::IfThenElse(x) => x.1.is_pure(),
            Stmt::While(x) => x.0.is_pure() && x.1.is_pure(),
            Stmt::Loop(x) => x.0.is_pure(),
            Stmt::For(x) => x.1.is_pure() && x.2.is_pure(),
            Stmt::Let(_) | Stmt::Const(_) => false,
            Stmt::Block(x) => x.0.iter().all(Stmt::is_pure),
            Stmt::Continue(_) | Stmt::Break(_) | Stmt::ReturnWithVal(_) => false,

            #[cfg(not(feature = "no_module"))]
            Stmt::Import(_) => false,
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(_) => false,

            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(_) => false,
        }
    }
}

/// [INTERNALS] A type wrapping a custom syntax definition.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Clone)]
pub struct CustomExpr(pub StaticVec<Expr>, pub Shared<FnCustomSyntaxEval>);

impl fmt::Debug for CustomExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl Hash for CustomExpr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// [INTERNALS] A type wrapping a floating-point number.
/// Exported under the `internals` feature only.
///
/// This type is mainly used to provide a standard `Hash` implementation
/// to floating-point numbers, allowing `Expr` to derive `Hash` automatically.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[cfg(not(feature = "no_float"))]
#[derive(Debug, PartialEq, PartialOrd, Clone)]
pub struct FloatWrapper(pub FLOAT, pub Position);

#[cfg(not(feature = "no_float"))]
impl Hash for FloatWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0.to_le_bytes());
        self.1.hash(state);
    }
}

/// [INTERNALS] An expression sub-tree.
/// Exported under the `internals` feature only.
///
/// Each variant is at most one pointer in size (for speed),
/// with everything being allocated together in one single tuple.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash)]
pub enum Expr {
    /// Integer constant.
    IntegerConstant(Box<(INT, Position)>),
    /// Floating-point constant.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(Box<FloatWrapper>),
    /// Decimal constant
    #[cfg(feature = "decimal")]
    DecimalConstant(Box<(Decimal, Position)>),
    /// Character constant.
    CharConstant(Box<(char, Position)>),
    /// String constant.
    StringConstant(Box<(ImmutableString, Position)>),
    /// FnPtr constant.
    FnPointer(Box<(ImmutableString, Position)>),
    /// Variable access - ((variable name, position), optional modules, hash, optional index)
    Variable(
        Box<(
            (String, Position),
            Option<Box<ModuleRef>>,
            u64,
            Option<NonZeroUsize>,
        )>,
    ),
    /// Property access.
    Property(Box<((ImmutableString, String, String), Position)>),
    /// { stmt }
    Stmt(Box<(Stmt, Position)>),
    /// Wrapped expression - should not be optimized away.
    Expr(Box<Expr>),
    /// func(expr, ... ) - ((function name, native_only, capture, position), optional modules, hash, arguments, optional default value)
    /// Use `Cow<'static, str>` because a lot of operators (e.g. `==`, `>=`) are implemented as function calls
    /// and the function names are predictable, so no need to allocate a new `String`.
    FnCall(
        Box<(
            (Cow<'static, str>, bool, bool, Position),
            Option<Box<ModuleRef>>,
            u64,
            StaticVec<Expr>,
            Option<bool>,
        )>,
    ),
    /// expr op= expr
    Assignment(Box<(Expr, Cow<'static, str>, Expr, Position)>),
    /// lhs.rhs
    Dot(Box<(Expr, Expr, Position)>),
    /// expr[expr]
    Index(Box<(Expr, Expr, Position)>),
    /// [ expr, ... ]
    Array(Box<(StaticVec<Expr>, Position)>),
    /// #{ name:expr, ... }
    Map(Box<(StaticVec<((ImmutableString, Position), Expr)>, Position)>),
    /// lhs in rhs
    In(Box<(Expr, Expr, Position)>),
    /// lhs && rhs
    And(Box<(Expr, Expr, Position)>),
    /// lhs || rhs
    Or(Box<(Expr, Expr, Position)>),
    /// true
    True(Position),
    /// false
    False(Position),
    /// ()
    Unit(Position),
    /// Custom syntax
    Custom(Box<(CustomExpr, Position)>),
}

impl Default for Expr {
    fn default() -> Self {
        Self::Unit(Default::default())
    }
}

impl Expr {
    /// Get the `Dynamic` value of a constant expression.
    ///
    /// # Panics
    ///
    /// Panics when the expression is not constant.
    pub fn get_constant_value(&self) -> Dynamic {
        match self {
            Self::Expr(x) => x.get_constant_value(),

            Self::IntegerConstant(x) => x.0.into(),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(x) => x.0.into(),

            #[cfg(feature = "decimal")]
            Self::DecimalConstant(x) => x.0.into(),

            Self::CharConstant(x) => x.0.into(),
            Self::StringConstant(x) => x.0.clone().into(),
            Self::FnPointer(x) => Dynamic(Union::FnPtr(Box::new(FnPtr::new_unchecked(
                x.0.clone(),
                Default::default(),
            )))),
            Self::True(_) => true.into(),
            Self::False(_) => false.into(),
            Self::Unit(_) => ().into(),

            #[cfg(not(feature = "no_index"))]
            Self::Array(x) if x.0.iter().all(Self::is_constant) => Dynamic(Union::Array(Box::new(
                x.0.iter().map(Self::get_constant_value).collect::<Vec<_>>(),
            ))),

            #[cfg(not(feature = "no_object"))]
            Self::Map(x) if x.0.iter().all(|(_, v)| v.is_constant()) => {
                Dynamic(Union::Map(Box::new(
                    x.0.iter()
                        .map(|((k, _), v)| (k.clone(), v.get_constant_value()))
                        .collect::<HashMap<_, _>>(),
                )))
            }

            _ => unreachable!("cannot get value of non-constant expression"),
        }
    }

    /// Get the display value of a constant expression.
    ///
    /// # Panics
    ///
    /// Panics when the expression is not constant.
    pub fn get_constant_str(&self) -> String {
        match self {
            Self::Expr(x) => x.get_constant_str(),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(x) => x.0.to_string(),

            #[cfg(feature = "decimal")]
            Self::DecimalConstant(x) => x.0.to_string(),

            Self::IntegerConstant(x) => x.0.to_string(),
            Self::CharConstant(x) => x.0.to_string(),
            Self::StringConstant(_) => "string".to_string(),
            Self::True(_) => "true".to_string(),
            Self::False(_) => "false".to_string(),
            Self::Unit(_) => "()".to_string(),

            Self::Array(x) if x.0.iter().all(Self::is_constant) => "array".to_string(),

            _ => unreachable!("cannot get value of non-constant expression"),
        }
    }

    /// Get the `Position` of the expression.
    pub fn position(&self) -> Position {
        match self {
            Self::Expr(x) => x.position(),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(x) => x.1,

            #[cfg(feature = "decimal")]
            Self::DecimalConstant(x) => x.1,

            Self::IntegerConstant(x) => x.1,
            Self::CharConstant(x) => x.1,
            Self::StringConstant(x) => x.1,
            Self::FnPointer(x) => x.1,
            Self::Array(x) => x.1,
            Self::Map(x) => x.1,
            Self::Property(x) => x.1,
            Self::Stmt(x) => x.1,
            Self::Variable(x) => (x.0).1,
            Self::FnCall(x) => (x.0).3,
            Self::Assignment(x) => x.0.position(),

            Self::And(x) | Self::Or(x) | Self::In(x) => x.2,

            Self::True(pos) | Self::False(pos) | Self::Unit(pos) => *pos,

            Self::Dot(x) | Self::Index(x) => x.0.position(),

            Self::Custom(x) => x.1,
        }
    }

    /// Override the `Position` of the expression.
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            Self::Expr(x) => {
                x.set_position(new_pos);
            }

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(x) => x.1 = new_pos,

            #[cfg(feature = "decimal")]
            Self::DecimalConstant(x) => x.1 = new_pos,

            Self::IntegerConstant(x) => x.1 = new_pos,
            Self::CharConstant(x) => x.1 = new_pos,
            Self::StringConstant(x) => x.1 = new_pos,
            Self::FnPointer(x) => x.1 = new_pos,
            Self::Array(x) => x.1 = new_pos,
            Self::Map(x) => x.1 = new_pos,
            Self::Variable(x) => (x.0).1 = new_pos,
            Self::Property(x) => x.1 = new_pos,
            Self::Stmt(x) => x.1 = new_pos,
            Self::FnCall(x) => (x.0).3 = new_pos,
            Self::And(x) => x.2 = new_pos,
            Self::Or(x) => x.2 = new_pos,
            Self::In(x) => x.2 = new_pos,
            Self::True(pos) => *pos = new_pos,
            Self::False(pos) => *pos = new_pos,
            Self::Unit(pos) => *pos = new_pos,
            Self::Assignment(x) => x.3 = new_pos,
            Self::Dot(x) => x.2 = new_pos,
            Self::Index(x) => x.2 = new_pos,
            Self::Custom(x) => x.1 = new_pos,
        }

        self
    }

    /// Is the expression pure?
    ///
    /// A pure expression has no side effects.
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Expr(x) => x.is_pure(),

            Self::Array(x) => x.0.iter().all(Self::is_pure),

            Self::Index(x) | Self::And(x) | Self::Or(x) | Self::In(x) => {
                let (lhs, rhs, _) = x.as_ref();
                lhs.is_pure() && rhs.is_pure()
            }

            Self::Stmt(x) => x.0.is_pure(),

            Self::Variable(_) => true,

            _ => self.is_constant(),
        }
    }

    /// Is the expression a constant?
    pub fn is_constant(&self) -> bool {
        match self {
            Self::Expr(x) => x.is_constant(),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_) => true,

            #[cfg(feature = "decimal")]
            Self::DecimalConstant(_) => true,

            Self::IntegerConstant(_)
            | Self::CharConstant(_)
            | Self::StringConstant(_)
            | Self::FnPointer(_)
            | Self::True(_)
            | Self::False(_)
            | Self::Unit(_) => true,

            // An array literal is constant if all items are constant
            Self::Array(x) => x.0.iter().all(Self::is_constant),

            // An map literal is constant if all items are constant
            Self::Map(x) => x.0.iter().map(|(_, expr)| expr).all(Self::is_constant),

            // Check in expression
            Self::In(x) => match (&x.0, &x.1) {
                (Self::StringConstant(_), Self::StringConstant(_))
                | (Self::CharConstant(_), Self::StringConstant(_)) => true,
                _ => false,
            },

            _ => false,
        }
    }

    /// Is a particular token allowed as a postfix operator to this expression?
    pub fn is_valid_postfix(&self, token: &Token) -> bool {
        match self {
            Self::Expr(x) => x.is_valid_postfix(token),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_) => false,

            #[cfg(feature = "decimal")]
            Self::DecimalConstant(_) => false,

            Self::IntegerConstant(_)
            | Self::CharConstant(_)
            | Self::FnPointer(_)
            | Self::In(_)
            | Self::And(_)
            | Self::Or(_)
            | Self::True(_)
            | Self::False(_)
            | Self::Unit(_)
            | Self::Assignment(_) => false,

            Self::StringConstant(_)
            | Self::Stmt(_)
            | Self::FnCall(_)
            | Self::Dot(_)
            | Self::Index(_)
            | Self::Array(_)
            | Self::Map(_) => match token {
                #[cfg(not(feature = "no_index"))]
                Token::LeftBracket => true,
                _ => false,
            },

            Self::Variable(_) => match token {
                #[cfg(not(feature = "no_index"))]
                Token::LeftBracket => true,
                Token::LeftParen => true,
                Token::Bang => true,
                Token::DoubleColon => true,
                _ => false,
            },

            Self::Property(_) => match token {
                #[cfg(not(feature = "no_index"))]
                Token::LeftBracket => true,
                Token::LeftParen => true,
                _ => false,
            },

            Self::Custom(_) => false,
        }
    }

    /// Convert a `Variable` into a `Property`.  All other variants are untouched.
    #[cfg(not(feature = "no_object"))]
    pub(crate) fn into_property(self) -> Self {
        match self {
            Self::Variable(x) if x.1.is_none() => {
                let (name, pos) = x.0;
                let getter = make_getter(&name);
                let setter = make_setter(&name);
                Self::Property(Box::new(((name.into(), getter, setter), pos)))
            }
            _ => self,
        }
    }
}

/// Consume a particular token, checking that it is the expected one.
fn eat_token(input: &mut TokenStream, token: Token) -> Position {
    let (t, pos) = input.next().unwrap();

    if t != token {
        unreachable!(
            "expecting {} (found {}) at {}",
            token.syntax(),
            t.syntax(),
            pos
        );
    }
    pos
}

/// Match a particular token, consuming it if matched.
fn match_token(input: &mut TokenStream, token: Token) -> Result<bool, ParseError> {
    let (t, _) = input.peek().unwrap();
    if *t == token {
        eat_token(input, token);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Parse ( expr )
fn parse_paren_expr(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    if match_token(input, Token::RightParen)? {
        return Ok(Expr::Unit(settings.pos));
    }

    let expr = parse_expr(input, state, lib, settings.level_up())?;

    match input.next().unwrap() {
        // ( xxx )
        (Token::RightParen, _) => Ok(expr),
        // ( <error>
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        // ( xxx ???
        (_, pos) => Err(PERR::MissingToken(
            Token::RightParen.into(),
            "for a matching ( in this expression".into(),
        )
        .into_err(pos)),
    }
}

/// Parse a function call.
fn parse_fn_call(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    id: String,
    capture: bool,
    mut modules: Option<Box<ModuleRef>>,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let (token, token_pos) = input.peek().unwrap();

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut args = StaticVec::new();

    match token {
        // id( <EOF>
        Token::EOF => {
            return Err(PERR::MissingToken(
                Token::RightParen.into(),
                format!("to close the arguments list of this function call '{}'", id),
            )
            .into_err(*token_pos))
        }
        // id( <error>
        Token::LexError(err) => return Err(err.into_err(*token_pos)),
        // id()
        Token::RightParen => {
            eat_token(input, Token::RightParen);

            let hash_script = if let Some(modules) = modules.as_mut() {
                modules.set_index(state.find_module(&modules[0].0));

                // Rust functions are indexed in two steps:
                // 1) Calculate a hash in a similar manner to script-defined functions,
                //    i.e. qualifiers + function name + number of arguments.
                // 2) Calculate a second hash with no qualifiers, empty function name,
                //    zero number of arguments, and the actual list of argument `TypeId`'s.
                // 3) The final hash is the XOR of the two hashes.
                let qualifiers = modules.iter().map(|(m, _)| m.as_str());
                calc_fn_hash(qualifiers, &id, 0, empty())
            } else {
                // Qualifiers (none) + function name + no parameters.
                calc_fn_hash(empty(), &id, 0, empty())
            };

            return Ok(Expr::FnCall(Box::new((
                (id.into(), false, capture, settings.pos),
                modules,
                hash_script,
                args,
                None,
            ))));
        }
        // id...
        _ => (),
    }

    let settings = settings.level_up();

    loop {
        match input.peek().unwrap() {
            // id(...args, ) - handle trailing comma
            (Token::RightParen, _) => (),
            _ => args.push(parse_expr(input, state, lib, settings)?),
        }

        match input.peek().unwrap() {
            // id(...args)
            (Token::RightParen, _) => {
                eat_token(input, Token::RightParen);

                let hash_script = if let Some(modules) = modules.as_mut() {
                    modules.set_index(state.find_module(&modules[0].0));

                    // Rust functions are indexed in two steps:
                    // 1) Calculate a hash in a similar manner to script-defined functions,
                    //    i.e. qualifiers + function name + number of arguments.
                    // 2) Calculate a second hash with no qualifiers, empty function name,
                    //    zero number of arguments, and the actual list of argument `TypeId`'s.
                    // 3) The final hash is the XOR of the two hashes.
                    let qualifiers = modules.iter().map(|(m, _)| m.as_str());
                    calc_fn_hash(qualifiers, &id, args.len(), empty())
                } else {
                    // Qualifiers (none) + function name + number of arguments.
                    calc_fn_hash(empty(), &id, args.len(), empty())
                };

                return Ok(Expr::FnCall(Box::new((
                    (id.into(), false, capture, settings.pos),
                    modules,
                    hash_script,
                    args,
                    None,
                ))));
            }
            // id(...args,
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            // id(...args <EOF>
            (Token::EOF, pos) => {
                return Err(PERR::MissingToken(
                    Token::RightParen.into(),
                    format!("to close the arguments list of this function call '{}'", id),
                )
                .into_err(*pos))
            }
            // id(...args <error>
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
            // id(...args ???
            (_, pos) => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    format!("to separate the arguments to function call '{}'", id),
                )
                .into_err(*pos))
            }
        }
    }
}

/// Parse an indexing chain.
/// Indexing binds to the right, so this call parses all possible levels of indexing following in the input.
#[cfg(not(feature = "no_index"))]
fn parse_index_chain(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    lhs: Expr,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let idx_expr = parse_expr(input, state, lib, settings.level_up())?;

    // Check type of indexing - must be integer or string
    match &idx_expr {
        // lhs[int]
        Expr::IntegerConstant(x) if x.0 < 0 => {
            return Err(PERR::MalformedIndexExpr(format!(
                "Array access expects non-negative index: {} < 0",
                x.0
            ))
            .into_err(x.1))
        }
        Expr::IntegerConstant(x) => match lhs {
            Expr::Array(_) | Expr::StringConstant(_) => (),

            Expr::Map(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Object map access expects string index, not a number".into(),
                )
                .into_err(x.1))
            }

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            #[cfg(feature = "decimal")]
            Expr::DecimalConstant(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            Expr::CharConstant(_)
            | Expr::Assignment(_)
            | Expr::And(_)
            | Expr::Or(_)
            | Expr::In(_)
            | Expr::True(_)
            | Expr::False(_)
            | Expr::Unit(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            _ => (),
        },

        // lhs[string]
        Expr::StringConstant(x) => match lhs {
            Expr::Map(_) => (),

            Expr::Array(_) | Expr::StringConstant(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Array or string expects numeric index, not a string".into(),
                )
                .into_err(x.1))
            }

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            #[cfg(feature = "decimal")]
            Expr::DecimalConstant(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            Expr::CharConstant(_)
            | Expr::Assignment(_)
            | Expr::And(_)
            | Expr::Or(_)
            | Expr::In(_)
            | Expr::True(_)
            | Expr::False(_)
            | Expr::Unit(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            _ => (),
        },

        // lhs[float]
        #[cfg(not(feature = "no_float"))]
        x @ Expr::FloatConstant(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a float".into(),
            )
            .into_err(x.position()))
        }
        #[cfg(feature = "decimal")]
        x @ Expr::DecimalConstant(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a decimal".into(),
            )
            .into_err(x.position()))
        }
        // lhs[char]
        x @ Expr::CharConstant(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a character".into(),
            )
            .into_err(x.position()))
        }
        // lhs[??? = ??? ]
        x @ Expr::Assignment(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not an assignment".into(),
            )
            .into_err(x.position()))
        }
        // lhs[()]
        x @ Expr::Unit(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not ()".into(),
            )
            .into_err(x.position()))
        }
        // lhs[??? && ???], lhs[??? || ???], lhs[??? in ???]
        x @ Expr::And(_) | x @ Expr::Or(_) | x @ Expr::In(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // lhs[true], lhs[false]
        x @ Expr::True(_) | x @ Expr::False(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // All other expressions
        _ => (),
    }

    // Check if there is a closing bracket
    match input.peek().unwrap() {
        (Token::RightBracket, _) => {
            eat_token(input, Token::RightBracket);

            // Any more indexing following?
            match input.peek().unwrap() {
                // If another indexing level, right-bind it
                (Token::LeftBracket, _) => {
                    let prev_pos = settings.pos;
                    settings.pos = eat_token(input, Token::LeftBracket);
                    // Recursively parse the indexing chain, right-binding each
                    let idx_expr =
                        parse_index_chain(input, state, lib, idx_expr, settings.level_up())?;
                    // Indexing binds to right
                    Ok(Expr::Index(Box::new((lhs, idx_expr, prev_pos))))
                }
                // Otherwise terminate the indexing chain
                _ => {
                    match idx_expr {
                        // Terminate with an `Expr::Expr` wrapper to prevent the last index expression
                        // inside brackets to be mis-parsed as another level of indexing, or a
                        // dot expression/function call to be mis-parsed as following the indexing chain.
                        Expr::Index(_) | Expr::Dot(_) | Expr::FnCall(_) => Ok(Expr::Index(
                            Box::new((lhs, Expr::Expr(Box::new(idx_expr)), settings.pos)),
                        )),
                        _ => Ok(Expr::Index(Box::new((lhs, idx_expr, settings.pos)))),
                    }
                }
            }
        }
        (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
        (_, pos) => Err(PERR::MissingToken(
            Token::RightBracket.into(),
            "for a matching [ in this index expression".into(),
        )
        .into_err(*pos)),
    }
}

/// Parse an array literal.
#[cfg(not(feature = "no_index"))]
fn parse_array_literal(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut arr = StaticVec::new();

    while !input.peek().unwrap().0.is_eof() {
        #[cfg(not(feature = "unchecked"))]
        if state.engine.limits.max_array_size > 0 && arr.len() >= state.engine.limits.max_array_size
        {
            return Err(PERR::LiteralTooLarge(
                "Size of array literal".to_string(),
                state.engine.limits.max_array_size,
            )
            .into_err(input.peek().unwrap().1));
        }

        match input.peek().unwrap() {
            (Token::RightBracket, _) => {
                eat_token(input, Token::RightBracket);
                break;
            }
            _ => {
                let expr = parse_expr(input, state, lib, settings.level_up())?;
                arr.push(expr);
            }
        }

        match input.peek().unwrap() {
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            (Token::RightBracket, _) => (),
            (Token::EOF, pos) => {
                return Err(PERR::MissingToken(
                    Token::RightBracket.into(),
                    "to end this array literal".into(),
                )
                .into_err(*pos))
            }
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
            (_, pos) => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "to separate the items of this array literal".into(),
                )
                .into_err(*pos))
            }
        };
    }

    Ok(Expr::Array(Box::new((arr, settings.pos))))
}

/// Parse a map literal.
#[cfg(not(feature = "no_object"))]
fn parse_map_literal(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut map = StaticVec::new();

    while !input.peek().unwrap().0.is_eof() {
        const MISSING_RBRACE: &str = "to end this object map literal";

        match input.peek().unwrap() {
            (Token::RightBrace, _) => {
                eat_token(input, Token::RightBrace);
                break;
            }
            _ => (),
        }

        let (name, pos) = match input.next().unwrap() {
            (Token::Identifier(s), pos) => (s, pos),
            (Token::StringConstant(s), pos) => (s, pos),
            (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                return Err(PERR::Reserved(s).into_err(pos));
            }
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (_, pos) if map.is_empty() => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(pos),
                );
            }
            (Token::EOF, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(pos),
                );
            }
            (_, pos) => return Err(PERR::PropertyExpected.into_err(pos)),
        };

        match input.next().unwrap() {
            (Token::Colon, _) => (),
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (_, pos) => {
                return Err(PERR::MissingToken(
                    Token::Colon.into(),
                    format!(
                        "to follow the property '{}' in this object map literal",
                        name
                    ),
                )
                .into_err(pos))
            }
        };

        #[cfg(not(feature = "unchecked"))]
        if state.engine.limits.max_map_size > 0 && map.len() >= state.engine.limits.max_map_size {
            return Err(PERR::LiteralTooLarge(
                "Number of properties in object map literal".to_string(),
                state.engine.limits.max_map_size,
            )
            .into_err(input.peek().unwrap().1));
        }

        let expr = parse_expr(input, state, lib, settings.level_up())?;
        map.push(((Into::<ImmutableString>::into(name), pos), expr));

        match input.peek().unwrap() {
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            (Token::RightBrace, _) => (),
            (Token::Identifier(_), pos) => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "to separate the items of this object map literal".into(),
                )
                .into_err(*pos))
            }
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
            (_, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(*pos),
                )
            }
        }
    }

    // Check for duplicating properties
    map.iter()
        .enumerate()
        .try_for_each(|(i, ((k1, _), _))| {
            map.iter()
                .skip(i + 1)
                .find(|((k2, _), _)| k2 == k1)
                .map_or_else(|| Ok(()), |((k2, pos), _)| Err((k2, *pos)))
        })
        .map_err(|(key, pos)| PERR::DuplicatedProperty(key.to_string()).into_err(pos))?;

    Ok(Expr::Map(Box::new((map, settings.pos))))
}

/// Parse a primary expression.
fn parse_primary(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, _) = match token {
        // { - block statement as expression
        Token::LeftBrace if settings.allow_stmt_expr => {
            return parse_block(input, state, lib, settings.level_up())
                .map(|block| Expr::Stmt(Box::new((block, settings.pos))))
        }
        Token::EOF => return Err(PERR::UnexpectedEOF.into_err(settings.pos)),
        _ => input.next().unwrap(),
    };

    let (next_token, _) = input.peek().unwrap();

    let mut root_expr = match token {
        Token::IntegerConstant(x) => Expr::IntegerConstant(Box::new((x, settings.pos))),
        #[cfg(not(feature = "no_float"))]
        Token::FloatConstant(x) => Expr::FloatConstant(Box::new(FloatWrapper(x, settings.pos))),
        #[cfg(feature = "decimal")]
        Token::DecimalConstant(x) => {
            Expr::DecimalConstant(Box::new((x, settings.pos)))
        }
        Token::CharConstant(c) => Expr::CharConstant(Box::new((c, settings.pos))),
        Token::StringConstant(s) => Expr::StringConstant(Box::new((s.into(), settings.pos))),

        // Function call
        Token::Identifier(s) if *next_token == Token::LeftParen || *next_token == Token::Bang => {
            Expr::Variable(Box::new(((s, settings.pos), None, 0, None)))
        }
        // Normal variable access
        Token::Identifier(s) => {
            let index = state.access_var(&s, settings.pos);
            Expr::Variable(Box::new(((s, settings.pos), None, 0, index)))
        }
        // Function call is allowed to have reserved keyword
        Token::Reserved(s) if *next_token == Token::LeftParen || *next_token == Token::Bang => {
            if is_keyword_function(&s) {
                Expr::Variable(Box::new(((s, settings.pos), None, 0, None)))
            } else {
                return Err(PERR::Reserved(s).into_err(settings.pos));
            }
        }
        // Access to `this` as a variable is OK
        Token::Reserved(s) if s == KEYWORD_THIS && *next_token != Token::LeftParen => {
            if !settings.is_function_scope {
                return Err(
                    PERR::BadInput(format!("'{}' can only be used in functions", s))
                        .into_err(settings.pos),
                );
            } else {
                Expr::Variable(Box::new(((s, settings.pos), None, 0, None)))
            }
        }
        Token::Reserved(s) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(settings.pos));
        }
        Token::LeftParen => parse_paren_expr(input, state, lib, settings.level_up())?,
        #[cfg(not(feature = "no_index"))]
        Token::LeftBracket => parse_array_literal(input, state, lib, settings.level_up())?,
        #[cfg(not(feature = "no_object"))]
        Token::MapStart => parse_map_literal(input, state, lib, settings.level_up())?,
        Token::True => Expr::True(settings.pos),
        Token::False => Expr::False(settings.pos),
        Token::LexError(err) => return Err(err.into_err(settings.pos)),
        _ => {
            return Err(
                PERR::BadInput(format!("Unexpected '{}'", token.syntax())).into_err(settings.pos)
            );
        }
    };

    // Tail processing all possible postfix operators
    loop {
        let (token, _) = input.peek().unwrap();

        if !root_expr.is_valid_postfix(token) {
            break;
        }

        let (token, token_pos) = input.next().unwrap();
        settings.pos = token_pos;

        root_expr = match (root_expr, token) {
            // Function call
            #[cfg(not(feature = "no_closure"))]
            (Expr::Variable(x), Token::Bang) => {
                if !match_token(input, Token::LeftParen)? {
                    return Err(PERR::MissingToken(
                        Token::LeftParen.syntax().into(),
                        "to start arguments list of function call".into(),
                    )
                    .into_err(input.peek().unwrap().1));
                }

                let ((name, pos), modules, _, _) = *x;
                settings.pos = pos;
                parse_fn_call(input, state, lib, name, true, modules, settings.level_up())?
            }
            // Function call
            (Expr::Variable(x), Token::LeftParen) => {
                let ((name, pos), modules, _, _) = *x;
                settings.pos = pos;
                parse_fn_call(input, state, lib, name, false, modules, settings.level_up())?
            }
            (Expr::Property(_), _) => unreachable!(),
            // module access
            (Expr::Variable(x), Token::DoubleColon) => match input.next().unwrap() {
                (Token::Identifier(id2), pos2) => {
                    let ((name, pos), mut modules, _, index) = *x;

                    if let Some(ref mut modules) = modules {
                        modules.push((name, pos));
                    } else {
                        let mut m: ModuleRef = Default::default();
                        m.push((name, pos));
                        modules = Some(Box::new(m));
                    }

                    Expr::Variable(Box::new(((id2, pos2), modules, 0, index)))
                }
                (Token::Reserved(id2), pos2) if is_valid_identifier(id2.chars()) => {
                    return Err(PERR::Reserved(id2).into_err(pos2));
                }
                (_, pos2) => return Err(PERR::VariableExpected.into_err(pos2)),
            },
            // Indexing
            #[cfg(not(feature = "no_index"))]
            (expr, Token::LeftBracket) => {
                parse_index_chain(input, state, lib, expr, settings.level_up())?
            }
            // Unknown postfix operator
            (expr, token) => unreachable!(
                "unknown postfix operator '{}' for {:?}",
                token.syntax(),
                expr
            ),
        }
    }

    match &mut root_expr {
        // Cache the hash key for module-qualified variables
        Expr::Variable(x) if x.1.is_some() => {
            let ((name, _), modules, hash, _) = x.as_mut();
            let modules = modules.as_mut().unwrap();

            // Qualifiers + variable name
            *hash = calc_fn_hash(modules.iter().map(|(v, _)| v.as_str()), name, 0, empty());
            modules.set_index(state.find_module(&modules[0].0));
        }
        _ => (),
    }

    // Make sure identifiers are valid
    Ok(root_expr)
}

/// Parse a potential unary operator.
fn parse_unary(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    match token {
        // If statement is allowed to act as expressions
        Token::If if settings.allow_if_expr => Ok(Expr::Stmt(Box::new((
            parse_if(input, state, lib, settings.level_up())?,
            settings.pos,
        )))),
        // -expr
        Token::UnaryMinus => {
            let pos = eat_token(input, Token::UnaryMinus);

            match parse_unary(input, state, lib, settings.level_up())? {
                // Negative integer
                Expr::IntegerConstant(x) => {
                    let (num, pos) = *x;

                    num.checked_neg()
                        .map(|i| Expr::IntegerConstant(Box::new((i, pos))))
                        .or_else(|| {
                            #[cfg(not(feature = "no_float"))]
                            return Some(Expr::FloatConstant(Box::new(FloatWrapper(
                                -(x.0 as FLOAT),
                                pos,
                            ))));

                            #[cfg(feature = "decimal")]
                            return Some(Expr::DecimalConstant(Box::new((
                                Decimal::from_i64(-x.0).unwrap(),
                                pos,
                            ))));

                            #[cfg(feature = "no_float")]
                            #[cfg(not(feature = "decimal"))]
                            return None;
                        })
                        .ok_or_else(|| LexError::MalformedNumber(format!("-{}", x.0)).into_err(pos))
                }

                // Negative float
                #[cfg(not(feature = "no_float"))]
                Expr::FloatConstant(x) => {
                    Ok(Expr::FloatConstant(Box::new(FloatWrapper(-x.0, x.1))))
                }

                // Negative Decimal
                #[cfg(feature = "decimal")]
                Expr::DecimalConstant(x) => {
                    Ok(Expr::DecimalConstant(Box::new((-x.0, x.1))))
                }

                // Call negative function
                expr => {
                    let op = "-";
                    let hash = calc_fn_hash(empty(), op, 2, empty());
                    let mut args = StaticVec::new();
                    args.push(expr);

                    Ok(Expr::FnCall(Box::new((
                        (op.into(), true, false, pos),
                        None,
                        hash,
                        args,
                        None,
                    ))))
                }
            }
        }
        // +expr
        Token::UnaryPlus => {
            eat_token(input, Token::UnaryPlus);
            parse_unary(input, state, lib, settings.level_up())
        }
        // !expr
        Token::Bang => {
            let pos = eat_token(input, Token::Bang);
            let mut args = StaticVec::new();
            let expr = parse_primary(input, state, lib, settings.level_up())?;
            args.push(expr);

            let op = "!";
            let hash = calc_fn_hash(empty(), op, 2, empty());

            Ok(Expr::FnCall(Box::new((
                (op.into(), true, false, pos),
                None,
                hash,
                args,
                Some(false), // NOT operator, when operating on invalid operand, defaults to false
            ))))
        }
        // | ...
        #[cfg(not(feature = "no_function"))]
        Token::Pipe | Token::Or => {
            let mut new_state = ParseState::new(
                state.engine,
                #[cfg(not(feature = "unchecked"))]
                state.max_function_expr_depth,
                #[cfg(not(feature = "unchecked"))]
                state.max_function_expr_depth,
            );

            let settings = ParseSettings {
                allow_if_expr: true,
                allow_stmt_expr: true,
                allow_anonymous_fn: true,
                is_global: false,
                is_function_scope: true,
                is_breakable: false,
                level: 0,
                pos: *token_pos,
            };

            let (expr, func) = parse_anon_fn(input, &mut new_state, lib, settings)?;

            #[cfg(not(feature = "no_closure"))]
            new_state.externals.iter().for_each(|(closure, pos)| {
                state.access_var(closure, *pos);
            });

            // Qualifiers (none) + function name + number of arguments.
            let hash = calc_fn_hash(empty(), &func.name, func.params.len(), empty());

            lib.insert(hash, func);

            Ok(expr)
        }
        // <EOF>
        Token::EOF => Err(PERR::UnexpectedEOF.into_err(settings.pos)),
        // All other tokens
        _ => parse_primary(input, state, lib, settings.level_up()),
    }
}

fn make_assignment_stmt<'a>(
    fn_name: Cow<'static, str>,
    state: &mut ParseState,
    lhs: Expr,
    rhs: Expr,
    pos: Position,
) -> Result<Expr, ParseError> {
    match &lhs {
        // var (non-indexed) = rhs
        Expr::Variable(x) if x.3.is_none() => {
            Ok(Expr::Assignment(Box::new((lhs, fn_name.into(), rhs, pos))))
        }
        // var (indexed) = rhs
        Expr::Variable(x) => {
            let ((name, name_pos), _, _, index) = x.as_ref();
            match state.stack[(state.stack.len() - index.unwrap().get())].1 {
                ScopeEntryType::Normal => {
                    Ok(Expr::Assignment(Box::new((lhs, fn_name.into(), rhs, pos))))
                }
                // Constant values cannot be assigned to
                ScopeEntryType::Constant => {
                    Err(PERR::AssignmentToConstant(name.clone()).into_err(*name_pos))
                }
            }
        }
        // xxx[???] = rhs, xxx.??? = rhs
        Expr::Index(x) | Expr::Dot(x) => match &x.0 {
            // var[???] (non-indexed) = rhs, var.??? (non-indexed) = rhs
            Expr::Variable(x) if x.3.is_none() => {
                Ok(Expr::Assignment(Box::new((lhs, fn_name.into(), rhs, pos))))
            }
            // var[???] (indexed) = rhs, var.??? (indexed) = rhs
            Expr::Variable(x) => {
                let ((name, name_pos), _, _, index) = x.as_ref();
                match state.stack[(state.stack.len() - index.unwrap().get())].1 {
                    ScopeEntryType::Normal => {
                        Ok(Expr::Assignment(Box::new((lhs, fn_name.into(), rhs, pos))))
                    }
                    // Constant values cannot be assigned to
                    ScopeEntryType::Constant => {
                        Err(PERR::AssignmentToConstant(name.clone()).into_err(*name_pos))
                    }
                }
            }
            // expr[???] = rhs, expr.??? = rhs
            _ => Err(PERR::AssignmentToCopy.into_err(x.0.position())),
        },
        // const_expr = rhs
        expr if expr.is_constant() => {
            Err(PERR::AssignmentToConstant("".into()).into_err(lhs.position()))
        }
        // ??? && ??? = rhs, ??? || ??? = rhs
        Expr::And(_) | Expr::Or(_) => {
            Err(PERR::BadInput("Possibly a typo of '=='?".to_string()).into_err(pos))
        }
        // expr = rhs
        _ => Err(PERR::AssignmentToCopy.into_err(lhs.position())),
    }
}

/// Parse an operator-assignment expression.
fn parse_op_assignment_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    lhs: Expr,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let op = match token {
        Token::Equals => "".into(),

        Token::PlusAssign
        | Token::MinusAssign
        | Token::MultiplyAssign
        | Token::DivideAssign
        | Token::LeftShiftAssign
        | Token::RightShiftAssign
        | Token::ModuloAssign
        | Token::PowerOfAssign
        | Token::AndAssign
        | Token::OrAssign
        | Token::XOrAssign => token.syntax(),

        _ => return Ok(lhs),
    };

    let (_, pos) = input.next().unwrap();
    let rhs = parse_expr(input, state, lib, settings.level_up())?;
    make_assignment_stmt(op, state, lhs, rhs, pos)
}

/// Make a dot expression.
#[cfg(not(feature = "no_object"))]
fn make_dot_expr(lhs: Expr, rhs: Expr, op_pos: Position) -> Result<Expr, ParseError> {
    Ok(match (lhs, rhs) {
        // idx_lhs[idx_expr].rhs
        // Attach dot chain to the bottom level of indexing chain
        (Expr::Index(x), rhs) => {
            let (idx_lhs, idx_expr, pos) = *x;
            Expr::Index(Box::new((
                idx_lhs,
                make_dot_expr(idx_expr, rhs, op_pos)?,
                pos,
            )))
        }
        // lhs.id
        (lhs, Expr::Variable(x)) if x.1.is_none() => {
            let (name, pos) = x.0;

            let getter = make_getter(&name);
            let setter = make_setter(&name);
            let rhs = Expr::Property(Box::new(((name.into(), getter, setter), pos)));

            Expr::Dot(Box::new((lhs, rhs, op_pos)))
        }
        // lhs.module::id - syntax error
        (_, Expr::Variable(x)) if x.1.is_some() => {
            return Err(PERR::PropertyExpected.into_err(x.1.unwrap()[0].1));
        }
        // lhs.prop
        (lhs, prop @ Expr::Property(_)) => Expr::Dot(Box::new((lhs, prop, op_pos))),
        // lhs.dot_lhs.dot_rhs
        (lhs, Expr::Dot(x)) => {
            let (dot_lhs, dot_rhs, pos) = *x;
            Expr::Dot(Box::new((
                lhs,
                Expr::Dot(Box::new((dot_lhs.into_property(), dot_rhs, pos))),
                op_pos,
            )))
        }
        // lhs.idx_lhs[idx_rhs]
        (lhs, Expr::Index(x)) => {
            let (dot_lhs, dot_rhs, pos) = *x;
            Expr::Dot(Box::new((
                lhs,
                Expr::Index(Box::new((dot_lhs.into_property(), dot_rhs, pos))),
                op_pos,
            )))
        }
        // lhs.func!(...)
        (_, Expr::FnCall(x)) if (x.0).2 => {
            return Err(PERR::MalformedCapture(
                "method-call style does not support capturing".into(),
            )
            .into_err((x.0).3))
        }
        // lhs.func(...)
        (lhs, func @ Expr::FnCall(_)) => Expr::Dot(Box::new((lhs, func, op_pos))),
        // lhs.rhs
        (_, rhs) => return Err(PERR::PropertyExpected.into_err(rhs.position())),
    })
}

/// Make an 'in' expression.
fn make_in_expr(lhs: Expr, rhs: Expr, op_pos: Position) -> Result<Expr, ParseError> {
    match (&lhs, &rhs) {
        (_, x @ Expr::IntegerConstant(_))
        | (_, x @ Expr::And(_))
        | (_, x @ Expr::Or(_))
        | (_, x @ Expr::In(_))
        | (_, x @ Expr::Assignment(_))
        | (_, x @ Expr::True(_))
        | (_, x @ Expr::False(_))
        | (_, x @ Expr::Unit(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression expects a string, array or object map".into(),
            )
            .into_err(x.position()))
        }

        #[cfg(not(feature = "no_float"))]
        (_, x @ Expr::FloatConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression expects a string, array or object map".into(),
            )
            .into_err(x.position()))
        }

        #[cfg(feature = "decimal")]
        (_, x @ Expr::DecimalConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression expects a string, array or object map".into(),
            )
            .into_err(x.position()))
        }

        // "xxx" in "xxxx", 'x' in "xxxx" - OK!
        (Expr::StringConstant(_), Expr::StringConstant(_))
        | (Expr::CharConstant(_), Expr::StringConstant(_)) => (),

        // 123.456 in "xxxx"
        #[cfg(not(feature = "no_float"))]
        (x @ Expr::FloatConstant(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a float".into(),
            )
            .into_err(x.position()))
        }

        #[cfg(feature = "decimal")]
        (x @ Expr::DecimalConstant(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a decimal".into(),
            )
            .into_err(x.position()))
        }

        // 123 in "xxxx"
        (x @ Expr::IntegerConstant(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a number".into(),
            )
            .into_err(x.position()))
        }
        // (??? && ???) in "xxxx", (??? || ???) in "xxxx", (??? in ???) in "xxxx",
        //  true in "xxxx", false in "xxxx"
        (x @ Expr::And(_), Expr::StringConstant(_))
        | (x @ Expr::Or(_), Expr::StringConstant(_))
        | (x @ Expr::In(_), Expr::StringConstant(_))
        | (x @ Expr::True(_), Expr::StringConstant(_))
        | (x @ Expr::False(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // [???, ???, ???] in "xxxx"
        (x @ Expr::Array(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not an array".into(),
            )
            .into_err(x.position()))
        }
        // #{...} in "xxxx"
        (x @ Expr::Map(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not an object map".into(),
            )
            .into_err(x.position()))
        }
        // (??? = ???) in "xxxx"
        (x @ Expr::Assignment(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not an assignment".into(),
            )
            .into_err(x.position()))
        }
        // () in "xxxx"
        (x @ Expr::Unit(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not ()".into(),
            )
            .into_err(x.position()))
        }

        // "xxx" in #{...}, 'x' in #{...} - OK!
        (Expr::StringConstant(_), Expr::Map(_)) | (Expr::CharConstant(_), Expr::Map(_)) => (),

        // 123.456 in #{...}
        #[cfg(not(feature = "no_float"))]
        (x @ Expr::FloatConstant(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not a float".into(),
            )
            .into_err(x.position()))
        }

        #[cfg(feature = "decimal")]
        (x @ Expr::DecimalConstant(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not a float".into(),
            )
            .into_err(x.position()))
        }

        // 123 in #{...}
        (x @ Expr::IntegerConstant(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not a number".into(),
            )
            .into_err(x.position()))
        }
        // (??? && ???) in #{...}, (??? || ???) in #{...}, (??? in ???) in #{...},
        // true in #{...}, false in #{...}
        (x @ Expr::And(_), Expr::Map(_))
        | (x @ Expr::Or(_), Expr::Map(_))
        | (x @ Expr::In(_), Expr::Map(_))
        | (x @ Expr::True(_), Expr::Map(_))
        | (x @ Expr::False(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // [???, ???, ???] in #{..}
        (x @ Expr::Array(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not an array".into(),
            )
            .into_err(x.position()))
        }
        // #{...} in #{..}
        (x @ Expr::Map(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not an object map".into(),
            )
            .into_err(x.position()))
        }
        // (??? = ???) in #{...}
        (x @ Expr::Assignment(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not an assignment".into(),
            )
            .into_err(x.position()))
        }
        // () in #{...}
        (x @ Expr::Unit(_), Expr::Map(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not ()".into(),
            )
            .into_err(x.position()))
        }

        _ => (),
    }

    Ok(Expr::In(Box::new((lhs, rhs, op_pos))))
}

/// Parse a binary expression.
fn parse_binary_op(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    parent_precedence: u8,
    lhs: Expr,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    settings.pos = lhs.position();

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut root = lhs;

    loop {
        let (current_op, _) = input.peek().unwrap();
        let custom = state.engine.custom_keywords.as_ref();
        let precedence = current_op.precedence(custom);
        let bind_right = current_op.is_bind_right();

        // Bind left to the parent lhs expression if precedence is higher
        // If same precedence, then check if the operator binds right
        if precedence < parent_precedence || (precedence == parent_precedence && !bind_right) {
            return Ok(root);
        }

        let (op_token, pos) = input.next().unwrap();

        if cfg!(not(feature = "no_object")) && op_token == Token::Period {
            if let (Token::Identifier(_), _) = input.peek().unwrap() {
                // prevents capturing of the object properties as vars: xxx.<var>
                #[cfg(not(feature = "no_closure"))]
                {
                    state.allow_capture = false;
                }
            }
        }

        let rhs = parse_unary(input, state, lib, settings)?;

        let next_precedence = input.peek().unwrap().0.precedence(custom);

        // Bind to right if the next operator has higher precedence
        // If same precedence, then check if the operator binds right
        let rhs = if (precedence == next_precedence && bind_right) || precedence < next_precedence {
            parse_binary_op(input, state, lib, precedence, rhs, settings)?
        } else {
            // Otherwise bind to left (even if next operator has the same precedence)
            rhs
        };

        settings = settings.level_up();
        settings.pos = pos;

        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let cmp_def = Some(false);
        let op = op_token.syntax();
        let hash = calc_fn_hash(empty(), &op, 2, empty());
        let op = (op, true, false, pos);

        let mut args = StaticVec::new();
        args.push(root);
        args.push(rhs);

        root = match op_token {
            Token::Plus
            | Token::Minus
            | Token::Multiply
            | Token::Divide
            | Token::LeftShift
            | Token::RightShift
            | Token::Modulo
            | Token::PowerOf
            | Token::Ampersand
            | Token::Pipe
            | Token::XOr => Expr::FnCall(Box::new((op, None, hash, args, None))),

            // '!=' defaults to true when passed invalid operands
            Token::NotEqualsTo => Expr::FnCall(Box::new((op, None, hash, args, Some(true)))),

            // Comparison operators default to false when passed invalid operands
            Token::EqualsTo
            | Token::LessThan
            | Token::LessThanEqualsTo
            | Token::GreaterThan
            | Token::GreaterThanEqualsTo => Expr::FnCall(Box::new((op, None, hash, args, cmp_def))),

            Token::Or => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                Expr::Or(Box::new((current_lhs, rhs, pos)))
            }
            Token::And => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                Expr::And(Box::new((current_lhs, rhs, pos)))
            }
            Token::In => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                make_in_expr(current_lhs, rhs, pos)?
            }

            #[cfg(not(feature = "no_object"))]
            Token::Period => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                make_dot_expr(current_lhs, rhs, pos)?
            }

            Token::Custom(s)
                if state
                    .engine
                    .custom_keywords
                    .as_ref()
                    .map(|c| c.contains_key(&s))
                    .unwrap_or(false) =>
            {
                // Accept non-native functions for custom operators
                let op = (op.0, false, op.2, op.3);
                Expr::FnCall(Box::new((op, None, hash, args, None)))
            }

            op_token => return Err(PERR::UnknownOperator(op_token.into()).into_err(pos)),
        };
    }
}

/// Parse an expression.
fn parse_expr(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    settings.pos = input.peek().unwrap().1;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // Check if it is a custom syntax.
    if let Some(ref custom) = state.engine.custom_syntax {
        let (token, pos) = input.peek().unwrap();
        let token_pos = *pos;

        match token {
            Token::Custom(key) if custom.contains_key(key) => {
                let custom = custom.get_key_value(key).unwrap();
                let (key, syntax) = custom;

                input.next().unwrap();

                let mut exprs: StaticVec<Expr> = Default::default();

                // Adjust the variables stack
                match syntax.scope_delta {
                    delta if delta > 0 => {
                        state.stack.push(("".to_string(), ScopeEntryType::Normal))
                    }
                    delta if delta < 0 && state.stack.len() <= delta.abs() as usize => {
                        state.stack.clear()
                    }
                    delta if delta < 0 => state
                        .stack
                        .truncate(state.stack.len() - delta.abs() as usize),
                    _ => (),
                }

                for segment in syntax.segments.iter() {
                    settings.pos = input.peek().unwrap().1;
                    let settings = settings.level_up();

                    match segment.as_str() {
                        MARKER_IDENT => match input.next().unwrap() {
                            (Token::Identifier(s), pos) => {
                                exprs.push(Expr::Variable(Box::new(((s, pos), None, 0, None))));
                            }
                            (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                                return Err(PERR::Reserved(s).into_err(pos));
                            }
                            (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
                        },
                        MARKER_EXPR => exprs.push(parse_expr(input, state, lib, settings)?),
                        MARKER_BLOCK => {
                            let stmt = parse_block(input, state, lib, settings)?;
                            let pos = stmt.position();
                            exprs.push(Expr::Stmt(Box::new((stmt, pos))))
                        }
                        s => match input.peek().unwrap() {
                            (t, _) if t.syntax().as_ref() == s => {
                                input.next().unwrap();
                            }
                            (_, pos) => {
                                return Err(PERR::MissingToken(
                                    s.to_string(),
                                    format!("for '{}' expression", key),
                                )
                                .into_err(*pos))
                            }
                        },
                    }
                }

                return Ok(Expr::Custom(Box::new((
                    CustomExpr(exprs, syntax.func.clone()),
                    token_pos,
                ))));
            }
            _ => (),
        }
    }

    // Parse expression normally.
    let lhs = parse_unary(input, state, lib, settings.level_up())?;
    parse_binary_op(input, state, lib, 1, lhs, settings.level_up())
}

/// Make sure that the expression is not a statement expression (i.e. wrapped in `{}`).
fn ensure_not_statement_expr(input: &mut TokenStream, type_name: &str) -> Result<(), ParseError> {
    match input.peek().unwrap() {
        // Disallow statement expressions
        (Token::LeftBrace, pos) | (Token::EOF, pos) => {
            Err(PERR::ExprExpected(type_name.to_string()).into_err(*pos))
        }
        // No need to check for others at this time - leave it for the expr parser
        _ => Ok(()),
    }
}

/// Make sure that the expression is not a mis-typed assignment (i.e. `a = b` instead of `a == b`).
fn ensure_not_assignment(input: &mut TokenStream) -> Result<(), ParseError> {
    match input.peek().unwrap() {
        (Token::Equals, pos) => {
            Err(PERR::BadInput("Possibly a typo of '=='?".to_string()).into_err(*pos))
        }
        (Token::PlusAssign, pos)
        | (Token::MinusAssign, pos)
        | (Token::MultiplyAssign, pos)
        | (Token::DivideAssign, pos)
        | (Token::LeftShiftAssign, pos)
        | (Token::RightShiftAssign, pos)
        | (Token::ModuloAssign, pos)
        | (Token::PowerOfAssign, pos)
        | (Token::AndAssign, pos)
        | (Token::OrAssign, pos)
        | (Token::XOrAssign, pos) => Err(PERR::BadInput(
            "Expecting a boolean expression, not an assignment".to_string(),
        )
        .into_err(*pos)),

        _ => Ok(()),
    }
}

/// Parse an if statement.
fn parse_if(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // if ...
    let token_pos = eat_token(input, Token::If);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // if guard { if_body }
    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, state, lib, settings.level_up())?;
    ensure_not_assignment(input)?;
    let if_body = parse_block(input, state, lib, settings.level_up())?;

    // if guard { if_body } else ...
    let else_body = if match_token(input, Token::Else).unwrap_or(false) {
        Some(if let (Token::If, _) = input.peek().unwrap() {
            // if guard { if_body } else if ...
            parse_if(input, state, lib, settings.level_up())?
        } else {
            // if guard { if_body } else { else-body }
            parse_block(input, state, lib, settings.level_up())?
        })
    } else {
        None
    };

    Ok(Stmt::IfThenElse(Box::new((
        guard, if_body, else_body, token_pos,
    ))))
}

/// Parse a while loop.
fn parse_while(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // while ...
    let token_pos = eat_token(input, Token::While);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // while guard { body }
    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, state, lib, settings.level_up())?;
    ensure_not_assignment(input)?;

    settings.is_breakable = true;
    let body = parse_block(input, state, lib, settings.level_up())?;

    Ok(Stmt::While(Box::new((guard, body, token_pos))))
}

/// Parse a loop statement.
fn parse_loop(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // loop ...
    let token_pos = eat_token(input, Token::Loop);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // loop { body }
    settings.is_breakable = true;
    let body = parse_block(input, state, lib, settings.level_up())?;

    Ok(Stmt::Loop(Box::new((body, token_pos))))
}

/// Parse a for loop.
fn parse_for(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // for ...
    let token_pos = eat_token(input, Token::For);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // for name ...
    let name = match input.next().unwrap() {
        // Variable name
        (Token::Identifier(s), _) => s,
        // Reserved keyword
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        // Bad identifier
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        // Not a variable name
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    // for name in ...
    match input.next().unwrap() {
        (Token::In, _) => (),
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => {
            return Err(
                PERR::MissingToken(Token::In.into(), "after the iteration variable".into())
                    .into_err(pos),
            )
        }
    }

    // for name in expr { body }
    ensure_not_statement_expr(input, "a boolean")?;
    let expr = parse_expr(input, state, lib, settings.level_up())?;

    let prev_stack_len = state.stack.len();
    state.stack.push((name.clone(), ScopeEntryType::Normal));

    settings.is_breakable = true;
    let body = parse_block(input, state, lib, settings.level_up())?;

    state.stack.truncate(prev_stack_len);

    Ok(Stmt::For(Box::new((name, expr, body, token_pos))))
}

/// Parse a variable definition statement.
fn parse_let(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    var_type: ScopeEntryType,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // let/const... (specified in `var_type`)
    let token_pos = input.next().unwrap().1;
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // let name ...
    let (name, pos) = match input.next().unwrap() {
        (Token::Identifier(s), pos) => (s, pos),
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    // let name = ...
    if match_token(input, Token::Equals)? {
        // let name = expr
        let init_value = parse_expr(input, state, lib, settings.level_up())?;

        match var_type {
            // let name = expr
            ScopeEntryType::Normal => {
                state.stack.push((name.clone(), ScopeEntryType::Normal));
                Ok(Stmt::Let(Box::new((
                    (name, pos),
                    Some(init_value),
                    token_pos,
                ))))
            }
            // const name = { expr:constant }
            ScopeEntryType::Constant if init_value.is_constant() => {
                state.stack.push((name.clone(), ScopeEntryType::Constant));
                Ok(Stmt::Const(Box::new(((name, pos), init_value, token_pos))))
            }
            // const name = expr: error
            ScopeEntryType::Constant => {
                Err(PERR::ForbiddenConstantExpr(name).into_err(init_value.position()))
            }
        }
    } else {
        // let name
        match var_type {
            ScopeEntryType::Normal => {
                state.stack.push((name.clone(), ScopeEntryType::Normal));
                Ok(Stmt::Let(Box::new(((name, pos), None, token_pos))))
            }
            ScopeEntryType::Constant => {
                state.stack.push((name.clone(), ScopeEntryType::Constant));
                Ok(Stmt::Const(Box::new((
                    (name, pos),
                    Expr::Unit(pos),
                    token_pos,
                ))))
            }
        }
    }
}

/// Parse an import statement.
#[cfg(not(feature = "no_module"))]
fn parse_import(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // import ...
    let token_pos = eat_token(input, Token::Import);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // import expr ...
    let expr = parse_expr(input, state, lib, settings.level_up())?;

    // import expr as ...
    if !match_token(input, Token::As)? {
        return Ok(Stmt::Import(Box::new((expr, None, token_pos))));
    }

    // import expr as name ...
    let (name, _) = match input.next().unwrap() {
        (Token::Identifier(s), pos) => (s, pos),
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    state.modules.push(name.clone());

    Ok(Stmt::Import(Box::new((
        expr,
        Some((name, settings.pos)),
        token_pos,
    ))))
}

/// Parse an export statement.
#[cfg(not(feature = "no_module"))]
fn parse_export(
    input: &mut TokenStream,
    _state: &mut ParseState,
    _lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let token_pos = eat_token(input, Token::Export);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(_state.max_expr_depth)?;

    let mut exports = StaticVec::new();

    loop {
        let (id, id_pos) = match input.next().unwrap() {
            (Token::Identifier(s), pos) => (s.clone(), pos),
            (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                return Err(PERR::Reserved(s).into_err(pos));
            }
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
        };

        let rename = if match_token(input, Token::As)? {
            match input.next().unwrap() {
                (Token::Identifier(s), pos) => Some((s.clone(), pos)),
                (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                    return Err(PERR::Reserved(s).into_err(pos));
                }
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
            }
        } else {
            None
        };

        exports.push(((id, id_pos), rename));

        match input.peek().unwrap() {
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            (Token::Identifier(_), pos) => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "to separate the list of exports".into(),
                )
                .into_err(*pos))
            }
            _ => break,
        }
    }

    // Check for duplicating parameters
    exports
        .iter()
        .enumerate()
        .try_for_each(|(i, ((id1, _), _))| {
            exports
                .iter()
                .skip(i + 1)
                .find(|((id2, _), _)| id2 == id1)
                .map_or_else(|| Ok(()), |((id2, pos), _)| Err((id2, *pos)))
        })
        .map_err(|(id2, pos)| PERR::DuplicatedExport(id2.to_string()).into_err(pos))?;

    Ok(Stmt::Export(Box::new((exports, token_pos))))
}

/// Parse a statement block.
fn parse_block(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // Must start with {
    settings.pos = match input.next().unwrap() {
        (Token::LeftBrace, pos) => pos,
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => {
            return Err(PERR::MissingToken(
                Token::LeftBrace.into(),
                "to start a statement block".into(),
            )
            .into_err(pos))
        }
    };

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut statements = StaticVec::new();
    let prev_stack_len = state.stack.len();
    let prev_mods_len = state.modules.len();

    while !match_token(input, Token::RightBrace)? {
        // Parse statements inside the block
        settings.is_global = false;

        let stmt = match parse_stmt(input, state, lib, settings.level_up())? {
            Some(s) => s,
            None => continue,
        };

        // See if it needs a terminating semicolon
        let need_semicolon = !stmt.is_self_terminated();

        statements.push(stmt);

        match input.peek().unwrap() {
            // { ... stmt }
            (Token::RightBrace, _) => {
                eat_token(input, Token::RightBrace);
                break;
            }
            // { ... stmt;
            (Token::SemiColon, _) if need_semicolon => {
                eat_token(input, Token::SemiColon);
            }
            // { ... { stmt } ;
            (Token::SemiColon, _) if !need_semicolon => (),
            // { ... { stmt } ???
            (_, _) if !need_semicolon => (),
            // { ... stmt <error>
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
            // { ... stmt ???
            (_, pos) => {
                // Semicolons are not optional between statements
                return Err(PERR::MissingToken(
                    Token::SemiColon.into(),
                    "to terminate this statement".into(),
                )
                .into_err(*pos));
            }
        }
    }

    state.stack.truncate(prev_stack_len);
    state.modules.truncate(prev_mods_len);

    Ok(Stmt::Block(Box::new((statements, settings.pos))))
}

/// Parse an expression as a statement.
fn parse_expr_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    settings.pos = input.peek().unwrap().1;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let expr = parse_expr(input, state, lib, settings.level_up())?;
    let expr = parse_op_assignment_stmt(input, state, lib, expr, settings.level_up())?;
    Ok(Stmt::Expr(Box::new(expr)))
}

/// Parse a single statement.
fn parse_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Option<Stmt>, ParseError> {
    use ScopeEntryType::{Constant, Normal};

    let (token, token_pos) = match input.peek().unwrap() {
        (Token::EOF, pos) => return Ok(Some(Stmt::Noop(*pos))),
        x => x,
    };
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    match token {
        // Semicolon - empty statement
        Token::SemiColon => Ok(Some(Stmt::Noop(settings.pos))),

        Token::LeftBrace => parse_block(input, state, lib, settings.level_up()).map(Some),

        // fn ...
        #[cfg(not(feature = "no_function"))]
        Token::Fn if !settings.is_global => Err(PERR::WrongFnDefinition.into_err(settings.pos)),

        #[cfg(not(feature = "no_function"))]
        Token::Fn | Token::Private => {
            let access = if matches!(token, Token::Private) {
                eat_token(input, Token::Private);
                FnAccess::Private
            } else {
                FnAccess::Public
            };

            match input.next().unwrap() {
                (Token::Fn, pos) => {
                    let mut new_state = ParseState::new(
                        state.engine,
                        #[cfg(not(feature = "unchecked"))]
                        state.max_function_expr_depth,
                        #[cfg(not(feature = "unchecked"))]
                        state.max_function_expr_depth,
                    );

                    let settings = ParseSettings {
                        allow_if_expr: true,
                        allow_stmt_expr: true,
                        allow_anonymous_fn: true,
                        is_global: false,
                        is_function_scope: true,
                        is_breakable: false,
                        level: 0,
                        pos: pos,
                    };

                    let func = parse_fn(input, &mut new_state, lib, access, settings)?;

                    // Qualifiers (none) + function name + number of arguments.
                    let hash = calc_fn_hash(empty(), &func.name, func.params.len(), empty());

                    lib.insert(hash, func);

                    Ok(None)
                }

                (_, pos) => Err(PERR::MissingToken(
                    Token::Fn.into(),
                    format!("following '{}'", Token::Private.syntax()),
                )
                .into_err(pos)),
            }
        }

        Token::If => parse_if(input, state, lib, settings.level_up()).map(Some),
        Token::While => parse_while(input, state, lib, settings.level_up()).map(Some),
        Token::Loop => parse_loop(input, state, lib, settings.level_up()).map(Some),
        Token::For => parse_for(input, state, lib, settings.level_up()).map(Some),

        Token::Continue if settings.is_breakable => {
            let pos = eat_token(input, Token::Continue);
            Ok(Some(Stmt::Continue(pos)))
        }
        Token::Break if settings.is_breakable => {
            let pos = eat_token(input, Token::Break);
            Ok(Some(Stmt::Break(pos)))
        }
        Token::Continue | Token::Break => Err(PERR::LoopBreak.into_err(settings.pos)),

        Token::Return | Token::Throw => {
            let (return_type, token_pos) = input
                .next()
                .map(|(token, pos)| {
                    (
                        match token {
                            Token::Return => ReturnType::Return,
                            Token::Throw => ReturnType::Exception,
                            _ => unreachable!(),
                        },
                        pos,
                    )
                })
                .unwrap();

            match input.peek().unwrap() {
                // `return`/`throw` at <EOF>
                (Token::EOF, pos) => Ok(Some(Stmt::ReturnWithVal(Box::new((
                    (return_type, *pos),
                    None,
                    token_pos,
                ))))),
                // `return;` or `throw;`
                (Token::SemiColon, _) => Ok(Some(Stmt::ReturnWithVal(Box::new((
                    (return_type, settings.pos),
                    None,
                    token_pos,
                ))))),
                // `return` or `throw` with expression
                (_, _) => {
                    let expr = parse_expr(input, state, lib, settings.level_up())?;
                    let pos = expr.position();

                    Ok(Some(Stmt::ReturnWithVal(Box::new((
                        (return_type, pos),
                        Some(expr),
                        token_pos,
                    )))))
                }
            }
        }

        Token::Let => parse_let(input, state, lib, Normal, settings.level_up()).map(Some),
        Token::Const => parse_let(input, state, lib, Constant, settings.level_up()).map(Some),

        #[cfg(not(feature = "no_module"))]
        Token::Import => parse_import(input, state, lib, settings.level_up()).map(Some),

        #[cfg(not(feature = "no_module"))]
        Token::Export if !settings.is_global => Err(PERR::WrongExport.into_err(settings.pos)),

        #[cfg(not(feature = "no_module"))]
        Token::Export => parse_export(input, state, lib, settings.level_up()).map(Some),

        _ => parse_expr_stmt(input, state, lib, settings.level_up()).map(Some),
    }
}

/// Parse a function definition.
#[cfg(not(feature = "no_function"))]
fn parse_fn(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    access: FnAccess,
    mut settings: ParseSettings,
) -> Result<ScriptFnDef, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, pos) = input.next().unwrap();

    let name = token
        .into_function_name_for_override()
        .map_err(|t| match t {
            Token::Reserved(s) => PERR::Reserved(s).into_err(pos),
            _ => PERR::FnMissingName.into_err(pos),
        })?;

    match input.peek().unwrap() {
        (Token::LeftParen, _) => eat_token(input, Token::LeftParen),
        (_, pos) => return Err(PERR::FnMissingParams(name).into_err(*pos)),
    };

    let mut params = Vec::new();

    if !match_token(input, Token::RightParen)? {
        let sep_err = format!("to separate the parameters of function '{}'", name);

        loop {
            match input.next().unwrap() {
                (Token::RightParen, _) => break,
                (Token::Identifier(s), pos) => {
                    state.stack.push((s.clone(), ScopeEntryType::Normal));
                    params.push((s, pos))
                }
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightParen.into(),
                        format!("to close the parameters list of function '{}'", name),
                    )
                    .into_err(pos))
                }
            }

            match input.next().unwrap() {
                (Token::RightParen, _) => break,
                (Token::Comma, _) => (),
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => {
                    return Err(PERR::MissingToken(Token::Comma.into(), sep_err).into_err(pos))
                }
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
    let body = match input.peek().unwrap() {
        (Token::LeftBrace, _) => {
            settings.is_breakable = false;
            parse_block(input, state, lib, settings.level_up())?
        }
        (_, pos) => return Err(PERR::FnMissingBody(name).into_err(*pos)),
    };

    let params: StaticVec<_> = params.into_iter().map(|(p, _)| p).collect();

    #[cfg(not(feature = "no_closure"))]
    let externals = state
        .externals
        .iter()
        .map(|(name, _)| name)
        .filter(|name| !params.contains(name))
        .cloned()
        .collect();

    Ok(ScriptFnDef {
        name: name.into(),
        access,
        params,
        #[cfg(not(feature = "no_closure"))]
        externals,
        body,
        pos: settings.pos,
    })
}

/// Creates a curried expression from a list of external variables
#[cfg(not(feature = "no_function"))]
fn make_curry_from_externals(
    fn_expr: Expr,
    externals: StaticVec<(String, Position)>,
    pos: Position,
) -> Expr {
    if externals.is_empty() {
        return fn_expr;
    }

    let num_externals = externals.len();
    let mut args: StaticVec<_> = Default::default();

    #[cfg(not(feature = "no_closure"))]
    externals.iter().for_each(|(var_name, pos)| {
        args.push(Expr::Variable(Box::new((
            (var_name.into(), *pos),
            None,
            0,
            None,
        ))));
    });

    #[cfg(feature = "no_closure")]
    externals.into_iter().for_each(|(var_name, pos)| {
        args.push(Expr::Variable(Box::new(((var_name, pos), None, 0, None))));
    });

    let hash = calc_fn_hash(empty(), KEYWORD_FN_PTR_CURRY, num_externals, empty());

    let fn_call = Expr::FnCall(Box::new((
        (KEYWORD_FN_PTR_CURRY.into(), false, false, pos),
        None,
        hash,
        args,
        None,
    )));

    let expr = Expr::Dot(Box::new((fn_expr, fn_call, pos)));

    // If there are captured variables, convert the entire expression into a statement block,
    // then insert the relevant `Share` statements.
    #[cfg(not(feature = "no_closure"))]
    {
        // Statement block
        let mut statements: StaticVec<_> = Default::default();
        // Insert `Share` statements
        statements.extend(externals.into_iter().map(|x| Stmt::Share(Box::new(x))));
        // Final expression
        statements.push(Stmt::Expr(Box::new(expr)));
        Expr::Stmt(Box::new((Stmt::Block(Box::new((statements, pos))), pos)))
    }

    #[cfg(feature = "no_closure")]
    return expr;
}

/// Parse an anonymous function definition.
#[cfg(not(feature = "no_function"))]
fn parse_anon_fn(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<(Expr, ScriptFnDef), ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut params = Vec::new();

    if input.next().unwrap().0 != Token::Or {
        if !match_token(input, Token::Pipe)? {
            loop {
                match input.next().unwrap() {
                    (Token::Pipe, _) => break,
                    (Token::Identifier(s), pos) => {
                        state.stack.push((s.clone(), ScopeEntryType::Normal));
                        params.push((s, pos))
                    }
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (_, pos) => {
                        return Err(PERR::MissingToken(
                            Token::Pipe.into(),
                            "to close the parameters list of anonymous function".into(),
                        )
                        .into_err(pos))
                    }
                }

                match input.next().unwrap() {
                    (Token::Pipe, _) => break,
                    (Token::Comma, _) => (),
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (_, pos) => {
                        return Err(PERR::MissingToken(
                            Token::Comma.into(),
                            "to separate the parameters of anonymous function".into(),
                        )
                        .into_err(pos))
                    }
                }
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
        .map_err(|(p, pos)| PERR::FnDuplicatedParam("".to_string(), p.to_string()).into_err(pos))?;

    // Parse function body
    settings.is_breakable = false;
    let pos = input.peek().unwrap().1;
    let body = parse_stmt(input, state, lib, settings.level_up())
        .map(|stmt| stmt.unwrap_or_else(|| Stmt::Noop(pos)))?;

    // External variables may need to be processed in a consistent order,
    // so extract them into a list.
    let externals: StaticVec<_> = {
        #[cfg(not(feature = "no_closure"))]
        {
            state
                .externals
                .iter()
                .map(|(k, &v)| (k.clone(), v))
                .collect()
        }
        #[cfg(feature = "no_closure")]
        Default::default()
    };

    let params: StaticVec<_> = if cfg!(not(feature = "no_closure")) {
        externals
            .iter()
            .map(|(k, _)| k)
            .cloned()
            .chain(params.into_iter().map(|(v, _)| v))
            .collect()
    } else {
        params.into_iter().map(|(v, _)| v).collect()
    };

    // Calculate hash
    #[cfg(feature = "no_std")]
    let mut s: AHasher = Default::default();
    #[cfg(not(feature = "no_std"))]
    let mut s = DefaultHasher::new();

    s.write_usize(params.len());
    params.iter().for_each(|a| a.hash(&mut s));
    body.hash(&mut s);
    let hash = s.finish();

    // Create unique function name
    let fn_name: ImmutableString = format!("{}{:016x}", FN_ANONYMOUS, hash).into();

    // Define the function
    let script = ScriptFnDef {
        name: fn_name.clone(),
        access: FnAccess::Public,
        params,
        #[cfg(not(feature = "no_closure"))]
        externals: Default::default(),
        body,
        pos: settings.pos,
    };

    let expr = Expr::FnPointer(Box::new((fn_name, settings.pos)));

    let expr = if cfg!(not(feature = "no_closure")) {
        make_curry_from_externals(expr, externals, settings.pos)
    } else {
        expr
    };

    Ok((expr, script))
}

impl Engine {
    pub(crate) fn parse_global_expr(
        &self,
        input: &mut TokenStream,
        scope: &Scope,
        optimization_level: OptimizationLevel,
    ) -> Result<AST, ParseError> {
        let mut functions = Default::default();
        let mut state = ParseState::new(
            self,
            #[cfg(not(feature = "unchecked"))]
            self.limits.max_expr_depth,
            #[cfg(not(feature = "unchecked"))]
            self.limits.max_function_expr_depth,
        );

        let settings = ParseSettings {
            allow_if_expr: false,
            allow_stmt_expr: false,
            allow_anonymous_fn: false,
            is_global: true,
            is_function_scope: false,
            is_breakable: false,
            level: 0,
            pos: Position::none(),
        };
        let expr = parse_expr(input, &mut state, &mut functions, settings)?;

        match input.peek().unwrap() {
            (Token::EOF, _) => (),
            // Return error if the expression doesn't end
            (token, pos) => {
                return Err(
                    PERR::BadInput(format!("Unexpected '{}'", token.syntax())).into_err(*pos)
                )
            }
        }

        let expr = vec![Stmt::Expr(Box::new(expr))];

        Ok(
            // Optimize AST
            optimize_into_ast(self, scope, expr, Default::default(), optimization_level),
        )
    }

    /// Parse the global level statements.
    fn parse_global_level(
        &self,
        input: &mut TokenStream,
    ) -> Result<(Vec<Stmt>, Vec<ScriptFnDef>), ParseError> {
        let mut statements: Vec<Stmt> = Default::default();
        let mut functions = Default::default();
        let mut state = ParseState::new(
            self,
            #[cfg(not(feature = "unchecked"))]
            self.limits.max_expr_depth,
            #[cfg(not(feature = "unchecked"))]
            self.limits.max_function_expr_depth,
        );

        while !input.peek().unwrap().0.is_eof() {
            let settings = ParseSettings {
                allow_if_expr: true,
                allow_stmt_expr: true,
                allow_anonymous_fn: true,
                is_global: true,
                is_function_scope: false,
                is_breakable: false,
                level: 0,
                pos: Position::none(),
            };

            let stmt = match parse_stmt(input, &mut state, &mut functions, settings)? {
                Some(s) => s,
                None => continue,
            };

            let need_semicolon = !stmt.is_self_terminated();

            statements.push(stmt);

            match input.peek().unwrap() {
                // EOF
                (Token::EOF, _) => break,
                // stmt ;
                (Token::SemiColon, _) if need_semicolon => {
                    eat_token(input, Token::SemiColon);
                }
                // stmt ;
                (Token::SemiColon, _) if !need_semicolon => (),
                // { stmt } ???
                (_, _) if !need_semicolon => (),
                // stmt <error>
                (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
                // stmt ???
                (_, pos) => {
                    // Semicolons are not optional between statements
                    return Err(PERR::MissingToken(
                        Token::SemiColon.into(),
                        "to terminate this statement".into(),
                    )
                    .into_err(*pos));
                }
            }
        }

        Ok((statements, functions.into_iter().map(|(_, v)| v).collect()))
    }

    /// Run the parser on an input stream, returning an AST.
    pub(crate) fn parse(
        &self,
        input: &mut TokenStream,
        scope: &Scope,
        optimization_level: OptimizationLevel,
    ) -> Result<AST, ParseError> {
        let (statements, lib) = self.parse_global_level(input)?;

        Ok(
            // Optimize AST
            optimize_into_ast(self, scope, statements, lib, optimization_level),
        )
    }
}

/// Map a `Dynamic` value to an expression.
///
/// Returns Some(expression) if conversion is successful.  Otherwise None.
pub fn map_dynamic_to_expr(value: Dynamic, pos: Position) -> Option<Expr> {
    match value.0 {
        #[cfg(not(feature = "no_float"))]
        Union::Float(value) => Some(Expr::FloatConstant(Box::new(FloatWrapper(value, pos)))),

        #[cfg(feature = "decimal")]
        Union::Decimal(value) => Some(Expr::DecimalConstant(Box::new((*value, pos)))),

        Union::Unit(_) => Some(Expr::Unit(pos)),
        Union::Int(value) => Some(Expr::IntegerConstant(Box::new((value, pos)))),
        Union::Char(value) => Some(Expr::CharConstant(Box::new((value, pos)))),
        Union::Str(value) => Some(Expr::StringConstant(Box::new((value.clone(), pos)))),
        Union::Bool(true) => Some(Expr::True(pos)),
        Union::Bool(false) => Some(Expr::False(pos)),
        #[cfg(not(feature = "no_index"))]
        Union::Array(array) => {
            let items: Vec<_> = array
                .into_iter()
                .map(|x| map_dynamic_to_expr(x, pos))
                .collect();

            if items.iter().all(Option::is_some) {
                Some(Expr::Array(Box::new((
                    items.into_iter().map(Option::unwrap).collect(),
                    pos,
                ))))
            } else {
                None
            }
        }
        #[cfg(not(feature = "no_object"))]
        Union::Map(map) => {
            let items: Vec<_> = map
                .into_iter()
                .map(|(k, v)| ((k, pos), map_dynamic_to_expr(v, pos)))
                .collect();

            if items.iter().all(|(_, expr)| expr.is_some()) {
                Some(Expr::Map(Box::new((
                    items
                        .into_iter()
                        .map(|((k, pos), expr)| ((k, pos), expr.unwrap()))
                        .collect(),
                    pos,
                ))))
            } else {
                None
            }
        }

        _ => None,
    }
}
