//! Module defining the AST (abstract syntax tree).

use crate::dynamic::{Dynamic, Union};
use crate::fn_native::{FnPtr, Shared};
use crate::module::{Module, ModuleRef};
use crate::syntax::FnCustomSyntaxEval;
use crate::token::{Position, Token, NO_POS};
use crate::utils::ImmutableString;
use crate::StaticVec;
use crate::INT;

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_index"))]
use crate::engine::Array;

#[cfg(not(feature = "no_object"))]
use crate::engine::{make_getter, make_setter, Map};

use crate::stdlib::{
    any::TypeId,
    borrow::Cow,
    boxed::Box,
    fmt,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    ops::{Add, AddAssign},
    string::String,
    vec,
    vec::Vec,
};

#[cfg(not(feature = "no_float"))]
use crate::stdlib::ops::Neg;

use crate::stdlib::collections::HashSet;

/// A type representing the access mode of a scripted function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum FnAccess {
    /// Public function.
    Public,
    /// Private function.
    Private,
}

impl fmt::Display for FnAccess {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Private => write!(f, "private"),
            Self::Public => write!(f, "public"),
        }
    }
}

impl FnAccess {
    /// Is this access mode private?
    #[inline(always)]
    pub fn is_private(self) -> bool {
        match self {
            Self::Public => false,
            Self::Private => true,
        }
    }
    /// Is this access mode public?
    #[inline(always)]
    pub fn is_public(self) -> bool {
        match self {
            Self::Public => true,
            Self::Private => false,
        }
    }
}

/// _[INTERNALS]_ A type containing information on a scripted function.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone)]
pub struct ScriptFnDef {
    /// Function body.
    pub body: Stmt,
    /// Encapsulated running environment, if any.
    pub lib: Option<Shared<Module>>,
    /// Function name.
    pub name: ImmutableString,
    /// Function access mode.
    pub access: FnAccess,
    /// Names of function parameters.
    pub params: StaticVec<String>,
    /// Access to external variables. Boxed because it occurs rarely.
    #[cfg(not(feature = "no_closure"))]
    pub externals: Option<Box<HashSet<String>>>,
}

