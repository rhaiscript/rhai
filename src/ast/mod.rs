//! Module defining the AST (abstract syntax tree).

#[allow(clippy::module_inception)]
pub mod ast;
pub mod expr;
pub mod flags;
pub mod ident;
pub mod namespace;
pub mod namespace_none;
pub mod script_fn;
pub mod stmt;

pub use ast::{ASTNode, EncapsulatedEnviron, AST};
#[cfg(not(feature = "no_custom_syntax"))]
pub use expr::CustomExpr;
pub use expr::{BinaryExpr, Expr, FnCallExpr, FnCallHashes};
pub use flags::{ASTFlags, FnAccess};
pub use ident::Ident;
#[cfg(not(feature = "no_module"))]
pub use namespace::Namespace;
#[cfg(feature = "no_module")]
pub use namespace_none::Namespace;
#[cfg(not(feature = "no_function"))]
pub use script_fn::{ScriptFnMetadata, ScriptFuncDef};
pub use stmt::{
    CaseBlocksList, FlowControl, OpAssignment, RangeCase, Stmt, StmtBlock, StmtBlockContainer,
    SwitchCasesCollection,
};

/// _(internals)_ Placeholder for a script-defined function.
/// Exported under the `internals` feature only.
#[cfg(feature = "no_function")]
pub type ScriptFuncDef = ();
