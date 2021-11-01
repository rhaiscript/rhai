//! Module defining the AST (abstract syntax tree).

use crate::calc_fn_hash;
use crate::dynamic::Union;
use crate::fn_native::shared_make_mut;
use crate::module::NamespaceRef;
use crate::token::Token;
use crate::{
    Dynamic, FnNamespace, Identifier, ImmutableString, Module, Position, Shared, StaticVec, INT,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    collections::BTreeMap,
    fmt,
    hash::Hash,
    mem,
    num::{NonZeroU8, NonZeroUsize},
    ops::{
        Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, Deref, DerefMut, Not, Sub,
        SubAssign,
    },
};

#[cfg(not(feature = "no_float"))]
use std::str::FromStr;

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_float"))]
use num_traits::Float;

#[cfg(not(feature = "no_index"))]
use crate::Array;

/// A type representing the access mode of a function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum FnAccess {
    /// Public function.
    Public,
    /// Private function.
    Private,
}

/// _(internals)_ A type containing information on a scripted function.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone)]
pub struct ScriptFnDef {
    /// Function body.
    pub body: StmtBlock,
    /// Encapsulated running environment, if any.
    pub lib: Option<Shared<Module>>,
    /// Encapsulated imported modules.
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    pub mods: crate::engine::Imports,
    /// Function name.
    pub name: Identifier,
    /// Function access mode.
    pub access: FnAccess,
    /// Names of function parameters.
    pub params: StaticVec<Identifier>,
    /// Access to external variables.
    ///
    /// Not available under `no_closure`.
    #[cfg(not(feature = "no_closure"))]
    pub externals: std::collections::BTreeSet<Identifier>,
    /// _(metadata)_ Function doc-comments (if any).
    /// Exported under the `metadata` feature only.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "metadata")]
    pub comments: StaticVec<String>,
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
                .collect::<StaticVec<_>>()
                .join(", ")
        )
    }
}

/// A type containing the metadata of a script-defined function.
///
/// Not available under `no_function`.
///
/// Created by [`AST::iter_functions`].
#[cfg(not(feature = "no_function"))]
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct ScriptFnMetadata<'a> {
    /// _(metadata)_ Function doc-comments (if any).
    /// Exported under the `metadata` feature only.
    ///
    /// Not available under `no_function`.
    ///
    /// Block doc-comments are kept in a single string slice with line-breaks within.
    ///
    /// Line doc-comments are kept in one string slice per line without the termination line-break.
    ///
    /// Leading white-spaces are stripped, and each string slice always starts with the corresponding
    /// doc-comment leader: `///` or `/**`.
    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "metadata")]
    pub comments: Vec<&'a str>,
    /// Function access mode.
    pub access: FnAccess,
    /// Function name.
    pub name: &'a str,
    /// Function parameters (if any).
    pub params: Vec<&'a str>,
}

#[cfg(not(feature = "no_function"))]
impl fmt::Display for ScriptFnMetadata<'_> {
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
                .cloned()
                .collect::<StaticVec<_>>()
                .join(", ")
        )
    }
}

#[cfg(not(feature = "no_function"))]
impl<'a> From<&'a ScriptFnDef> for ScriptFnMetadata<'a> {
    #[inline]
    fn from(value: &'a ScriptFnDef) -> Self {
        Self {
            #[cfg(not(feature = "no_function"))]
            #[cfg(feature = "metadata")]
            comments: value.comments.iter().map(|s| s.as_str()).collect(),
            access: value.access,
            name: &value.name,
            params: value.params.iter().map(|s| s.as_str()).collect(),
        }
    }
}

#[cfg(not(feature = "no_function"))]
impl std::cmp::PartialOrd for ScriptFnMetadata<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(not(feature = "no_function"))]
impl std::cmp::Ord for ScriptFnMetadata<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.name.cmp(other.name) {
            std::cmp::Ordering::Equal => self.params.len().cmp(&other.params.len()),
            cmp => cmp,
        }
    }
}

/// Compiled AST (abstract syntax tree) of a Rhai script.
///
/// # Thread Safety
///
/// Currently, [`AST`] is neither `Send` nor `Sync`. Turn on the `sync` feature to make it `Send + Sync`.
#[derive(Debug, Clone)]
pub struct AST {
    /// Source of the [`AST`].
    source: Option<Identifier>,
    /// Global statements.
    body: StmtBlock,
    /// Script-defined functions.
    functions: Shared<Module>,
    /// Embedded module resolver, if any.
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    resolver: Option<Shared<crate::module::resolvers::StaticModuleResolver>>,
}

impl Default for AST {
    #[inline(always)]
    fn default() -> Self {
        Self::empty()
    }
}

