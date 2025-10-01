//! Archive trait implementations for Rhai types.

use crate::{Dynamic, ImmutableString, INT};
use rkyv::{Archive, Deserialize, Serialize};
use std::string::String;

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

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
/// This doesn't include all variants yet - we'll expand it incrementally.
///
/// Phase 1: Basic scalar types (Unit, Bool, Str, Char, Int, Float, Blob)
/// Phase 2: Collections (Array, Map) - requires handling recursion
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
    /// Blob
    #[cfg(not(feature = "no_index"))]
    Blob(Vec<u8>),
    // TODO: Add Array and Map support
    // These require special handling due to recursion
}

impl From<&Dynamic> for SimpleDynamic {
    fn from(value: &Dynamic) -> Self {
        match &value.0 {
            Union::Unit(_, _, _) => SimpleDynamic::Unit,
            Union::Bool(v, _, _) => SimpleDynamic::Bool(*v),
            Union::Str(s, _, _) => SimpleDynamic::Str(s.to_string()),
            Union::Char(c, _, _) => SimpleDynamic::Char(*c),
            Union::Int(i, _, _) => SimpleDynamic::Int(*i),

            #[cfg(not(feature = "no_float"))]
            Union::Float(f, _, _) => SimpleDynamic::Float(**f),

            #[cfg(not(feature = "no_index"))]
            Union::Blob(blob, _, _) => SimpleDynamic::Blob(blob.to_vec()),

            // For unsupported variants (arrays, maps, etc.), default to Unit
            // We'll expand this as we add support for more types
            _ => SimpleDynamic::Unit,
        }
    }
}

impl From<SimpleDynamic> for Dynamic {
    fn from(value: SimpleDynamic) -> Self {
        match value {
            SimpleDynamic::Unit => Dynamic::UNIT,
            SimpleDynamic::Bool(v) => Dynamic::from(v),
            SimpleDynamic::Str(s) => Dynamic::from(s),
            SimpleDynamic::Char(c) => Dynamic::from(c),
            SimpleDynamic::Int(i) => Dynamic::from(i),

            #[cfg(not(feature = "no_float"))]
            SimpleDynamic::Float(f) => Dynamic::from(f),

            #[cfg(not(feature = "no_index"))]
            SimpleDynamic::Blob(blob) => Dynamic::from(blob),
        }
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
        Ok(Dynamic::from(simple))
    }
}
