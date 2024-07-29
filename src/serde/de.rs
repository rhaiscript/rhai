//! Implement deserialization support of [`Dynamic`][crate::Dynamic] for [`serde`].

use crate::api::formatting::map_std_type_name;
use crate::types::dynamic::Union;
use crate::{Dynamic, ImmutableString, LexError, Position, RhaiError, RhaiResultOf, ERR};
use serde::de::{Error, IntoDeserializer, Visitor};
use serde::{Deserialize, Deserializer};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{any::type_name, fmt};

/// Deserializer for [`Dynamic`][crate::Dynamic].
pub struct DynamicDeserializer<'de>(&'de Dynamic);

impl<'de> IntoDeserializer<'de, RhaiError> for &'de Dynamic {
    type Deserializer = DynamicDeserializer<'de>;

    #[inline(always)]
    #[must_use]
    fn into_deserializer(self) -> Self::Deserializer {
        DynamicDeserializer(self)
    }
}

impl<'de> DynamicDeserializer<'de> {
    /// Create a [`DynamicDeserializer`] from a reference to a [`Dynamic`][crate::Dynamic] value.
    ///
    /// The reference is necessary because the deserialized type may hold references
    /// (especially `&str`) to the source [`Dynamic`][crate::Dynamic].
    #[inline(always)]
    #[must_use]
    pub const fn new(value: &'de Dynamic) -> Self {
        Self(value)
    }
    /// Shortcut for a type conversion error.
    #[cold]
    #[inline(always)]
    fn type_error<T>(&self) -> RhaiResultOf<T> {
        self.type_error_str(map_std_type_name(type_name::<T>(), false))
    }
    /// Shortcut for a type conversion error.
    #[cold]
    #[inline(never)]
    fn type_error_str<T>(&self, actual: &str) -> RhaiResultOf<T> {
        let expected = map_std_type_name(self.0.type_name(), false).into();
        Err(ERR::ErrorMismatchOutputType(actual.into(), expected, Position::NONE).into())
    }
    #[inline(always)]
    fn deserialize_int<V: Visitor<'de>>(v: crate::INT, visitor: V) -> RhaiResultOf<V::Value> {
        #[cfg(not(feature = "only_i32"))]
        return visitor.visit_i64(v);
        #[cfg(feature = "only_i32")]
        return visitor.visit_i32(v);
    }
}

/// Deserialize a [`Dynamic`][crate::Dynamic] value into a Rust type that implements [`serde::Deserialize`].
///
/// # Example
///
/// ```
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
/// # #[cfg(not(feature = "no_index"))]
/// # #[cfg(not(feature = "no_object"))]
/// # {
/// use rhai::{Dynamic, Array, Map, INT};
/// use rhai::serde::from_dynamic;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize, PartialEq)]
/// struct Hello {
///     a: INT,
///     b: bool,
/// }
///
/// #[derive(Debug, Deserialize, PartialEq)]
/// struct Test {
///     int: u32,
///     seq: Vec<String>,
///     obj: Hello,
/// }
///
/// let mut map = Map::new();
/// map.insert("int".into(), Dynamic::from(42_u32));
///
/// let mut map2 = Map::new();
/// map2.insert("a".into(), (123 as INT).into());
/// map2.insert("b".into(), true.into());
///
/// map.insert("obj".into(), map2.into());
///
/// let arr: Array = vec!["foo".into(), "bar".into(), "baz".into()];
/// map.insert("seq".into(), arr.into());
///
/// let value: Test = from_dynamic(&map.into())?;
///
/// let expected = Test {
///     int: 42,
///     seq: vec!["foo".into(), "bar".into(), "baz".into()],
///     obj: Hello { a: 123, b: true },
/// };
///
/// assert_eq!(value, expected);
/// # }
/// # Ok(())
/// # }
/// ```
pub fn from_dynamic<'de, T: Deserialize<'de>>(value: &'de Dynamic) -> RhaiResultOf<T> {
    T::deserialize(DynamicDeserializer::new(value))
}

impl Error for RhaiError {
    #[cold]
    #[inline(never)]
    fn custom<T: fmt::Display>(err: T) -> Self {
        LexError::ImproperSymbol(String::new(), err.to_string())
            .into_err(Position::NONE)
            .into()
    }
}