impl AST {
    /// Create a new [`AST`].
    #[inline]
    #[must_use]
    pub fn new(
        statements: impl IntoIterator<Item = Stmt>,
        functions: impl Into<Shared<Module>>,
    ) -> Self {
        Self {
            source: None,
            body: StmtBlock::new(statements, Position::NONE),
            functions: functions.into(),
            #[cfg(not(feature = "no_module"))]
            resolver: None,
        }
    }
    /// Create an empty [`AST`].
    #[inline]
    #[must_use]
    pub(crate) fn empty() -> Self {
        Self {
            source: None,
            body: Default::default(),
            functions: Default::default(),
            #[cfg(not(feature = "no_module"))]
            resolver: None,
        }
    }
    /// Create a new [`AST`] with a source name.
    #[inline(always)]
    #[must_use]
    pub fn new_with_source(
        statements: impl IntoIterator<Item = Stmt>,
        functions: impl Into<Shared<Module>>,
        source: impl Into<Identifier>,
    ) -> Self {
        Self {
            source: Some(source.into()),
            body: StmtBlock::new(statements, Position::NONE),
            functions: functions.into(),
            #[cfg(not(feature = "no_module"))]
            resolver: None,
        }
    }
    /// Get the source, if any.
    #[inline(always)]
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_ref().map(|s| s.as_str())
    }
    /// Get a reference to the source.
    #[inline(always)]
    #[must_use]
    pub(crate) fn source_raw(&self) -> Option<&Identifier> {
        self.source.as_ref()
    }
    /// Set the source.
    #[inline]
    pub fn set_source(&mut self, source: impl Into<Identifier>) -> &mut Self {
        let source = source.into();
        Shared::get_mut(&mut self.functions)
            .as_mut()
            .map(|m| m.set_id(source.clone()));
        self.source = Some(source);
        self
    }
    /// Clear the source.
    #[inline(always)]
    pub fn clear_source(&mut self) -> &mut Self {
        self.source = None;
        self
    }
    /// Get the statements.
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    #[must_use]
    pub(crate) fn statements(&self) -> &[Stmt] {
        &self.body.0
    }
    /// _(internals)_ Get the statements.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[deprecated = "this method is volatile and may change"]
    #[inline(always)]
    #[must_use]
    pub fn statements(&self) -> &[Stmt] {
        &self.body.0
    }
    /// Get a mutable reference to the statements.
    #[cfg(not(feature = "no_optimize"))]
    #[inline(always)]
    #[must_use]
    pub(crate) fn statements_mut(&mut self) -> &mut StaticVec<Stmt> {
        &mut self.body.0
    }
    /// Get the internal shared [`Module`] containing all script-defined functions.
    #[cfg(not(feature = "internals"))]
    #[cfg(not(feature = "no_module"))]
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub(crate) fn shared_lib(&self) -> Shared<Module> {
        self.functions.clone()
    }
    /// _(internals)_ Get the internal shared [`Module`] containing all script-defined functions.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_function` or `no_module`.
    #[cfg(feature = "internals")]
    #[deprecated = "this method is volatile and may change"]
    #[cfg(not(feature = "no_module"))]
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub fn shared_lib(&self) -> Shared<Module> {
        self.functions.clone()
    }
    /// Get the internal [`Module`] containing all script-defined functions.
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    #[must_use]
    pub(crate) fn lib(&self) -> &Module {
        &self.functions
    }
    /// _(internals)_ Get the internal [`Module`] containing all script-defined functions.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_function`.
    #[cfg(feature = "internals")]
    #[deprecated = "this method is volatile and may change"]
    #[inline(always)]
    #[must_use]
    pub fn lib(&self) -> &Module {
        &self.functions
    }
    /// Get the embedded [module resolver][`ModuleResolver`].
    #[cfg(not(feature = "no_module"))]
    #[cfg(not(feature = "internals"))]
    #[inline(always)]
    #[must_use]
    pub(crate) fn resolver(
        &self,
    ) -> Option<Shared<crate::module::resolvers::StaticModuleResolver>> {
        self.resolver.clone()
    }
    /// _(internals)_ Get the embedded [module resolver][crate::ModuleResolver].
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[cfg(feature = "internals")]
    #[inline(always)]
    #[must_use]
    pub fn resolver(&self) -> Option<Shared<crate::module::resolvers::StaticModuleResolver>> {
        self.resolver.clone()
    }
    /// Set the embedded [module resolver][`ModuleResolver`].
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    pub(crate) fn set_resolver(
        &mut self,
        resolver: impl Into<Shared<crate::module::resolvers::StaticModuleResolver>>,
    ) -> &mut Self {
        self.resolver = Some(resolver.into());
        self
    }
    /// Clone the [`AST`]'s functions into a new [`AST`].
    /// No statements are cloned.
    ///
    /// Not available under `no_function`.
    ///
    /// This operation is cheap because functions are shared.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub fn clone_functions_only(&self) -> Self {
        self.clone_functions_only_filtered(|_, _, _, _, _| true)
    }
    /// Clone the [`AST`]'s functions into a new [`AST`] based on a filter predicate.
    /// No statements are cloned.
    ///
    /// Not available under `no_function`.
    ///
    /// This operation is cheap because functions are shared.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    #[must_use]
    pub fn clone_functions_only_filtered(
        &self,
        filter: impl Fn(FnNamespace, FnAccess, bool, &str, usize) -> bool,
    ) -> Self {
        let mut functions: Module = Default::default();
        functions.merge_filtered(&self.functions, &filter);
        Self {
            source: self.source.clone(),
            body: Default::default(),
            functions: functions.into(),
            #[cfg(not(feature = "no_module"))]
            resolver: self.resolver.clone(),
        }
    }
    /// Clone the [`AST`]'s script statements into a new [`AST`].
    /// No functions are cloned.
    #[inline(always)]
    #[must_use]
    pub fn clone_statements_only(&self) -> Self {
        Self {
            source: self.source.clone(),
            body: self.body.clone(),
            functions: Default::default(),
            #[cfg(not(feature = "no_module"))]
            resolver: self.resolver.clone(),
        }
    }
    /// Merge two [`AST`] into one.  Both [`AST`]'s are untouched and a new, merged,
    /// version is returned.
    ///
    /// Statements in the second [`AST`] are simply appended to the end of the first _without any processing_.
    /// Thus, the return value of the first [`AST`] (if using expression-statement syntax) is buried.
    /// Of course, if the first [`AST`] uses a `return` statement at the end, then
    /// the second [`AST`] will essentially be dead code.
    ///
    /// All script-defined functions in the second [`AST`] overwrite similarly-named functions
    /// in the first [`AST`] with the same number of parameters.
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
    /// let ast1 = engine.compile("
    ///     fn foo(x) { 42 + x }
    ///     foo(1)
    /// ")?;
    ///
    /// let ast2 = engine.compile(r#"
    ///     fn foo(n) { `hello${n}` }
    ///     foo("!")
    /// "#)?;
    ///
    /// let ast = ast1.merge(&ast2);    // Merge 'ast2' into 'ast1'
    ///
    /// // Notice that using the '+' operator also works:
    /// // let ast = &ast1 + &ast2;
    ///
    /// // 'ast' is essentially:
    /// //
    /// //    fn foo(n) { `hello${n}` } // <- definition of first 'foo' is overwritten
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
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        self.merge_filtered(other, |_, _, _, _, _| true)
    }
    /// Combine one [`AST`] with another.  The second [`AST`] is consumed.
    ///
    /// Statements in the second [`AST`] are simply appended to the end of the first _without any processing_.
    /// Thus, the return value of the first [`AST`] (if using expression-statement syntax) is buried.
    /// Of course, if the first [`AST`] uses a `return` statement at the end, then
    /// the second [`AST`] will essentially be dead code.
    ///
    /// All script-defined functions in the second [`AST`] overwrite similarly-named functions
    /// in the first [`AST`] with the same number of parameters.
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
    /// let mut ast1 = engine.compile("
    ///     fn foo(x) { 42 + x }
    ///     foo(1)
    /// ")?;
    ///
    /// let ast2 = engine.compile(r#"
    ///     fn foo(n) { `hello${n}` }
    ///     foo("!")
    /// "#)?;
    ///
    /// ast1.combine(ast2);    // Combine 'ast2' into 'ast1'
    ///
    /// // Notice that using the '+=' operator also works:
    /// // ast1 += ast2;
    ///
    /// // 'ast1' is essentially:
    /// //
    /// //    fn foo(n) { `hello${n}` } // <- definition of first 'foo' is overwritten
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
        self.combine_filtered(other, |_, _, _, _, _| true)
    }
    /// Merge two [`AST`] into one.  Both [`AST`]'s are untouched and a new, merged, version
    /// is returned.
    ///
    /// Statements in the second [`AST`] are simply appended to the end of the first _without any processing_.
    /// Thus, the return value of the first [`AST`] (if using expression-statement syntax) is buried.
    /// Of course, if the first [`AST`] uses a `return` statement at the end, then
    /// the second [`AST`] will essentially be dead code.
    ///
    /// All script-defined functions in the second [`AST`] are first selected based on a filter
    /// predicate, then overwrite similarly-named functions in the first [`AST`] with the
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
    /// let ast1 = engine.compile("
    ///     fn foo(x) { 42 + x }
    ///     foo(1)
    /// ")?;
    ///
    /// let ast2 = engine.compile(r#"
    ///     fn foo(n) { `hello${n}` }
    ///     fn error() { 0 }
    ///     foo("!")
    /// "#)?;
    ///
    /// // Merge 'ast2', picking only 'error()' but not 'foo(_)', into 'ast1'
    /// let ast = ast1.merge_filtered(&ast2, |_, _, script, name, params|
    ///                                 script && name == "error" && params == 0);
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
    #[must_use]
    pub fn merge_filtered(
        &self,
        other: &Self,
        filter: impl Fn(FnNamespace, FnAccess, bool, &str, usize) -> bool,
    ) -> Self {
        let Self {
            body, functions, ..
        } = self;

        let merged = match (body.is_empty(), other.body.is_empty()) {
            (false, false) => {
                let mut body = body.clone();
                body.0.extend(other.body.0.iter().cloned());
                body
            }
            (false, true) => body.clone(),
            (true, false) => other.body.clone(),
            (true, true) => Default::default(),
        };

        let source = other.source.clone().or_else(|| self.source.clone());

        let mut functions = functions.as_ref().clone();
        functions.merge_filtered(&other.functions, &filter);

        if let Some(source) = source {
            Self::new_with_source(merged.0, functions, source)
        } else {
            Self::new(merged.0, functions)
        }
    }
    /// Combine one [`AST`] with another.  The second [`AST`] is consumed.
    ///
    /// Statements in the second [`AST`] are simply appended to the end of the first _without any processing_.
    /// Thus, the return value of the first [`AST`] (if using expression-statement syntax) is buried.
    /// Of course, if the first [`AST`] uses a `return` statement at the end, then
    /// the second [`AST`] will essentially be dead code.
    ///
    /// All script-defined functions in the second [`AST`] are first selected based on a filter
    /// predicate, then overwrite similarly-named functions in the first [`AST`] with the
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
    /// let mut ast1 = engine.compile("
    ///     fn foo(x) { 42 + x }
    ///     foo(1)
    /// ")?;
    ///
    /// let ast2 = engine.compile(r#"
    ///     fn foo(n) { `hello${n}` }
    ///     fn error() { 0 }
    ///     foo("!")
    /// "#)?;
    ///
    /// // Combine 'ast2', picking only 'error()' but not 'foo(_)', into 'ast1'
    /// ast1.combine_filtered(ast2, |_, _, script, name, params|
    ///                                 script && name == "error" && params == 0);
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
    #[inline]
    pub fn combine_filtered(
        &mut self,
        other: Self,
        filter: impl Fn(FnNamespace, FnAccess, bool, &str, usize) -> bool,
    ) -> &mut Self {
        self.body.0.extend(other.body.0.into_iter());

        if !other.functions.is_empty() {
            shared_make_mut(&mut self.functions).merge_filtered(&other.functions, &filter);
        }
        self
    }
    /// Filter out the functions, retaining only some based on a filter predicate.
    ///
    /// Not available under `no_function`.
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
    ///     fn foo(n) { n + 1 }
    ///     fn bar() { print("hello"); }
    /// "#)?;
    ///
    /// // Remove all functions except 'foo(_)'
    /// ast.retain_functions(|_, _, name, params| name == "foo" && params == 1);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub fn retain_functions(
        &mut self,
        filter: impl Fn(FnNamespace, FnAccess, &str, usize) -> bool,
    ) -> &mut Self {
        if !self.functions.is_empty() {
            shared_make_mut(&mut self.functions).retain_script_functions(filter);
        }
        self
    }
    /// Iterate through all function definitions.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    pub(crate) fn iter_fn_def(&self) -> impl Iterator<Item = &ScriptFnDef> {
        self.functions
            .iter_script_fn()
            .map(|(_, _, _, _, fn_def)| fn_def.as_ref())
    }
    /// Iterate through all function definitions.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn iter_functions<'a>(&'a self) -> impl Iterator<Item = ScriptFnMetadata> + 'a {
        self.functions
            .iter_script_fn()
            .map(|(_, _, _, _, fn_def)| fn_def.as_ref().into())
    }
    /// Clear all function definitions in the [`AST`].
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn clear_functions(&mut self) -> &mut Self {
        self.functions = Default::default();
        self
    }
    /// Clear all statements in the [`AST`], leaving only function definitions.
    #[inline(always)]
    pub fn clear_statements(&mut self) -> &mut Self {
        self.body = Default::default();
        self
    }
    /// Recursively walk the [`AST`], including function bodies (if any).
    /// Return `false` from the callback to terminate the walk.
    #[cfg(not(feature = "internals"))]
    #[cfg(not(feature = "no_module"))]
    #[inline]
    pub(crate) fn walk(&self, on_node: &mut impl FnMut(&[ASTNode]) -> bool) -> bool {
        let path = &mut Default::default();

        for stmt in self.statements() {
            if !stmt.walk(path, on_node) {
                return false;
            }
        }
        #[cfg(not(feature = "no_function"))]
        for stmt in self.iter_fn_def().flat_map(|f| f.body.0.iter()) {
            if !stmt.walk(path, on_node) {
                return false;
            }
        }

        true
    }
    /// _(internals)_ Recursively walk the [`AST`], including function bodies (if any).
    /// Return `false` from the callback to terminate the walk.
    /// Exported under the `internals` feature only.
    #[cfg(feature = "internals")]
    #[inline]
    pub fn walk(&self, on_node: &mut impl FnMut(&[ASTNode]) -> bool) -> bool {
        let path = &mut Default::default();

        for stmt in self.statements() {
            if !stmt.walk(path, on_node) {
                return false;
            }
        }
        #[cfg(not(feature = "no_function"))]
        for stmt in self.iter_fn_def().flat_map(|f| f.body.0.iter()) {
            if !stmt.walk(path, on_node) {
                return false;
            }
        }

        true
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

/// _(internals)_ An identifier containing a name and a [position][Position].
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Ident {
    /// Identifier name.
    pub name: Identifier,
    /// Declaration position.
    pub pos: Position,
}

impl fmt::Debug for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.name)?;
        self.pos.debug_print(f)
    }
}

