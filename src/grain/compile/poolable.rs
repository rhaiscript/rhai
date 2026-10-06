use crate::grain::bytecode::constant_caps;
use crate::Dynamic;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Whether a constant can live in the artifact's constant pool.
///
/// Two constraints happen to coincide here, so one check enforces both.
///
/// The artifact must be loadable in another process, which rules out anything
/// carrying a host `TypeId`, a live `Rc`, or a clock reading: `Variant`,
/// `Shared`, `TimeStamp`.
pub(crate) fn is_poolable(value: &Dynamic) -> bool {
    // Check if the value is a valid constant.
    // Anything else — TimeStamp, a custom type, a shared cell, etc.,
    // cannot be pooled.
    constant_caps(value).is_some()
}
