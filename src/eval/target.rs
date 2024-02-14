//! Type to hold a mutable reference to the target of an evaluation.

use crate::{Dynamic, Position, RhaiResultOf};
use std::borrow::{Borrow, BorrowMut};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Calculate an offset+len pair given an actual length of the underlying array.
///
/// Negative starting positions count from the end.
///
/// Values going over bounds are limited to the actual length.
#[cfg(not(feature = "no_index"))]
#[inline]
#[allow(
    clippy::cast_sign_loss,
    clippy::absurd_extreme_comparisons,
    clippy::cast_possible_truncation
)]
pub fn calc_offset_len(length: usize, start: crate::INT, len: crate::INT) -> (usize, usize) {
    let start = if start < 0 {
        let abs_start = start.unsigned_abs();

        #[allow(clippy::unnecessary_cast)]
        if abs_start as u64 > crate::MAX_USIZE_INT as u64 {
            0
        } else {
            length - usize::min(abs_start as usize, length)
        }
    } else if start > crate::MAX_USIZE_INT || start as usize >= length {
        return (length, 0);
    } else {
        start as usize
    };

    let len = if len <= 0 {
        0
    } else if len > crate::MAX_USIZE_INT || len as usize > length - start {
        length - start
    } else {
        len as usize
    };

    (start, len)
}

/// Calculate an offset+len pair given an actual length of the underlying array.
///
/// Negative starting positions count from the end.
///
/// Values going over bounds call the provided closure to return a default value or an error.
#[inline]
#[allow(dead_code)]
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::absurd_extreme_comparisons
)]
pub fn calc_index<E>(
    length: usize,
    index: crate::INT,
    negative_count_from_end: bool,
    err_func: impl FnOnce() -> Result<usize, E>,
) -> Result<usize, E> {
    if length == 0 {
        // Empty array, do nothing
    } else if index < 0 {
        if negative_count_from_end {
            let abs_index = index.unsigned_abs();

            #[allow(clippy::unnecessary_cast)]
            if abs_index as u64 <= crate::MAX_USIZE_INT as u64 && (abs_index as usize) <= length {
                return Ok(length - (abs_index as usize));
            }
        }
    } else if index <= crate::MAX_USIZE_INT && (index as usize) < length {
        return Ok(index as usize);
    }

    err_func()
}

/// A type that encapsulates a mutation target for an expression with side effects.
#[derive(Debug)]
#[must_use]
pub enum Target<'a> {
    /// The target is a mutable reference to a [`Dynamic`].
    RefMut(&'a mut Dynamic),
    /// The target is a mutable reference to a _shared_ [`Dynamic`].
    #[cfg(not(feature = "no_closure"))]
    SharedValue {
        /// Lock guard to the shared [`Dynamic`].
        guard: crate::types::dynamic::DynamicWriteLock<'a, Dynamic>,
        /// Copy of the shared value.
        shared_value: Dynamic,
    },
    /// The target is a temporary [`Dynamic`] value (i.e. its mutation can cause no side effects).
    TempValue(Dynamic),
    /// The target is a bit inside an [`INT`][crate::INT].
    /// This is necessary because directly pointing to a bit inside an [`INT`][crate::INT] is impossible.
    #[cfg(not(feature = "no_index"))]
    Bit {
        /// Mutable reference to the source [`Dynamic`].
        source: &'a mut Dynamic,
        /// Copy of the boolean bit, as a [`Dynamic`].
        value: Dynamic,
        /// Bit offset.
        bit: u8,
    },
    /// The target is a range of bits inside an [`INT`][crate::INT].
    /// This is necessary because directly pointing to a range of bits inside an [`INT`][crate::INT] is impossible.
    #[cfg(not(feature = "no_index"))]
    BitField {
        /// Mutable reference to the source [`Dynamic`].
        source: &'a mut Dynamic,
        /// Copy of the integer value of the bits, as a [`Dynamic`].
        value: Dynamic,
        /// Bitmask to apply to the source value (i.e. shifted)
        mask: crate::INT,
        /// Number of bits to right-shift the source value.
        shift: u8,
    },
    /// The target is a byte inside a [`Blob`][crate::Blob].
    /// This is necessary because directly pointing to a byte (in [`Dynamic`] form) inside a blob is impossible.
    #[cfg(not(feature = "no_index"))]
    BlobByte {
        /// Mutable reference to the source [`Dynamic`].
        source: &'a mut Dynamic,
        /// Copy of the byte at the index, as a [`Dynamic`].
        value: Dynamic,
        /// Offset index.
        index: usize,
    },
    /// The target is a character inside a string.
    /// This is necessary because directly pointing to a char inside a String is impossible.
    #[cfg(not(feature = "no_index"))]
    StringChar {
        /// Mutable reference to the source [`Dynamic`].
        source: &'a mut Dynamic,
        /// Copy of the character at the offset, as a [`Dynamic`].
        value: Dynamic,
        /// Offset index.
        index: usize,
    },
}

