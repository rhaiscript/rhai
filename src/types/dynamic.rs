//! Helper module which defines the [`Dynamic`] data type.

use crate::{ExclusiveRange, FnPtr, ImmutableString, InclusiveRange, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::{type_name, Any, TypeId},
    fmt,
    hash::{Hash, Hasher},
    mem,
    ops::{Deref, DerefMut},
    str::FromStr,
};

pub use super::Variant;

#[cfg(not(feature = "no_time"))]
#[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
pub use std::time::Instant;

#[cfg(not(feature = "no_time"))]
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use web_time::Instant;

#[cfg(not(feature = "no_index"))]
use crate::{Array, Blob};

#[cfg(not(feature = "no_object"))]
use crate::Map;

/// _(internals)_ Modes of access.
/// Exported under the `internals` feature only.
#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[non_exhaustive]
pub enum AccessMode {
    /// Mutable.
    ReadWrite,
    /// Immutable.
    ReadOnly,
}

/// Arbitrary data attached to a [`Dynamic`] value.
#[cfg(target_pointer_width = "64")]
pub type Tag = i32;

/// Arbitrary data attached to a [`Dynamic`] value.
#[cfg(target_pointer_width = "32")]
pub type Tag = i16;

/// Default tag value for [`Dynamic`].
const DEFAULT_TAG_VALUE: Tag = 0;

/// Dynamic type containing any value.
#[must_use]
pub struct Dynamic(pub(crate) Union);

/// Internal [`Dynamic`] representation.
///
/// Most variants are boxed to reduce the size.
#[must_use]
pub enum Union {
    /// The Unit value - ().
    Unit((), Tag, AccessMode),
    /// A boolean value.
    Bool(bool, Tag, AccessMode),
    /// An [`ImmutableString`] value.
    Str(ImmutableString, Tag, AccessMode),
    /// A character value.
    Char(char, Tag, AccessMode),
    /// An integer value.
    Int(INT, Tag, AccessMode),
    /// A floating-point value.
    #[cfg(not(feature = "no_float"))]
    Float(super::FloatWrapper<crate::FLOAT>, Tag, AccessMode),
    /// _(decimal)_ A fixed-precision decimal value.
    /// Exported under the `decimal` feature only.
    #[cfg(feature = "decimal")]
    Decimal(Box<rust_decimal::Decimal>, Tag, AccessMode),
    /// An array value.
    #[cfg(not(feature = "no_index"))]
    Array(Box<Array>, Tag, AccessMode),
    /// An blob (byte array).
    #[cfg(not(feature = "no_index"))]
    Blob(Box<Blob>, Tag, AccessMode),
    /// An object map value.
    #[cfg(not(feature = "no_object"))]
    Map(Box<Map>, Tag, AccessMode),
    /// A function pointer.
    FnPtr(Box<FnPtr>, Tag, AccessMode),
    /// A timestamp value.
    #[cfg(not(feature = "no_time"))]
    TimeStamp(Box<Instant>, Tag, AccessMode),

    /// Any type as a trait object.
    ///
    /// An extra level of redirection is used in order to avoid bloating the size of [`Dynamic`]
    /// because `Box<dyn Variant>` is a fat pointer.
    Variant(Box<Box<dyn Variant>>, Tag, AccessMode),

    /// A _shared_ value of any type.
    #[cfg(not(feature = "no_closure"))]
    Shared(crate::Shared<crate::Locked<Dynamic>>, Tag, AccessMode),
}

/// _(internals)_ Lock guard for reading a [`Dynamic`].
/// Exported under the `internals` feature only.
///
/// This type provides transparent interoperability between normal [`Dynamic`] and shared
/// [`Dynamic`] values.
#[derive(Debug)]
#[must_use]
pub struct DynamicReadLock<'d, T: Clone>(DynamicReadLockInner<'d, T>);

/// Different types of read guards for [`DynamicReadLock`].
#[derive(Debug)]
#[must_use]
enum DynamicReadLockInner<'d, T: Clone> {
    /// A simple reference to a non-shared value.
    Reference(&'d T),

    /// A read guard to a _shared_ value.
    #[cfg(not(feature = "no_closure"))]
    Guard(crate::func::native::LockGuard<'d, Dynamic>),
}

impl<T: Any + Clone> Deref for DynamicReadLock<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self.0 {
            DynamicReadLockInner::Reference(reference) => reference,
            #[cfg(not(feature = "no_closure"))]
            DynamicReadLockInner::Guard(ref guard) => guard.downcast_ref().unwrap(),
        }
    }
}

/// _(internals)_ Lock guard for writing a [`Dynamic`].
/// Exported under the `internals` feature only.
///
/// This type provides transparent interoperability between normal [`Dynamic`] and shared
/// [`Dynamic`] values.
#[derive(Debug)]
#[must_use]
pub struct DynamicWriteLock<'d, T: Clone>(DynamicWriteLockInner<'d, T>);

/// Different types of write guards for [`DynamicReadLock`].
#[derive(Debug)]
#[must_use]
enum DynamicWriteLockInner<'d, T: Clone> {
    /// A simple mutable reference to a non-shared value.
    Reference(&'d mut T),

    /// A write guard to a _shared_ value.
    #[cfg(not(feature = "no_closure"))]
    Guard(crate::func::native::LockGuardMut<'d, Dynamic>),
}

impl<T: Any + Clone> Deref for DynamicWriteLock<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self.0 {
            DynamicWriteLockInner::Reference(ref reference) => reference,
            #[cfg(not(feature = "no_closure"))]
            DynamicWriteLockInner::Guard(ref guard) => guard.downcast_ref().unwrap(),
        }
    }
}

impl<T: Any + Clone> DerefMut for DynamicWriteLock<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self.0 {
            DynamicWriteLockInner::Reference(ref mut reference) => reference,
            #[cfg(not(feature = "no_closure"))]
            DynamicWriteLockInner::Guard(ref mut guard) => guard.downcast_mut().unwrap(),
        }
    }
}

impl Dynamic {
    /// Get the arbitrary data attached to this [`Dynamic`].
    #[must_use]
    pub const fn tag(&self) -> Tag {
        match self.0 {
            Union::Unit((), tag, _)
            | Union::Bool(_, tag, _)
            | Union::Str(_, tag, _)
            | Union::Char(_, tag, _)
            | Union::Int(_, tag, _)
            | Union::FnPtr(_, tag, _)
            | Union::Variant(_, tag, _) => tag,

            #[cfg(not(feature = "no_float"))]
            Union::Float(_, tag, _) => tag,
            #[cfg(feature = "decimal")]
            Union::Decimal(_, tag, _) => tag,
            #[cfg(not(feature = "no_index"))]
            Union::Array(_, tag, _) | Union::Blob(_, tag, _) => tag,
            #[cfg(not(feature = "no_object"))]
            Union::Map(_, tag, _) => tag,
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(_, tag, _) => tag,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(_, tag, _) => tag,
        }
    }
    /// Attach arbitrary data to this [`Dynamic`].
    pub fn set_tag(&mut self, value: Tag) -> &mut Self {
        match self.0 {
            Union::Unit((), ref mut tag, _)
            | Union::Bool(_, ref mut tag, _)
            | Union::Str(_, ref mut tag, _)
            | Union::Char(_, ref mut tag, _)
            | Union::Int(_, ref mut tag, _)
            | Union::FnPtr(_, ref mut tag, _)
            | Union::Variant(_, ref mut tag, _) => *tag = value,

            #[cfg(not(feature = "no_float"))]
            Union::Float(_, ref mut tag, _) => *tag = value,
            #[cfg(feature = "decimal")]
            Union::Decimal(_, ref mut tag, _) => *tag = value,
            #[cfg(not(feature = "no_index"))]
            Union::Array(_, ref mut tag, _) | Union::Blob(_, ref mut tag, _) => *tag = value,
            #[cfg(not(feature = "no_object"))]
            Union::Map(_, ref mut tag, _) => *tag = value,
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(_, ref mut tag, _) => *tag = value,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(_, ref mut tag, _) => *tag = value,
        }
        self
    }
    /// Does this [`Dynamic`] hold a variant data type instead of one of the supported system
    /// primitive types?
    #[inline(always)]
    #[must_use]
    pub const fn is_variant(&self) -> bool {
        matches!(self.0, Union::Variant(..))
    }
    /// Is the value held by this [`Dynamic`] shared?
    ///
    /// Not available under `no_closure`.
    #[cfg(not(feature = "no_closure"))]
    #[inline(always)]
    #[must_use]
    pub const fn is_shared(&self) -> bool {
        matches!(self.0, Union::Shared(..))
    }
    /// Is the value held by this [`Dynamic`] a particular type?
    ///
    /// # Panics or Deadlocks When Value is Shared
    ///
    /// Under the `sync` feature, this call may deadlock, or [panic](https://doc.rust-lang.org/std/sync/struct.RwLock.html#panics-1).
    /// Otherwise, this call panics if the data is currently borrowed for write.
    #[inline]
    #[must_use]
    pub fn is<T: Any + Clone>(&self) -> bool {
        #[cfg(not(feature = "no_closure"))]
        if self.is_shared() {
            return TypeId::of::<T>() == self.type_id();
        }

        if TypeId::of::<T>() == TypeId::of::<()>() {
            return matches!(self.0, Union::Unit(..));
        }
        if TypeId::of::<T>() == TypeId::of::<bool>() {
            return matches!(self.0, Union::Bool(..));
        }
        if TypeId::of::<T>() == TypeId::of::<char>() {
            return matches!(self.0, Union::Char(..));
        }
        if TypeId::of::<T>() == TypeId::of::<INT>() {
            return matches!(self.0, Union::Int(..));
        }
        #[cfg(not(feature = "no_float"))]
        if TypeId::of::<T>() == TypeId::of::<crate::FLOAT>() {
            return matches!(self.0, Union::Float(..));
        }
        if TypeId::of::<T>() == TypeId::of::<ImmutableString>()
            || TypeId::of::<T>() == TypeId::of::<String>()
        {
            return matches!(self.0, Union::Str(..));
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            return matches!(self.0, Union::Array(..));
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Blob>() {
            return matches!(self.0, Union::Blob(..));
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            return matches!(self.0, Union::Map(..));
        }
        #[cfg(feature = "decimal")]
        if TypeId::of::<T>() == TypeId::of::<rust_decimal::Decimal>() {
            return matches!(self.0, Union::Decimal(..));
        }
        if TypeId::of::<T>() == TypeId::of::<FnPtr>() {
            return matches!(self.0, Union::FnPtr(..));
        }
        #[cfg(not(feature = "no_time"))]
        if TypeId::of::<T>() == TypeId::of::<crate::Instant>() {
            return matches!(self.0, Union::TimeStamp(..));
        }

        TypeId::of::<T>() == self.type_id()
    }
    /// Get the [`TypeId`] of the value held by this [`Dynamic`].
    ///
    /// # Panics or Deadlocks When Value is Shared
    ///
    /// Under the `sync` feature, this call may deadlock, or [panic](https://doc.rust-lang.org/std/sync/struct.RwLock.html#panics-1).
    /// Otherwise, this call panics if the data is currently borrowed for write.
    #[must_use]
    pub fn type_id(&self) -> TypeId {
        match self.0 {
            Union::Unit(..) => TypeId::of::<()>(),
            Union::Bool(..) => TypeId::of::<bool>(),
            Union::Str(..) => TypeId::of::<ImmutableString>(),
            Union::Char(..) => TypeId::of::<char>(),
            Union::Int(..) => TypeId::of::<INT>(),
            #[cfg(not(feature = "no_float"))]
            Union::Float(..) => TypeId::of::<crate::FLOAT>(),
            #[cfg(feature = "decimal")]
            Union::Decimal(..) => TypeId::of::<rust_decimal::Decimal>(),
            #[cfg(not(feature = "no_index"))]
            Union::Array(..) => TypeId::of::<Array>(),
            #[cfg(not(feature = "no_index"))]
            Union::Blob(..) => TypeId::of::<Blob>(),
            #[cfg(not(feature = "no_object"))]
            Union::Map(..) => TypeId::of::<Map>(),
            Union::FnPtr(..) => TypeId::of::<FnPtr>(),
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(..) => TypeId::of::<Instant>(),

            Union::Variant(ref v, ..) => (***v).type_id(),

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => (*crate::func::locked_read(cell).unwrap()).type_id(),
        }
    }
    /// Get the name of the type of the value held by this [`Dynamic`].
    ///
    /// # Panics or Deadlocks When Value is Shared
    ///
    /// Under the `sync` feature, this call may deadlock, or [panic](https://doc.rust-lang.org/std/sync/struct.RwLock.html#panics-1).
    /// Otherwise, this call panics if the data is currently borrowed for write.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self.0 {
            Union::Unit(..) => "()",
            Union::Bool(..) => "bool",
            Union::Str(..) => "string",
            Union::Char(..) => "char",
            Union::Int(..) => type_name::<INT>(),
            #[cfg(not(feature = "no_float"))]
            Union::Float(..) => type_name::<crate::FLOAT>(),
            #[cfg(feature = "decimal")]
            Union::Decimal(..) => "decimal",
            #[cfg(not(feature = "no_index"))]
            Union::Array(..) => "array",
            #[cfg(not(feature = "no_index"))]
            Union::Blob(..) => "blob",
            #[cfg(not(feature = "no_object"))]
            Union::Map(..) => "map",
            Union::FnPtr(..) => "Fn",
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(..) => "timestamp",

            Union::Variant(ref v, ..) => (***v).type_name(),

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => (*crate::func::locked_read(cell).unwrap()).type_name(),
        }
    }
}