impl<'de> Deserializer<'de> for DynamicDeserializer<'de> {
    type Error = RhaiError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        if type_name::<V::Value>() == type_name::<Dynamic>() {
            return Ok(reify! { self.0.clone() => !!! V::Value });
        }

        match self.0 .0 {
            Union::Unit(..) => self.deserialize_unit(visitor),
            Union::Bool(..) => self.deserialize_bool(visitor),
            Union::Str(..) => self.deserialize_str(visitor),
            Union::Char(..) => self.deserialize_char(visitor),

            #[cfg(not(feature = "only_i32"))]
            Union::Int(..) => self.deserialize_i64(visitor),
            #[cfg(feature = "only_i32")]
            Union::Int(..) => self.deserialize_i32(visitor),

            #[cfg(not(feature = "no_float"))]
            #[cfg(not(feature = "f32_float"))]
            Union::Float(..) => self.deserialize_f64(visitor),
            #[cfg(not(feature = "no_float"))]
            #[cfg(feature = "f32_float")]
            Union::Float(..) => self.deserialize_f32(visitor),

            #[cfg(feature = "decimal")]
            #[cfg(not(feature = "f32_float"))]
            Union::Decimal(..) => self.deserialize_f64(visitor),
            #[cfg(feature = "decimal")]
            #[cfg(feature = "f32_float")]
            Union::Decimal(..) => self.deserialize_f32(visitor),

            #[cfg(not(feature = "no_index"))]
            Union::Array(..) => self.deserialize_seq(visitor),
            #[cfg(not(feature = "no_index"))]
            Union::Blob(..) => self.deserialize_bytes(visitor),
            #[cfg(not(feature = "no_object"))]
            Union::Map(..) => self.deserialize_map(visitor),
            Union::FnPtr(..) => self.type_error(),
            #[cfg(not(feature = "no_time"))]
            Union::TimeStamp(..) => self.type_error(),

            Union::Variant(ref value, ..) if value.is::<i8>() => self.deserialize_i8(visitor),
            Union::Variant(ref value, ..) if value.is::<i16>() => self.deserialize_i16(visitor),
            Union::Variant(ref value, ..) if value.is::<i32>() => self.deserialize_i32(visitor),
            Union::Variant(ref value, ..) if value.is::<i64>() => self.deserialize_i64(visitor),
            Union::Variant(ref value, ..) if value.is::<i128>() => self.deserialize_i128(visitor),
            Union::Variant(ref value, ..) if value.is::<u8>() => self.deserialize_u8(visitor),
            Union::Variant(ref value, ..) if value.is::<u16>() => self.deserialize_u16(visitor),
            Union::Variant(ref value, ..) if value.is::<u32>() => self.deserialize_u32(visitor),
            Union::Variant(ref value, ..) if value.is::<u64>() => self.deserialize_u64(visitor),
            Union::Variant(ref value, ..) if value.is::<u128>() => self.deserialize_u128(visitor),

            Union::Variant(..) => self.type_error(),

            #[cfg(not(feature = "no_closure"))]
            Union::Shared(..) => self.type_error(),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        visitor.visit_bool(self.0.as_bool().or_else(|_| self.type_error())?)
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            Err(_) => self
                .0
                .downcast_ref::<i8>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_i8(x)),
        }
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            Err(_) => self
                .0
                .downcast_ref::<i16>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_i16(x)),
        }
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            _ if cfg!(feature = "only_i32") => self.type_error(),
            _ => self
                .0
                .downcast_ref::<i32>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_i32(x)),
        }
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            _ if cfg!(not(feature = "only_i32")) => self.type_error(),
            _ => self
                .0
                .downcast_ref::<i64>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_i64(x)),
        }
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            _ if cfg!(not(feature = "only_i32")) => self.type_error(),
            _ => self
                .0
                .downcast_ref::<i128>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_i128(x)),
        }
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            Err(_) => self
                .0
                .downcast_ref::<u8>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_u8(x)),
        }
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            Err(_) => self
                .0
                .downcast_ref::<u16>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_u16(x)),
        }
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            Err(_) => self
                .0
                .downcast_ref::<u32>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_u32(x)),
        }
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            Err(_) => self
                .0
                .downcast_ref::<u64>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_u64(x)),
        }
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        match self.0.as_int() {
            Ok(v) => Self::deserialize_int(v, visitor),
            Err(_) => self
                .0
                .downcast_ref::<u128>()
                .map_or_else(|| self.type_error(), |&x| visitor.visit_u128(x)),
        }
    }

    fn deserialize_f32<V: Visitor<'de>>(self, _visitor: V) -> RhaiResultOf<V::Value> {
        #[cfg(not(feature = "no_float"))]
        return self
            .0
            .downcast_ref::<f32>()
            .map_or_else(|| self.type_error(), |&x| _visitor.visit_f32(x));

        #[allow(unreachable_code)]
        {
            #[cfg(feature = "decimal")]
            {
                use rust_decimal::prelude::ToPrimitive;

                return self
                    .0
                    .downcast_ref::<rust_decimal::Decimal>()
                    .and_then(|&x| x.to_f32())
                    .map_or_else(|| self.type_error(), |v| _visitor.visit_f32(v));
            }

            self.type_error_str("f32")
        }
    }

    fn deserialize_f64<V: Visitor<'de>>(self, _visitor: V) -> RhaiResultOf<V::Value> {
        #[cfg(not(feature = "no_float"))]
        return self
            .0
            .downcast_ref::<f64>()
            .map_or_else(|| self.type_error(), |&x| _visitor.visit_f64(x));

        #[allow(unreachable_code)]
        {
            #[cfg(feature = "decimal")]
            {
                use rust_decimal::prelude::ToPrimitive;

                return self
                    .0
                    .downcast_ref::<rust_decimal::Decimal>()
                    .and_then(|&x| x.to_f64())
                    .map_or_else(|| self.type_error(), |v| _visitor.visit_f64(v));
            }

            self.type_error_str("f64")
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        self.0
            .downcast_ref::<char>()
            .map_or_else(|| self.type_error(), |&x| visitor.visit_char(x))
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        self.0
            .downcast_ref::<ImmutableString>()
            .map_or_else(|| self.type_error(), |x| visitor.visit_borrowed_str(x))
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, _visitor: V) -> RhaiResultOf<V::Value> {
        #[cfg(not(feature = "no_index"))]
        return self
            .0
            .downcast_ref::<crate::Blob>()
            .map_or_else(|| self.type_error(), |x| _visitor.visit_bytes(x));

        #[cfg(feature = "no_index")]
        return self.type_error();
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        if self.0.is_unit() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        self.0
            .downcast_ref::<()>()
            .map_or_else(|| self.type_error(), |()| visitor.visit_unit())
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> RhaiResultOf<V::Value> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> RhaiResultOf<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, _visitor: V) -> RhaiResultOf<V::Value> {
        #[cfg(not(feature = "no_index"))]
        return self.0.downcast_ref::<crate::Array>().map_or_else(
            || self.type_error(),
            |arr| _visitor.visit_seq(IterateDynamicArray::new(arr.iter())),
        );

        #[cfg(feature = "no_index")]
        return self.type_error();
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, visitor: V) -> RhaiResultOf<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> RhaiResultOf<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, _visitor: V) -> RhaiResultOf<V::Value> {
        #[cfg(not(feature = "no_object"))]
        return self.0.downcast_ref::<crate::Map>().map_or_else(
            || self.type_error(),
            |map| {
                _visitor.visit_map(IterateMap::new(
                    map.keys().map(crate::SmartString::as_str),
                    map.values(),
                ))
            },
        );

        #[cfg(feature = "no_object")]
        return self.type_error();
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> RhaiResultOf<V::Value> {
        #[cfg(not(feature = "no_object"))]
        return self.0.downcast_ref::<crate::Map>().map_or_else(
            || {
                Err(ERR::ErrorMismatchOutputType(
                    map_std_type_name(type_name::<crate::Map>(), false).into(),
                    map_std_type_name(self.0.type_name(), false).into(),
                    Position::NONE,
                )
                .into())
            },
            |map| {
                _visitor.visit_map(IterateMap::new(
                    map.keys().map(crate::SmartString::as_str),
                    map.values(),
                ))
            },
        );

        #[cfg(feature = "no_object")]
        return self.type_error();
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> RhaiResultOf<V::Value> {
        match self.0.as_immutable_string_ref() {
            Ok(s) => visitor.visit_enum(s.into_deserializer()),
            Err(_) => {
                #[cfg(not(feature = "no_object"))]
                return self.0.downcast_ref::<crate::Map>().map_or_else(
                    || self.type_error(),
                    |map| {
                        let mut iter = map.iter();
                        let first = iter.next();
                        let second = iter.next();
                        match (first, second) {
                            (Some((key, value)), None) => visitor.visit_enum(EnumDeserializer {
                                tag: key,
                                content: DynamicDeserializer::new(value),
                            }),
                            _ => self.type_error(),
                        }
                    },
                );
                #[cfg(feature = "no_object")]
                return self.type_error();
            }
        }
    }

    #[inline(always)]
    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        self.deserialize_str(visitor)
    }

    #[inline(always)]
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> RhaiResultOf<V::Value> {
        self.deserialize_any(visitor)
    }
}

