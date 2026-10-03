//! Module defining mechanisms to handle function calls in Rhai.

pub mod builtin;
#[cfg(not(feature = "no_ast"))]
pub mod call;
pub mod func_args;
pub mod func_call;
#[allow(clippy::module_inception)]
#[cfg(not(feature = "no_ast"))]
pub mod func_trait;
pub mod function;
pub mod hashing;
pub mod native;
pub mod plugin;
pub mod register;
pub mod script;

pub use builtin::{get_builtin_binary_op_fn, get_builtin_op_assignment_fn};
pub use func_args::FuncArgs;
#[cfg(not(feature = "no_closure"))]
#[cfg(not(feature = "unchecked"))]
#[cfg(not(feature = "no_ast"))]
pub use func_call::ensure_no_data_race;
pub use func_call::{FnCallArgs, FnCallHashes};
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_ast"))]
pub use func_trait::Func;
#[cfg(not(feature = "no_function"))]
pub use function::is_anonymous_fn;
pub use function::{is_valid_function_name, FnAccess, RhaiFunc};
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_ast"))]
pub use hashing::calc_closure_hash;
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "no_function"))]
pub use hashing::calc_typed_method_hash;
pub use hashing::{
    calc_fn_hash, calc_fn_hash_full, calc_switch_value_hash, calc_var_hash, StraightHashMap,
};
#[cfg(feature = "internals")]
#[allow(deprecated)]
pub use native::NativeCallContextStore;
#[allow(unused_imports)]
pub use native::{
    locked_read, locked_write, shared_get_mut, shared_make_mut, shared_take, shared_take_or_clone,
    FnIterator, Locked, NativeCallContext, SendSync, Shared,
};
pub use register::RhaiNativeFunc;

#[cfg(not(feature = "no_function"))]
pub use script::{EncapsulatedEnviron, ScriptFnMetadata, ScriptFuncDef, ScriptFuncPayload};

/// _(internals)_ Empty placeholder for a script-defined function.
/// Exported under the `internals` feature only.
#[cfg(feature = "no_function")]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub struct ScriptFuncDef;
