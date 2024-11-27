//! Module defining script options.

use bitflags::bitflags;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// A type representing the access mode of a function.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
#[non_exhaustive]
pub enum FnAccess {
    /// Private function.
    Private,
    /// Public function.
    Public,
}

impl FnAccess {
    /// Is this function private?
    #[inline(always)]
    #[must_use]
    pub const fn is_private(self) -> bool {
        match self {
            Self::Private => true,
            Self::Public => false,
        }
    }
    /// Is this function public?
    #[inline(always)]
    #[must_use]
    pub const fn is_public(self) -> bool {
        match self {
            Self::Private => false,
            Self::Public => true,
        }
    }
}

bitflags! {
    /// _(internals)_ Bit-flags containing [`AST`][crate::AST] node configuration options.
    /// Exported under the `internals` feature only.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
    #[cfg_attr(feature = "ast_serde", derive(serde::Serialize, serde::Deserialize),serde(transparent))]
    pub struct ASTFlags: u8 {
        /// The [`AST`][crate::AST] node is read-only.
        const CONSTANT = 0b_0000_0001;
        /// The [`AST`][crate::AST] node is exposed to the outside (i.e. public).
        const EXPORTED = 0b_0000_0010;
        /// The [`AST`][crate::AST] node is negated (i.e. whatever information is the opposite).
        const NEGATED = 0b_0000_0100;
        /// The [`AST`][crate::AST] node breaks out of normal control flow.
        const BREAK = 0b_0000_1000;
    }
}

impl ASTFlags {
    /// No flags.
    pub const NONE: Self = Self::empty();
}

impl std::fmt::Debug for ASTFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", &self.0)
    }
}