/// `SeqAccess` implementation for arrays.
#[cfg(not(feature = "no_index"))]
struct IterateDynamicArray<'de, ITER: Iterator<Item = &'de Dynamic>> {
    /// Iterator for a stream of [`Dynamic`][crate::Dynamic] values.
    iter: ITER,
}

#[cfg(not(feature = "no_index"))]
impl<'de, ITER: Iterator<Item = &'de Dynamic>> IterateDynamicArray<'de, ITER> {
    #[inline(always)]
    #[must_use]
    pub const fn new(iter: ITER) -> Self {
        Self { iter }
    }
}

#[cfg(not(feature = "no_index"))]
impl<'de, ITER: Iterator<Item = &'de Dynamic>> serde::de::SeqAccess<'de>
    for IterateDynamicArray<'de, ITER>
{
    type Error = RhaiError;

    fn next_element_seed<T: serde::de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> RhaiResultOf<Option<T::Value>> {
        // Deserialize each item coming out of the iterator.
        self.iter.next().map_or(Ok(None), |item| {
            seed.deserialize(item.into_deserializer()).map(Some)
        })
    }
}

/// `MapAccess` implementation for maps.
#[cfg(not(feature = "no_object"))]
struct IterateMap<'de, K: Iterator<Item = &'de str>, V: Iterator<Item = &'de Dynamic>> {
    // Iterator for a stream of [`Dynamic`][crate::Dynamic] keys.
    keys: K,
    // Iterator for a stream of [`Dynamic`][crate::Dynamic] values.
    values: V,
}