/// _(internals)_ An [`AST`] node, consisting of either an [`Expr`] or a [`Stmt`].
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash)]
pub enum ASTNode<'a> {
    /// A statement ([`Stmt`]).
    Stmt(&'a Stmt),
    /// An expression ([`Expr`]).
    Expr(&'a Expr),
}

impl<'a> From<&'a Stmt> for ASTNode<'a> {
    fn from(stmt: &'a Stmt) -> Self {
        Self::Stmt(stmt)
    }
}

impl<'a> From<&'a Expr> for ASTNode<'a> {
    fn from(expr: &'a Expr) -> Self {
        Self::Expr(expr)
    }
}

impl ASTNode<'_> {
    /// Get the [`Position`] of this [`ASTNode`].
    pub const fn position(&self) -> Position {
        match self {
            ASTNode::Stmt(stmt) => stmt.position(),
            ASTNode::Expr(expr) => expr.position(),
        }
    }
}

/// _(internals)_ A scoped block of statements.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Clone, Hash, Default)]
pub struct StmtBlock(StaticVec<Stmt>, Position);

impl StmtBlock {
    /// Create a new [`StmtBlock`].
    #[must_use]
    pub fn new(statements: impl IntoIterator<Item = Stmt>, pos: Position) -> Self {
        let mut statements: StaticVec<_> = statements.into_iter().collect();
        statements.shrink_to_fit();
        Self(statements, pos)
    }
    /// Is this statements block empty?
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Number of statements in this statements block.
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Get the position (location of the beginning `{`) of this statements block.
    #[inline(always)]
    #[must_use]
    pub const fn position(&self) -> Position {
        self.1
    }
}

impl Deref for StmtBlock {
    type Target = StaticVec<Stmt>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StmtBlock {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for StmtBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Block")?;
        fmt::Debug::fmt(&self.0, f)?;
        self.1.debug_print(f)
    }
}

impl From<StmtBlock> for Stmt {
    #[inline(always)]
    fn from(block: StmtBlock) -> Self {
        let block_pos = block.position();
        Self::Block(block.0.into_boxed_slice(), block_pos)
    }
}

/// A type that holds a configuration option with bit-flags.
/// Exported under the `internals` feature only.
#[derive(PartialEq, Eq, Copy, Clone, Hash, Default)]
pub struct OptionFlags(u8);

impl OptionFlags {
    /// Does this [`OptionFlags`] contain a particular option flag?
    #[inline(always)]
    #[must_use]
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }
}

impl Not for OptionFlags {
    type Output = Self;

    /// Return the negation of the [`OptionFlags`].
    #[inline(always)]
    fn not(self) -> Self::Output {
        Self(!self.0) & AST_OPTION_FLAGS::AST_OPTION_ALL
    }
}

impl Add for OptionFlags {
    type Output = Self;

    /// Return the union of two [`OptionFlags`].
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl AddAssign for OptionFlags {
    /// Add the option flags in one [`OptionFlags`] to another.
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0
    }
}

impl BitOr for OptionFlags {
    type Output = Self;

    /// Return the union of two [`OptionFlags`].
    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for OptionFlags {
    /// Add the option flags in one [`OptionFlags`] to another.
    #[inline(always)]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0
    }
}

impl Sub for OptionFlags {
    type Output = Self;

    /// Return the difference of two [`OptionFlags`].
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 & !rhs.0)
    }
}

impl SubAssign for OptionFlags {
    /// Remove the option flags in one [`OptionFlags`] from another.
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 &= !rhs.0
    }
}

impl BitAnd for OptionFlags {
    type Output = Self;