impl fmt::Display for ScriptFnDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}({})",
            if self.access.is_private() {
                "private "
            } else {
                ""
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

/// Compiled AST (abstract syntax tree) of a Rhai script.
///
/// # Thread Safety
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
    #[inline(always)]
    pub fn new(statements: Vec<Stmt>, lib: Module) -> Self {
        Self(statements, lib)
    }

    /// Get the statements.
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    pub(crate) fn statements(&self) -> &[Stmt] {
        &self.0
    }

    /// _[INTERNALS]_ Get the statements.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[deprecated(note = "this method is volatile and may change")]
    #[inline(always)]
    pub fn statements(&self) -> &[Stmt] {
        &self.0
    }

    /// Get a mutable reference to the statements.
    #[cfg(not(feature = "no_optimize"))]
    #[inline(always)]
    pub(crate) fn statements_mut(&mut self) -> &mut Vec<Stmt> {
        &mut self.0
    }

    /// Get the internal `Module` containing all script-defined functions.
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    pub(crate) fn lib(&self) -> &Module {
        &self.1
    }

    /// _[INTERNALS]_ Get the internal `Module` containing all script-defined functions.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[deprecated(note = "this method is volatile and may change")]
    #[inline(always)]
    pub fn lib(&self) -> &Module {
        &self.1
    }

    /// Clone the `AST`'s functions into a new `AST`.
    /// No statements are cloned.
    ///
    /// This operation is cheap because functions are shared.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn clone_functions_only(&self) -> Self {
        self.clone_functions_only_filtered(|_, _, _| true)
    }

    /// Clone the `AST`'s functions into a new `AST` based on a filter predicate.
    /// No statements are cloned.
    ///
    /// This operation is cheap because functions are shared.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn clone_functions_only_filtered(
        &self,
        mut filter: impl FnMut(FnAccess, &str, usize) -> bool,
    ) -> Self {
        let mut functions: Module = Default::default();
        functions.merge_filtered(&self.1, &mut filter);
        Self(Default::default(), functions)
    }

    /// Clone the `AST`'s script statements into a new `AST`.
    /// No functions are cloned.
    #[inline(always)]
    pub fn clone_statements_only(&self) -> Self {
        Self(self.0.clone(), Default::default())
    }

    /// Merge two `AST` into one.  Both `AST`'s are untouched and a new, merged, version
    /// is returned.
    ///
    /// Statements in the second `AST` are simply appended to the end of the first _without any processing_.
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
    #[inline(always)]
    pub fn merge(&self, other: &Self) -> Self {
        self.merge_filtered(other, |_, _, _| true)
    }

    /// Combine one `AST` with another.  The second `AST` is consumed.
    ///
    /// Statements in the second `AST` are simply appended to the end of the first _without any processing_.
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
    /// let mut ast1 = engine.compile(r#"
    ///                     fn foo(x) { 42 + x }
    ///                     foo(1)
    ///                 "#)?;
    ///
    /// let ast2 = engine.compile(r#"
    ///                 fn foo(n) { "hello" + n }
    ///                 foo("!")
    ///             "#)?;
    ///
    /// ast1.combine(ast2);    // Combine 'ast2' into 'ast1'
    ///
    /// // Notice that using the '+=' operator also works:
    /// // ast1 += ast2;
    ///
    /// // 'ast1' is essentially:
    /// //
    /// //    fn foo(n) { "hello" + n } // <- definition of first 'foo' is overwritten
    /// //    foo(1)                    // <- notice this will be "hello1" instead of 43,
    /// //                              //    but it is no longer the return value
    /// //    foo("!")                  // returns "hello!"
    ///
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast::<String>(&ast1)?, "hello!");
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn combine(&mut self, other: Self) -> &mut Self {
        self.combine_filtered(other, |_, _, _| true)
    }

    /// Merge two `AST` into one.  Both `AST`'s are untouched and a new, merged, version
    /// is returned.
    ///
    /// Statements in the second `AST` are simply appended to the end of the first _without any processing_.
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
    #[inline]
    pub fn merge_filtered(
        &self,
        other: &Self,
        mut filter: impl FnMut(FnAccess, &str, usize) -> bool,
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
        functions.merge_filtered(&other.1, &mut filter);

        Self::new(ast, functions)
    }

    /// Combine one `AST` with another.  The second `AST` is consumed.
    ///
    /// Statements in the second `AST` are simply appended to the end of the first _without any processing_.
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
    /// let mut ast1 = engine.compile(r#"
    ///                     fn foo(x) { 42 + x }
    ///                     foo(1)
    ///                 "#)?;
    ///
    /// let ast2 = engine.compile(r#"
    ///                 fn foo(n) { "hello" + n }
    ///                 fn error() { 0 }
    ///                 foo("!")
    ///             "#)?;
    ///
    /// // Combine 'ast2', picking only 'error()' but not 'foo(_)', into 'ast1'
    /// ast1.combine_filtered(ast2, |_, name, params| name == "error" && params == 0);
    ///
    /// // 'ast1' is essentially:
    /// //
    /// //    fn foo(n) { 42 + n }      // <- definition of 'ast1::foo' is not overwritten
    /// //                              //    because 'ast2::foo' is filtered away
    /// //    foo(1)                    // <- notice this will be 43 instead of "hello1",
    /// //                              //    but it is no longer the return value
    /// //    fn error() { 0 }          // <- this function passes the filter and is merged
    /// //    foo("!")                  // <- returns "42!"
    ///
    /// // Evaluate it
    /// assert_eq!(engine.eval_ast::<String>(&ast1)?, "42!");
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn combine_filtered(
        &mut self,
        other: Self,
        mut filter: impl FnMut(FnAccess, &str, usize) -> bool,
    ) -> &mut Self {
        let Self(ref mut statements, ref mut functions) = self;
        statements.extend(other.0.into_iter());
        functions.merge_filtered(&other.1, &mut filter);
        self
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
    #[inline(always)]
    pub fn retain_functions(&mut self, filter: impl FnMut(FnAccess, &str, usize) -> bool) {
        self.1.retain_functions(filter);
    }

    /// Iterate through all functions
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn iter_functions<'a>(
        &'a self,
    ) -> impl Iterator<Item = (FnAccess, &str, usize, Shared<ScriptFnDef>)> + 'a {
        self.1.iter_script_fn()
    }

    /// Clear all function definitions in the `AST`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn clear_functions(&mut self) {
        self.1 = Default::default();
    }

    /// Clear all statements in the `AST`, leaving only function definitions.
    #[inline(always)]
    pub fn clear_statements(&mut self) {
        self.0 = vec![];
    }

    /// Extract all referenced variables, but not the variables defined in the script itself
    pub fn extract_variables(&self) -> HashSet<String> {
        let mut vars = HashSet::new();
        let mut defs = HashSet::new();
        for stmt in self.0.iter() {
            extract_stmt_variables(stmt, &mut defs, &mut vars);
        }
        defs.iter().for_each(|n| {
            vars.remove(n);
        });
        vars
    }
}

