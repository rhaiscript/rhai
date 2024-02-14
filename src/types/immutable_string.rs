//! The `ImmutableString` type.

use crate::func::{shared_get_mut, shared_make_mut, shared_take};
use crate::{Shared, SmartString};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    borrow::Borrow,
    cmp::Ordering,
    fmt,
    hash::Hash,
    iter::FromIterator,
    ops::{Add, AddAssign, Deref, Sub, SubAssign},
    str::FromStr,
};

/// The system immutable string type.
///
/// An [`ImmutableString`] wraps an `Rc<SmartString>` (or `Arc<SmartString>` under the `sync` feature)
/// so that it can be simply shared and not cloned.
///
/// # Example
///
/// ```
/// use rhai::ImmutableString;
///
/// let s1: ImmutableString = "hello".into();
///
/// // No actual cloning of the string is involved below.
/// let s2 = s1.clone();
/// let s3 = s2.clone();
///
/// assert_eq!(s1, s2);
///
/// // Clones the underlying string (because it is already shared) and extracts it.
/// let mut s: String = s1.into_owned();
///
/// // Changing the clone has no impact on the previously shared version.
/// s.push_str(", world!");
///
/// // The old version still exists.
/// assert_eq!(s2, s3);
/// assert_eq!(s2.as_str(), "hello");
///
/// // Not equals!
/// assert_ne!(s2.as_str(), s.as_str());
/// assert_eq!(s, "hello, world!");
/// ```
#[derive(Clone, Eq, Ord, Hash, Default)]
pub struct ImmutableString(Shared<SmartString>);

impl Deref for ImmutableString {
    type Target = str;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for ImmutableString {
    #[inline(always)]
    #[must_use]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<SmartString> for ImmutableString {
    #[inline(always)]
    #[must_use]
    fn as_ref(&self) -> &SmartString {
        &self.0
    }
}

impl Borrow<str> for ImmutableString {
    #[inline(always)]
    #[must_use]
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Borrow<SmartString> for ImmutableString {
    #[inline(always)]
    #[must_use]
    fn borrow(&self) -> &SmartString {
        &self.0
    }
}

impl From<&Self> for ImmutableString {
    #[inline(always)]
    fn from(value: &Self) -> Self {
        Self(value.0.clone())
    }
}

impl From<&str> for ImmutableString {
    #[inline(always)]
    fn from(value: &str) -> Self {
        let value: SmartString = value.into();
        Self(value.into())
    }
}

impl From<Box<str>> for ImmutableString {
    #[inline(always)]
    fn from(value: Box<str>) -> Self {
        let value: SmartString = value.into();
        Self(value.into())
    }
}
impl From<&String> for ImmutableString {
    #[inline(always)]
    fn from(value: &String) -> Self {
        let value: SmartString = value.into();
        Self(value.into())
    }
}
impl From<String> for ImmutableString {
    #[inline(always)]
    fn from(value: String) -> Self {
        let value: SmartString = value.into();
        Self(value.into())
    }
}
impl From<&SmartString> for ImmutableString {
    #[inline(always)]
    fn from(value: &SmartString) -> Self {
        Self(value.clone().into())
    }
}
impl From<SmartString> for ImmutableString {
    #[inline(always)]
    fn from(value: SmartString) -> Self {
        Self(value.into())
    }
}
impl From<&ImmutableString> for SmartString {
    #[inline(always)]
    fn from(value: &ImmutableString) -> Self {
        value.0.as_ref().clone()
    }
}
impl From<ImmutableString> for SmartString {
    #[inline(always)]
    fn from(mut value: ImmutableString) -> Self {
        let _ = value.make_mut(); // Make sure it is unique reference
        shared_take(value.0) // Should succeed
    }
}
impl From<&ImmutableString> for String {
    #[inline(always)]
    fn from(value: &ImmutableString) -> Self {
        value.0.as_ref().to_string()
    }
}
impl From<ImmutableString> for String {
    #[inline(always)]
    fn from(value: ImmutableString) -> Self {
        value.into_owned()
    }
}
impl From<&ImmutableString> for Box<str> {
    #[inline(always)]
    fn from(value: &ImmutableString) -> Self {
        value.0.as_str().into()
    }
}
impl From<ImmutableString> for Box<str> {
    #[inline(always)]
    fn from(value: ImmutableString) -> Self {
        value.0.as_str().into()
    }
}

impl FromStr for ImmutableString {
    type Err = ();