impl Hash for Dynamic {
    /// Hash the [`Dynamic`] value.
    ///
    /// # Panics
    ///
    /// Panics if the [`Dynamic`] value contains an unrecognized trait object.
    fn hash<H: Hasher>(&self, state: &mut H) {
        mem::discriminant(&self.0).hash(state);

        match self.0 {
            Union::Unit(..) => (),
            Union::Bool(ref b, ..) => b.hash(state),
            Union::Str(ref s, ..) => s.hash(state),
            Union::Char(ref c, ..) => c.hash(state),
            Union::Int(ref i, ..) => i.hash(state),
            #[cfg(not(feature = "no_float"))]
            Union::Float(ref f, ..) => f.hash(state),
            #[cfg(feature = "decimal")]
            Union::Decimal(ref d, ..) => d.hash(state),
            #[cfg(not(feature = "no_index"))]
            Union::Array(ref a, ..) => a.hash(state),
            #[cfg(not(feature = "no_index"))]
            Union::Blob(ref a, ..) => a.hash(state),
            #[cfg(not(feature = "no_object"))]
            Union::Map(ref m, ..) => m.hash(state),
            #[cfg(not(feature = "no_function"))]
            Union::FnPtr(ref f, ..) if f.env.is_some() => {
                unimplemented!("FnPtr with embedded environment cannot be hashed")
            }
            Union::FnPtr(ref f, ..) => {
                f.fn_name().hash(state);
                f.curry().hash(state);
            }

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => (*crate::func::locked_read(cell).unwrap()).hash(state),

            Union::Variant(ref v, ..) => {
                let _value_any = (***v).as_any();

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                if let Some(value) = _value_any.downcast_ref::<u8>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<u16>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<u32>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<u64>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<i8>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<i16>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<i32>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<i64>() {
                    return value.hash(state);
                }

                #[cfg(not(feature = "no_float"))]
                #[cfg(not(feature = "f32_float"))]
                if let Some(value) = _value_any.downcast_ref::<f32>() {
                    return value.to_ne_bytes().hash(state);
                }
                #[cfg(not(feature = "no_float"))]
                #[cfg(feature = "f32_float")]
                if let Some(value) = _value_any.downcast_ref::<f64>() {
                    return value.to_ne_bytes().hash(state);
                }

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                #[cfg(not(target_family = "wasm"))]
                if let Some(value) = _value_any.downcast_ref::<u128>() {
                    return value.hash(state);
                } else if let Some(value) = _value_any.downcast_ref::<i128>() {
                    return value.hash(state);
                }

                if let Some(range) = _value_any.downcast_ref::<ExclusiveRange>() {
                    return range.hash(state);
                } else if let Some(range) = _value_any.downcast_ref::<InclusiveRange>() {
                    return range.hash(state);
                }

                unimplemented!("Custom type {} cannot be hashed", self.type_name())
            }

            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(..) => unimplemented!("Timestamp cannot be hashed"),
        }
    }
}

impl fmt::Display for Dynamic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Union::Unit(..) => Ok(()),
            Union::Bool(ref v, ..) => fmt::Display::fmt(v, f),
            Union::Str(ref v, ..) => fmt::Display::fmt(v, f),
            Union::Char(ref v, ..) => fmt::Display::fmt(v, f),
            Union::Int(ref v, ..) => fmt::Display::fmt(v, f),
            #[cfg(not(feature = "no_float"))]
            Union::Float(ref v, ..) => fmt::Display::fmt(v, f),
            #[cfg(feature = "decimal")]
            Union::Decimal(ref v, ..) => fmt::Display::fmt(v, f),
            #[cfg(not(feature = "no_index"))]
            Union::Array(..) => fmt::Debug::fmt(self, f),
            #[cfg(not(feature = "no_index"))]
            Union::Blob(..) => fmt::Debug::fmt(self, f),
            #[cfg(not(feature = "no_object"))]
            Union::Map(..) => fmt::Debug::fmt(self, f),
            Union::FnPtr(ref v, ..) => fmt::Display::fmt(v, f),
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(..) => f.write_str("<timestamp>"),

            Union::Variant(ref v, ..) => {
                let _value_any = (***v).as_any();
                let _type_id = _value_any.type_id();

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                if let Some(value) = _value_any.downcast_ref::<u8>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<u16>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<u32>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<u64>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i8>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i16>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i32>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i64>() {
                    return fmt::Display::fmt(value, f);
                }

                #[cfg(not(feature = "no_float"))]
                #[cfg(not(feature = "f32_float"))]
                if let Some(value) = _value_any.downcast_ref::<f32>() {
                    return fmt::Display::fmt(value, f);
                }
                #[cfg(not(feature = "no_float"))]
                #[cfg(feature = "f32_float")]
                if let Some(value) = _value_any.downcast_ref::<f64>() {
                    return fmt::Display::fmt(value, f);
                }

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                #[cfg(not(target_family = "wasm"))]
                if let Some(value) = _value_any.downcast_ref::<u128>() {
                    return fmt::Display::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i128>() {
                    return fmt::Display::fmt(value, f);
                }

                if let Some(range) = _value_any.downcast_ref::<ExclusiveRange>() {
                    return if range.end == INT::MAX {
                        write!(f, "{}..", range.start)
                    } else {
                        write!(f, "{}..{}", range.start, range.end)
                    };
                } else if let Some(range) = _value_any.downcast_ref::<InclusiveRange>() {
                    return if *range.end() == INT::MAX {
                        write!(f, "{}..=", range.start())
                    } else {
                        write!(f, "{}..={}", range.start(), range.end())
                    };
                }

                f.write_str((***v).type_name())
            }

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) if cfg!(feature = "unchecked") => {
                match crate::func::locked_read(cell) {
                    Some(v) => write!(f, "{} (shared)", *v),
                    _ => f.write_str("<shared>"),
                }
            }
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => {
                #[cfg(feature = "no_std")]
                use hashbrown::HashSet;
                #[cfg(not(feature = "no_std"))]
                use std::collections::HashSet;

                // Avoid infinite recursion for shared values in a reference loop.
                fn display_fmt_print(
                    f: &mut fmt::Formatter<'_>,
                    value: &Dynamic,
                    dict: &mut HashSet<*const Dynamic>,
                ) -> fmt::Result {
                    match value.0 {
                        #[cfg(not(feature = "no_closure"))]
                        Union::Shared(ref cell, ..) => match crate::func::locked_read(cell) {
                            Some(v) => {
                                if dict.insert(value) {
                                    display_fmt_print(f, &v, dict)?;
                                    f.write_str(" (shared)")
                                } else {
                                    f.write_str("<shared>")
                                }
                            }
                            _ => f.write_str("<shared>"),
                        },
                        #[cfg(not(feature = "no_index"))]
                        Union::Array(ref arr, ..) => {
                            dict.insert(value);

                            f.write_str("[")?;
                            for (i, v) in arr.iter().enumerate() {
                                if i > 0 {
                                    f.write_str(", ")?;
                                }
                                display_fmt_print(f, v, dict)?;
                            }
                            f.write_str("]")
                        }
                        #[cfg(not(feature = "no_object"))]
                        Union::Map(ref map, ..) => {
                            dict.insert(value);

                            f.write_str("#{")?;
                            for (i, (k, v)) in map.iter().enumerate() {
                                if i > 0 {
                                    f.write_str(", ")?;
                                }
                                fmt::Display::fmt(k, f)?;
                                f.write_str(": ")?;
                                display_fmt_print(f, v, dict)?;
                            }
                            f.write_str("}")
                        }
                        _ => fmt::Display::fmt(value, f),
                    }
                }

                display_fmt_print(f, self, &mut <_>::default())
            }
        }
    }
}