impl<A: AsRef<AST>> Add<A> for &AST {
    type Output = AST;

    #[inline(always)]
    fn add(self, rhs: A) -> Self::Output {
        self.merge(rhs.as_ref())
    }
}

impl<A: Into<AST>> AddAssign<A> for AST {
    #[inline(always)]
    fn add_assign(&mut self, rhs: A) {
        self.combine(rhs.into());
    }
}

impl AsRef<[Stmt]> for AST {
    #[inline(always)]
    fn as_ref(&self) -> &[Stmt] {
        self.statements()
    }
}

impl AsRef<Module> for AST {
    #[inline(always)]
    fn as_ref(&self) -> &Module {
        self.lib()
    }
}

/// An identifier containing a string name and a position.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Ident {
    pub name: String,
    pub pos: Position,
}

impl Ident {
    /// Create a new `Identifier`.
    pub fn new(name: String, pos: Position) -> Self {
        Self { name, pos }
    }
}

/// An identifier containing an immutable name and a position.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IdentX {
    pub name: ImmutableString,
    pub pos: Position,
}

impl From<Ident> for IdentX {
    fn from(value: Ident) -> Self {
        Self {
            name: value.name.into(),
            pos: value.pos,
        }
    }
}

impl IdentX {
    /// Create a new `Identifier`.
    pub fn new(name: impl Into<ImmutableString>, pos: Position) -> Self {
        Self {
            name: name.into(),
            pos,
        }
    }
}

/// _[INTERNALS]_ A type encapsulating the mode of a `return`/`throw` statement.
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

