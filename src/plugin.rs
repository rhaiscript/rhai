//! Module defining plugins in Rhai. Is exported for use by plugin authors.

pub use crate::{
    stdlib::any::TypeId,
    stdlib::boxed::Box,
    stdlib::format,
    stdlib::string::ToString,
    stdlib::vec::Vec,
    stdlib::vec as new_vec,
    stdlib::mem,
    fn_native::CallableFunction,
    Dynamic,
    Engine,
    EvalAltResult,
    FnAccess,
    ImmutableString,
    Module,
    Position,
    RegisterResultFn,
};

#[cfg(features = "no_module")]
pub use rhai_codegen::{export_fn, register_exported_fn};
#[cfg(not(features = "no_module"))]
pub use rhai_codegen::*;

#[cfg(features = "sync")]
/// Represents an externally-written plugin for the Rhai interpreter.
///
/// This trait should not be used directly. Use the `#[plugin]` procedural attribute instead.
pub trait Plugin: Send {
    fn register_contents(self, engine: &mut Engine);
}

#[cfg(not(features = "sync"))]
/// Represents an externally-written plugin for the Rhai interpreter.
///
/// This trait should not be used directly. Use the `#[plugin]` procedural attribute instead.
pub trait Plugin: Send + Sync {
    fn register_contents(self, engine: &mut Engine);
}

/// Represents a function that is statically defined within a plugin.
///
/// This trait should not be used directly. Use the `#[plugin]` procedural attribute instead.
pub trait PluginFunction {
    fn is_method_call(&self) -> bool;
    fn is_varadic(&self) -> bool;

    fn call(&self, args: &mut [&mut Dynamic], pos: Position)
        -> Result<Dynamic, Box<EvalAltResult>>;

    fn clone_boxed(&self) -> Box<dyn PluginFunction>;

    fn input_types(&self) -> Box<[TypeId]>;
}