impl fmt::Debug for Dynamic {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Union::Unit(ref v, ..) => fmt::Debug::fmt(v, f),
            Union::Bool(ref v, ..) => fmt::Debug::fmt(v, f),
            Union::Str(ref v, ..) => fmt::Debug::fmt(v, f),
            Union::Char(ref v, ..) => fmt::Debug::fmt(v, f),
            Union::Int(ref v, ..) => fmt::Debug::fmt(v, f),
            #[cfg(not(feature = "no_float"))]
            Union::Float(ref v, ..) => fmt::Debug::fmt(v, f),
            #[cfg(feature = "decimal")]
            Union::Decimal(ref v, ..) => fmt::Debug::fmt(v, f),
            #[cfg(not(feature = "no_index"))]
            Union::Array(ref v, ..) => fmt::Debug::fmt(v, f),
            #[cfg(not(feature = "no_index"))]
            Union::Blob(ref v, ..) => {
                f.write_str("[")?;
                v.iter().enumerate().try_for_each(|(i, v)| {
                    if i > 0 && i % 8 == 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{v:02x}")
                })?;
                f.write_str("]")
            }
            #[cfg(not(feature = "no_object"))]
            Union::Map(ref v, ..) => {
                f.write_str("#")?;
                fmt::Debug::fmt(v, f)
            }
            Union::FnPtr(ref v, ..) => fmt::Debug::fmt(v, f),
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(..) => write!(f, "<timestamp>"),

            Union::Variant(ref v, ..) => {
                let _value_any = (***v).as_any();
                let _type_id = _value_any.type_id();

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                if let Some(value) = _value_any.downcast_ref::<u8>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<u16>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<u32>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<u64>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i8>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i16>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i32>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i64>() {
                    return fmt::Debug::fmt(value, f);
                }

                #[cfg(not(feature = "no_float"))]
                #[cfg(not(feature = "f32_float"))]
                if let Some(value) = _value_any.downcast_ref::<f32>() {
                    return fmt::Debug::fmt(value, f);
                }
                #[cfg(not(feature = "no_float"))]
                #[cfg(feature = "f32_float")]
                if let Some(value) = _value_any.downcast_ref::<f64>() {
                    return fmt::Debug::fmt(value, f);
                }

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                #[cfg(not(target_family = "wasm"))]
                if let Some(value) = _value_any.downcast_ref::<u128>() {
                    return fmt::Debug::fmt(value, f);
                } else if let Some(value) = _value_any.downcast_ref::<i128>() {
                    return fmt::Debug::fmt(value, f);
                }

                if let Some(range) = _value_any.downcast_ref::<ExclusiveRange>() {
                    return if range.end == INT::MAX {
                        write!(f, "{}..", range.start)
                    } else {
                        write!(f, "{}..{}", range.start, range.end)
                    };
                } else if let Some(range) = _value_any.downcast_ref::<InclusiveRange>() {
                    return if *range.end() == INT::MAX {
                        write!(f, "{}..=", range.start())
                    } else {
                        write!(f, "{}..={}", range.start(), range.end())
                    };
                }

                f.write_str((***v).type_name())
            }

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) if cfg!(feature = "unchecked") => {
                match crate::func::locked_read(cell) {
                    Some(v) => write!(f, "{:?} (shared)", *v),
                    _ => f.write_str("<shared>"),
                }
            }
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => {
                #[cfg(feature = "no_std")]
                use hashbrown::HashSet;
                #[cfg(not(feature = "no_std"))]
                use std::collections::HashSet;

                // Avoid infinite recursion for shared values in a reference loop.
                fn checked_debug_fmt(
                    f: &mut fmt::Formatter<'_>,
                    value: &Dynamic,
                    dict: &mut HashSet<*const Dynamic>,
                ) -> fmt::Result {
                    match value.0 {
                        Union::Shared(ref cell, ..) => match crate::func::locked_read(cell) {
                            Some(v) => {
                                if dict.insert(value) {
                                    checked_debug_fmt(f, &v, dict)?;
                                    f.write_str(" (shared)")
                                } else {
                                    f.write_str("<shared>")
                                }
                            }
                            _ => f.write_str("<shared>"),
                        },
                        #[cfg(not(feature = "no_index"))]
                        Union::Array(ref arr, ..) => {
                            dict.insert(value);

                            f.write_str("[")?;
                            for (i, v) in arr.iter().enumerate() {
                                if i > 0 {
                                    f.write_str(", ")?;
                                }
                                checked_debug_fmt(f, v, dict)?;
                            }
                            f.write_str("]")
                        }
                        #[cfg(not(feature = "no_object"))]
                        Union::Map(ref map, ..) => {
                            dict.insert(value);

                            f.write_str("#{")?;
                            for (i, (k, v)) in map.iter().enumerate() {
                                if i > 0 {
                                    f.write_str(", ")?;
                                }
                                fmt::Debug::fmt(k, f)?;
                                f.write_str(": ")?;
                                checked_debug_fmt(f, v, dict)?;
                            }
                            f.write_str("}")
                        }
                        Union::FnPtr(ref fnptr, ..) => {
                            dict.insert(value);

                            fmt::Display::fmt(&fnptr.typ, f)?;
                            f.write_str("(")?;
                            fmt::Debug::fmt(fnptr.fn_name(), f)?;
                            for curry in &fnptr.curry {
                                f.write_str(", ")?;
                                checked_debug_fmt(f, curry, dict)?;
                            }
                            f.write_str(")")
                        }
                        _ => fmt::Debug::fmt(value, f),
                    }
                }

                checked_debug_fmt(f, self, &mut <_>::default())
            }
        }
    }
}

#[allow(clippy::enum_glob_use)]
use AccessMode::*;

impl Clone for Dynamic {
    /// Clone the [`Dynamic`] value.
    ///
    /// # WARNING
    ///
    /// The cloned copy is marked read-write even if the original is read-only.
    fn clone(&self) -> Self {
        match self.0 {
            Union::Unit(v, tag, ..) => Self(Union::Unit(v, tag, ReadWrite)),
            Union::Bool(v, tag, ..) => Self(Union::Bool(v, tag, ReadWrite)),
            Union::Str(ref v, tag, ..) => Self(Union::Str(v.clone(), tag, ReadWrite)),
            Union::Char(v, tag, ..) => Self(Union::Char(v, tag, ReadWrite)),
            Union::Int(v, tag, ..) => Self(Union::Int(v, tag, ReadWrite)),
            #[cfg(not(feature = "no_float"))]
            Union::Float(v, tag, ..) => Self(Union::Float(v, tag, ReadWrite)),
            #[cfg(feature = "decimal")]
            Union::Decimal(ref v, tag, ..) => Self(Union::Decimal(v.clone(), tag, ReadWrite)),
            #[cfg(not(feature = "no_index"))]
            Union::Array(ref v, tag, ..) => Self(Union::Array(v.clone(), tag, ReadWrite)),
            #[cfg(not(feature = "no_index"))]
            Union::Blob(ref v, tag, ..) => Self(Union::Blob(v.clone(), tag, ReadWrite)),
            #[cfg(not(feature = "no_object"))]
            Union::Map(ref v, tag, ..) => Self(Union::Map(v.clone(), tag, ReadWrite)),
            Union::FnPtr(ref v, tag, ..) => Self(Union::FnPtr(v.clone(), tag, ReadWrite)),
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(ref v, tag, ..) => Self(Union::TimeStamp(v.clone(), tag, ReadWrite)),

            Union::Variant(ref v, tag, ..) => Self(Union::Variant(
                v.as_ref().as_ref().clone_object().into(),
                tag,
                ReadWrite,
            )),

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, tag, ..) => Self(Union::Shared(cell.clone(), tag, ReadWrite)),
        }
    }
}

impl Default for Dynamic {
    #[inline(always)]
    fn default() -> Self {
        Self::UNIT
    }
}

#[cfg(not(feature = "no_float"))]
#[cfg(feature = "f32_float")]
use std::f32::consts as FloatConstants;
#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "f32_float"))]
use std::f64::consts as FloatConstants;