    #[inline(always)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s: SmartString = s.into();
        Ok(Self(s.into()))
    }
}

impl FromIterator<char> for ImmutableString {
    #[inline]
    #[must_use]
    fn from_iter<T: IntoIterator<Item = char>>(iter: T) -> Self {
        Self(iter.into_iter().collect::<SmartString>().into())
    }
}

impl<'a> FromIterator<&'a char> for ImmutableString {
    #[inline]
    #[must_use]
    fn from_iter<T: IntoIterator<Item = &'a char>>(iter: T) -> Self {
        Self(iter.into_iter().copied().collect::<SmartString>().into())
    }
}

impl<'a> FromIterator<&'a str> for ImmutableString {
    #[inline]
    #[must_use]
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        Self(iter.into_iter().collect::<SmartString>().into())
    }
}

impl FromIterator<String> for ImmutableString {
    #[inline]
    #[must_use]
    fn from_iter<T: IntoIterator<Item = String>>(iter: T) -> Self {
        Self(iter.into_iter().collect::<SmartString>().into())
    }
}

impl FromIterator<SmartString> for ImmutableString {
    #[inline]
    #[must_use]
    fn from_iter<T: IntoIterator<Item = SmartString>>(iter: T) -> Self {
        Self(iter.into_iter().collect::<SmartString>().into())
    }
}

impl fmt::Display for ImmutableString {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl fmt::Debug for ImmutableString {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl Add for ImmutableString {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: Self) -> Self::Output {
        if rhs.is_empty() {
            self
        } else if self.is_empty() {
            rhs
        } else {
            self.make_mut().push_str(rhs.as_str());
            self
        }
    }
}

impl Add for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        if rhs.is_empty() {
            self.clone()
        } else if self.is_empty() {
            rhs.clone()
        } else {
            let mut s = self.clone();
            s.make_mut().push_str(rhs.as_str());
            s
        }
    }
}

impl Add<&Self> for ImmutableString {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: &Self) -> Self::Output {
        if rhs.is_empty() {
            self
        } else if self.is_empty() {
            rhs.clone()
        } else {
            self.make_mut().push_str(rhs.as_str());
            self
        }
    }
}

impl Add<ImmutableString> for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn add(self, rhs: ImmutableString) -> Self::Output {
        if rhs.is_empty() {
            self.clone()
        } else if self.is_empty() {
            rhs
        } else {
            let mut s = self.clone();
            s.make_mut().push_str(rhs.as_str());
            s
        }
    }
}

impl AddAssign<Self> for ImmutableString {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        if !rhs.is_empty() {
            if self.is_empty() {
                self.0 = rhs.0;
            } else {
                self.make_mut().push_str(&rhs);
            }
        }
    }
}

impl AddAssign<&Self> for ImmutableString {
    #[inline]
    fn add_assign(&mut self, rhs: &Self) {
        if !rhs.is_empty() {
            if self.is_empty() {
                self.0 = rhs.0.clone();
            } else {
                self.make_mut().push_str(rhs.as_str());
            }
        }
    }
}

impl Add<&str> for ImmutableString {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: &str) -> Self::Output {
        if !rhs.is_empty() {
            self.make_mut().push_str(rhs);
        }
        self
    }
}

impl Add<&str> for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn add(self, rhs: &str) -> Self::Output {
        if rhs.is_empty() {
            self.clone()
        } else {
            let mut s = self.clone();
            s.make_mut().push_str(rhs);
            s
        }
    }
}

impl AddAssign<&str> for ImmutableString {
    #[inline]
    fn add_assign(&mut self, rhs: &str) {
        if !rhs.is_empty() {
            self.make_mut().push_str(rhs);
        }
    }
}

impl Add<String> for ImmutableString {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: String) -> Self::Output {
        if rhs.is_empty() {
            self
        } else if self.is_empty() {
            rhs.into()
        } else {
            self.make_mut().push_str(&rhs);
            self
        }
    }
}

impl Add<String> for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn add(self, rhs: String) -> Self::Output {
        if rhs.is_empty() {
            self.clone()
        } else if self.is_empty() {
            rhs.into()
        } else {
            let mut s = self.clone();
            s.make_mut().push_str(&rhs);
            s
        }
    }
}

