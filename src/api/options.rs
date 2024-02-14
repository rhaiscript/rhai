//! Settings for [`Engine`]'s language options.

use crate::Engine;
use bitflags::bitflags;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

bitflags! {
    /// Bit-flags containing all language options for the [`Engine`].
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct LangOptions: u16 {
        /// Is `if`-expression allowed?
        const IF_EXPR = 0b_0000_0000_0001;
        /// Is `switch` expression allowed?
        const SWITCH_EXPR = 0b_0000_0000_0010;
        /// Are loop expressions allowed?
        const LOOP_EXPR = 0b_0000_0000_0100;
        /// Is statement-expression allowed?
        const STMT_EXPR = 0b_0000_0000_1000;
        /// Is anonymous function allowed?
        #[cfg(not(feature = "no_function"))]
        const ANON_FN = 0b_0000_0001_0000;
        /// Is looping allowed?
        const LOOPING = 0b_0000_0010_0000;
        /// Is variables shadowing allowed?
        const SHADOWING = 0b_0000_0100_0000;
        /// Strict variables mode?
        const STRICT_VAR = 0b_0000_1000_0000;
        /// Raise error if an object map property does not exist?
        /// Returns `()` if `false`.
        #[cfg(not(feature = "no_object"))]
        const FAIL_ON_INVALID_MAP_PROPERTY = 0b_0001_0000_0000;
        /// Fast operators mode?
        const FAST_OPS = 0b_0010_0000_0000;
    }
}

impl LangOptions {
    /// Create a new [`LangOptions`] with default values.
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self::from_bits_truncate(
            Self::IF_EXPR.bits()
                | Self::SWITCH_EXPR.bits()
                | Self::LOOP_EXPR.bits()
                | Self::STMT_EXPR.bits()
                | Self::LOOPING.bits()
                | Self::SHADOWING.bits()
                | Self::FAST_OPS.bits()
                | {
                    #[cfg(not(feature = "no_function"))]
                    {
                        Self::ANON_FN.bits()
                    }
                    #[cfg(feature = "no_function")]
                    {
                        Self::empty().bits()
                    }
                },
        )
    }
}

impl Engine {
    /// Is `if`-expression allowed?
    /// Default is `true`.
    #[inline(always)]
    #[must_use]
    pub const fn allow_if_expression(&self) -> bool {
        self.options.intersects(LangOptions::IF_EXPR)
    }
    /// Set whether `if`-expression is allowed.
    #[inline(always)]
    pub fn set_allow_if_expression(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::IF_EXPR, enable);
        self
    }
    /// Is `switch` expression allowed?
    /// Default is `true`.
    #[inline(always)]
    #[must_use]
    pub const fn allow_switch_expression(&self) -> bool {
        self.options.intersects(LangOptions::SWITCH_EXPR)
    }
    /// Set whether `switch` expression is allowed.
    #[inline(always)]
    pub fn set_allow_switch_expression(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::SWITCH_EXPR, enable);
        self
    }
    /// Are loop expressions allowed?
    /// Default is `true`.
    #[inline(always)]
    #[must_use]
    pub const fn allow_loop_expressions(&self) -> bool {
        self.options.intersects(LangOptions::LOOP_EXPR)
    }
    /// Set whether loop expressions are allowed.
    #[inline(always)]
    pub fn set_allow_loop_expressions(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::LOOP_EXPR, enable);
        self
    }
    /// Is statement-expression allowed?
    /// Default is `true`.
    #[inline(always)]
    #[must_use]
    pub const fn allow_statement_expression(&self) -> bool {
        self.options.intersects(LangOptions::STMT_EXPR)
    }
    /// Set whether statement-expression is allowed.
    #[inline(always)]
    pub fn set_allow_statement_expression(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::STMT_EXPR, enable);
        self
    }
    /// Is anonymous function allowed?
    /// Default is `true`.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub const fn allow_anonymous_fn(&self) -> bool {
        self.options.intersects(LangOptions::ANON_FN)
    }
    /// Set whether anonymous function is allowed.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    pub fn set_allow_anonymous_fn(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::ANON_FN, enable);
        self
    }
    /// Is looping allowed?
    /// Default is `true`.
    #[inline(always)]
    #[must_use]
    pub const fn allow_looping(&self) -> bool {
        self.options.intersects(LangOptions::LOOPING)
    }
    /// Set whether looping is allowed.
    #[inline(always)]
    pub fn set_allow_looping(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::LOOPING, enable);
        self
    }
    /// Is variables shadowing allowed?
    /// Default is `true`.
    #[inline(always)]
    #[must_use]
    pub const fn allow_shadowing(&self) -> bool {
        self.options.intersects(LangOptions::SHADOWING)
    }
    /// Set whether variables shadowing is allowed.
    #[inline(always)]
    pub fn set_allow_shadowing(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::SHADOWING, enable);
        self
    }
    /// Is strict variables mode enabled?
    /// Default is `false`.
    #[inline(always)]
    #[must_use]
    pub const fn strict_variables(&self) -> bool {
        self.options.intersects(LangOptions::STRICT_VAR)
    }
    /// Set whether strict variables mode is enabled.
    #[inline(always)]
    pub fn set_strict_variables(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::STRICT_VAR, enable);
        self
    }
    /// Raise error if an object map property does not exist?
    /// Default is `false`.
    ///
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    #[must_use]
    pub const fn fail_on_invalid_map_property(&self) -> bool {
        self.options
            .intersects(LangOptions::FAIL_ON_INVALID_MAP_PROPERTY)
    }
    /// Set whether to raise error if an object map property does not exist.
    ///
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    pub fn set_fail_on_invalid_map_property(&mut self, enable: bool) -> &mut Self {
        self.options
            .set(LangOptions::FAIL_ON_INVALID_MAP_PROPERTY, enable);
        self
    }
    /// Is fast operators mode enabled?
    /// Default is `false`.
    #[inline(always)]
    #[must_use]
    pub const fn fast_operators(&self) -> bool {
        self.options.intersects(LangOptions::FAST_OPS)
    }
    /// Set whether fast operators mode is enabled.
    #[inline(always)]
    pub fn set_fast_operators(&mut self, enable: bool) -> &mut Self {
        self.options.set(LangOptions::FAST_OPS, enable);
        self
    }
}
