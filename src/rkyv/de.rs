//! Deserialization helpers for converting bytes back to Rhai types using rkyv.

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::{Dynamic, RhaiResultOf};
use rkyv::{Deserialize, Infallible};

// Import SimpleDynamic from the archive module
use super::archive::SimpleDynamic;

/// Deserialize a Dynamic value from bytes without validation (unsafe, but fast).
///
/// This function deserializes bytes that were created by serializing a Dynamic value.
/// It properly handles the SimpleDynamic intermediate representation.
///
/// # Safety
///
/// This function is **unsafe** because it does not validate the byte buffer.
/// Using this with corrupted or malicious data can lead to undefined behavior.
///
/// Only use this function if:
/// - You serialized the data yourself using `to_bytes`
/// - The data comes from a trusted source
/// - Performance is critical and you can't afford validation
///
/// # Example
///
/// ```ignore
/// use rhai::Dynamic;
/// use rhai::rkyv::{to_bytes, from_bytes_owned_unchecked};
///
/// let value = Dynamic::from(42);
/// let bytes = to_bytes(&value)?;
///
/// // UNSAFE: Deserialize to owned value
/// let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };
/// assert_eq!(42, restored.as_int().unwrap());
/// # Ok::<_, Box<rhai::EvalAltResult>>(())
/// ```
pub unsafe fn from_bytes_owned_unchecked(bytes: &[u8]) -> RhaiResultOf<Dynamic> {
    // Dynamic is serialized through SimpleDynamic, so we deserialize SimpleDynamic first
    let archived = rkyv::archived_root::<SimpleDynamic>(bytes);
    let simple: SimpleDynamic = archived.deserialize(&mut Infallible).unwrap();

    // Convert SimpleDynamic to Dynamic manually to avoid Dynamic::from wrapping it as Variant
    let dynamic = match simple {
        SimpleDynamic::Unit => Dynamic::UNIT,
        SimpleDynamic::Bool(v) => Dynamic::from(v),
        SimpleDynamic::Str(s) => Dynamic::from(s),
        SimpleDynamic::Char(c) => Dynamic::from(c),
        SimpleDynamic::Int(i) => Dynamic::from(i),

        #[cfg(not(feature = "no_float"))]
        SimpleDynamic::Float(f) => Dynamic::from(f),

        #[cfg(not(feature = "no_index"))]
        SimpleDynamic::Blob(blob) => Dynamic::from(blob),
    };

    Ok(dynamic)
}

/// Deserialize a Dynamic value from bytes (safe wrapper).
///
/// This is a safe wrapper around `from_bytes_owned_unchecked` for convenience.
/// It assumes the bytes come from a trusted source (created by `to_bytes`).
///
/// For maximum performance in trusted environments, use the unsafe version directly.
///
/// # Example
///
/// ```ignore
/// use rhai::Dynamic;
/// use rhai::rkyv::{to_bytes, from_bytes_owned};
///
/// let value = Dynamic::from(42);
/// let bytes = to_bytes(&value)?;
///
/// let restored: Dynamic = from_bytes_owned(&bytes)?;
/// assert_eq!(42, restored.as_int().unwrap());
/// # Ok::<_, Box<rhai::EvalAltResult>>(())
/// ```
#[inline(always)]
pub fn from_bytes_owned(bytes: &[u8]) -> RhaiResultOf<Dynamic> {
    // SAFETY: This is marked as safe because it's meant for convenience when
    // deserializing data from trusted sources (e.g., your own serialized data).
    // Users should only call this with data created by `to_bytes`.
    unsafe { from_bytes_owned_unchecked(bytes) }
}

/// Deserialize bytes into a specific type T (safe wrapper).
///
/// This is a safe wrapper for deserializing types that directly implement Archive.
/// For Dynamic values, use [`from_bytes_owned`] instead.
///
/// # Example
///
/// ```ignore
/// use rhai::ImmutableString;
/// use rhai::rkyv::{to_bytes, from_bytes_owned_generic};
///
/// let value = ImmutableString::from("Hello, World!");
/// let bytes = to_bytes(&value)?;
///
/// let restored: ImmutableString = from_bytes_owned_generic(&bytes)?;
/// assert_eq!(value, restored);
/// # Ok::<_, Box<rhai::EvalAltResult>>(())
/// ```
#[inline(always)]
pub fn from_bytes_owned_generic<T>(bytes: &[u8]) -> RhaiResultOf<T>
where
    T: rkyv::Archive,
    T::Archived: Deserialize<T, Infallible>,
{
    // SAFETY: This is marked as safe because it's meant for convenience when
    // deserializing data from trusted sources (e.g., your own serialized data).
    // Users should only call this with data created by `to_bytes`.
    unsafe { from_bytes_owned_unchecked_generic(bytes) }
}

/// Deserialize bytes into a specific type T without validation (unsafe).
///
/// This is a generic deserialization function for types that directly implement Archive.
/// For Dynamic values, use [`from_bytes_owned_unchecked`] instead.
///
/// # Safety
///
/// See [`from_bytes_owned_unchecked`] for safety requirements.
pub unsafe fn from_bytes_owned_unchecked_generic<T>(bytes: &[u8]) -> RhaiResultOf<T>
where
    T: rkyv::Archive,
    T::Archived: Deserialize<T, Infallible>,
{
    let archived = rkyv::archived_root::<T>(bytes);
    Ok(archived.deserialize(&mut Infallible).unwrap())
}