/// _[INTERNALS]_ A statement.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash)]
pub enum Stmt {
    /// No-op.
    Noop(Position),
    /// if expr { stmt } else { stmt }
    IfThenElse(Expr, Box<(Stmt, Option<Stmt>)>, Position),
    /// while expr { stmt }
    While(Expr, Box<Stmt>, Position),
    /// loop { stmt }
    Loop(Box<Stmt>, Position),
    /// for id in expr { stmt }
    For(Expr, Box<(String, Stmt)>, Position),
    /// let id = expr
    Let(Box<Ident>, Option<Expr>, Position),
    /// const id = expr
    Const(Box<Ident>, Option<Expr>, Position),
    /// expr op= expr
    Assignment(Box<(Expr, Cow<'static, str>, Expr)>, Position),
    /// { stmt; ... }
    Block(Vec<Stmt>, Position),
    /// try { stmt; ... } catch ( var ) { stmt; ... }
    TryCatch(Box<(Stmt, Option<Ident>, Stmt)>, Position, Position),
    /// expr
    Expr(Expr),
    /// continue
    Continue(Position),
    /// break
    Break(Position),
    /// return/throw
    ReturnWithVal((ReturnType, Position), Option<Expr>, Position),
    /// import expr as var
    #[cfg(not(feature = "no_module"))]
    Import(Expr, Option<Box<IdentX>>, Position),
    /// export var as var, ...
    #[cfg(not(feature = "no_module"))]
    Export(Vec<(Ident, Option<Ident>)>, Position),
    /// Convert a variable to shared.
    #[cfg(not(feature = "no_closure"))]
    Share(Box<Ident>),
}

impl Default for Stmt {
    #[inline(always)]
    fn default() -> Self {
        Self::Noop(NO_POS)
    }
}

impl Stmt {
    /// Is this statement `Noop`?
    pub fn is_noop(&self) -> bool {
        match self {
            Self::Noop(_) => true,
            _ => false,
        }
    }

    /// Get the `Position` of this statement.
    pub fn position(&self) -> Position {
        match self {
            Self::Noop(pos)
            | Self::Continue(pos)
            | Self::Break(pos)
            | Self::Block(_, pos)
            | Self::Assignment(_, pos)
            | Self::IfThenElse(_, _, pos)
            | Self::While(_, _, pos)
            | Self::Loop(_, pos)
            | Self::For(_, _, pos)
            | Self::ReturnWithVal((_, pos), _, _) => *pos,

            Self::Let(x, _, _) | Self::Const(x, _, _) => x.pos,
            Self::TryCatch(_, pos, _) => *pos,

            Self::Expr(x) => x.position(),

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, pos) => *pos,
            #[cfg(not(feature = "no_module"))]
            Self::Export(_, pos) => *pos,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(x) => x.pos,
        }
    }

    /// Override the `Position` of this statement.
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            Self::Noop(pos)
            | Self::Continue(pos)
            | Self::Break(pos)
            | Self::Block(_, pos)
            | Self::Assignment(_, pos)
            | Self::IfThenElse(_, _, pos)
            | Self::While(_, _, pos)
            | Self::Loop(_, pos)
            | Self::For(_, _, pos)
            | Self::ReturnWithVal((_, pos), _, _) => *pos = new_pos,

            Self::Let(x, _, _) | Self::Const(x, _, _) => x.pos = new_pos,
            Self::TryCatch(_, pos, _) => *pos = new_pos,

            Self::Expr(x) => {
                x.set_position(new_pos);
            }

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, pos) => *pos = new_pos,
            #[cfg(not(feature = "no_module"))]
            Self::Export(_, pos) => *pos = new_pos,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(x) => x.pos = new_pos,
        }

        self
    }

    /// Is this statement self-terminated (i.e. no need for a semicolon terminator)?
    pub fn is_self_terminated(&self) -> bool {
        match self {
            Self::IfThenElse(_, _, _)
            | Self::While(_, _, _)
            | Self::Loop(_, _)
            | Self::For(_, _, _)
            | Self::Block(_, _)
            | Self::TryCatch(_, _, _) => true,

            // A No-op requires a semicolon in order to know it is an empty statement!
            Self::Noop(_) => false,

            Self::Let(_, _, _)
            | Self::Const(_, _, _)
            | Self::Assignment(_, _)
            | Self::Expr(_)
            | Self::Continue(_)
            | Self::Break(_)
            | Self::ReturnWithVal(_, _, _) => false,

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, _) | Self::Export(_, _) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(_) => false,
        }
    }

    /// Is this statement _pure_?
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Noop(_) => true,
            Self::Expr(expr) => expr.is_pure(),
            Self::IfThenElse(condition, x, _) if x.1.is_some() => {
                condition.is_pure() && x.0.is_pure() && x.1.as_ref().unwrap().is_pure()
            }
            Self::IfThenElse(condition, x, _) => condition.is_pure() && x.0.is_pure(),
            Self::While(condition, block, _) => condition.is_pure() && block.is_pure(),
            Self::Loop(block, _) => block.is_pure(),
            Self::For(iterable, x, _) => iterable.is_pure() && x.1.is_pure(),
            Self::Let(_, _, _) | Self::Const(_, _, _) | Self::Assignment(_, _) => false,
            Self::Block(block, _) => block.iter().all(|stmt| stmt.is_pure()),
            Self::Continue(_) | Self::Break(_) | Self::ReturnWithVal(_, _, _) => false,
            Self::TryCatch(x, _, _) => x.0.is_pure() && x.2.is_pure(),

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, _) => false,
            #[cfg(not(feature = "no_module"))]
            Self::Export(_, _) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(_) => false,
        }
    }
}

/// _[INTERNALS]_ A type wrapping a custom syntax definition.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Clone)]
pub struct CustomExpr {
    /// List of keywords.
    pub(crate) keywords: StaticVec<Expr>,
    /// Implementation function.
    pub(crate) func: Shared<FnCustomSyntaxEval>,
}

impl fmt::Debug for CustomExpr {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.keywords, f)
    }
}

impl Hash for CustomExpr {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.keywords.hash(state);
    }
}

impl CustomExpr {
    /// Get the keywords for this `CustomExpr`.
    #[inline(always)]
    pub fn keywords(&self) -> &[Expr] {
        &self.keywords
    }
    /// Get the implementation function for this `CustomExpr`.
    #[inline(always)]
    pub fn func(&self) -> &FnCustomSyntaxEval {
        self.func.as_ref()
    }
}

