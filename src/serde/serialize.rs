//! Implementations of [`serde::Serialize`].

use crate::types::dynamic::Union;
use crate::{Dynamic, ImmutableString, Scope};
use serde::{ser::SerializeSeq, Serialize, Serializer};
use std::iter::once;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_object"))]
use serde::ser::SerializeMap;

#[cfg(not(feature = "no_time"))]
use crate::types::dynamic::Variant;

impl Serialize for Dynamic {
    #[inline(always)]
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        DynamicSer {
            value: self,
            shared: None,
        }
        .serialize(ser)
    }
}

/// Shared values currently being serialized, innermost first.
#[cfg_attr(feature = "no_closure", allow(dead_code))]
struct SharedPath<'a> {
    ptr: *const crate::Locked<Dynamic>,
    parent: Option<&'a SharedPath<'a>>,
}

/// Serializes a [`Dynamic`] while keeping track of the shared values it is nested in,
/// so that a reference loop (e.g. a closure capturing the object map it is stored in)
/// is reported as an error instead of recursing forever.
struct DynamicSer<'a> {
    value: &'a Dynamic,
    shared: Option<&'a SharedPath<'a>>,
}

impl Serialize for DynamicSer<'_> {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let shared = self.shared;

        match self.value.0 {
            Union::Unit(..) => ser.serialize_unit(),
            Union::Bool(x, ..) => ser.serialize_bool(x),
            Union::Str(ref s, ..) => ser.serialize_str(s),
            Union::Char(c, ..) => ser.serialize_char(c),

            #[cfg(not(feature = "only_i32"))]
            Union::Int(x, ..) => ser.serialize_i64(x),
            #[cfg(feature = "only_i32")]
            Union::Int(x, ..) => ser.serialize_i32(x),

            #[cfg(not(feature = "no_float"))]
            #[cfg(not(feature = "f32_float"))]
            Union::Float(x, ..) => ser.serialize_f64(*x),
            #[cfg(not(feature = "no_float"))]
            #[cfg(feature = "f32_float")]
            Union::Float(x, ..) => ser.serialize_f32(*x),

            #[cfg(feature = "decimal")]
            #[cfg(not(feature = "f32_float"))]
            Union::Decimal(ref x, ..) => {
                use rust_decimal::prelude::ToPrimitive;

                match x.to_f64() {
                    Some(v) => ser.serialize_f64(v),
                    None => ser.serialize_str(&x.to_string()),
                }
            }
            #[cfg(feature = "decimal")]
            #[cfg(feature = "f32_float")]
            Union::Decimal(ref x, ..) => {
                use rust_decimal::prelude::ToPrimitive;

                match x.to_f32() {
                    Some(v) => ser.serialize_f32(v),
                    _ => ser.serialize_str(&x.to_string()),
                }
            }

            #[cfg(not(feature = "no_index"))]
            Union::Array(ref a, ..) => {
                ser.collect_seq(a.iter().map(|value| DynamicSer { value, shared }))
            }
            #[cfg(not(feature = "no_index"))]
            Union::Blob(ref a, ..) => ser.serialize_bytes(a),
            #[cfg(not(feature = "no_object"))]
            Union::Map(ref m, ..) => {
                let mut map = ser.serialize_map(Some(m.len()))?;
                m.iter().try_for_each(|(k, value)| {
                    map.serialize_entry(k.as_str(), &DynamicSer { value, shared })
                })?;
                map.end()
            }
            Union::FnPtr(ref f, ..) if f.is_curried() => {
                let fn_name: Dynamic = f.fn_name().into();
                ser.collect_seq(
                    once(&fn_name)
                        .chain(f.iter_curry())
                        .map(|value| DynamicSer { value, shared }),
                )
            }
            Union::FnPtr(ref f, ..) => ser.serialize_str(f.fn_name()),
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(ref x, ..) => ser.serialize_str(x.as_ref().type_name()),

            Union::Variant(ref v, ..) => ser.serialize_str((***v).type_name()),

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(ref cell, ..) => {
                use serde::ser::Error;

                let ptr: *const crate::Locked<Dynamic> = &**cell;

                let mut path = shared;
                while let Some(p) = path {
                    if std::ptr::eq(p.ptr, ptr) {
                        return Err(S::Error::custom("shared value contains a reference loop"));
                    }
                    path = p.parent;
                }

                let value = crate::func::locked_read(cell)
                    .ok_or_else(|| S::Error::custom("shared value is locked"))?;

                let path = SharedPath {
                    ptr,
                    parent: shared,
                };

                DynamicSer {
                    value: &value,
                    shared: Some(&path),
                }
                .serialize(ser)
            }
        }
    }
}

impl Serialize for ImmutableString {
    #[inline(always)]
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self)
    }
}

impl Serialize for Scope<'_> {
    #[inline(always)]
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        #[derive(Debug, Clone, Hash, Serialize)]
        struct ScopeEntry<'a> {
            pub name: &'a str,
            pub value: &'a Dynamic,
            #[serde(default, skip_serializing_if = "is_false")]
            pub is_constant: bool,
        }

        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn is_false(value: &bool) -> bool {
            !value
        }

        let mut ser = ser.serialize_seq(Some(self.len()))?;

        for (name, is_constant, value) in self.iter_inner() {
            let entry = ScopeEntry {
                name,
                value,
                is_constant,
            };
            ser.serialize_element(&entry)?;
        }

        ser.end()
    }
}