    /// Return the intersection of two [`OptionFlags`].
    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & !rhs.0)
    }
}

impl BitAndAssign for OptionFlags {
    /// Keep only the intersection of one [`OptionFlags`] with another.
    #[inline(always)]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= !rhs.0
    }
}

/// Option bit-flags for [`AST`] nodes.
#[allow(non_snake_case)]
pub mod AST_OPTION_FLAGS {
    use super::OptionFlags;

    /// _(internals)_ No options for the [`AST`][crate::AST] node.
    /// Exported under the `internals` feature only.
    pub const AST_OPTION_NONE: OptionFlags = OptionFlags(0b0000_0000);
    /// _(internals)_ The [`AST`][crate::AST] node is constant.
    /// Exported under the `internals` feature only.
    pub const AST_OPTION_CONSTANT: OptionFlags = OptionFlags(0b0000_0001);
    /// _(internals)_ The [`AST`][crate::AST] node is public.
    /// Exported under the `internals` feature only.
    pub const AST_OPTION_PUBLIC: OptionFlags = OptionFlags(0b0000_0010);
    /// _(internals)_ The [`AST`][crate::AST] node is in negated mode.
    /// Exported under the `internals` feature only.
    pub const AST_OPTION_NEGATED: OptionFlags = OptionFlags(0b0000_0100);
    /// _(internals)_ The [`AST`][crate::AST] node breaks out of normal control flow.
    /// Exported under the `internals` feature only.
    pub const AST_OPTION_BREAK_OUT: OptionFlags = OptionFlags(0b0000_1000);
    /// _(internals)_ Mask of all options.
    /// Exported under the `internals` feature only.
    pub(crate) const AST_OPTION_ALL: OptionFlags = OptionFlags(
        AST_OPTION_CONSTANT.0 | AST_OPTION_PUBLIC.0 | AST_OPTION_NEGATED.0 | AST_OPTION_BREAK_OUT.0,
    );

    impl std::fmt::Debug for OptionFlags {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            fn write_option(
                options: &OptionFlags,
                f: &mut std::fmt::Formatter<'_>,
                num_flags: &mut usize,
                flag: OptionFlags,
                name: &str,
            ) -> std::fmt::Result {
                if options.contains(flag) {
                    if *num_flags > 0 {
                        f.write_str("+")?;
                    }
                    f.write_str(name)?;
                    *num_flags += 1;
                }
                Ok(())
            }

            let num_flags = &mut 0;

            f.write_str("(")?;
            write_option(self, f, num_flags, AST_OPTION_CONSTANT, "Constant")?;
            write_option(self, f, num_flags, AST_OPTION_PUBLIC, "Public")?;
            write_option(self, f, num_flags, AST_OPTION_NEGATED, "Negated")?;
            write_option(self, f, num_flags, AST_OPTION_BREAK_OUT, "Break")?;
            f.write_str(")")?;

            Ok(())
        }
    }
}

/// _(internals)_ A statement.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash)]
pub enum Stmt {
    /// No-op.
    Noop(Position),
    /// `if` expr `{` stmt `}` `else` `{` stmt `}`
    If(Expr, Box<(StmtBlock, StmtBlock)>, Position),
    /// `switch` expr `if` condition `{` literal or _ `=>` stmt `,` ... `}`
    Switch(
        Expr,
        Box<(BTreeMap<u64, Box<(Option<Expr>, StmtBlock)>>, StmtBlock)>,
        Position,
    ),
    /// `while` expr `{` stmt `}` | `loop` `{` stmt `}`
    ///
    /// If the guard expression is [`UNIT`][Expr::Unit], then it is a `loop` statement.
    While(Expr, Box<StmtBlock>, Position),
    /// `do` `{` stmt `}` `while`|`until` expr
    ///
    /// ### Option Flags
    ///
    /// * [`AST_OPTION_NONE`][AST_OPTION_FLAGS::AST_OPTION_NONE] = `while`
    /// * [`AST_OPTION_NEGATED`][AST_OPTION_FLAGS::AST_OPTION_NEGATED] = `until`
    Do(Box<StmtBlock>, Expr, OptionFlags, Position),
    /// `for` `(` id `,` counter `)` `in` expr `{` stmt `}`
    For(Expr, Box<(Ident, Option<Ident>, StmtBlock)>, Position),
    /// \[`export`\] `let`|`const` id `=` expr
    ///
    /// ### Option Flags
    ///
    /// * [`AST_OPTION_PUBLIC`][AST_OPTION_FLAGS::AST_OPTION_PUBLIC] = `export`
    /// * [`AST_OPTION_CONSTANT`][AST_OPTION_FLAGS::AST_OPTION_CONSTANT] = `const`
    Var(Expr, Box<Ident>, OptionFlags, Position),
    /// expr op`=` expr
    Assignment(Box<(Expr, Option<OpAssignment<'static>>, Expr)>, Position),
    /// func `(` expr `,` ... `)`
    ///
    /// Note - this is a duplicate of [`Expr::FnCall`] to cover the very common pattern of a single
    ///        function call forming one statement.
    FnCall(Box<FnCallExpr>, Position),
    /// `{` stmt`;` ... `}`
    Block(Box<[Stmt]>, Position),
    /// `try` `{` stmt; ... `}` `catch` `(` var `)` `{` stmt; ... `}`
    TryCatch(Box<(StmtBlock, Option<Ident>, StmtBlock)>, Position),
    /// [expression][Expr]
    Expr(Expr),
    /// `continue`/`break`
    ///
    /// ### Option Flags
    ///
    /// * [`AST_OPTION_NONE`][AST_OPTION_FLAGS::AST_OPTION_NONE] = `continue`
    /// * [`AST_OPTION_BREAK_OUT`][AST_OPTION_FLAGS::AST_OPTION_BREAK_OUT] = `break`
    BreakLoop(OptionFlags, Position),
    /// `return`/`throw`
    ///
    /// ### Option Flags
    ///
    /// * [`AST_OPTION_NONE`][AST_OPTION_FLAGS::AST_OPTION_NONE] = `return`
    /// * [`AST_OPTION_BREAK_OUT`][AST_OPTION_FLAGS::AST_OPTION_BREAK_OUT] = `throw`
    Return(OptionFlags, Option<Expr>, Position),
    /// `import` expr `as` var
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    Import(Expr, Option<Box<Ident>>, Position),
    /// `export` var `as` var `,` ...
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    Export(Box<[(Ident, Ident)]>, Position),
    /// Convert a variable to shared.
    ///
    /// Not available under `no_closure`.
    ///
    /// # Notes
    ///
    /// This variant does not map to any language structure.  It is currently only used only to
    /// convert a normal variable into a shared variable when the variable is _captured_ by a closure.
    #[cfg(not(feature = "no_closure"))]
    Share(Identifier),
}

impl Default for Stmt {
    #[inline(always)]
    fn default() -> Self {
        Self::Noop(Position::NONE)
    }
}

impl From<Stmt> for StmtBlock {
    #[inline]
    fn from(stmt: Stmt) -> Self {
        match stmt {
            Stmt::Block(mut block, pos) => Self(block.iter_mut().map(mem::take).collect(), pos),
            Stmt::Noop(pos) => Self(Default::default(), pos),
            _ => {
                let pos = stmt.position();
                Self(vec![stmt].into(), pos)
            }
        }
    }
}

impl Stmt {
    /// Is this statement [`Noop`][Stmt::Noop]?
    #[inline(always)]
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        matches!(self, Self::Noop(_))
    }
    /// Get the [position][Position] of this statement.
    #[must_use]
    pub const fn position(&self) -> Position {
        match self {
            Self::Noop(pos)
            | Self::BreakLoop(_, pos)
            | Self::Block(_, pos)
            | Self::Assignment(_, pos)
            | Self::FnCall(_, pos)
            | Self::If(_, _, pos)
            | Self::Switch(_, _, pos)
            | Self::While(_, _, pos)
            | Self::Do(_, _, _, pos)
            | Self::For(_, _, pos)
            | Self::Return(_, _, pos)
            | Self::Var(_, _, _, pos)
            | Self::TryCatch(_, pos) => *pos,

            Self::Expr(x) => x.position(),

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, pos) => *pos,
            #[cfg(not(feature = "no_module"))]
            Self::Export(_, pos) => *pos,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(_) => Position::NONE,
        }
    }
    /// Override the [position][Position] of this statement.
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            Self::Noop(pos)
            | Self::BreakLoop(_, pos)
            | Self::Block(_, pos)
            | Self::Assignment(_, pos)
            | Self::FnCall(_, pos)
            | Self::If(_, _, pos)
            | Self::Switch(_, _, pos)
            | Self::While(_, _, pos)
            | Self::Do(_, _, _, pos)
            | Self::For(_, _, pos)
            | Self::Return(_, _, pos)
            | Self::Var(_, _, _, pos)
            | Self::TryCatch(_, pos) => *pos = new_pos,