impl AddAssign<String> for ImmutableString {
    #[inline]
    fn add_assign(&mut self, rhs: String) {
        if !rhs.is_empty() {
            if self.is_empty() {
                let rhs: SmartString = rhs.into();
                self.0 = rhs.into();
            } else {
                self.make_mut().push_str(&rhs);
            }
        }
    }
}

impl Add<char> for ImmutableString {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: char) -> Self::Output {
        self.make_mut().push(rhs);
        self
    }
}

impl Add<char> for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn add(self, rhs: char) -> Self::Output {
        let mut s = self.clone();
        s.make_mut().push(rhs);
        s
    }
}

impl AddAssign<char> for ImmutableString {
    #[inline]
    fn add_assign(&mut self, rhs: char) {
        self.make_mut().push(rhs);
    }
}

impl Sub for ImmutableString {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        if rhs.is_empty() {
            self
        } else if self.is_empty() {
            rhs
        } else {
            self.replace(rhs.as_str(), "").into()
        }
    }
}

impl Sub for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        if rhs.is_empty() {
            self.clone()
        } else if self.is_empty() {
            rhs.clone()
        } else {
            self.replace(rhs.as_str(), "").into()
        }
    }
}

impl SubAssign<Self> for ImmutableString {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        if !rhs.is_empty() {
            if self.is_empty() {
                self.0 = rhs.0;
            } else {
                let rhs: SmartString = self.replace(rhs.as_str(), "").into();
                self.0 = rhs.into();
            }
        }
    }
}

impl SubAssign<&Self> for ImmutableString {
    #[inline]
    fn sub_assign(&mut self, rhs: &Self) {
        if !rhs.is_empty() {
            if self.is_empty() {
                self.0 = rhs.0.clone();
            } else {
                let rhs: SmartString = self.replace(rhs.as_str(), "").into();
                self.0 = rhs.into();
            }
        }
    }
}

impl Sub<String> for ImmutableString {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: String) -> Self::Output {
        if rhs.is_empty() {
            self
        } else if self.is_empty() {
            rhs.into()
        } else {
            self.replace(&rhs, "").into()
        }
    }
}

impl Sub<String> for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn sub(self, rhs: String) -> Self::Output {
        if rhs.is_empty() {
            self.clone()
        } else if self.is_empty() {
            rhs.into()
        } else {
            self.replace(&rhs, "").into()
        }
    }
}

impl SubAssign<String> for ImmutableString {
    #[inline]
    fn sub_assign(&mut self, rhs: String) {
        if !rhs.is_empty() {
            let rhs: SmartString = self.replace(&rhs, "").into();
            self.0 = rhs.into();
        }
    }
}

impl Sub<&str> for ImmutableString {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: &str) -> Self::Output {
        if rhs.is_empty() {
            self
        } else if self.is_empty() {
            rhs.into()
        } else {
            self.replace(rhs, "").into()
        }
    }
}

impl Sub<&str> for &ImmutableString {
    type Output = ImmutableString;

    #[inline]
    fn sub(self, rhs: &str) -> Self::Output {
        if rhs.is_empty() {
            self.clone()
        } else if self.is_empty() {
            rhs.into()
        } else {
            self.replace(rhs, "").into()
        }
    }
}

impl SubAssign<&str> for ImmutableString {
    #[inline]
    fn sub_assign(&mut self, rhs: &str) {
        if !rhs.is_empty() {
            let rhs: SmartString = self.replace(rhs, "").into();
            self.0 = rhs.into();
        }
    }
}

impl Sub<char> for ImmutableString {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: char) -> Self::Output {
        self.replace(rhs, "").into()
    }
}

impl Sub<char> for &ImmutableString {
    type Output = ImmutableString;

    #[inline(always)]
    fn sub(self, rhs: char) -> Self::Output {
        self.replace(rhs, "").into()
    }
}

impl SubAssign<char> for ImmutableString {
    #[inline]
    fn sub_assign(&mut self, rhs: char) {
        let rhs: SmartString = self.replace(rhs, "").into();
        self.0 = rhs.into();
    }
}

impl<S: AsRef<str> + ?Sized> PartialEq<S> for ImmutableString {
    #[inline(always)]
    fn eq(&self, other: &S) -> bool {
        self.as_str().eq(other.as_ref())
    }
}