impl Dynamic {
    /// A [`Dynamic`] containing a `()`.
    pub const UNIT: Self = Self(Union::Unit((), DEFAULT_TAG_VALUE, ReadWrite));
    /// A [`Dynamic`] containing a `true`.
    pub const TRUE: Self = Self::from_bool(true);
    /// A [`Dynamic`] containing a [`false`].
    pub const FALSE: Self = Self::from_bool(false);
    /// A [`Dynamic`] containing the integer zero.
    pub const ZERO: Self = Self::from_int(0);
    /// A [`Dynamic`] containing the integer 1.
    pub const ONE: Self = Self::from_int(1);
    /// A [`Dynamic`] containing the integer 2.
    pub const TWO: Self = Self::from_int(2);
    /// A [`Dynamic`] containing the integer 3.
    pub const THREE: Self = Self::from_int(3);
    /// A [`Dynamic`] containing the integer 10.
    pub const TEN: Self = Self::from_int(10);
    /// A [`Dynamic`] containing the integer 100.
    pub const HUNDRED: Self = Self::from_int(100);
    /// A [`Dynamic`] containing the integer 1,000.
    pub const THOUSAND: Self = Self::from_int(1000);
    /// A [`Dynamic`] containing the integer 1,000,000.
    pub const MILLION: Self = Self::from_int(1_000_000);
    /// A [`Dynamic`] containing the integer -1.
    pub const NEGATIVE_ONE: Self = Self::from_int(-1);
    /// A [`Dynamic`] containing the integer -2.
    pub const NEGATIVE_TWO: Self = Self::from_int(-2);
    /// A [`Dynamic`] containing `0.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_ZERO: Self = Self::from_float(0.0);
    /// A [`Dynamic`] containing `1.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_ONE: Self = Self::from_float(1.0);
    /// A [`Dynamic`] containing `2.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_TWO: Self = Self::from_float(2.0);
    /// A [`Dynamic`] containing `10.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_TEN: Self = Self::from_float(10.0);
    /// A [`Dynamic`] containing `100.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_HUNDRED: Self = Self::from_float(100.0);
    /// A [`Dynamic`] containing `1000.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_THOUSAND: Self = Self::from_float(1000.0);
    /// A [`Dynamic`] containing `1000000.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_MILLION: Self = Self::from_float(1_000_000.0);
    /// A [`Dynamic`] containing `-1.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_NEGATIVE_ONE: Self = Self::from_float(-1.0);
    /// A [`Dynamic`] containing `-2.0`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_NEGATIVE_TWO: Self = Self::from_float(-2.0);
    /// A [`Dynamic`] containing `0.5`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_HALF: Self = Self::from_float(0.5);
    /// A [`Dynamic`] containing `0.25`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_QUARTER: Self = Self::from_float(0.25);
    /// A [`Dynamic`] containing `0.2`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_FIFTH: Self = Self::from_float(0.2);
    /// A [`Dynamic`] containing `0.1`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_TENTH: Self = Self::from_float(0.1);
    /// A [`Dynamic`] containing `0.01`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_HUNDREDTH: Self = Self::from_float(0.01);
    /// A [`Dynamic`] containing `0.001`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_THOUSANDTH: Self = Self::from_float(0.001);
    /// A [`Dynamic`] containing `0.000001`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_MILLIONTH: Self = Self::from_float(0.000_001);
    /// A [`Dynamic`] containing π.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_PI: Self = Self::from_float(FloatConstants::PI);
    /// A [`Dynamic`] containing π/2.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_HALF_PI: Self = Self::from_float(FloatConstants::FRAC_PI_2);
    /// A [`Dynamic`] containing π/4.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_QUARTER_PI: Self = Self::from_float(FloatConstants::FRAC_PI_4);
    /// A [`Dynamic`] containing 2π.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_TWO_PI: Self = Self::from_float(FloatConstants::TAU);
    /// A [`Dynamic`] containing 1/π.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_INVERSE_PI: Self = Self::from_float(FloatConstants::FRAC_1_PI);
    /// A [`Dynamic`] containing _e_.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_E: Self = Self::from_float(FloatConstants::E);
    /// A [`Dynamic`] containing `log` _e_.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_LOG_E: Self = Self::from_float(FloatConstants::LOG10_E);
    /// A [`Dynamic`] containing `ln 10`.
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    pub const FLOAT_LN_10: Self = Self::from_float(FloatConstants::LN_10);