/// _[INTERNALS]_ A type wrapping a floating-point number.
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
pub struct FloatWrapper(pub FLOAT);

#[cfg(not(feature = "no_float"))]
impl Hash for FloatWrapper {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0.to_le_bytes());
    }
}

#[cfg(not(feature = "no_float"))]
impl Neg for FloatWrapper {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

#[cfg(not(feature = "no_float"))]
impl From<INT> for FloatWrapper {
    fn from(value: INT) -> Self {
        Self(value as FLOAT)
    }
}

/// A binary expression structure.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash)]
pub struct BinaryExpr {
    /// LHS expression.
    pub lhs: Expr,
    /// RHS expression.
    pub rhs: Expr,
}

/// _[INTERNALS]_ A function call.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash, Default)]
pub struct FnCallInfo {
    /// Pre-calculated hash for a script-defined function of the same name and number of parameters.
    pub hash: u64,
    /// Call native functions only? Set to `true` to skip searching for script-defined function overrides
    /// when it is certain that the function must be native (e.g. an operator).
    pub native_only: bool,
    /// Does this function call capture the parent scope?
    pub capture: bool,
    /// Default value when the function is not found, mostly used to provide a default for comparison functions.
    /// Type is `bool` in order for `FnCallInfo` to be `Hash`
    pub def_value: Option<bool>,
    /// Namespace of the function, if any. Boxed because it occurs rarely.
    pub namespace: Option<Box<ModuleRef>>,
    /// Function name.
    /// Use `Cow<'static, str>` because a lot of operators (e.g. `==`, `>=`) are implemented as function calls
    /// and the function names are predictable, so no need to allocate a new `String`.
    pub name: Cow<'static, str>,
    /// List of function call arguments.
    pub args: StaticVec<Expr>,
}

/// _[INTERNALS]_ An expression sub-tree.
/// Exported under the `internals` feature only.
///
/// ## WARNING
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash)]
pub enum Expr {
    /// Integer constant.
    IntegerConstant(INT, Position),
    /// Floating-point constant.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(FloatWrapper, Position),
    /// Character constant.
    CharConstant(char, Position),
    /// String constant.
    StringConstant(Box<IdentX>),
    /// FnPtr constant.
    FnPointer(Box<IdentX>),
    /// Variable access - (optional index, optional modules, hash, variable name)
    Variable(Box<(Option<NonZeroUsize>, Option<Box<ModuleRef>>, u64, Ident)>),
    /// Property access - (getter, setter), prop
    Property(Box<((String, String), IdentX)>),
    /// { stmt }
    Stmt(Box<StaticVec<Stmt>>, Position),
    /// Wrapped expression - should not be optimized away.
    Expr(Box<Expr>),
    /// func(expr, ... )
    FnCall(Box<FnCallInfo>, Position),
    /// lhs.rhs
    Dot(Box<BinaryExpr>, Position),
    /// expr[expr]
    Index(Box<BinaryExpr>, Position),
    /// [ expr, ... ]
    Array(Box<StaticVec<Expr>>, Position),
    /// #{ name:expr, ... }
    Map(Box<StaticVec<(IdentX, Expr)>>, Position),
    /// lhs in rhs
    In(Box<BinaryExpr>, Position),
    /// lhs && rhs
    And(Box<BinaryExpr>, Position),
    /// lhs || rhs
    Or(Box<BinaryExpr>, Position),
    /// true
    True(Position),
    /// false
    False(Position),
    /// ()
    Unit(Position),
    /// Custom syntax
    Custom(Box<CustomExpr>, Position),
}

impl Default for Expr {
    #[inline(always)]
    fn default() -> Self {
        Self::Unit(NO_POS)
    }
}

