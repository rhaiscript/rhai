#![cfg(not(feature = "no_float"))]

#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
    str::FromStr,
};

use num_traits::float::FloatCore as Float;

/// _(internals)_ A type that wraps a floating-point number and implements [`Hash`].
/// Exported under the `internals` feature only.
///
/// Not available under `no_float`.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd)]
#[must_use]
#[cfg_attr(feature = "ast_serde", derive(serde::Serialize, serde::Deserialize),serde(transparent))]
pub struct FloatWrapper<F>(F);

impl Hash for FloatWrapper<crate::FLOAT> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_ne_bytes().hash(state);
    }
}

impl<F: Float> AsRef<F> for FloatWrapper<F> {
    #[inline(always)]
    #[must_use]
    fn as_ref(&self) -> &F {
        &self.0
    }
}

impl<F: Float> AsMut<F> for FloatWrapper<F> {
    #[inline(always)]
    #[must_use]
    fn as_mut(&mut self) -> &mut F {
        &mut self.0
    }
}

impl<F: Float> Deref for FloatWrapper<F> {
    type Target = F;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<F: Float> DerefMut for FloatWrapper<F> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<F: Float + fmt::Debug> fmt::Debug for FloatWrapper<F> {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<F: Float + fmt::Display + fmt::LowerExp + From<f32>> fmt::Display for FloatWrapper<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let abs = self.0.abs();
        if abs.is_zero() {
            f.write_str("0.0")
        } else if abs > Self::MAX_NATURAL_FLOAT_FOR_DISPLAY.into()
            || abs < Self::MIN_NATURAL_FLOAT_FOR_DISPLAY.into()
        {
            write!(f, "{:e}", self.0)
        } else {
            fmt::Display::fmt(&self.0, f)?;
            if abs.fract().is_zero() {
                f.write_str(".0")?;
            }
            Ok(())
        }
    }
}

impl<F: Float> From<F> for FloatWrapper<F> {
    #[inline(always)]
    fn from(value: F) -> Self {
        Self::new(value)
    }
}

impl<F: Float + FromStr> FromStr for FloatWrapper<F> {
    type Err = <F as FromStr>::Err;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        F::from_str(s).map(Into::into)
    }
}

impl<F: Float> FloatWrapper<F> {
    /// Maximum floating-point number for natural display before switching to scientific notation.
    pub const MAX_NATURAL_FLOAT_FOR_DISPLAY: f32 = 10_000_000_000_000.0;

    /// Minimum floating-point number for natural display before switching to scientific notation.
    pub const MIN_NATURAL_FLOAT_FOR_DISPLAY: f32 = 0.000_000_000_000_1;

    /// Create a new [`FloatWrapper`].
    #[inline(always)]
    pub const fn new(value: F) -> Self {
        Self(value)
    }
}
