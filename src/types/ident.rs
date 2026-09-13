//! Module defining script identifiers.

use crate::{ImmutableString, Position};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{borrow::Borrow, fmt, hash::Hash};

/// _(internals)_ An identifier containing a name and a [position][Position].
/// Exported under the `internals` feature only.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Ident {
    /// Identifier name.
    pub name: ImmutableString,
    /// Position.
    pub pos: Position,
}

impl fmt::Debug for Ident {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.name)?;
        if !self.pos.is_none() {
            write!(f, " @ {:?}", self.pos)?;
        }
        Ok(())
    }
}

impl Borrow<str> for Ident {
    #[inline(always)]
    fn borrow(&self) -> &str {
        self.name.as_ref()
    }
}

impl AsRef<str> for Ident {
    #[inline(always)]
    fn as_ref(&self) -> &str {
        self.name.as_ref()
    }
}

impl Ident {
    /// Get the name of the identifier as a string slice.
    #[inline(always)]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.name
    }
    /// Is the identifier empty?
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }
}
