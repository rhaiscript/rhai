//! Serialization helpers for converting Rhai types to bytes using rkyv.

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::{Dynamic, EvalAltResult, RhaiResultOf};

use super::archive::SimpleDynamic;

/// Serialize a Dynamic value to bytes using rkyv.
///
/// This uses SimpleDynamic as the root type for consistent serialization/deserialization.
/// Uses a fixed-size scratch buffer of 1024 bytes for serialization.
/// For larger objects, consider using `to_bytes_aligned` with a bigger buffer.
///
/// # Example
///
/// ```ignore
/// use rhai::{Dynamic, rkyv};
///
/// let value = Dynamic::from(42);
/// let bytes = rkyv::to_bytes(&value)?;
/// ```
pub fn to_bytes(value: &Dynamic) -> RhaiResultOf<Vec<u8>> {
    let simple = SimpleDynamic::from(value);
    rkyv::to_bytes::<SimpleDynamic, 1024>(&simple)
        .map(|aligned_vec| aligned_vec.into_vec())
        .map_err(|e| {
            let err_msg = format!("rkyv serialization error: {}", e);
            EvalAltResult::ErrorSystem(err_msg.clone(), err_msg.into()).into()
        })
}

/// Serialize a Dynamic value to an aligned byte vector using rkyv.
///
/// This is similar to [`to_bytes`] but returns an [`AlignedVec`] which may be
/// more efficient for certain use cases.
///
/// # Example
///
/// ```ignore
/// use rhai::Dynamic;
/// use rhai::rkyv::to_bytes_aligned;
///
/// let value = Dynamic::from("hello");
/// let bytes = to_bytes_aligned(&value)?;
/// # Ok::<_, Box<rhai::EvalAltResult>>(())
/// ```
pub fn to_bytes_aligned(value: &Dynamic) -> RhaiResultOf<Vec<u8>> {
    let simple = SimpleDynamic::from(value);
    rkyv::to_bytes::<SimpleDynamic, 1024>(&simple)
        .map(|aligned_vec| aligned_vec.into_vec())
        .map_err(|e| {
            let err_msg = format!("rkyv serialization error: {}", e);
            EvalAltResult::ErrorSystem(err_msg.clone(), err_msg.into()).into()
        })
}