    /// Create a new [`Dynamic`] from a [`bool`].
    #[inline(always)]
    pub const fn from_bool(value: bool) -> Self {
        Self(Union::Bool(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
    /// Create a new [`Dynamic`] from an [`INT`].
    #[inline(always)]
    pub const fn from_int(value: INT) -> Self {
        Self(Union::Int(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
    /// Create a new [`Dynamic`] from a [`char`].
    #[inline(always)]
    pub const fn from_char(value: char) -> Self {
        Self(Union::Char(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
    /// Create a new [`Dynamic`] from a [`FLOAT`][crate::FLOAT].
    ///
    /// Not available under `no_float`.
    #[cfg(not(feature = "no_float"))]
    #[inline(always)]
    pub const fn from_float(value: crate::FLOAT) -> Self {
        Self(Union::Float(
            super::FloatWrapper::new(value),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
    /// Create a new [`Dynamic`] from a [`Decimal`](https://docs.rs/rust_decimal).
    ///
    /// Exported under the `decimal` feature only.
    #[cfg(feature = "decimal")]
    #[inline(always)]
    pub fn from_decimal(value: rust_decimal::Decimal) -> Self {
        Self(Union::Decimal(value.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
    /// Create a [`Dynamic`] from an [`Array`].
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn from_array(array: Array) -> Self {
        Self(Union::Array(array.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
    /// Create a [`Dynamic`] from a [`Blob`].
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn from_blob(blob: Blob) -> Self {
        Self(Union::Blob(blob.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
    /// Create a [`Dynamic`] from a [`Map`].
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn from_map(map: Map) -> Self {
        Self(Union::Map(map.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
    /// Create a new [`Dynamic`] from an [`Instant`].
    ///
    /// Not available under `no-std` or `no_time`.
    #[cfg(not(feature = "no_time"))]
    #[inline(always)]
    pub fn from_timestamp(value: Instant) -> Self {
        Self(Union::TimeStamp(value.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }

    /// Get the [`AccessMode`] for this [`Dynamic`].
    #[must_use]
    pub(crate) const fn access_mode(&self) -> AccessMode {
        match self.0 {
            Union::Unit(.., access)
            | Union::Bool(.., access)
            | Union::Str(.., access)
            | Union::Char(.., access)
            | Union::Int(.., access)
            | Union::FnPtr(.., access)
            | Union::Variant(.., access) => access,

            #[cfg(not(feature = "no_float"))]
            Union::Float(.., access) => access,
            #[cfg(feature = "decimal")]
            Union::Decimal(.., access) => access,
            #[cfg(not(feature = "no_index"))]
            Union::Array(.., access) | Union::Blob(.., access) => access,
            #[cfg(not(feature = "no_object"))]
            Union::Map(.., access) => access,
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(.., access) => access,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(.., access) => access,
        }
    }
    /// Set the [`AccessMode`] for this [`Dynamic`].
    pub(crate) fn set_access_mode(&mut self, typ: AccessMode) -> &mut Self {
        match self.0 {
            Union::Unit(.., ref mut access)
            | Union::Bool(.., ref mut access)
            | Union::Str(.., ref mut access)
            | Union::Char(.., ref mut access)
            | Union::Int(.., ref mut access)
            | Union::FnPtr(.., ref mut access)
            | Union::Variant(.., ref mut access) => *access = typ,

            #[cfg(not(feature = "no_float"))]
            Union::Float(.., ref mut access) => *access = typ,
            #[cfg(feature = "decimal")]
            Union::Decimal(.., ref mut access) => *access = typ,
            #[cfg(not(feature = "no_index"))]
            Union::Array(ref mut a, _, ref mut access) => {
                *access = typ;
                for v in a.as_mut() {
                    v.set_access_mode(typ);
                }
            }
            #[cfg(not(feature = "no_index"))]
            Union::Blob(.., ref mut access) => *access = typ,
            #[cfg(not(feature = "no_object"))]
            Union::Map(ref mut m, _, ref mut access) => {
                *access = typ;
                for v in m.values_mut() {
                    v.set_access_mode(typ);
                }
            }
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(.., ref mut access) => *access = typ,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(.., ref mut access) => *access = typ,
        }
        self
    }
    /// Make this [`Dynamic`] read-only (i.e. a constant).
    #[inline(always)]
    pub fn into_read_only(self) -> Self {
        let mut value = self;
        value.set_access_mode(AccessMode::ReadOnly);
        value
    }
    /// Is this [`Dynamic`] read-only?
    ///
    /// Constant [`Dynamic`] values are read-only.
    ///
    /// # Usage
    ///
    /// If a [`&mut Dynamic`][Dynamic] to such a constant is passed to a Rust function, the function
    /// can use this information to return the error
    /// [`ErrorAssignmentToConstant`][crate::EvalAltResult::ErrorAssignmentToConstant] if its value
    /// will be modified.
    ///
    /// This safe-guards constant values from being modified within Rust functions.
    ///
    /// # Shared Value
    ///
    /// If a [`Dynamic`] holds a _shared_ value, then it is read-only only if the shared value
    /// itself is read-only.
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its access mode cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        #[cfg(not(feature = "no_closure"))]
        if let Union::Shared(ref cell, ..) = self.0 {
            return match crate::func::locked_read(cell).map_or(ReadWrite, |v| v.access_mode()) {
                ReadWrite => false,
                ReadOnly => true,
            };
        }

        match self.access_mode() {
            ReadWrite => false,
            ReadOnly => true,
        }
    }
    /// Can this [`Dynamic`] be hashed?
    ///
    /// # Shared Value
    ///
    /// If a [`Dynamic`] holds a _shared_ value, then it is hashable only if the shared value
    /// itself is hashable.
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[must_use]
    pub(crate) fn is_hashable(&self) -> bool {
        match self.0 {
            Union::Unit(..)
            | Union::Bool(..)
            | Union::Str(..)
            | Union::Char(..)
            | Union::Int(..) => true,

            #[cfg(not(feature = "no_float"))]
            Union::Float(..) => true,
            #[cfg(feature = "decimal")]
            Union::Decimal(..) => true,
            #[cfg(not(feature = "no_index"))]
            Union::Array(ref a, ..) => a.iter().all(Self::is_hashable),
            #[cfg(not(feature = "no_index"))]
            Union::Blob(..) => true,
            #[cfg(not(feature = "no_object"))]
            Union::Map(ref m, ..) => m.values().all(Self::is_hashable),
            #[cfg(not(feature = "no_function"))]
            Union::FnPtr(ref f, ..) if f.env.is_some() => false,
            Union::FnPtr(ref f, ..) => f.curry().iter().all(Self::is_hashable),
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(..) => false,

            Union::Variant(ref v, ..) => {
                let _value_any = (***v).as_any();
                let _type_id = _value_any.type_id();

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                if _type_id == TypeId::of::<u8>()
                    || _type_id == TypeId::of::<u16>()
                    || _type_id == TypeId::of::<u32>()
                    || _type_id == TypeId::of::<u64>()
                    || _type_id == TypeId::of::<i8>()
                    || _type_id == TypeId::of::<i16>()
                    || _type_id == TypeId::of::<i32>()
                    || _type_id == TypeId::of::<i64>()
                {
                    return true;
                }

                #[cfg(not(feature = "no_float"))]
                #[cfg(not(feature = "f32_float"))]
                if _type_id == TypeId::of::<f32>() {
                    return true;
                }
                #[cfg(not(feature = "no_float"))]
                #[cfg(feature = "f32_float")]
                if _type_id == TypeId::of::<f64>() {
                    return true;
                }

                #[cfg(not(feature = "only_i32"))]
                #[cfg(not(feature = "only_i64"))]
                #[cfg(not(target_family = "wasm"))]
                if _type_id == TypeId::of::<u128>() || _type_id == TypeId::of::<i128>() {
                    return true;
                }

                if _type_id == TypeId::of::<ExclusiveRange>()
                    || _type_id == TypeId::of::<InclusiveRange>()
                {
                    return true;
                }

                false
            }

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) if cfg!(feature = "unchecked") => {
                crate::func::locked_read(cell).map_or(false, |v| v.is_hashable())
            }
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => {
                #[cfg(feature = "no_std")]
                use hashbrown::HashSet;
                #[cfg(not(feature = "no_std"))]
                use std::collections::HashSet;

                // Avoid infinite recursion for shared values in a reference loop.
                fn checked_is_hashable(
                    value: &Dynamic,
                    dict: &mut HashSet<*const Dynamic>,
                ) -> bool {
                    match value.0 {
                        Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                            .map_or(false, |v| {
                                dict.insert(value) && checked_is_hashable(&v, dict)
                            }),
                        #[cfg(not(feature = "no_index"))]
                        Union::Array(ref a, ..) => a.iter().all(|v| checked_is_hashable(v, dict)),
                        #[cfg(not(feature = "no_object"))]
                        Union::Map(ref m, ..) => m.values().all(|v| checked_is_hashable(v, dict)),
                        Union::FnPtr(ref f, ..) => {
                            f.env.is_none()
                                && f.curry().iter().all(|v| checked_is_hashable(v, dict))
                        }
                        _ => value.is_hashable(),
                    }
                }

                checked_is_hashable(self, &mut <_>::default())
            }
        }
    }
    /// Create a [`Dynamic`] from any type.  A [`Dynamic`] value is simply returned as is.
    ///
    /// # Arrays
    ///
    /// Beware that you need to pass in an [`Array`] type for it to be recognized as
    /// an [`Array`]. A [`Vec<T>`][Vec] does not get automatically converted to an
    /// [`Array`], but will be a custom type instead (stored as a trait object).
    ///
    /// Use `array.into()` or `array.into_iter()` to convert a [`Vec<T>`][Vec] into a [`Dynamic`] as
    /// an [`Array`] value.  See the examples for details.
    ///
    /// # Hash Maps
    ///
    /// Similarly, passing in a [`HashMap<String, T>`][std::collections::HashMap] or
    /// [`BTreeMap<String, T>`][std::collections::BTreeMap] will not get a [`Map`] but a
    /// custom type.
    ///
    /// Again, use `map.into()` to get a [`Dynamic`] with a [`Map`] value.
    /// See the examples for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Dynamic;
    ///
    /// let result = Dynamic::from(42_i64);
    /// assert_eq!(result.type_name(), "i64");
    /// assert_eq!(result.to_string(), "42");
    ///
    /// let result = Dynamic::from("hello");
    /// assert_eq!(result.type_name(), "string");
    /// assert_eq!(result.to_string(), "hello");
    ///
    /// let new_result = Dynamic::from(result);
    /// assert_eq!(new_result.type_name(), "string");
    /// assert_eq!(new_result.to_string(), "hello");
    ///
    /// # #[cfg(not(feature = "no_index"))]
    /// # {
    /// // Arrays - this is a custom object!
    /// let result = Dynamic::from(vec![1_i64, 2, 3]);
    /// assert_eq!(result.type_name(), "alloc::vec::Vec<i64>");
    ///
    /// // Use '.into()' to convert a Vec<T> into an Array
    /// let result: Dynamic = vec![1_i64, 2, 3].into();
    /// assert_eq!(result.type_name(), "array");
    /// # }
    ///
    /// # #[cfg(not(feature = "no_object"))]
    /// # {
    /// # use std::collections::HashMap;
    /// // Hash map
    /// let mut map = HashMap::new();
    /// map.insert("a".to_string(), 1_i64);
    ///
    /// // This is a custom object!
    /// let result = Dynamic::from(map.clone());
    /// assert_eq!(result.type_name(), "std::collections::hash::map::HashMap<alloc::string::String, i64>");
    ///
    /// // Use '.into()' to convert a HashMap<String, T> into an object map
    /// let result: Dynamic = map.into();
    /// assert_eq!(result.type_name(), "map");
    /// # }
    /// ```
    #[inline]
    pub fn from<T: Variant + Clone>(value: T) -> Self {
        // Coded this way in order to maximally leverage potentials for dead-code removal.

        reify! { value => |v: Self| return v }
        reify! { value => |v: INT| return v.into() }

        #[cfg(not(feature = "no_float"))]
        reify! { value => |v: crate::FLOAT| return v.into() }

        #[cfg(feature = "decimal")]
        reify! { value => |v: rust_decimal::Decimal| return v.into() }

        reify! { value => |v: bool| return v.into() }
        reify! { value => |v: char| return v.into() }
        reify! { value => |v: ImmutableString| return v.into() }
        reify! { value => |v: String| return v.into() }
        reify! { value => |v: &str| return v.into() }
        reify! { value => |v: ()| return v.into() }

        #[cfg(not(feature = "no_index"))]
        reify! { value => |v: Array| return v.into() }
        #[cfg(not(feature = "no_index"))]
        // don't use blob.into() because it'll be converted into an Array
        reify! { value => |v: Blob| return Self::from_blob(v) }
        #[cfg(not(feature = "no_object"))]
        reify! { value => |v: Map| return v.into() }
        reify! { value => |v: FnPtr| return v.into() }

        #[cfg(not(feature = "no_time"))]
        reify! { value => |v: Instant| return v.into() }
        #[cfg(not(feature = "no_closure"))]
        reify! { value => |v: crate::Shared<crate::Locked<Self>>| return v.into() }

        Self(Union::Variant(
            Box::new(Box::new(value)),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
    /// Turn the [`Dynamic`] value into a shared [`Dynamic`] value backed by an
    /// [`Rc<RefCell<Dynamic>>`][std::rc::Rc] or [`Arc<RwLock<Dynamic>>`][std::sync::Arc]
    /// depending on the `sync` feature.
    ///
    /// Not available under `no_closure`.
    ///
    /// Shared [`Dynamic`] values are relatively cheap to clone as they simply increment the
    /// reference counts.
    ///
    /// Shared [`Dynamic`] values can be converted seamlessly to and from ordinary [`Dynamic`]
    /// values.
    ///
    /// If the [`Dynamic`] value is already shared, this method returns itself.
    #[cfg(not(feature = "no_closure"))]
    #[inline]
    pub fn into_shared(self) -> Self {
        let _access = self.access_mode();

        match self.0 {
            Union::Shared(..) => self,
            _ => Self(Union::Shared(
                crate::Locked::new(self).into(),
                DEFAULT_TAG_VALUE,
                _access,
            )),
        }
    }
    /// Return this [`Dynamic`], replacing it with [`Dynamic::UNIT`].
    #[inline(always)]
    pub fn take(&mut self) -> Self {
        mem::take(self)
    }
    /// Convert the [`Dynamic`] value into specific type.
    ///
    /// Casting to a [`Dynamic`] simply returns itself.
    ///
    /// # Errors
    ///
    /// Returns [`None`] if types mismatch.
    ///
    /// # Shared Value
    ///
    /// If the [`Dynamic`] is a _shared_ value, it returns the shared value if there are no
    /// outstanding references, or a cloned copy otherwise.
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Dynamic;
    ///
    /// let x = Dynamic::from(42_u32);
    ///
    /// assert_eq!(x.try_cast::<u32>().expect("x should be u32"), 42);
    /// ```
    #[inline(always)]
    #[must_use]
    #[allow(unused_mut)]
    pub fn try_cast<T: Any>(mut self) -> Option<T> {
        self.try_cast_result().ok()
    }
    /// Convert the [`Dynamic`] value into specific type.
    ///
    /// Casting to a [`Dynamic`] simply returns itself.
    ///
    /// # Errors
    ///
    /// Returns itself as an error if types mismatch.
    ///
    /// # Shared Value
    ///
    /// If the [`Dynamic`] is a _shared_ value, it returns the shared value if there are no
    /// outstanding references, or a cloned copy otherwise.
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[allow(unused_mut)]
    pub fn try_cast_result<T: Any>(mut self) -> Result<T, Self> {
        // Coded this way in order to maximally leverage potentials for dead-code removal.

        #[cfg(not(feature = "no_closure"))]
        {
            self = self.flatten();
        }

        if TypeId::of::<T>() == TypeId::of::<Self>() {
            return Ok(reify! { self => !!! T });
        }
        if TypeId::of::<T>() == TypeId::of::<()>() {
            return match self.0 {
                Union::Unit(..) => Ok(reify! { () => !!! T }),
                _ => Err(self),
            };
        }
        if TypeId::of::<T>() == TypeId::of::<INT>() {
            return match self.0 {
                Union::Int(n, ..) => Ok(reify! { n => !!! T }),
                _ => Err(self),
            };
        }
        #[cfg(not(feature = "no_float"))]
        if TypeId::of::<T>() == TypeId::of::<crate::FLOAT>() {
            return match self.0 {
                Union::Float(v, ..) => Ok(reify! { *v => !!! T }),
                _ => Err(self),
            };
        }
        #[cfg(feature = "decimal")]
        if TypeId::of::<T>() == TypeId::of::<rust_decimal::Decimal>() {
            return match self.0 {
                Union::Decimal(v, ..) => Ok(reify! { *v => !!! T }),
                _ => Err(self),
            };
        }
        if TypeId::of::<T>() == TypeId::of::<bool>() {
            return match self.0 {
                Union::Bool(b, ..) => Ok(reify! { b => !!! T }),
                _ => Err(self),
            };
        }
        if TypeId::of::<T>() == TypeId::of::<ImmutableString>() {
            return match self.0 {
                Union::Str(s, ..) => Ok(reify! { s => !!! T }),
                _ => Err(self),
            };
        }
        if TypeId::of::<T>() == TypeId::of::<String>() {
            return match self.0 {
                Union::Str(s, ..) => Ok(reify! { s.to_string() => !!! T }),
                _ => Err(self),
            };
        }
        if TypeId::of::<T>() == TypeId::of::<char>() {
            return match self.0 {
                Union::Char(c, ..) => Ok(reify! { c => !!! T }),
                _ => Err(self),
            };
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            return match self.0 {
                Union::Array(a, ..) => Ok(reify! { *a => !!! T }),
                _ => Err(self),
            };
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Blob>() {
            return match self.0 {
                Union::Blob(b, ..) => Ok(reify! { *b => !!! T }),
                _ => Err(self),
            };
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            return match self.0 {
                Union::Map(m, ..) => Ok(reify! { *m => !!! T }),
                _ => Err(self),
            };
        }
        if TypeId::of::<T>() == TypeId::of::<FnPtr>() {
            return match self.0 {
                Union::FnPtr(f, ..) => Ok(reify! { *f => !!! T }),
                _ => Err(self),
            };
        }
        #[cfg(not(feature = "no_time"))]
        if TypeId::of::<T>() == TypeId::of::<Instant>() {
            return match self.0 {
                Union::TimeStamp(t, ..) => Ok(reify! { *t => !!! T }),
                _ => Err(self),
            };
        }

        match self.0 {
            Union::Variant(v, ..) if TypeId::of::<T>() == (**v).type_id() => {
                Ok((*v).as_boxed_any().downcast().map(|x| *x).unwrap())
            }
            _ => Err(self),
        }
    }
    /// Convert the [`Dynamic`] value into a specific type.
    ///
    /// Casting to a [`Dynamic`] just returns as is.
    ///
    /// # Panics
    ///
    /// Panics if the cast fails (e.g. the type of the actual value is not the same as the specified type).
    ///
    /// # Shared Value
    ///
    /// If the [`Dynamic`] is a _shared_ value, it returns the shared value if there are no
    /// outstanding references, or a cloned copy otherwise.
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the _shared_ value is simply cloned, which means that the returned
    /// value is also shared.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Dynamic;
    ///
    /// let x = Dynamic::from(42_u32);
    ///
    /// assert_eq!(x.cast::<u32>(), 42);
    /// ```
    #[inline]
    #[must_use]
    pub fn cast<T: Any + Clone>(self) -> T {
        // Bail out early if the return type needs no cast
        if TypeId::of::<T>() == TypeId::of::<Self>() {
            return reify! { self => !!! T };
        }

        #[cfg(not(feature = "no_closure"))]
        let self_type_name = if self.is_shared() {
            // Avoid panics/deadlocks with shared values
            "<shared>"
        } else {
            self.type_name()
        };
        #[cfg(feature = "no_closure")]
        let self_type_name = self.type_name();

        self.try_cast::<T>()
            .unwrap_or_else(|| panic!("cannot cast {} to {}", self_type_name, type_name::<T>()))
    }
    /// Clone the [`Dynamic`] value and convert it into a specific type.
    ///
    /// Casting to a [`Dynamic`] just returns as is.
    ///
    /// # Panics
    ///
    /// Panics if the cast fails (e.g. the type of the actual value is not the
    /// same as the specified type).
    ///
    /// # Shared Value
    ///
    /// If the [`Dynamic`] is a _shared_ value, a cloned copy of the shared value is returned.
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the _shared_ value is simply cloned.
    ///
    /// This normally shouldn't occur since most operations in Rhai are single-threaded.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Dynamic;
    ///
    /// let x = Dynamic::from(42_u32);
    /// let y = &x;
    ///
    /// assert_eq!(y.clone_cast::<u32>(), 42);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn clone_cast<T: Any + Clone>(&self) -> T {
        self.flatten_clone().cast::<T>()
    }
    /// Flatten the [`Dynamic`] and clone it.
    ///
    /// If the [`Dynamic`] is not a _shared_ value, a cloned copy is returned.
    ///
    /// If the [`Dynamic`] is a _shared_ value, a cloned copy of the shared value is returned.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the _shared_ value is simply cloned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn flatten_clone(&self) -> Self {
        match self.0 {
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or_else(|| self.clone(), |v| v.flatten_clone())
            }
            _ => self.clone(),
        }
    }
    /// Flatten the [`Dynamic`].
    ///
    /// If the [`Dynamic`] is not a _shared_ value, it simply returns itself.
    ///
    /// If the [`Dynamic`] is a _shared_ value, it returns the shared value if there are no
    /// outstanding references, or a cloned copy otherwise.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the _shared_ value is simply cloned, meaning that the result
    /// value will also be _shared_.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn flatten(self) -> Self {
        match self.0 {
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(cell, tag, access) => match crate::func::native::shared_try_take(cell) {
                // If there are no outstanding references, consume the shared value and return it
                #[cfg(not(feature = "sync"))]
                Ok(value) => value.into_inner().flatten(),
                #[cfg(feature = "sync")]
                #[cfg(not(feature = "no_std"))]
                Ok(value) => value.into_inner().unwrap().flatten(),
                #[cfg(feature = "sync")]
                #[cfg(feature = "no_std")]
                Ok(value) => value.into_inner().flatten(),
                // If there are outstanding references, return a cloned copy
                Err(cell) => {
                    if let Some(guard) = crate::func::locked_read(&cell) {
                        return guard.flatten_clone();
                    }
                    Self(Union::Shared(cell, tag, access))
                }
            },
            _ => self,
        }
    }
    /// Is the [`Dynamic`] a _shared_ value that is locked?
    ///
    /// Not available under `no_closure`.
    ///
    /// ## Note
    ///
    /// Under the `sync` feature, shared values use [`RwLock`][std::sync::RwLock] and they are never locked.
    /// Access just waits until the [`RwLock`][std::sync::RwLock] is released.
    /// So this method always returns [`false`] under [`Sync`].
    #[cfg(not(feature = "no_closure"))]
    #[inline]
    #[must_use]
    pub fn is_locked(&self) -> bool {
        #[cfg(not(feature = "no_closure"))]
        if let Union::Shared(ref _cell, ..) = self.0 {
            #[cfg(not(feature = "sync"))]
            return _cell.try_borrow().is_err();
            #[cfg(feature = "sync")]
            return false;
        }

        false
    }
    /// Get a reference of a specific type to the [`Dynamic`].
    ///
    /// Casting to [`Dynamic`] just returns a reference to it.
    ///
    /// Returns [`None`] if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, this call also fails if the data is currently borrowed for write.
    ///
    /// Under these circumstances, [`None`] is also returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn read_lock<T: Any + Clone>(&self) -> Option<DynamicReadLock<'_, T>> {
        match self.0 {
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                return match crate::func::locked_read(cell) {
                    Some(guard)
                        if TypeId::of::<Self>() == TypeId::of::<T>()
                            || (*guard).type_id() == TypeId::of::<T>() =>
                    {
                        Some(DynamicReadLock(DynamicReadLockInner::Guard(guard)))
                    }
                    _ => None,
                };
            }
            _ => (),
        }

        self.downcast_ref()
            .map(DynamicReadLockInner::Reference)
            .map(DynamicReadLock)
    }
    /// Get a mutable reference of a specific type to the [`Dynamic`].
    ///
    /// Casting to [`Dynamic`] just returns a mutable reference to it.
    ///
    /// Returns [`None`] if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, this call also fails if the data is currently borrowed for write.
    ///
    /// Under these circumstances, [`None`] is also returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn write_lock<T: Any + Clone>(&mut self) -> Option<DynamicWriteLock<'_, T>> {
        match self.0 {
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                return match crate::func::locked_write(cell) {
                    Some(guard)
                        if TypeId::of::<Self>() == TypeId::of::<T>()
                            || (*guard).type_id() == TypeId::of::<T>() =>
                    {
                        Some(DynamicWriteLock(DynamicWriteLockInner::Guard(guard)))
                    }
                    _ => None,
                };
            }
            _ => (),
        }

        self.downcast_mut()
            .map(DynamicWriteLockInner::Reference)
            .map(DynamicWriteLock)
    }
    /// Get a reference of a specific type to the [`Dynamic`].
    ///
    /// Casting to [`Dynamic`] just returns a reference to it.
    ///
    /// Returns [`None`] if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Returns [`None`] also if the value is _shared_.
    #[inline]
    #[must_use]
    pub(crate) fn downcast_ref<T: Any + Clone>(&self) -> Option<&T> {
        // Coded this way in order to maximally leverage potentials for dead-code removal.

        if TypeId::of::<T>() == TypeId::of::<INT>() {
            return match self.0 {
                Union::Int(ref v, ..) => v.as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_float"))]
        if TypeId::of::<T>() == TypeId::of::<crate::FLOAT>() {
            return match self.0 {
                Union::Float(ref v, ..) => v.as_ref().as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        #[cfg(feature = "decimal")]
        if TypeId::of::<T>() == TypeId::of::<rust_decimal::Decimal>() {
            return match self.0 {
                Union::Decimal(ref v, ..) => v.as_ref().as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<bool>() {
            return match self.0 {
                Union::Bool(ref v, ..) => v.as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<ImmutableString>() {
            return match self.0 {
                Union::Str(ref v, ..) => v.as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<char>() {
            return match self.0 {
                Union::Char(ref v, ..) => v.as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            return match self.0 {
                Union::Array(ref v, ..) => v.as_ref().as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Blob>() {
            return match self.0 {
                Union::Blob(ref v, ..) => v.as_ref().as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            return match self.0 {
                Union::Map(ref v, ..) => v.as_ref().as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<FnPtr>() {
            return match self.0 {
                Union::FnPtr(ref v, ..) => v.as_ref().as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_time"))]
        if TypeId::of::<T>() == TypeId::of::<Instant>() {
            return match self.0 {
                Union::TimeStamp(ref v, ..) => v.as_ref().as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<()>() {
            return match self.0 {
                Union::Unit(ref v, ..) => v.as_any().downcast_ref::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<Self>() {
            return self.as_any().downcast_ref::<T>();
        }

        match self.0 {
            Union::Variant(ref v, ..) => (***v).as_any().downcast_ref::<T>(),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => None,
            _ => None,
        }
    }
    /// Get a mutable reference of a specific type to the [`Dynamic`].
    ///
    /// Casting to [`Dynamic`] just returns a mutable reference to it.
    ///
    /// Returns [`None`] if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Returns [`None`] also if the value is _shared_.
    #[inline]
    #[must_use]
    pub(crate) fn downcast_mut<T: Any + Clone>(&mut self) -> Option<&mut T> {
        // Coded this way in order to maximally leverage potentials for dead-code removal.

        if TypeId::of::<T>() == TypeId::of::<INT>() {
            return match self.0 {
                Union::Int(ref mut v, ..) => v.as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_float"))]
        if TypeId::of::<T>() == TypeId::of::<crate::FLOAT>() {
            return match self.0 {
                Union::Float(ref mut v, ..) => v.as_mut().as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        #[cfg(feature = "decimal")]
        if TypeId::of::<T>() == TypeId::of::<rust_decimal::Decimal>() {
            return match self.0 {
                Union::Decimal(ref mut v, ..) => v.as_mut().as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<bool>() {
            return match self.0 {
                Union::Bool(ref mut v, ..) => v.as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<ImmutableString>() {
            return match self.0 {
                Union::Str(ref mut v, ..) => v.as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<char>() {
            return match self.0 {
                Union::Char(ref mut v, ..) => v.as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Array>() {
            return match self.0 {
                Union::Array(ref mut v, ..) => v.as_mut().as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_index"))]
        if TypeId::of::<T>() == TypeId::of::<Blob>() {
            return match self.0 {
                Union::Blob(ref mut v, ..) => v.as_mut().as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_object"))]
        if TypeId::of::<T>() == TypeId::of::<Map>() {
            return match self.0 {
                Union::Map(ref mut v, ..) => v.as_mut().as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<FnPtr>() {
            return match self.0 {
                Union::FnPtr(ref mut v, ..) => v.as_mut().as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        #[cfg(not(feature = "no_time"))]
        if TypeId::of::<T>() == TypeId::of::<Instant>() {
            return match self.0 {
                Union::TimeStamp(ref mut v, ..) => v.as_mut().as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<()>() {
            return match self.0 {
                Union::Unit(ref mut v, ..) => v.as_any_mut().downcast_mut::<T>(),
                _ => None,
            };
        }
        if TypeId::of::<T>() == TypeId::of::<Self>() {
            return self.as_any_mut().downcast_mut::<T>();
        }

        match self.0 {
            Union::Variant(ref mut v, ..) => (***v).as_any_mut().downcast_mut::<T>(),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => None,
            _ => None,
        }
    }

    /// Return `true` if the [`Dynamic`] holds a `()`.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    #[must_use]
    pub fn is_unit(&self) -> bool {
        match self.0 {
            Union::Unit(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Unit(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds the system integer type [`INT`].
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    #[must_use]
    pub fn is_int(&self) -> bool {
        match self.0 {
            Union::Int(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Int(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds the system floating-point type [`FLOAT`][crate::FLOAT].
    ///
    /// Not available under `no_float`.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_float"))]
    #[inline]
    #[must_use]
    pub fn is_float(&self) -> bool {
        match self.0 {
            Union::Float(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Float(..)))
            }
            _ => false,
        }
    }
    /// _(decimal)_ Return `true` if the [`Dynamic`] holds a [`Decimal`][rust_decimal::Decimal].
    /// Exported under the `decimal` feature only.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(feature = "decimal")]
    #[inline]
    #[must_use]
    pub fn is_decimal(&self) -> bool {
        match self.0 {
            Union::Decimal(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Decimal(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds a [`bool`].
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    #[must_use]
    pub fn is_bool(&self) -> bool {
        match self.0 {
            Union::Bool(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Bool(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds a [`char`].
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    #[must_use]
    pub fn is_char(&self) -> bool {
        match self.0 {
            Union::Char(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Char(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds an [`ImmutableString`].
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    #[must_use]
    pub fn is_string(&self) -> bool {
        match self.0 {
            Union::Str(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Str(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds an [`Array`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline]
    #[must_use]
    pub fn is_array(&self) -> bool {
        match self.0 {
            Union::Array(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Array(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds a [`Blob`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline]
    #[must_use]
    pub fn is_blob(&self) -> bool {
        match self.0 {
            Union::Blob(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Blob(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds a [`Map`].
    ///
    /// Not available under `no_object`.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    #[must_use]
    pub fn is_map(&self) -> bool {
        match self.0 {
            Union::Map(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::Map(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds a [`FnPtr`].
    ///
    /// Note that there is no accompanying `as_fnptr()` method, use [`cast()`](Self::cast) to obtain the [`FnPtr`].
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    #[must_use]
    pub fn is_fnptr(&self) -> bool {
        match self.0 {
            Union::FnPtr(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                crate::func::locked_read(cell).map_or(false, |v| matches!(v.0, Union::FnPtr(..)))
            }
            _ => false,
        }
    }
    /// Return `true` if the [`Dynamic`] holds a [timestamp][Instant].
    ///
    /// Not available under `no_time`.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, `false` is returned.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_time"))]
    #[inline]
    #[must_use]
    pub fn is_timestamp(&self) -> bool {
        match self.0 {
            Union::TimeStamp(..) => true,
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .map_or(false, |v| matches!(v.0, Union::TimeStamp(..))),
            _ => false,
        }
    }

    /// Cast the [`Dynamic`] as a unit `()`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn as_unit(&self) -> Result<(), &'static str> {
        match self.0 {
            Union::Unit(..) => Ok(()),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Unit(..) => Some(()),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Cast the [`Dynamic`] as the system integer type [`INT`].
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn as_int(&self) -> Result<INT, &'static str> {
        match self.0 {
            Union::Int(n, ..) => Ok(n),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Int(n, ..) => Some(n),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Cast the [`Dynamic`] as the system floating-point type [`FLOAT`][crate::FLOAT].
    ///
    /// Not available under `no_float`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_float"))]
    #[inline]
    pub fn as_float(&self) -> Result<crate::FLOAT, &'static str> {
        match self.0 {
            Union::Float(n, ..) => Ok(*n),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Float(n, ..) => Some(*n),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// _(decimal)_ Cast the [`Dynamic`] as a [`Decimal`][rust_decimal::Decimal].
    /// Exported under the `decimal` feature only.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(feature = "decimal")]
    #[inline]
    pub fn as_decimal(&self) -> Result<rust_decimal::Decimal, &'static str> {
        match self.0 {
            Union::Decimal(ref n, ..) => Ok(**n),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Decimal(ref n, ..) => Some(**n),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Cast the [`Dynamic`] as a [`bool`].
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn as_bool(&self) -> Result<bool, &'static str> {
        match self.0 {
            Union::Bool(b, ..) => Ok(b),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Bool(b, ..) => Some(b),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Cast the [`Dynamic`] as a [`char`].
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn as_char(&self) -> Result<char, &'static str> {
        match self.0 {
            Union::Char(c, ..) => Ok(c),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Char(c, ..) => Some(c),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Cast the [`Dynamic`] as an [`ImmutableString`].
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn as_immutable_string_ref(
        &self,
    ) -> Result<impl Deref<Target = ImmutableString> + '_, &'static str> {
        self.read_lock::<ImmutableString>()
            .ok_or_else(|| self.type_name())
    }
    /// Cast the [`Dynamic`] as a mutable reference to an [`ImmutableString`].
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn as_immutable_string_mut(
        &mut self,
    ) -> Result<impl DerefMut<Target = ImmutableString> + '_, &'static str> {
        let type_name = self.type_name();
        self.write_lock::<ImmutableString>().ok_or(type_name)
    }
    /// Cast the [`Dynamic`] as an [`Array`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn as_array_ref(&self) -> Result<impl Deref<Target = Array> + '_, &'static str> {
        self.read_lock::<Array>().ok_or_else(|| self.type_name())
    }
    /// Cast the [`Dynamic`] as a mutable reference to an [`Array`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn as_array_mut(&mut self) -> Result<impl DerefMut<Target = Array> + '_, &'static str> {
        let type_name = self.type_name();
        self.write_lock::<Array>().ok_or(type_name)
    }
    /// Cast the [`Dynamic`] as a [`Blob`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn as_blob_ref(&self) -> Result<impl Deref<Target = Blob> + '_, &'static str> {
        self.read_lock::<Blob>().ok_or_else(|| self.type_name())
    }
    /// Cast the [`Dynamic`] as a mutable reference to a [`Blob`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn as_blob_mut(&mut self) -> Result<impl DerefMut<Target = Blob> + '_, &'static str> {
        let type_name = self.type_name();
        self.write_lock::<Blob>().ok_or(type_name)
    }
    /// Cast the [`Dynamic`] as a [`Map`].
    ///
    /// Not available under `no_object`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn as_map_ref(&self) -> Result<impl Deref<Target = Map> + '_, &'static str> {
        self.read_lock::<Map>().ok_or_else(|| self.type_name())
    }
    /// Cast the [`Dynamic`] as a mutable reference to a [`Map`].
    ///
    /// Not available under `no_object`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn as_map_mut(&mut self) -> Result<impl DerefMut<Target = Map> + '_, &'static str> {
        let type_name = self.type_name();
        self.write_lock::<Map>().ok_or(type_name)
    }
    /// Convert the [`Dynamic`] into a [`String`].
    ///
    /// If there are other references to the same string, a cloned copy is returned.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn into_string(self) -> Result<String, &'static str> {
        self.into_immutable_string()
            .map(ImmutableString::into_owned)
    }
    /// Convert the [`Dynamic`] into an [`ImmutableString`].
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[inline]
    pub fn into_immutable_string(self) -> Result<ImmutableString, &'static str> {
        match self.0 {
            Union::Str(s, ..) => Ok(s),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Str(ref s, ..) => Some(s.clone()),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Convert the [`Dynamic`] into an [`Array`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn into_array(self) -> Result<Array, &'static str> {
        match self.0 {
            Union::Array(a, ..) => Ok(*a),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Array(ref a, ..) => Some(a.as_ref().clone()),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Convert the [`Dynamic`] into a [`Vec`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn into_typed_array<T: Variant + Clone>(self) -> Result<Vec<T>, &'static str> {
        match self.0 {
            Union::Array(a, ..) => a
                .into_iter()
                .map(|v| {
                    #[cfg(not(feature = "no_closure"))]
                    let typ = if v.is_shared() {
                        // Avoid panics/deadlocks with shared values
                        "<shared>"
                    } else {
                        v.type_name()
                    };
                    #[cfg(feature = "no_closure")]
                    let typ = v.type_name();

                    v.try_cast::<T>().ok_or(typ)
                })
                .collect(),
            Union::Blob(b, ..) if TypeId::of::<T>() == TypeId::of::<u8>() => {
                Ok(reify! { *b => !!! Vec<T> })
            }
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Array(ref a, ..) => a
                        .iter()
                        .map(|v| v.read_lock::<T>().map(|v| v.clone()))
                        .collect(),
                    Union::Blob(ref b, ..) if TypeId::of::<T>() == TypeId::of::<u8>() => {
                        Some(reify! { b.clone() => !!! Vec<T> })
                    }
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }
    /// Convert the [`Dynamic`] into a [`Blob`].
    ///
    /// Not available under `no_index`.
    ///
    /// # Errors
    ///
    /// Returns the name of the actual type as an error if the cast fails.
    ///
    /// # Shared Value
    ///
    /// Under the `sync` feature, a _shared_ value may deadlock.
    /// Otherwise, the data may currently be borrowed for write (so its type cannot be determined).
    ///
    /// Under these circumstances, the cast also fails.
    ///
    /// These normally shouldn't occur since most operations in Rhai are single-threaded.
    #[cfg(not(feature = "no_index"))]
    #[inline(always)]
    pub fn into_blob(self) -> Result<Blob, &'static str> {
        match self.0 {
            Union::Blob(b, ..) => Ok(*b),
            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => crate::func::locked_read(cell)
                .and_then(|guard| match guard.0 {
                    Union::Blob(ref b, ..) => Some(b.as_ref().clone()),
                    _ => None,
                })
                .ok_or_else(|| cell.type_name()),
            _ => Err(self.type_name()),
        }
    }

    /// Recursively scan for [`Dynamic`] values within this [`Dynamic`] (e.g. items in an array or map),
    /// calling a filter function on each.
    ///
    /// # Shared Value
    ///
    /// Shared values are _NOT_ scanned.
    #[inline]
    #[allow(clippy::only_used_in_recursion)]
    pub fn deep_scan(&mut self, mut filter: impl FnMut(&mut Self)) {
        fn scan_inner(value: &mut Dynamic, filter: &mut (impl FnMut(&mut Dynamic) + ?Sized)) {
            filter(value);

            match &mut value.0 {
                #[cfg(not(feature = "no_index"))]
                Union::Array(a, ..) => a.iter_mut().for_each(|v| scan_inner(v, filter)),
                #[cfg(not(feature = "no_object"))]
                Union::Map(m, ..) => m.values_mut().for_each(|v| scan_inner(v, filter)),
                Union::FnPtr(f, ..) => f.iter_curry_mut().for_each(|v| scan_inner(v, filter)),
                _ => (),
            }
        }

        scan_inner(self, &mut filter);
    }
}

impl From<()> for Dynamic {
    #[inline(always)]
    fn from(value: ()) -> Self {
        Self(Union::Unit(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
}
impl From<bool> for Dynamic {
    #[inline(always)]
    fn from(value: bool) -> Self {
        Self(Union::Bool(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
}
impl From<INT> for Dynamic {
    #[inline(always)]
    fn from(value: INT) -> Self {
        Self(Union::Int(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
}
#[cfg(not(feature = "no_float"))]
impl From<crate::FLOAT> for Dynamic {
    #[inline(always)]
    fn from(value: crate::FLOAT) -> Self {
        Self(Union::Float(value.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
}
#[cfg(not(feature = "no_float"))]
impl From<super::FloatWrapper<crate::FLOAT>> for Dynamic {
    #[inline(always)]
    fn from(value: super::FloatWrapper<crate::FLOAT>) -> Self {
        Self(Union::Float(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
}
#[cfg(feature = "decimal")]
impl From<rust_decimal::Decimal> for Dynamic {
    #[inline(always)]
    fn from(value: rust_decimal::Decimal) -> Self {
        Self(Union::Decimal(value.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
}
impl From<char> for Dynamic {
    #[inline(always)]
    fn from(value: char) -> Self {
        Self(Union::Char(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
}
impl<S: Into<ImmutableString>> From<S> for Dynamic {
    #[inline(always)]
    fn from(value: S) -> Self {
        Self(Union::Str(value.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
}
impl FromStr for Dynamic {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Union::Str(value.into(), DEFAULT_TAG_VALUE, ReadWrite)))
    }
}
#[cfg(not(feature = "no_index"))]
impl<T: Variant + Clone> From<Vec<T>> for Dynamic {
    #[inline]
    fn from(value: Vec<T>) -> Self {
        Self(Union::Array(
            Box::new(value.into_iter().map(Self::from).collect()),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
}
#[cfg(not(feature = "no_index"))]
impl<T: Variant + Clone> From<&[T]> for Dynamic {
    #[inline]
    fn from(value: &[T]) -> Self {
        Self(Union::Array(
            Box::new(value.iter().cloned().map(Self::from).collect()),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
}
#[cfg(not(feature = "no_index"))]
impl<T: Variant + Clone> std::iter::FromIterator<T> for Dynamic {
    #[inline]
    fn from_iter<X: IntoIterator<Item = T>>(iter: X) -> Self {
        Self(Union::Array(
            Box::new(iter.into_iter().map(Self::from).collect()),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
}
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "no_std"))]
impl<K: Into<crate::Identifier>, T: Variant + Clone> From<std::collections::HashMap<K, T>>
    for Dynamic
{
    #[inline]
    fn from(value: std::collections::HashMap<K, T>) -> Self {
        Self(Union::Map(
            Box::new(
                value
                    .into_iter()
                    .map(|(k, v)| (k.into(), Self::from(v)))
                    .collect(),
            ),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
}
#[cfg(not(feature = "no_object"))]
#[cfg(not(feature = "no_std"))]
impl<K: Into<crate::Identifier>> From<std::collections::HashSet<K>> for Dynamic {
    #[inline]
    fn from(value: std::collections::HashSet<K>) -> Self {
        Self(Union::Map(
            Box::new(value.into_iter().map(|k| (k.into(), Self::UNIT)).collect()),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
}
#[cfg(not(feature = "no_object"))]
impl<K: Into<crate::Identifier>, T: Variant + Clone> From<std::collections::BTreeMap<K, T>>
    for Dynamic
{
    #[inline]
    fn from(value: std::collections::BTreeMap<K, T>) -> Self {
        Self(Union::Map(
            Box::new(
                value
                    .into_iter()
                    .map(|(k, v)| (k.into(), Self::from(v)))
                    .collect(),
            ),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
}
#[cfg(not(feature = "no_object"))]
impl<K: Into<crate::Identifier>> From<std::collections::BTreeSet<K>> for Dynamic {
    #[inline]
    fn from(value: std::collections::BTreeSet<K>) -> Self {
        Self(Union::Map(
            Box::new(value.into_iter().map(|k| (k.into(), Self::UNIT)).collect()),
            DEFAULT_TAG_VALUE,
            ReadWrite,
        ))
    }
}
impl From<FnPtr> for Dynamic {
    #[inline(always)]
    fn from(value: FnPtr) -> Self {
        Self(Union::FnPtr(value.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
}
#[cfg(not(feature = "no_time"))]
impl From<Instant> for Dynamic {
    #[inline(always)]
    fn from(value: Instant) -> Self {
        Self(Union::TimeStamp(value.into(), DEFAULT_TAG_VALUE, ReadWrite))
    }
}
#[cfg(not(feature = "no_closure"))]
impl From<crate::Shared<crate::Locked<Self>>> for Dynamic {
    #[inline(always)]
    fn from(value: crate::Shared<crate::Locked<Self>>) -> Self {
        Self(Union::Shared(value, DEFAULT_TAG_VALUE, ReadWrite))
    }
}

impl From<ExclusiveRange> for Dynamic {
    #[inline(always)]
    fn from(value: ExclusiveRange) -> Self {
        Self::from(value)
    }
}
impl From<InclusiveRange> for Dynamic {
    #[inline(always)]
    fn from(value: InclusiveRange) -> Self {
        Self::from(value)
    }
}