#[cfg(not(feature = "no_object"))]
impl<'de, K: Iterator<Item = &'de str>, V: Iterator<Item = &'de Dynamic>> IterateMap<'de, K, V> {
    #[inline(always)]
    #[must_use]
    pub const fn new(keys: K, values: V) -> Self {
        Self { keys, values }
    }
}

#[cfg(not(feature = "no_object"))]
impl<'de, K: Iterator<Item = &'de str>, V: Iterator<Item = &'de Dynamic>> serde::de::MapAccess<'de>
    for IterateMap<'de, K, V>
{
    type Error = RhaiError;

    fn next_key_seed<S: serde::de::DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> RhaiResultOf<Option<S::Value>> {
        // Deserialize each `Identifier` key coming out of the keys iterator.
        self.keys
            .next()
            .map(<_>::into_deserializer)
            .map_or(Ok(None), |d| seed.deserialize(d).map(Some))
    }

    fn next_value_seed<S: serde::de::DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> RhaiResultOf<S::Value> {
        // Deserialize each value item coming out of the iterator.
        seed.deserialize(self.values.next().unwrap().into_deserializer())
    }
}

#[cfg(not(feature = "no_object"))]
struct EnumDeserializer<'de> {
    tag: &'de str,
    content: DynamicDeserializer<'de>,
}

#[cfg(not(feature = "no_object"))]
impl<'de> serde::de::EnumAccess<'de> for EnumDeserializer<'de> {
    type Error = RhaiError;
    type Variant = Self;

    fn variant_seed<V: serde::de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> RhaiResultOf<(V::Value, Self::Variant)> {
        seed.deserialize(self.tag.into_deserializer())
            .map(|v| (v, self))
    }
}

#[cfg(not(feature = "no_object"))]
impl<'de> serde::de::VariantAccess<'de> for EnumDeserializer<'de> {
    type Error = RhaiError;

    #[inline(always)]
    fn unit_variant(self) -> RhaiResultOf<()> {
        Deserialize::deserialize(self.content)
    }

    #[inline(always)]
    fn newtype_variant_seed<T: serde::de::DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> RhaiResultOf<T::Value> {
        seed.deserialize(self.content)
    }

    #[inline(always)]
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> RhaiResultOf<V::Value> {
        self.content.deserialize_tuple(len, visitor)
    }

    #[inline(always)]
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> RhaiResultOf<V::Value> {
        self.content.deserialize_struct("", fields, visitor)
    }
}
