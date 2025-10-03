//! Archive trait implementations for Rhai types.

use crate::{Dynamic, ImmutableString, INT};
use rkyv::{Archive, Deserialize, Serialize};
use std::string::String;

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_index"))]
use crate::Array;
#[cfg(not(feature = "no_object"))]
use crate::Map;

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
use super::{de, ser};

// ============================================================================
// ImmutableString
// ============================================================================

/// ImmutableString can be archived as a regular String since rkyv has built-in
/// support for String, and we can convert back and forth easily.
impl Archive for ImmutableString {
    type Archived = rkyv::string::ArchivedString;
    type Resolver = rkyv::string::StringResolver;

    #[inline]
    unsafe fn resolve(&self, pos: usize, resolver: Self::Resolver, out: *mut Self::Archived) {
        rkyv::string::ArchivedString::resolve_from_str(self.as_str(), pos, resolver, out);
    }
}

impl<S> Serialize<S> for ImmutableString
where
    S: rkyv::ser::Serializer + ?Sized,
{
    #[inline]
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        rkyv::string::ArchivedString::serialize_from_str(self.as_str(), serializer)
    }
}

impl<D> Deserialize<ImmutableString, D> for rkyv::string::ArchivedString
where
    D: rkyv::Fallible + ?Sized,
{
    #[inline]
    fn deserialize(&self, _: &mut D) -> Result<ImmutableString, D::Error> {
        Ok(ImmutableString::from(self.as_str()))
    }
}

// ============================================================================
// Dynamic - This is the most complex type
// ============================================================================

// Note: Dynamic contains a Union enum which has many variants. We'll need to
// create an archived representation that can handle all these variants.
// For now, we'll start with a simpler approach using a SimpleDynamic intermediate.

use crate::types::dynamic::Union;

/// Simplified representation of Dynamic for archiving.
/// Mirrors serde's approach by recursively containing SimpleDynamic for arrays and maps.
#[derive(Clone, Archive, Deserialize, Serialize)]
pub enum SimpleDynamic {
    /// Unit value
    Unit,
    /// Boolean
    Bool(bool),
    /// String
    Str(String),
    /// Character
    Char(char),
    /// Integer
    Int(INT),
    /// Float
    #[cfg(not(feature = "no_float"))]
    Float(FLOAT),
    /// Blob (binary data)
    #[cfg(not(feature = "no_index"))]
    Blob(Vec<u8>),
    /// Array of archived Dynamic values (each element stored as serialized bytes)
    #[cfg(not(feature = "no_index"))]
    Array(Vec<Vec<u8>>),
    /// Object map of archived Dynamic values (each value stored as serialized bytes)
    #[cfg(not(feature = "no_object"))]
    Map(Vec<(String, Vec<u8>)>),
}

impl From<&Dynamic> for SimpleDynamic {
    fn from(value: &Dynamic) -> Self {
        println!(
            "From<&Dynamic> for SimpleDynamic: type={}, is_int={}",
            value.type_name(),
            value.is_int()
        );
        println!("Union variant: {:?}", std::mem::discriminant(&value.0));
        let result = match &value.0 {
            Union::Unit(_, _, _) => {
                println!("Matched Union::Unit");
                SimpleDynamic::Unit
            }
            Union::Bool(v, _, _) => {
                println!("Matched Union::Bool({})", *v);
                SimpleDynamic::Bool(*v)
            }
            Union::Str(s, _, _) => {
                println!("Matched Union::Str({})", s.as_str());
                SimpleDynamic::Str(String::from(s.as_str()))
            }
            Union::Char(c, _, _) => {
                println!("Matched Union::Char({})", *c);
                SimpleDynamic::Char(*c)
            }
            Union::Int(i, _, _) => {
                println!("Matched Union::Int({})", *i);
                SimpleDynamic::Int(*i)
            }

            #[cfg(not(feature = "no_float"))]
            Union::Float(f, _, _) => {
                println!("Matched Union::Float({})", **f);
                SimpleDynamic::Float(**f)
            }

            #[cfg(not(feature = "no_index"))]
            Union::Blob(blob, _, _) => {
                println!("Matched Union::Blob");
                SimpleDynamic::Blob(blob.to_vec())
            }

            #[cfg(not(feature = "no_index"))]
            Union::Array(array, _, _) => {
                println!("Matched Union::Array with {} elements", array.len());
                SimpleDynamic::Array(
                    array
                        .iter()
                        .map(|item| serialize_nested_dynamic(item))
                        .collect(),
                )
            }

            #[cfg(not(feature = "no_object"))]
            Union::Map(map, _, _) => {
                println!("Matched Union::Map with {} entries", map.len());
                SimpleDynamic::Map(
                    map.iter()
                        .map(|(key, value)| {
                            (String::from(key.as_str()), serialize_nested_dynamic(value))
                        })
                        .collect(),
                )
            }

            #[cfg(not(feature = "no_closure"))]
            #[cfg(feature = "sync")]
            Union::Shared(cell, _, _) => SimpleDynamic::from(&*cell.read().unwrap()),

            _ => {
                println!("Fell through to fallback path");
                // Fallback path using safe Dynamic API conversions
                // INT
                if let Ok(i) = value.as_int() {
                    println!("Fallback: converting int {}", i);
                    return SimpleDynamic::Int(i);
                }
                // Bool
                if let Ok(b) = value.as_bool() {
                    println!("Fallback: converting bool {}", b);
                    return SimpleDynamic::Bool(b);
                }
                // Char
                if let Ok(c) = value.as_char() {
                    println!("Fallback: converting char {}", c);
                    return SimpleDynamic::Char(c);
                }
                // String
                if let Ok(s) = value.clone().into_immutable_string() {
                    println!("Fallback: converting string {}", s);
                    return SimpleDynamic::Str(String::from(s.as_str()));
                }
                // Blob
                #[cfg(not(feature = "no_index"))]
                if let Ok(blob) = value.clone().into_blob() {
                    println!("Fallback: converting blob");
                    return SimpleDynamic::Blob(blob);
                }
                // Array
                #[cfg(not(feature = "no_index"))]
                if let Ok(arr) = value.clone().into_array() {
                    println!("Fallback: converting array");
                    let serialized = arr
                        .iter()
                        .map(|item| serialize_nested_dynamic(item))
                        .collect();
                    return SimpleDynamic::Array(serialized);
                }
                // Map
                #[cfg(not(feature = "no_object"))]
                if let Some(map) = value.clone().try_cast::<Map>() {
                    println!("Fallback: converting map");
                    let entries = map
                        .into_iter()
                        .map(|(k, v)| (String::from(k.as_str()), serialize_nested_dynamic(&v)))
                        .collect();
                    return SimpleDynamic::Map(entries);
                }
                println!(
                    "Fallback: converting to unit for type {}",
                    value.type_name()
                );
                SimpleDynamic::Unit
            }
        };
        println!(
            "From<&Dynamic> result: {:?}",
            std::mem::discriminant(&result)
        );
        result
    }
}