impl Expr {
    /// Get the type of an expression.
    ///
    /// Returns `None` if the expression's result type is not constant.
    pub fn get_type_id(&self) -> Option<TypeId> {
        Some(match self {
            Self::Expr(x) => return x.get_type_id(),

            Self::IntegerConstant(_, _) => TypeId::of::<INT>(),
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, _) => TypeId::of::<FLOAT>(),
            Self::CharConstant(_, _) => TypeId::of::<char>(),
            Self::StringConstant(_) => TypeId::of::<ImmutableString>(),
            Self::FnPointer(_) => TypeId::of::<FnPtr>(),
            Self::True(_) | Self::False(_) | Self::In(_, _) | Self::And(_, _) | Self::Or(_, _) => {
                TypeId::of::<bool>()
            }
            Self::Unit(_) => TypeId::of::<()>(),

            #[cfg(not(feature = "no_index"))]
            Self::Array(_, _) => TypeId::of::<Array>(),

            #[cfg(not(feature = "no_object"))]
            Self::Map(_, _) => TypeId::of::<Map>(),

            _ => return None,
        })
    }

    /// Get the `Dynamic` value of a constant expression.
    ///
    /// Returns `None` if the expression is not constant.
    pub fn get_constant_value(&self) -> Option<Dynamic> {
        Some(match self {
            Self::Expr(x) => return x.get_constant_value(),

            Self::IntegerConstant(x, _) => (*x).into(),
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(x, _) => x.0.into(),
            Self::CharConstant(x, _) => (*x).into(),
            Self::StringConstant(x) => x.name.clone().into(),
            Self::FnPointer(x) => Dynamic(Union::FnPtr(Box::new(FnPtr::new_unchecked(
                x.name.clone(),
                Default::default(),
            )))),
            Self::True(_) => true.into(),
            Self::False(_) => false.into(),
            Self::Unit(_) => ().into(),

            #[cfg(not(feature = "no_index"))]
            Self::Array(x, _) if x.iter().all(Self::is_constant) => Dynamic(Union::Array(
                Box::new(x.iter().map(|v| v.get_constant_value().unwrap()).collect()),
            )),

            #[cfg(not(feature = "no_object"))]
            Self::Map(x, _) if x.iter().all(|(_, v)| v.is_constant()) => {
                Dynamic(Union::Map(Box::new(
                    x.iter()
                        .map(|(k, v)| (k.name.clone(), v.get_constant_value().unwrap()))
                        .collect(),
                )))
            }

            _ => return None,
        })
    }

    /// Is the expression a simple variable access?
    pub(crate) fn get_variable_access(&self, non_qualified: bool) -> Option<&str> {
        match self {
            Self::Variable(x) if !non_qualified || x.1.is_none() => Some((x.3).name.as_str()),
            _ => None,
        }
    }

    /// Get the `Position` of the expression.
    pub fn position(&self) -> Position {
        match self {
            Self::Expr(x) => x.position(),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, pos) => *pos,

            Self::IntegerConstant(_, pos) => *pos,
            Self::CharConstant(_, pos) => *pos,
            Self::StringConstant(x) => x.pos,
            Self::FnPointer(x) => x.pos,
            Self::Array(_, pos) => *pos,
            Self::Map(_, pos) => *pos,
            Self::Property(x) => (x.1).pos,
            Self::Stmt(_, pos) => *pos,
            Self::Variable(x) => (x.3).pos,
            Self::FnCall(_, pos) => *pos,

            Self::And(x, _) | Self::Or(x, _) | Self::In(x, _) => x.lhs.position(),

            Self::True(pos) | Self::False(pos) | Self::Unit(pos) => *pos,

            Self::Dot(x, _) | Self::Index(x, _) => x.lhs.position(),

            Self::Custom(_, pos) => *pos,
        }
    }

    /// Override the `Position` of the expression.
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            Self::Expr(x) => {
                x.set_position(new_pos);
            }

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, pos) => *pos = new_pos,

            Self::IntegerConstant(_, pos) => *pos = new_pos,
            Self::CharConstant(_, pos) => *pos = new_pos,
            Self::StringConstant(x) => x.pos = new_pos,
            Self::FnPointer(x) => x.pos = new_pos,
            Self::Array(_, pos) => *pos = new_pos,
            Self::Map(_, pos) => *pos = new_pos,
            Self::Variable(x) => (x.3).pos = new_pos,
            Self::Property(x) => (x.1).pos = new_pos,
            Self::Stmt(_, pos) => *pos = new_pos,
            Self::FnCall(_, pos) => *pos = new_pos,
            Self::And(_, pos) | Self::Or(_, pos) | Self::In(_, pos) => *pos = new_pos,
            Self::True(pos) | Self::False(pos) | Self::Unit(pos) => *pos = new_pos,
            Self::Dot(_, pos) | Self::Index(_, pos) => *pos = new_pos,
            Self::Custom(_, pos) => *pos = new_pos,
        }

        self
    }

    /// Is the expression pure?
    ///
    /// A pure expression has no side effects.
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Expr(x) => x.is_pure(),

            Self::Array(x, _) => x.iter().all(Self::is_pure),

            Self::Map(x, _) => x.iter().map(|(_, v)| v).all(Self::is_pure),

            Self::Index(x, _) | Self::And(x, _) | Self::Or(x, _) | Self::In(x, _) => {
                x.lhs.is_pure() && x.rhs.is_pure()
            }

            Self::Stmt(x, _) => x.iter().all(Stmt::is_pure),

            Self::Variable(_) => true,

            _ => self.is_constant(),
        }
    }

    /// Is the expression the unit `()` literal?
    #[inline(always)]
    pub fn is_unit(&self) -> bool {
        match self {
            Self::Unit(_) => true,
            _ => false,
        }
    }

    /// Is the expression a simple constant literal?
    pub fn is_literal(&self) -> bool {
        match self {
            Self::Expr(x) => x.is_literal(),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, _) => true,

            Self::IntegerConstant(_, _)
            | Self::CharConstant(_, _)
            | Self::StringConstant(_)
            | Self::FnPointer(_)
            | Self::True(_)
            | Self::False(_)
            | Self::Unit(_) => true,

            // An array literal is literal if all items are literals
            Self::Array(x, _) => x.iter().all(Self::is_literal),

            // An map literal is literal if all items are literals
            Self::Map(x, _) => x.iter().map(|(_, expr)| expr).all(Self::is_literal),

            // Check in expression
            Self::In(x, _) => match (&x.lhs, &x.rhs) {
                (Self::StringConstant(_), Self::StringConstant(_))
                | (Self::CharConstant(_, _), Self::StringConstant(_)) => true,
                _ => false,
            },

            _ => false,
        }
    }

    /// Is the expression a constant?
    pub fn is_constant(&self) -> bool {
        match self {
            Self::Expr(x) => x.is_constant(),

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, _) => true,

            Self::IntegerConstant(_, _)
            | Self::CharConstant(_, _)
            | Self::StringConstant(_)
            | Self::FnPointer(_)
            | Self::True(_)
            | Self::False(_)
            | Self::Unit(_) => true,

            // An array literal is constant if all items are constant
            Self::Array(x, _) => x.iter().all(Self::is_constant),

            // An map literal is constant if all items are constant
            Self::Map(x, _) => x.iter().map(|(_, expr)| expr).all(Self::is_constant),

            // Check in expression
            Self::In(x, _) => match (&x.lhs, &x.rhs) {
                (Self::StringConstant(_), Self::StringConstant(_))
                | (Self::CharConstant(_, _), Self::StringConstant(_)) => true,
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
            Self::FloatConstant(_, _) => false,

            Self::IntegerConstant(_, _)
            | Self::CharConstant(_, _)
            | Self::FnPointer(_)
            | Self::In(_, _)
            | Self::And(_, _)
            | Self::Or(_, _)
            | Self::True(_)
            | Self::False(_)
            | Self::Unit(_) => false,

            Self::StringConstant(_)
            | Self::Stmt(_, _)
            | Self::FnCall(_, _)
            | Self::Dot(_, _)
            | Self::Index(_, _)
            | Self::Array(_, _)
            | Self::Map(_, _) => match token {
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

            Self::Custom(_, _) => false,
        }
    }

    /// Convert a `Variable` into a `Property`.  All other variants are untouched.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    pub(crate) fn into_property(self) -> Self {
        match self {
            Self::Variable(x) if x.1.is_none() => {
                let ident = x.3;
                let getter = make_getter(&ident.name);
                let setter = make_setter(&ident.name);
                Self::Property(Box::new(((getter, setter), ident.into())))
            }
            _ => self,
        }
    }
}

