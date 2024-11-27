//! Placeholder script character position type.
#![cfg(feature = "no_position")]
#![allow(unused_variables)]

#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    fmt,
    ops::{Add, AddAssign},
};

/// A location (line number + character position) in the input script.
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy, Default)]
#[cfg_attr(feature = "ast-serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Position;

impl Position {
    /// A [`Position`] representing no position.
    pub const NONE: Self = Self;
    /// A [`Position`] representing the first position.
    pub const START: Self = Self;

    /// Create a new [`Position`].
    #[inline(always)]
    #[must_use]
    pub const fn new(line: u16, position: u16) -> Self {
        Self
    }
    /// Get the line number (1-based), or [`None`] if there is no position.
    ///
    /// Always returns [`None`].
    #[inline(always)]
    #[must_use]
    pub const fn line(self) -> Option<usize> {
        None
    }
    /// Get the character position (1-based), or [`None`] if at beginning of a line.
    ///
    /// Always returns [`None`].
    #[inline(always)]
    #[must_use]
    pub const fn position(self) -> Option<usize> {
        None
    }
    /// Advance by one character position.
    #[inline(always)]
    pub(crate) fn advance(&mut self) {}
    /// Go backwards by one character position.
    #[inline(always)]
    pub(crate) fn rewind(&mut self) {}
    /// Advance to the next line.
    #[inline(always)]
    pub(crate) fn new_line(&mut self) {}
    /// Is this [`Position`] at the beginning of a line?
    ///
    /// Always returns `false`.
    #[inline(always)]
    #[must_use]
    pub const fn is_beginning_of_line(self) -> bool {
        false
    }
    /// Is there no [`Position`]?
    ///
    /// Always returns `true`.
    #[inline(always)]
    #[must_use]
    pub const fn is_none(self) -> bool {
        true
    }
    /// Returns an fallback [`Position`] if it is [`NONE`][Position::NONE]?
    ///
    /// Always returns the fallback.
    #[inline(always)]
    #[must_use]
    pub const fn or_else(self, pos: Self) -> Self {
        pos
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "none")
    }
}

impl fmt::Debug for Position {
    #[cold]
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("none")
    }
}

impl Add for Position {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self
    }
}

impl AddAssign for Position {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {}
}

/// _(internals)_ A span consisting of a starting and an ending [positions][Position].
/// Exported under the `internals` feature only.
#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy, Default)]
#[cfg_attr(feature = "ast-serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span;

impl Span {
    /// Empty [`Span`].
    pub const NONE: Self = Self;

    /// Create a new [`Span`].
    #[inline(always)]
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self
    }
    /// Is this [`Span`] non-existent?
    ///
    /// Always returns `true`.
    #[inline(always)]
    #[must_use]
    pub const fn is_none(&self) -> bool {
        true
    }
    /// Get the [`Span`]'s starting [position][Position].
    ///
    /// Always returns [`Position::NONE`].
    #[inline(always)]
    #[must_use]
    pub const fn start(&self) -> Position {
        Position::NONE
    }
    /// Get the [`Span`]'s ending [position][Position].
    ///
    /// Always returns [`Position::NONE`].
    #[inline(always)]
    #[must_use]
    pub const fn end(&self) -> Position {
        Position::NONE
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let f = f;
        write!(f, "{:?}", Position)
    }
}

impl fmt::Debug for Span {
    #[cold]
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
