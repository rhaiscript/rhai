mod cache;
mod chaining;
mod data_check;
mod debugger;
mod eval_context;
mod expr;
mod global_state;
mod stmt;
mod target;

#[allow(unused_imports)]
pub use cache::FnResolutionCache;
pub use cache::{Caches, FnResolutionCacheEntry};
#[cfg(not(feature = "unchecked"))]
#[cfg(not(feature = "no_index"))]
pub use data_check::calc_array_sizes;
#[cfg(not(feature = "unchecked"))]
#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
pub use data_check::calc_data_sizes;
#[cfg(feature = "debugging")]
#[cfg(not(feature = "no_function"))]
pub use debugger::CallStackFrame;
#[cfg(feature = "debugging")]
pub use debugger::{
    BreakPoint, Debugger, DebuggerCommand, DebuggerEvent, DebuggerStatus, OnDebuggerCallback,
    OnDebuggingInit,
};
pub use eval_context::EvalContext;
#[cfg(not(feature = "no_module"))]
pub use expr::search_imports;
pub use expr::search_namespace;

pub use global_state::GlobalRuntimeState;
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_function"))]
pub use global_state::SharedGlobalConstants;
#[cfg(not(feature = "no_index"))]
pub use target::calc_offset_len;
pub use target::{calc_index, Target};

#[cfg(feature = "unchecked")]
mod unchecked {
    use crate::{eval::GlobalRuntimeState, Dynamic, Engine, Position, RhaiResultOf};
    use std::borrow::Borrow;
    #[cfg(feature = "no_std")]
    use std::prelude::v1::*;

    impl Engine {
        /// Check if the number of operations stay within limit.
        #[inline(always)]
        pub(crate) const fn track_operation(
            &self,
            _: &GlobalRuntimeState,
            _: Position,
        ) -> RhaiResultOf<()> {
            Ok(())
        }

        /// Check whether the size of a [`Dynamic`] is within limits.
        #[inline(always)]
        pub(crate) const fn check_data_size<T: Borrow<Dynamic>>(
            &self,
            value: T,
            _: Position,
        ) -> RhaiResultOf<T> {
            Ok(value)
        }
    }
}