impl From<SimpleDynamic> for Dynamic {
    fn from(value: SimpleDynamic) -> Self {
        let result = match value {
            SimpleDynamic::Unit => Dynamic::UNIT,
            SimpleDynamic::Bool(v) => Dynamic::from(v),
            SimpleDynamic::Str(s) => Dynamic::from(s),
            SimpleDynamic::Char(c) => Dynamic::from(c),
            SimpleDynamic::Int(i) => {
                println!("Converting SimpleDynamic::Int({}) to Dynamic", i);
                Dynamic::from(i)
            }

            #[cfg(not(feature = "no_float"))]
            SimpleDynamic::Float(f) => Dynamic::from(f),

            #[cfg(not(feature = "no_index"))]
            SimpleDynamic::Blob(blob) => Dynamic::from(blob),

            #[cfg(not(feature = "no_index"))]
            SimpleDynamic::Array(elements) => {
                println!(
                    "Converting SimpleDynamic::Array with {} elements",
                    elements.len()
                );
                let array: Array = elements
                    .into_iter()
                    .map(|bytes| deserialize_nested_dynamic(&bytes))
                    .collect();
                Dynamic::from_array(array)
            }

            #[cfg(not(feature = "no_object"))]
            SimpleDynamic::Map(entries) => {
                let mut map: Map = Map::new();
                for (key, bytes) in entries {
                    map.insert(key.into(), deserialize_nested_dynamic(&bytes));
                }
                Dynamic::from_map(map)
            }
        };
        println!("SimpleDynamic::from result: type={}", result.type_name());
        result
    }
}

// Implement Archive for Dynamic by using SimpleDynamic as an intermediate
impl Archive for Dynamic {
    type Archived = rkyv::Archived<SimpleDynamic>;
    type Resolver = rkyv::Resolver<SimpleDynamic>;

    #[inline]
    unsafe fn resolve(&self, pos: usize, resolver: Self::Resolver, out: *mut Self::Archived) {
        let simple = SimpleDynamic::from(self);
        simple.resolve(pos, resolver, out);
    }
}

impl<S> Serialize<S> for Dynamic
where
    S: rkyv::ser::Serializer + rkyv::ser::ScratchSpace + ?Sized,
{
    #[inline]
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let simple = SimpleDynamic::from(self);
        simple.serialize(serializer)
    }
}

impl<D> Deserialize<Dynamic, D> for rkyv::Archived<SimpleDynamic>
where
    D: rkyv::Fallible + ?Sized,
{
    #[inline]
    fn deserialize(&self, deserializer: &mut D) -> Result<Dynamic, D::Error> {
        let simple: SimpleDynamic = self.deserialize(deserializer)?;
        Ok(simple.into())
    }
}

// Nested helpers for byte-based serialization to avoid recursive derive issues
#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
fn serialize_nested_dynamic(value: &Dynamic) -> Vec<u8> {
    // Serialize as SimpleDynamic for nested elements to match deserialization
    let simple = SimpleDynamic::from(value);
    rkyv::to_bytes::<SimpleDynamic, 1024>(&simple)
        .expect("serializing nested Dynamic values should not fail")
        .into_vec()
}

#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
fn deserialize_nested_dynamic(bytes: &[u8]) -> Dynamic {
    // Deserialize using SimpleDynamic root for nested elements
    let archived = unsafe { rkyv::archived_root::<SimpleDynamic>(bytes) };
    let simple: SimpleDynamic = archived.deserialize(&mut rkyv::Infallible).unwrap();
    simple.into()
}
