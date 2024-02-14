//! Module defining mechanisms to handle function calls in Rhai.

pub mod builtin;
pub mod call;
pub mod func_args;
#[allow(clippy::module_inception)]
pub mod func_trait;
pub mod function;
pub mod hashing;
pub mod native;
pub mod plugin;
pub mod register;
pub mod script;

pub use builtin::{get_builtin_binary_op_fn, get_builtin_op_assignment_fn};
#[cfg(not(feature = "no_closure"))]
pub use call::ensure_no_data_race;
#[cfg(not(feature = "no_function"))]
pub use call::is_anonymous_fn;
pub use call::FnCallArgs;
pub use func_args::FuncArgs;
#[cfg(not(feature = "no_function"))]
pub use func_trait::Func;
pub use function::RhaiFunc;
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "no_function"))]
pub use hashing::calc_typed_method_hash;
pub use hashing::{calc_fn_hash, calc_fn_hash_full, calc_var_hash, get_hasher, StraightHashMap};
#[cfg(feature = "internals")]
#[allow(deprecated)]
pub use native::NativeCallContextStore;
#[allow(unused_imports)]
pub use native::{
    locked_read, locked_write, shared_get_mut, shared_make_mut, shared_take, shared_take_or_clone,
    FnIterator, Locked, NativeCallContext, SendSync, Shared,
};
pub use register::RhaiNativeFunc;