/// Extract variables from a statement, removing variables defined in the script itself
fn extract_stmt_variables(stmt: &Stmt, defs: &mut HashSet<String>, vars: &mut HashSet<String>) {
    match stmt {
        Stmt::IfThenElse(e, bs, _) => {
            extract_expr_variables(e, defs, vars);
            extract_stmt_variables(&bs.0, defs, vars);
            if let Some(s) = &bs.1 {
                extract_stmt_variables(s, defs, vars);
            }
        }
        Stmt::While(e, s, _) => {
            extract_expr_variables(e, defs, vars);
            extract_stmt_variables(s, defs, vars);
        }
        Stmt::Loop(s, _) => extract_stmt_variables(s, defs, vars),
        Stmt::For(e, bs, _) => {
            extract_expr_variables(e, defs, vars);
            extract_stmt_variables(&bs.1, defs, vars);
        }
        Stmt::Let(id, oe, _) => {
            if let Some(e) = &oe {
                extract_expr_variables(e, defs, vars);
            }
            defs.insert(id.name.clone());
        }
        Stmt::Const(id, oe, _) => {
            if let Some(e) = &oe {
                extract_expr_variables(e, defs, vars);
            }
            defs.insert(id.name.clone());
        }
        Stmt::Assignment(be, _) => {
            extract_expr_variables(&be.0, defs, vars);
            extract_expr_variables(&be.2, defs, vars);
        }
        Stmt::Block(ss, _) => ss
            .iter()
            .for_each(|s| extract_stmt_variables(s, defs, vars)),
        Stmt::TryCatch(bs, _, _) => {
            extract_stmt_variables(&bs.0, defs, vars);
            extract_stmt_variables(&bs.2, defs, vars);
        }
        Stmt::Expr(e) => extract_expr_variables(e, defs, vars),
        _ => (),
    };
}