            Self::Expr(x) => {
                x.set_position(new_pos);
            }

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, pos) => *pos = new_pos,
            #[cfg(not(feature = "no_module"))]
            Self::Export(_, pos) => *pos = new_pos,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(_) => (),
        }

        self
    }
    /// Does this statement return a value?
    #[must_use]
    pub const fn returns_value(&self) -> bool {
        match self {
            Self::If(_, _, _)
            | Self::Switch(_, _, _)
            | Self::Block(_, _)
            | Self::Expr(_)
            | Self::FnCall(_, _) => true,

            Self::Noop(_)
            | Self::While(_, _, _)
            | Self::Do(_, _, _, _)
            | Self::For(_, _, _)
            | Self::TryCatch(_, _) => false,

            Self::Var(_, _, _, _)
            | Self::Assignment(_, _)
            | Self::BreakLoop(_, _)
            | Self::Return(_, _, _) => false,

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, _) | Self::Export(_, _) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(_) => false,
        }
    }
    /// Is this statement self-terminated (i.e. no need for a semicolon terminator)?
    #[must_use]
    pub const fn is_self_terminated(&self) -> bool {
        match self {
            Self::If(_, _, _)
            | Self::Switch(_, _, _)
            | Self::While(_, _, _)
            | Self::For(_, _, _)
            | Self::Block(_, _)
            | Self::TryCatch(_, _) => true,

            // A No-op requires a semicolon in order to know it is an empty statement!
            Self::Noop(_) => false,

            Self::Expr(Expr::Custom(x, _)) if x.is_self_terminated() => true,

            Self::Var(_, _, _, _)
            | Self::Assignment(_, _)
            | Self::Expr(_)
            | Self::FnCall(_, _)
            | Self::Do(_, _, _, _)
            | Self::BreakLoop(_, _)
            | Self::Return(_, _, _) => false,

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, _) | Self::Export(_, _) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(_) => false,
        }
    }
    /// Is this statement _pure_?
    ///
    /// A pure statement has no side effects.
    #[must_use]
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Noop(_) => true,
            Self::Expr(expr) => expr.is_pure(),
            Self::If(condition, x, _) => {
                condition.is_pure()
                    && (x.0).0.iter().all(Stmt::is_pure)
                    && (x.1).0.iter().all(Stmt::is_pure)
            }
            Self::Switch(expr, x, _) => {
                expr.is_pure()
                    && x.0.values().all(|block| {
                        block.0.as_ref().map(Expr::is_pure).unwrap_or(true)
                            && (block.1).0.iter().all(Stmt::is_pure)
                    })
                    && (x.1).0.iter().all(Stmt::is_pure)
            }

            // Loops that exit can be pure because it can never be infinite.
            Self::While(Expr::BoolConstant(false, _), _, _) => true,
            Self::Do(body, Expr::BoolConstant(x, _), options, _)
                if *x == options.contains(AST_OPTION_FLAGS::AST_OPTION_NEGATED) =>
            {
                body.iter().all(Stmt::is_pure)
            }

            // Loops are never pure since they can be infinite - and that's a side effect.
            Self::While(_, _, _) | Self::Do(_, _, _, _) => false,

            // For loops can be pure because if the iterable is pure, it is finite,
            // so infinite loops can never occur.
            Self::For(iterable, x, _) => iterable.is_pure() && (x.2).0.iter().all(Stmt::is_pure),

            Self::Var(_, _, _, _) | Self::Assignment(_, _) | Self::FnCall(_, _) => false,
            Self::Block(block, _) => block.iter().all(|stmt| stmt.is_pure()),
            Self::BreakLoop(_, _) | Self::Return(_, _, _) => false,
            Self::TryCatch(x, _) => {
                (x.0).0.iter().all(Stmt::is_pure) && (x.2).0.iter().all(Stmt::is_pure)
            }

            #[cfg(not(feature = "no_module"))]
            Self::Import(_, _, _) => false,
            #[cfg(not(feature = "no_module"))]
            Self::Export(_, _) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(_) => false,
        }
    }
    /// Is this statement _pure_ within the containing block?
    ///
    /// An internally pure statement only has side effects that disappear outside the block.
    ///
    /// Currently only variable declarations (i.e. `let` and `const`) and `import`/`export`
    /// statements are internally pure.
    #[inline]
    #[must_use]
    pub fn is_internally_pure(&self) -> bool {
        match self {
            Self::Var(expr, _, _, _) => expr.is_pure(),

            #[cfg(not(feature = "no_module"))]
            Self::Import(expr, _, _) => expr.is_pure(),
            #[cfg(not(feature = "no_module"))]
            Self::Export(_, _) => true,

            _ => self.is_pure(),
        }
    }
    /// Does this statement break the current control flow through the containing block?
    ///
    /// Currently this is only true for `return`, `throw`, `break` and `continue`.
    ///
    /// All statements following this statement will essentially be dead code.
    #[inline]
    #[must_use]
    pub const fn is_control_flow_break(&self) -> bool {
        match self {
            Self::Return(_, _, _) | Self::BreakLoop(_, _) => true,
            _ => false,
        }
    }
    /// Recursively walk this statement.
    /// Return `false` from the callback to terminate the walk.
    pub fn walk<'a>(
        &'a self,
        path: &mut Vec<ASTNode<'a>>,
        on_node: &mut impl FnMut(&[ASTNode]) -> bool,
    ) -> bool {
        // Push the current node onto the path
        path.push(self.into());

        if !on_node(path) {
            return false;
        }

        match self {
            Self::Var(e, _, _, _) => {
                if !e.walk(path, on_node) {
                    return false;
                }
            }
            Self::If(e, x, _) => {
                if !e.walk(path, on_node) {
                    return false;
                }
                for s in &(x.0).0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
                for s in &(x.1).0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Switch(e, x, _) => {
                if !e.walk(path, on_node) {
                    return false;
                }
                for b in x.0.values() {
                    if !b.0.as_ref().map(|e| e.walk(path, on_node)).unwrap_or(true) {
                        return false;
                    }
                    for s in &(b.1).0 {
                        if !s.walk(path, on_node) {
                            return false;
                        }
                    }
                }
                for s in &(x.1).0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::While(e, s, _) | Self::Do(s, e, _, _) => {
                if !e.walk(path, on_node) {
                    return false;
                }
                for s in &s.0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::For(e, x, _) => {
                if !e.walk(path, on_node) {
                    return false;
                }
                for s in &(x.2).0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Assignment(x, _) => {
                if !x.0.walk(path, on_node) {
                    return false;
                }
                if !x.2.walk(path, on_node) {
                    return false;
                }
            }
            Self::FnCall(x, _) => {
                for s in &x.args {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Block(x, _) => {
                for s in x.iter() {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::TryCatch(x, _) => {
                for s in &(x.0).0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
                for s in &(x.2).0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Expr(e) | Self::Return(_, Some(e), _) => {
                if !e.walk(path, on_node) {
                    return false;
                }
            }
            #[cfg(not(feature = "no_module"))]
            Self::Import(e, _, _) => {
                if !e.walk(path, on_node) {
                    return false;
                }
            }
            _ => (),
        }

        path.pop().expect("`path` contains current node");

        true
    }
}

/// _(internals)_ A custom syntax expression.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Hash)]
pub struct CustomExpr {
    /// List of keywords.
    pub keywords: StaticVec<Expr>,
    /// Is the current [`Scope`][crate::Scope] possibly modified by this custom statement
    /// (e.g. introducing a new variable)?
    pub scope_may_be_changed: bool,
    /// List of tokens actually parsed.
    pub tokens: StaticVec<Identifier>,
    /// Is this custom syntax self-terminated?
    pub self_terminated: bool,
}

impl CustomExpr {
    /// Is this custom syntax self-terminated (i.e. no need for a semicolon terminator)?
    ///
    /// A self-terminated custom syntax always ends in `$block$`, `}` or `;`
    #[must_use]
    #[inline(always)]
    pub const fn is_self_terminated(&self) -> bool {
        self.self_terminated
    }
}

/// _(internals)_ A binary expression.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Clone, Hash)]
pub struct BinaryExpr {
    /// LHS expression.
    pub lhs: Expr,
    /// RHS expression.
    pub rhs: Expr,
}

/// _(internals)_ An op-assignment operator.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct OpAssignment<'a> {
    /// Hash of the op-assignment call.
    pub hash_op_assign: u64,
    /// Hash of the underlying operator call.
    pub hash_op: u64,
    /// Op-assignment operator.
    pub op: &'a str,
}

impl OpAssignment<'_> {
    /// Create a new [`OpAssignment`].
    ///
    /// # Panics
    ///
    /// Panics if the operator name is not an op-assignment operator.
    #[must_use]
    pub fn new(op: Token) -> Self {
        let op_raw = op
            .map_op_assignment()
            .expect("token is op-assignment operator")
            .literal_syntax();
        let op_assignment = op.literal_syntax();

        Self {
            hash_op_assign: calc_fn_hash(op_assignment, 2),
            hash_op: calc_fn_hash(op_raw, 2),
            op: op_assignment,
        }
    }
}

/// _(internals)_ An set of function call hashes.
/// Exported under the `internals` feature only.
///
/// Two separate hashes are pre-calculated because of the following pattern:
///
/// ```ignore
/// func(a, b, c);      // Native: func(a, b, c) - 3 parameters
///                     // Script: func(a, b, c) - 3 parameters
///
/// a.func(b, c);       // Native: func(&mut a, b, c) - 3 parameters
///                     // Script: func(b, c) - 2 parameters
/// ```
///
/// For normal function calls, the native hash equals the script hash.
/// For method-style calls, the script hash contains one fewer parameter.
///
/// Function call hashes are used in the following manner:
///
/// * First, the script hash is tried, which contains only the called function's name plus the
///   of parameters.
///
/// * Next, the actual types of arguments are hashed and _combined_ with the native hash, which is
///   then used to search for a native function.
///   In other words, a native function call hash always contains the called function's name plus
///   the types of the arguments.  This is to due to possible function overloading for different parameter types.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Default)]
pub struct FnCallHashes {
    /// Pre-calculated hash for a script-defined function ([`None`] if native functions only).
    pub script: Option<u64>,
    /// Pre-calculated hash for a native Rust function with no parameter types.
    pub native: u64,
}

impl fmt::Debug for FnCallHashes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(script) = self.script {
            if script == self.native {
                fmt::Debug::fmt(&self.native, f)
            } else {
                write!(f, "({}, {})", script, self.native)
            }
        } else {
            write!(f, "{} (native only)", self.native)
        }
    }
}

impl FnCallHashes {
    /// Create a [`FnCallHashes`] with only the native Rust hash.
    #[inline(always)]
    #[must_use]
    pub const fn from_native(hash: u64) -> Self {
        Self {
            script: None,
            native: hash,
        }
    }
    /// Create a [`FnCallHashes`] with both native Rust and script function hashes set to the same value.
    #[inline(always)]
    #[must_use]
    pub const fn from_script(hash: u64) -> Self {
        Self {
            script: Some(hash),
            native: hash,
        }
    }
    /// Create a [`FnCallHashes`] with both native Rust and script function hashes.
    #[inline(always)]
    #[must_use]
    pub const fn from_script_and_native(script: u64, native: u64) -> Self {
        Self {
            script: Some(script),
            native,
        }
    }
    /// Is this [`FnCallHashes`] native Rust only?
    #[inline(always)]
    #[must_use]
    pub const fn is_native_only(&self) -> bool {
        self.script.is_none()
    }
}

/// _(internals)_ A function call.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Debug, Clone, Default, Hash)]
pub struct FnCallExpr {
    /// Namespace of the function, if any.
    pub namespace: Option<NamespaceRef>,
    /// Pre-calculated hashes.
    pub hashes: FnCallHashes,
    /// List of function call argument expressions.
    pub args: StaticVec<Expr>,
    /// List of function call arguments that are constants.
    ///
    /// Any arguments in `args` that is [`Expr::Stack`][Expr::Stack] indexes into this
    /// array to find the constant for use as its argument value.
    pub constants: smallvec::SmallVec<[Dynamic; 2]>,
    /// Function name.
    pub name: Identifier,
    /// Does this function call capture the parent scope?
    pub capture: bool,
}

impl FnCallExpr {
    /// Does this function call contain a qualified namespace?
    #[inline(always)]
    #[must_use]
    pub const fn is_qualified(&self) -> bool {
        self.namespace.is_some()
    }
    /// Convert this into a [`FnCall`][Expr::FnCall].
    #[inline(always)]
    #[must_use]
    pub fn into_fn_call_expr(self, pos: Position) -> Expr {
        Expr::FnCall(self.into(), pos)
    }
}

/// A type that wraps a floating-point number and implements [`Hash`].
///
/// Not available under `no_float`.
#[cfg(not(feature = "no_float"))]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct FloatWrapper<F>(F);

#[cfg(not(feature = "no_float"))]
impl Hash for FloatWrapper<FLOAT> {
    #[inline(always)]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ne_bytes().hash(state);
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float> AsRef<F> for FloatWrapper<F> {
    #[inline(always)]
    fn as_ref(&self) -> &F {
        &self.0
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float> AsMut<F> for FloatWrapper<F> {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut F {
        &mut self.0
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float> std::ops::Deref for FloatWrapper<F> {
    type Target = F;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float> std::ops::DerefMut for FloatWrapper<F> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float + fmt::Debug> fmt::Debug for FloatWrapper<F> {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float + fmt::Display + fmt::LowerExp + From<f32>> fmt::Display for FloatWrapper<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let abs = self.0.abs();
        if abs > Self::MAX_NATURAL_FLOAT_FOR_DISPLAY.into()
            || abs < Self::MIN_NATURAL_FLOAT_FOR_DISPLAY.into()
        {
            write!(f, "{:e}", self.0)
        } else {
            fmt::Display::fmt(&self.0, f)?;
            if abs.fract().is_zero() {
                f.write_str(".0")?;
            }
            Ok(())
        }
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float> From<F> for FloatWrapper<F> {
    #[inline(always)]
    fn from(value: F) -> Self {
        Self::new(value)
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float + FromStr> FromStr for FloatWrapper<F> {
    type Err = <F as FromStr>::Err;

    #[inline(always)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        F::from_str(s).map(Into::<Self>::into)
    }
}

#[cfg(not(feature = "no_float"))]
impl<F: Float> FloatWrapper<F> {
    /// Maximum floating-point number for natural display before switching to scientific notation.
    pub const MAX_NATURAL_FLOAT_FOR_DISPLAY: f32 = 10000000000000.0;

    /// Minimum floating-point number for natural display before switching to scientific notation.
    pub const MIN_NATURAL_FLOAT_FOR_DISPLAY: f32 = 0.0000000000001;

    /// Create a new [`FloatWrapper`].
    #[inline(always)]
    #[must_use]
    pub fn new(value: F) -> Self {
        Self(value)
    }
}

#[cfg(not(feature = "no_float"))]
impl FloatWrapper<FLOAT> {
    /// Create a new [`FloatWrapper`].
    #[inline(always)]
    #[must_use]
    pub const fn new_const(value: FLOAT) -> Self {
        Self(value)
    }
}

/// _(internals)_ An expression sub-tree.
/// Exported under the `internals` feature only.
///
/// # Volatile Data Structure
///
/// This type is volatile and may change.
#[derive(Clone, Hash)]
pub enum Expr {
    /// Dynamic constant.
    /// Used to hold either an [`Array`] or [`Map`][crate::Map] literal for quick cloning.
    /// All other primitive data types should use the appropriate variants for better speed.
    DynamicConstant(Box<Dynamic>, Position),
    /// Boolean constant.
    BoolConstant(bool, Position),
    /// Integer constant.
    IntegerConstant(INT, Position),
    /// Floating-point constant.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(FloatWrapper<FLOAT>, Position),
    /// Character constant.
    CharConstant(char, Position),
    /// [String][ImmutableString] constant.
    StringConstant(ImmutableString, Position),
    /// An interpolated [string][ImmutableString].
    InterpolatedString(Box<StaticVec<Expr>>, Position),
    /// [ expr, ... ]
    Array(Box<StaticVec<Expr>>, Position),
    /// #{ name:expr, ... }
    Map(
        Box<(StaticVec<(Ident, Expr)>, BTreeMap<Identifier, Dynamic>)>,
        Position,
    ),
    /// ()
    Unit(Position),
    /// Variable access - optional short index, position, (optional index, optional (hash, modules), variable name)
    ///
    /// The short index is [`u8`] which is used when the index is <= 255, which should be the vast
    /// majority of cases (unless there are more than 255 variables defined!).
    /// This is to avoid reading a pointer redirection during each variable access.
    Variable(
        Option<NonZeroU8>,
        Position,
        Box<(
            Option<NonZeroUsize>,
            Option<(NamespaceRef, u64)>,
            Identifier,
        )>,
    ),
    /// Property access - ((getter, hash), (setter, hash), prop)
    Property(
        Box<(
            (Identifier, u64),
            (Identifier, u64),
            (ImmutableString, Position),
        )>,
    ),
    /// Stack slot
    ///
    /// # Notes
    ///
    /// This variant does not map to any language structure.  It is currently only used in function
    /// calls with constant arguments where the `usize` number indexes into an array containing a
    /// list of constant arguments for the function call.  See [`FnCallExpr`] for more details.
    Stack(usize, Position),
    /// { [statement][Stmt] ... }
    Stmt(Box<StmtBlock>),
    /// func `(` expr `,` ... `)`
    FnCall(Box<FnCallExpr>, Position),
    /// lhs `.` rhs - bool variable is a dummy
    Dot(Box<BinaryExpr>, bool, Position),
    /// expr `[` expr `]` - boolean indicates whether the dotting/indexing chain stops
    Index(Box<BinaryExpr>, bool, Position),
    /// lhs `&&` rhs
    And(Box<BinaryExpr>, Position),
    /// lhs `||` rhs
    Or(Box<BinaryExpr>, Position),
    /// Custom syntax
    Custom(Box<CustomExpr>, Position),
}

impl Default for Expr {
    #[inline(always)]
    fn default() -> Self {
        Self::Unit(Position::NONE)
    }
}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut display_pos = self.position();

        match self {
            Self::DynamicConstant(value, _) => write!(f, "{:?}", value),
            Self::BoolConstant(value, _) => write!(f, "{:?}", value),
            Self::IntegerConstant(value, _) => write!(f, "{:?}", value),
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(value, _) => write!(f, "{:?}", value),
            Self::CharConstant(value, _) => write!(f, "{:?}", value),
            Self::StringConstant(value, _) => write!(f, "{:?}", value),
            Self::Unit(_) => f.write_str("()"),

            Self::InterpolatedString(x, _) => {
                f.write_str("InterpolatedString")?;
                return f.debug_list().entries(x.iter()).finish();
            }
            Self::Array(x, _) => {
                f.write_str("Array")?;
                f.debug_list().entries(x.iter()).finish()
            }
            Self::Map(x, _) => {
                f.write_str("Map")?;
                f.debug_map()
                    .entries(x.0.iter().map(|(k, v)| (k, v)))
                    .finish()
            }
            Self::Variable(i, _, x) => {
                f.write_str("Variable(")?;
                match x.1 {
                    Some((_, ref namespace)) => write!(f, "{}", namespace)?,
                    _ => (),
                }
                f.write_str(&x.2)?;
                match i.map_or_else(|| x.0, |n| NonZeroUsize::new(n.get() as usize)) {
                    Some(n) => write!(f, " #{}", n)?,
                    _ => (),
                }
                f.write_str(")")
            }
            Self::Property(x) => write!(f, "Property({})", (x.2).0),
            Self::Stack(x, _) => write!(f, "StackSlot({})", x),
            Self::Stmt(x) => {
                f.write_str("ExprStmtBlock")?;
                f.debug_list().entries(x.0.iter()).finish()
            }
            Self::FnCall(x, _) => {
                let mut ff = f.debug_struct("FnCall");
                x.namespace.as_ref().map(|ns| ff.field("namespace", ns));
                ff.field("name", &x.name)
                    .field("hash", &x.hashes)
                    .field("args", &x.args);
                if !x.constants.is_empty() {
                    ff.field("constants", &x.constants);
                }
                if x.capture {
                    ff.field("capture", &x.capture);
                }
                ff.finish()
            }
            Self::Index(x, term, pos) => {
                display_pos = *pos;

                f.debug_struct("Index")
                    .field("lhs", &x.lhs)
                    .field("rhs", &x.rhs)
                    .field("terminate", term)
                    .finish()
            }
            Self::Dot(x, _, pos) | Self::And(x, pos) | Self::Or(x, pos) => {
                let op_name = match self {
                    Self::Dot(_, _, _) => "Dot",
                    Self::And(_, _) => "And",
                    Self::Or(_, _) => "Or",
                    _ => unreachable!(),
                };

                display_pos = *pos;

                f.debug_struct(op_name)
                    .field("lhs", &x.lhs)
                    .field("rhs", &x.rhs)
                    .finish()
            }
            Self::Custom(x, _) => f.debug_tuple("Custom").field(x).finish(),
        }?;

        display_pos.debug_print(f)
    }
}

impl Expr {
    /// Get the [`Dynamic`] value of a literal constant expression.
    ///
    /// Returns [`None`] if the expression is not a literal constant.
    #[inline]
    #[must_use]
    pub fn get_literal_value(&self) -> Option<Dynamic> {
        Some(match self {
            Self::DynamicConstant(x, _) => x.as_ref().clone(),
            Self::IntegerConstant(x, _) => (*x).into(),
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(x, _) => (*x).into(),
            Self::CharConstant(x, _) => (*x).into(),
            Self::StringConstant(x, _) => x.clone().into(),
            Self::BoolConstant(x, _) => (*x).into(),
            Self::Unit(_) => Dynamic::UNIT,

            #[cfg(not(feature = "no_index"))]
            Self::Array(x, _) if self.is_constant() => {
                let mut arr = Array::with_capacity(x.len());
                arr.extend(x.iter().map(|v| {
                    v.get_literal_value()
                        .expect("constant array has constant value")
                }));
                Dynamic::from_array(arr)
            }

            #[cfg(not(feature = "no_object"))]
            Self::Map(x, _) if self.is_constant() => {
                let mut map = x.1.clone();
                x.0.iter().for_each(|(k, v)| {
                    *map.get_mut(k.name.as_str())
                        .expect("template contains all keys") = v
                        .get_literal_value()
                        .expect("constant map has constant value")
                });
                Dynamic::from_map(map)
            }

            _ => return None,
        })
    }
    /// Create an [`Expr`] from a [`Dynamic`] value.
    #[inline]
    #[must_use]
    pub fn from_dynamic(value: Dynamic, pos: Position) -> Self {
        match value.0 {
            Union::Unit(_, _, _) => Self::Unit(pos),
            Union::Bool(b, _, _) => Self::BoolConstant(b, pos),
            Union::Str(s, _, _) => Self::StringConstant(s, pos),
            Union::Char(c, _, _) => Self::CharConstant(c, pos),
            Union::Int(i, _, _) => Self::IntegerConstant(i, pos),

            #[cfg(feature = "decimal")]
            Union::Decimal(value, _, _) => Self::DynamicConstant(Box::new((*value).into()), pos),

            #[cfg(not(feature = "no_float"))]
            Union::Float(f, _, _) => Self::FloatConstant(f, pos),

            #[cfg(not(feature = "no_index"))]
            Union::Array(a, _, _) => Self::DynamicConstant(Box::new((*a).into()), pos),

            #[cfg(not(feature = "no_object"))]
            Union::Map(m, _, _) => Self::DynamicConstant(Box::new((*m).into()), pos),

            _ => Self::DynamicConstant(value.into(), pos),
        }
    }
    /// Is the expression a simple variable access?
    #[inline(always)]
    #[must_use]
    pub(crate) const fn is_variable_access(&self, non_qualified: bool) -> bool {
        match self {
            Self::Variable(_, _, x) => !non_qualified || x.1.is_none(),
            _ => false,
        }
    }
    /// Return the variable name if the expression a simple variable access.
    #[inline(always)]
    #[must_use]
    pub(crate) fn get_variable_name(&self, non_qualified: bool) -> Option<&str> {
        match self {
            Self::Variable(_, _, x) if !non_qualified || x.1.is_none() => Some(x.2.as_str()),
            _ => None,
        }
    }
    /// Get the [position][Position] of the expression.
    #[inline]
    #[must_use]
    pub const fn position(&self) -> Position {
        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, pos) => *pos,

            Self::DynamicConstant(_, pos)
            | Self::BoolConstant(_, pos)
            | Self::IntegerConstant(_, pos)
            | Self::CharConstant(_, pos)
            | Self::Unit(pos)
            | Self::StringConstant(_, pos)
            | Self::Array(_, pos)
            | Self::Map(_, pos)
            | Self::Variable(_, pos, _)
            | Self::Stack(_, pos)
            | Self::FnCall(_, pos)
            | Self::Custom(_, pos)
            | Self::InterpolatedString(_, pos) => *pos,

            Self::Property(x) => (x.2).1,
            Self::Stmt(x) => x.1,

            Self::And(x, _) | Self::Or(x, _) | Self::Dot(x, _, _) | Self::Index(x, _, _) => {
                x.lhs.position()
            }
        }
    }
    /// Override the [position][Position] of the expression.
    #[inline]
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, pos) => *pos = new_pos,

            Self::DynamicConstant(_, pos)
            | Self::BoolConstant(_, pos)
            | Self::IntegerConstant(_, pos)
            | Self::CharConstant(_, pos)
            | Self::Unit(pos)
            | Self::StringConstant(_, pos)
            | Self::Array(_, pos)
            | Self::Map(_, pos)
            | Self::And(_, pos)
            | Self::Or(_, pos)
            | Self::Dot(_, _, pos)
            | Self::Index(_, _, pos)
            | Self::Variable(_, pos, _)
            | Self::Stack(_, pos)
            | Self::FnCall(_, pos)
            | Self::Custom(_, pos)
            | Self::InterpolatedString(_, pos) => *pos = new_pos,

            Self::Property(x) => (x.2).1 = new_pos,
            Self::Stmt(x) => x.1 = new_pos,
        }

        self
    }
    /// Is the expression pure?
    ///
    /// A pure expression has no side effects.
    #[inline]
    #[must_use]
    pub fn is_pure(&self) -> bool {
        match self {
            Self::InterpolatedString(x, _) | Self::Array(x, _) => x.iter().all(Self::is_pure),

            Self::Map(x, _) => x.0.iter().map(|(_, v)| v).all(Self::is_pure),

            Self::And(x, _) | Self::Or(x, _) => x.lhs.is_pure() && x.rhs.is_pure(),

            Self::Stmt(x) => x.0.iter().all(Stmt::is_pure),

            Self::Variable(_, _, _) | Self::Stack(_, _) => true,

            _ => self.is_constant(),
        }
    }
    /// Is the expression the unit `()` literal?
    #[inline(always)]
    #[must_use]
    pub const fn is_unit(&self) -> bool {
        matches!(self, Self::Unit(_))
    }
    /// Is the expression a constant?
    #[inline]
    #[must_use]
    pub fn is_constant(&self) -> bool {
        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, _) => true,

            Self::DynamicConstant(_, _)
            | Self::BoolConstant(_, _)
            | Self::IntegerConstant(_, _)
            | Self::CharConstant(_, _)
            | Self::StringConstant(_, _)
            | Self::Unit(_)
            | Self::Stack(_, _) => true,

            Self::InterpolatedString(x, _) | Self::Array(x, _) => x.iter().all(Self::is_constant),

            Self::Map(x, _) => x.0.iter().map(|(_, expr)| expr).all(Self::is_constant),

            _ => false,
        }
    }
    /// Is a particular [token][Token] allowed as a postfix operator to this expression?
    #[inline]
    #[must_use]
    pub const fn is_valid_postfix(&self, token: &Token) -> bool {
        match token {
            #[cfg(not(feature = "no_object"))]
            Token::Period => return true,
            _ => (),
        }

        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(_, _) => false,

            Self::DynamicConstant(_, _)
            | Self::BoolConstant(_, _)
            | Self::CharConstant(_, _)
            | Self::And(_, _)
            | Self::Or(_, _)
            | Self::Unit(_) => false,

            Self::IntegerConstant(_, _)
            | Self::StringConstant(_, _)
            | Self::InterpolatedString(_, _)
            | Self::FnCall(_, _)
            | Self::Stmt(_)
            | Self::Dot(_, _, _)
            | Self::Index(_, _, _)
            | Self::Array(_, _)
            | Self::Map(_, _) => match token {
                #[cfg(not(feature = "no_index"))]
                Token::LeftBracket => true,
                _ => false,
            },

            Self::Variable(_, _, _) => match token {
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

            Self::Stack(_, _) => false,
        }
    }
    /// Recursively walk this expression.
    /// Return `false` from the callback to terminate the walk.
    pub fn walk<'a>(
        &'a self,
        path: &mut Vec<ASTNode<'a>>,
        on_node: &mut impl FnMut(&[ASTNode]) -> bool,
    ) -> bool {
        // Push the current node onto the path
        path.push(self.into());

        if !on_node(path) {
            return false;
        }

        match self {
            Self::Stmt(x) => {
                for s in &x.0 {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::InterpolatedString(x, _) | Self::Array(x, _) => {
                for e in x.as_ref() {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Map(x, _) => {
                for (_, e) in &x.0 {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Index(x, _, _) | Self::Dot(x, _, _) | Expr::And(x, _) | Expr::Or(x, _) => {
                if !x.lhs.walk(path, on_node) {
                    return false;
                }
                if !x.rhs.walk(path, on_node) {
                    return false;
                }
            }
            Self::FnCall(x, _) => {
                for e in &x.args {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Custom(x, _) => {
                for e in &x.keywords {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            _ => (),
        }

        path.pop().expect("`path` contains current node");

        true
    }
}
