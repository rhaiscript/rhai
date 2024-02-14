//! Namespace reference type.
#![cfg(not(feature = "no_module"))]

use crate::ast::Ident;
use crate::{Position, StaticVec};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{fmt, num::NonZeroUsize};

/// _(internals)_ A chain of [module][crate::Module] names to namespace-qualify a variable or function call.
/// Exported under the `internals` feature only.
///
/// Not available under `no_module`.
///
/// A [`u64`] offset to the current stack of imported [modules][crate::Module] in the
/// [global runtime state][crate::GlobalRuntimeState] is cached for quick search purposes.
///
/// A [`StaticVec`] is used because the vast majority of namespace-qualified access contains only
/// one level, and it is wasteful to always allocate a [`Vec`] with one element.
#[derive(Clone, Eq, PartialEq, Default, Hash)]
#[non_exhaustive]
pub struct Namespace {
    /// Path segments.
    pub path: StaticVec<Ident>,
    /// Cached index into the current stack of imported [modules][crate::Module], if any.
    pub index: Option<NonZeroUsize>,
}

impl fmt::Debug for Namespace {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("NONE");
        }

        if let Some(index) = self.index {
            write!(f, "{index} -> ")?;
        }

        f.write_str(
            &self
                .path
                .iter()
                .map(Ident::as_str)
                .collect::<StaticVec<_>>()
                .join(crate::engine::NAMESPACE_SEPARATOR),
        )
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return Ok(());
        }

        f.write_str(
            &self
                .path
                .iter()
                .map(Ident::as_str)
                .collect::<StaticVec<_>>()
                .join(crate::engine::NAMESPACE_SEPARATOR),
        )
    }
}

impl Namespace {
    /// Constant for no namespace.
    pub const NONE: Self = Self {
        index: None,
        path: StaticVec::new_const(),
    };
    /// Is this [`Namespace`] empty?
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.path.is_empty()
    }
    /// Get the [position][Position] of this [`Namespace`].
    ///
    /// # Panics
    ///
    /// Panics if the path is empty.
    #[inline(always)]
    #[must_use]
    pub fn position(&self) -> Position {
        self.path[0].pos
    }
    /// Get the first path segment of this [`Namespace`].
    ///
    /// # Panics
    ///
    /// Panics if the path is empty.
    #[inline(always)]
    #[must_use]
    pub fn root(&self) -> &str {
        &self.path[0].name
    }
}
