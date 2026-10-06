//! Encoding of namespace-qualified names in the name pool.
//!
//! A qualified variable or function is stored as one entry, written the way a
//! script writes it: `ns::name`, or `a::b::name` for a nested namespace. A call
//! that captures the parent scope additionally carries a trailing `!`, which is
//! stripped before the name is split.

use crate::types::Token;
#[cfg(all(feature = "no_std", not(feature = "no_module")))]
use std::prelude::v1::*;

/// Separator between a namespace and a name; the same as
/// [`NAMESPACE_SEPARATOR`][crate::engine::NAMESPACE_SEPARATOR], which does not
/// exist under `no_module`.
const SEPARATOR: &str = Token::DoubleColon.literal_syntax();

/// Encode `name` qualified by `namespace`.
#[cfg(not(feature = "no_module"))]
#[inline]
pub(crate) fn qualify(namespace: impl core::fmt::Display, name: &str) -> String {
    format!("{namespace}{SEPARATOR}{name}")
}

/// Split a name-pool entry into its namespace and unqualified name.
///
/// Returns `None` for an unqualified name, including one with an empty
/// namespace or name on either side of the separator.
#[inline]
pub(crate) fn split_qualified(name: &str) -> Option<(&str, &str)> {
    match name.rsplit_once(SEPARATOR)? {
        ("", _) | (_, "") => None,
        parts => Some(parts),
    }
}