impl<'a> Target<'a> {
    /// Is the [`Target`] a reference pointing to other data?
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn is_ref(&self) -> bool {
        match self {
            Self::RefMut(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { .. } => true,
            Self::TempValue(..) => false,
            #[cfg(not(feature = "no_index"))]
            Self::Bit { .. }
            | Self::BitField { .. }
            | Self::BlobByte { .. }
            | Self::StringChar { .. } => false,
        }
    }
    /// Is the [`Target`] a temp value?
    #[inline]
    #[must_use]
    pub const fn is_temp_value(&self) -> bool {
        match self {
            Self::RefMut(..) => false,
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { .. } => false,
            Self::TempValue(..) => true,
            #[cfg(not(feature = "no_index"))]
            Self::Bit { .. }
            | Self::BitField { .. }
            | Self::BlobByte { .. }
            | Self::StringChar { .. } => false,
        }
    }
    /// Is the [`Target`] a shared value?
    #[inline]
    #[must_use]
    pub fn is_shared(&self) -> bool {
        #[cfg(not(feature = "no_closure"))]
        return match self {
            Self::RefMut(r) => r.is_shared(),
            Self::SharedValue { .. } => true,
            Self::TempValue(value) => value.is_shared(),
            #[cfg(not(feature = "no_index"))]
            Self::Bit { .. }
            | Self::BitField { .. }
            | Self::BlobByte { .. }
            | Self::StringChar { .. } => false,
        };
        #[cfg(feature = "no_closure")]
        return false;
    }
    /// Get the value of the [`Target`] as a [`Dynamic`], cloning a referenced value if necessary.
    #[inline]
    pub fn take_or_clone(self) -> Dynamic {
        match self {
            Self::RefMut(r) => r.clone(), // Referenced value is cloned
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { shared_value, .. } => shared_value, // Original shared value is simply taken
            Self::TempValue(value) => value, // Owned value is simply taken
            #[cfg(not(feature = "no_index"))]
            Self::Bit { value, .. } => value, // boolean is taken
            #[cfg(not(feature = "no_index"))]
            Self::BitField { value, .. } => value, // INT is taken
            #[cfg(not(feature = "no_index"))]
            Self::BlobByte { value, .. } => value, // byte is taken
            #[cfg(not(feature = "no_index"))]
            Self::StringChar { value, .. } => value, // char is taken
        }
    }
    /// Take a `&mut Dynamic` reference from the `Target`.
    #[inline(always)]
    #[must_use]
    pub fn take_ref(self) -> Option<&'a mut Dynamic> {
        match self {
            Self::RefMut(r) => Some(r),
            _ => None,
        }
    }
    /// Convert a shared or reference [`Target`] into a target with an owned value.
    #[inline(always)]
    pub fn into_owned(self) -> Self {
        match self {
            Self::RefMut(r) => Self::TempValue(r.clone()),
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { shared_value, .. } => Self::TempValue(shared_value),
            _ => self,
        }
    }
    /// Get the source [`Dynamic`] of the [`Target`].
    #[allow(dead_code)]
    #[inline]
    pub fn source(&self) -> &Dynamic {
        match self {
            Self::RefMut(r) => r,
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { guard, .. } => guard,
            Self::TempValue(value) => value,
            #[cfg(not(feature = "no_index"))]
            Self::Bit { source, .. } => source,
            #[cfg(not(feature = "no_index"))]
            Self::BitField { source, .. } => source,
            #[cfg(not(feature = "no_index"))]
            Self::BlobByte { source, .. } => source,
            #[cfg(not(feature = "no_index"))]
            Self::StringChar { source, .. } => source,
        }
    }
    /// Propagate a changed value back to the original source.
    /// This has no effect for direct references.
    #[inline]
    pub fn propagate_changed_value(&mut self, _pos: Position) -> RhaiResultOf<()> {
        match self {
            Self::RefMut(..) | Self::TempValue(..) => (),
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { .. } => (),
            #[cfg(not(feature = "no_index"))]
            Self::Bit { source, value, bit } => {
                // Replace the bit at the specified index position
                let new_bit = value.as_bool().map_err(|err| {
                    Box::new(crate::ERR::ErrorMismatchDataType(
                        "bool".to_string(),
                        err.to_string(),
                        _pos,
                    ))
                })?;

                let value = &mut *source.write_lock::<crate::INT>().unwrap();

                let index = *bit;

                let mask = 1 << index;
                if new_bit {
                    *value |= mask;
                } else {
                    *value &= !mask;
                }
            }
            #[cfg(not(feature = "no_index"))]
            Self::BitField {
                source,
                value,
                mask,
                shift,
            } => {
                let shift = *shift;
                let mask = *mask;

                // Replace the bit at the specified index position
                let new_value = value.as_int().map_err(|err| {
                    Box::new(crate::ERR::ErrorMismatchDataType(
                        "integer".to_string(),
                        err.to_string(),
                        _pos,
                    ))
                })?;

                let new_value = (new_value << shift) & mask;
                let value = &mut *source.write_lock::<crate::INT>().unwrap();

                *value &= !mask;
                *value |= new_value;
            }
            #[cfg(not(feature = "no_index"))]
            Self::BlobByte {
                source,
                value,
                index,
            } => {
                // Replace the byte at the specified index position
                let new_byte = value.as_int().map_err(|err| {
                    Box::new(crate::ERR::ErrorMismatchDataType(
                        "INT".to_string(),
                        err.to_string(),
                        _pos,
                    ))
                })?;

                let value = &mut *source.write_lock::<crate::Blob>().unwrap();

                #[allow(clippy::cast_sign_loss)]
                {
                    value[*index] = (new_byte & 0x00ff) as u8;
                }
            }
            #[cfg(not(feature = "no_index"))]
            Self::StringChar {
                source,
                value,
                index,
            } => {
                // Replace the character at the specified index position
                let new_ch = value.as_char().map_err(|err| {
                    Box::new(crate::ERR::ErrorMismatchDataType(
                        "char".to_string(),
                        err.to_string(),
                        _pos,
                    ))
                })?;

                let s = &mut *source.write_lock::<crate::ImmutableString>().unwrap();

                *s = s
                    .chars()
                    .enumerate()
                    .map(|(i, ch)| if i == *index { new_ch } else { ch })
                    .collect();
            }
        }

        Ok(())
    }
}