impl PartialEq<str> for &ImmutableString {
    #[inline(always)]
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq(other)
    }
}

impl PartialEq<String> for &ImmutableString {
    #[inline(always)]
    fn eq(&self, other: &String) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl PartialEq<ImmutableString> for str {
    #[inline(always)]
    fn eq(&self, other: &ImmutableString) -> bool {
        self.eq(other.as_str())
    }
}

impl PartialEq<ImmutableString> for &str {
    #[inline(always)]
    fn eq(&self, other: &ImmutableString) -> bool {
        (*self).eq(other.as_str())
    }
}

impl PartialEq<ImmutableString> for String {
    #[inline(always)]
    fn eq(&self, other: &ImmutableString) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl PartialEq<&ImmutableString> for String {
    #[inline(always)]
    fn eq(&self, other: &&ImmutableString) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl PartialEq<ImmutableString> for &String {
    #[inline(always)]
    fn eq(&self, other: &ImmutableString) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl<S: AsRef<str> + ?Sized> PartialOrd<S> for ImmutableString {
    fn partial_cmp(&self, other: &S) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_ref())
    }
}

impl PartialOrd<str> for &ImmutableString {
    fn partial_cmp(&self, other: &str) -> Option<Ordering> {
        self.as_str().partial_cmp(other)
    }
}

impl PartialOrd<String> for &ImmutableString {
    fn partial_cmp(&self, other: &String) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl PartialOrd<ImmutableString> for str {
    #[inline(always)]
    fn partial_cmp(&self, other: &ImmutableString) -> Option<Ordering> {
        self.partial_cmp(other.as_str())
    }
}

impl PartialOrd<ImmutableString> for &str {
    #[inline(always)]
    fn partial_cmp(&self, other: &ImmutableString) -> Option<Ordering> {
        (*self).partial_cmp(other.as_str())
    }
}

impl PartialOrd<ImmutableString> for String {
    #[inline(always)]
    fn partial_cmp(&self, other: &ImmutableString) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl PartialOrd<&ImmutableString> for String {
    #[inline(always)]
    fn partial_cmp(&self, other: &&ImmutableString) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl PartialOrd<ImmutableString> for &String {
    #[inline(always)]
    fn partial_cmp(&self, other: &ImmutableString) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl ImmutableString {
    /// Create a new [`ImmutableString`].
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self(SmartString::new_const().into())
    }
    /// Get the string slice.
    #[inline(always)]
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    /// Strong count of references to the underlying string.
    #[inline(always)]
    #[must_use]
    pub fn strong_count(&self) -> usize {
        Shared::strong_count(&self.0)
    }
    /// Consume the [`ImmutableString`] and convert it into a [`String`].
    ///
    /// If there are other references to the same string, a cloned copy is returned.
    #[inline]
    #[must_use]
    pub fn into_owned(mut self) -> String {
        let _ = self.make_mut(); // Make sure it is unique reference
        shared_take(self.0).into() // Should succeed
    }
    /// Make sure that the [`ImmutableString`] is unique (i.e. no other outstanding references).
    /// Then return a mutable reference to the [`SmartString`].
    ///
    /// If there are other references to the same string, a cloned copy is used.
    #[inline(always)]
    #[must_use]
    pub fn make_mut(&mut self) -> &mut SmartString {
        shared_make_mut(&mut self.0)
    }
    /// Return a mutable reference to the [` SmartString`] wrapped by the [`ImmutableString`]
    /// if there are no other outstanding references to it.
    #[inline(always)]
    pub fn get_mut(&mut self) -> Option<&mut SmartString> {
        shared_get_mut(&mut self.0)
    }
    /// Returns `true` if the two [`ImmutableString`]'s point to the same allocation.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::ImmutableString;
    ///
    /// let s1: ImmutableString = "hello".into();
    /// let s2 = s1.clone();
    /// let s3: ImmutableString = "hello".into();
    ///
    /// assert_eq!(s1, s2);
    /// assert_eq!(s1, s3);
    /// assert_eq!(s2, s3);
    ///
    /// assert!(s1.ptr_eq(&s2));
    /// assert!(!s1.ptr_eq(&s3));
    /// assert!(!s2.ptr_eq(&s3));
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Shared::ptr_eq(&self.0, &other.0)
    }
}
