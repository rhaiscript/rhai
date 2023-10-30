//! Variable declaration information.

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Information on a variable declaration.
#[derive(Debug, Clone, Hash)]
pub struct VarDefInfo<'a> {
    /// Name of the variable to be declared.
    #[deprecated(
        since = "1.16.0",
        note = "VarDefInfo fields will be private in the next major version. Use the methods instead."
    )]
    pub name: &'a str,
    /// `true` if the statement is `const`, otherwise it is `let`.
    #[deprecated(
        since = "1.16.0",
        note = "VarDefInfo fields will be private in the next major version. Use the methods instead."
    )]
    pub is_const: bool,
    /// The current nesting level, with zero being the global level.
    #[deprecated(
        since = "1.16.0",
        note = "VarDefInfo fields will be private in the next major version. Use the methods instead."
    )]
    pub nesting_level: usize,
    /// Will the variable _shadow_ an existing variable?
    #[deprecated(
        since = "1.16.0",
        note = "VarDefInfo fields will be private in the next major version. Use the methods instead."
    )]
    pub will_shadow: bool,
}

impl<'a> VarDefInfo<'a> {
    /// Create a new [`VarDefInfo`].
    #[inline(always)]
    #[must_use]
    pub(crate) const fn new(
        name: &'a str,
        is_const: bool,
        nesting_level: usize,
        will_shadow: bool,
    ) -> Self {
        Self {
            name,
            is_const,
            nesting_level,
            will_shadow,
        }
    }
    /// Name of the variable to be declared.
    #[inline(always)]
    #[must_use]
    pub const fn name(&self) -> &str {
        self.name
    }
    /// `true` if the statement is `const`, otherwise it is `let`.
    #[inline(always)]
    #[must_use]
    pub const fn is_const(&self) -> bool {
        self.is_const
    }
    /// The current nesting level, with zero being the global level.
    #[inline(always)]
    #[must_use]
    pub const fn nesting_level(&self) -> usize {
        self.nesting_level
    }
    /// `true` if the variable is declared at global level (i.e. nesting level zero).
    #[inline(always)]
    #[must_use]
    pub const fn is_global_level(&self) -> bool {
        self.nesting_level == 0
    }
    /// Will the variable _shadow_ an existing variable?
    #[inline(always)]
    #[must_use]
    pub const fn will_shadow_other_variables(&self) -> bool {
        self.will_shadow
    }
}