impl<'a> From<&'a mut Dynamic> for Target<'a> {
    #[inline]
    fn from(value: &'a mut Dynamic) -> Self {
        #[cfg(not(feature = "no_closure"))]
        if value.is_shared() {
            // Cloning is cheap for a shared value
            let shared_value = value.clone();
            let guard = value.write_lock::<Dynamic>().unwrap();
            return Self::SharedValue {
                guard,
                shared_value,
            };
        }

        Self::RefMut(value)
    }
}

impl AsRef<Dynamic> for Target<'_> {
    #[inline]
    fn as_ref(&self) -> &Dynamic {
        match self {
            Self::RefMut(r) => r,
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { guard, .. } => guard,
            Self::TempValue(ref value) => value,
            #[cfg(not(feature = "no_index"))]
            Self::Bit { ref value, .. }
            | Self::BitField { ref value, .. }
            | Self::BlobByte { ref value, .. }
            | Self::StringChar { ref value, .. } => value,
        }
    }
}

impl Borrow<Dynamic> for Target<'_> {
    #[inline(always)]
    fn borrow(&self) -> &Dynamic {
        self.as_ref()
    }
}

impl AsMut<Dynamic> for Target<'_> {
    #[inline]
    fn as_mut(&mut self) -> &mut Dynamic {
        match self {
            Self::RefMut(r) => r,
            #[cfg(not(feature = "no_closure"))]
            Self::SharedValue { guard, .. } => &mut *guard,
            Self::TempValue(ref mut value) => value,
            #[cfg(not(feature = "no_index"))]
            Self::Bit { ref mut value, .. }
            | Self::BitField { ref mut value, .. }
            | Self::BlobByte { ref mut value, .. }
            | Self::StringChar { ref mut value, .. } => value,
        }
    }
}

impl BorrowMut<Dynamic> for Target<'_> {
    #[inline(always)]
    fn borrow_mut(&mut self) -> &mut Dynamic {
        self.as_mut()
    }
}

impl<T: Into<Dynamic>> From<T> for Target<'_> {
    #[inline(always)]
    fn from(value: T) -> Self {
        Self::TempValue(value.into())
    }
}