/// Extract variables from an expression
fn extract_expr_variables(expr: &Expr, defs: &mut HashSet<String>, vars: &mut HashSet<String>) {
    match expr {
        Expr::Variable(x) => {
            vars.insert(x.3.name.clone());
        }
        Expr::Stmt(ss, _) => ss
            .iter()
            .for_each(|s| extract_stmt_variables(s, defs, vars)),
        Expr::Expr(e) => extract_expr_variables(e, defs, vars),
        Expr::FnCall(ci, _) => ci
            .args
            .iter()
            .for_each(|e| extract_expr_variables(e, defs, vars)),
        Expr::Dot(be, _) => {
            extract_expr_variables(&be.lhs, defs, vars);
            extract_expr_variables(&be.rhs, defs, vars);
        }
        Expr::Index(be, _) => {
            extract_expr_variables(&be.lhs, defs, vars);
            extract_expr_variables(&be.rhs, defs, vars);
        }
        Expr::Array(es, _) => es
            .iter()
            .for_each(|e| extract_expr_variables(e, defs, vars)),
        Expr::Map(es, _) => es
            .iter()
            .for_each(|(_, e)| extract_expr_variables(e, defs, vars)),
        Expr::In(be, _) => extract_expr_variables(&be.rhs, defs, vars),
        Expr::And(be, _) => {
            extract_expr_variables(&be.lhs, defs, vars);
            extract_expr_variables(&be.rhs, defs, vars);
        }
        Expr::Or(be, _) => {
            extract_expr_variables(&be.lhs, defs, vars);
            extract_expr_variables(&be.rhs, defs, vars);
        }
        _ => (),
    };
}

#[cfg(test)]
mod tests {
    /// This test is to make sure no code changes increase the sizes of critical data structures.
    #[test]
    fn check_struct_sizes() {
        use std::mem::size_of;

        assert_eq!(size_of::<crate::Dynamic>(), 16);
        assert_eq!(size_of::<Option<crate::Dynamic>>(), 16);
        assert_eq!(size_of::<crate::Position>(), 4);
        assert_eq!(size_of::<crate::ast::Expr>(), 16);
        assert_eq!(size_of::<Option<crate::ast::Expr>>(), 16);
        assert_eq!(size_of::<crate::ast::Stmt>(), 32);
        assert_eq!(size_of::<Option<crate::ast::Stmt>>(), 32);
        assert_eq!(size_of::<crate::Scope>(), 72);
        assert_eq!(size_of::<crate::LexError>(), 32);
        assert_eq!(size_of::<crate::ParseError>(), 16);
        assert_eq!(size_of::<crate::EvalAltResult>(), 64);
    }
}
