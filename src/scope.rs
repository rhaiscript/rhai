//! Module that defines the [`Scope`] type representing a function call-stack scope.

use crate::dynamic::{AccessMode, Variant};
use crate::{Dynamic, Identifier, StaticVec};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{borrow::Cow, iter::Extend};

/// Keep a number of entries inline (since [`Dynamic`] is usually small enough).
const SCOPE_ENTRIES_INLINED: usize = 8;

/// Type containing information about the current scope.
/// Useful for keeping state between [`Engine`][crate::Engine] evaluation runs.
///
/// # Thread Safety
///
/// Currently, [`Scope`] is neither [`Send`] nor [`Sync`].
/// Turn on the `sync` feature to make it [`Send`] `+` [`Sync`].
///
/// # Example
///
/// ```
/// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
/// use rhai::{Engine, Scope};
///
/// let engine = Engine::new();
/// let mut my_scope = Scope::new();
///
/// my_scope.push("z", 40_i64);
///
/// engine.eval_with_scope::<()>(&mut my_scope, "let x = z + 1; z = 0;")?;
///
/// assert_eq!(engine.eval_with_scope::<i64>(&mut my_scope, "x + 1")?, 42);
///
/// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 41);
/// assert_eq!(my_scope.get_value::<i64>("z").expect("z should exist"), 0);
/// # Ok(())
/// # }
/// ```
///
/// When searching for entries, newly-added entries are found before similarly-named but older entries,
/// allowing for automatic _shadowing_.
//
// # Implementation Notes
//
// [`Scope`] is implemented as two [`Vec`]'s of exactly the same length.  Variables data (name, type, etc.)
// is manually split into two equal-length arrays.  That's because variable names take up the most space,
// with [`Cow<str>`][Cow] being four words long, but in the vast majority of cases the name is NOT used to
// look up a variable.  Variable lookup is usually via direct indexing, by-passing the name altogether.
//
// Since [`Dynamic`] is reasonably small, packing it tightly improves cache locality when variables are accessed.
#[derive(Debug, Clone, Hash, Default)]
pub struct Scope<'a> {
    /// Current value of the entry.
    values: smallvec::SmallVec<[Dynamic; SCOPE_ENTRIES_INLINED]>,
    /// (Name, aliases) of the entry.
    names: smallvec::SmallVec<
        [(Cow<'a, str>, Option<Box<StaticVec<Identifier>>>); SCOPE_ENTRIES_INLINED],
    >,
}

impl<'a> IntoIterator for Scope<'a> {
    type Item = (Cow<'a, str>, Dynamic);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        Box::new(
            self.values
                .into_iter()
                .zip(self.names.into_iter())
                .map(|(value, (name, _))| (name, value)),
        )
    }
}

impl<'a> Scope<'a> {
    /// Create a new [`Scope`].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }
    /// Empty the [`Scope`].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert!(my_scope.contains("x"));
    /// assert_eq!(my_scope.len(), 1);
    /// assert!(!my_scope.is_empty());
    ///
    /// my_scope.clear();
    /// assert!(!my_scope.contains("x"));
    /// assert_eq!(my_scope.len(), 0);
    /// assert!(my_scope.is_empty());
    /// ```
    #[inline(always)]
    pub fn clear(&mut self) -> &mut Self {
        self.names.clear();
        self.values.clear();
        self
    }
    /// Get the number of entries inside the [`Scope`].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    /// assert_eq!(my_scope.len(), 0);
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.len(), 1);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }
    /// Is the [`Scope`] empty?
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    /// assert!(my_scope.is_empty());
    ///
    /// my_scope.push("x", 42_i64);
    /// assert!(!my_scope.is_empty());
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// Add (push) a new entry to the [`Scope`].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    /// ```
    #[inline(always)]
    pub fn push(
        &mut self,
        name: impl Into<Cow<'a, str>>,
        value: impl Variant + Clone,
    ) -> &mut Self {
        self.push_dynamic_value(name, AccessMode::ReadWrite, Dynamic::from(value))
    }
    /// Add (push) a new [`Dynamic`] entry to the [`Scope`].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Dynamic,  Scope};
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push_dynamic("x", Dynamic::from(42_i64));
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    /// ```
    #[inline(always)]
    pub fn push_dynamic(&mut self, name: impl Into<Cow<'a, str>>, value: Dynamic) -> &mut Self {
        self.push_dynamic_value(name, value.access_mode(), value)
    }
    /// Add (push) a new constant to the [`Scope`].
    ///
    /// Constants are immutable and cannot be assigned to.  Their values never change.
    /// Constants propagation is a technique used to optimize an [`AST`][crate::AST].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push_constant("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    /// ```
    #[inline(always)]
    pub fn push_constant(
        &mut self,
        name: impl Into<Cow<'a, str>>,
        value: impl Variant + Clone,
    ) -> &mut Self {
        self.push_dynamic_value(name, AccessMode::ReadOnly, Dynamic::from(value))
    }
    /// Add (push) a new constant with a [`Dynamic`] value to the Scope.
    ///
    /// Constants are immutable and cannot be assigned to.  Their values never change.
    /// Constants propagation is a technique used to optimize an [`AST`][crate::AST].
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Dynamic, Scope};
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push_constant_dynamic("x", Dynamic::from(42_i64));
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    /// ```
    #[inline(always)]
    pub fn push_constant_dynamic(
        &mut self,
        name: impl Into<Cow<'a, str>>,
        value: Dynamic,
    ) -> &mut Self {
        self.push_dynamic_value(name, AccessMode::ReadOnly, value)
    }
    /// Add (push) a new entry with a [`Dynamic`] value to the [`Scope`].
    #[inline]
    pub(crate) fn push_dynamic_value(
        &mut self,
        name: impl Into<Cow<'a, str>>,
        access: AccessMode,
        mut value: Dynamic,
    ) -> &mut Self {
        self.names.push((name.into(), Default::default()));
        value.set_access_mode(access);
        self.values.push(value);
        self
    }
    /// Truncate (rewind) the [`Scope`] to a previous size.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// my_scope.push("y", 123_i64);
    /// assert!(my_scope.contains("x"));
    /// assert!(my_scope.contains("y"));
    /// assert_eq!(my_scope.len(), 2);
    ///
    /// my_scope.rewind(1);
    /// assert!(my_scope.contains("x"));
    /// assert!(!my_scope.contains("y"));
    /// assert_eq!(my_scope.len(), 1);
    ///
    /// my_scope.rewind(0);
    /// assert!(!my_scope.contains("x"));
    /// assert!(!my_scope.contains("y"));
    /// assert_eq!(my_scope.len(), 0);
    /// assert!(my_scope.is_empty());
    /// ```
    #[inline(always)]
    pub fn rewind(&mut self, size: usize) -> &mut Self {
        self.names.truncate(size);
        self.values.truncate(size);
        self
    }
    /// Does the [`Scope`] contain the entry?
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert!(my_scope.contains("x"));
    /// assert!(!my_scope.contains("y"));
    /// ```
    #[inline]
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.names
            .iter()
            .rev() // Always search a Scope in reverse order
            .any(|(key, _)| name == key.as_ref())
    }
    /// Find an entry in the [`Scope`], starting from the last.
    #[inline]
    #[must_use]
    pub(crate) fn get_index(&self, name: &str) -> Option<(usize, AccessMode)> {
        self.names
            .iter()
            .rev() // Always search a Scope in reverse order
            .enumerate()
            .find_map(|(index, (key, _))| {
                if name == key.as_ref() {
                    let index = self.len() - 1 - index;
                    Some((index, self.values[index].access_mode()))
                } else {
                    None
                }
            })
    }
    /// Get the value of an entry in the [`Scope`], starting from the last.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    /// ```
    #[inline]
    #[must_use]
    pub fn get_value<T: Variant + Clone>(&self, name: &str) -> Option<T> {
        self.names
            .iter()
            .rev()
            .enumerate()
            .find(|(_, (key, _))| name == key.as_ref())
            .and_then(|(index, _)| {
                self.values[self.len() - 1 - index]
                    .flatten_clone()
                    .try_cast()
            })
    }
    /// Check if the named entry in the [`Scope`] is constant.
    ///
    /// Search starts backwards from the last, stopping at the first entry matching the specified name.
    ///
    /// Returns [`None`] if no entry matching the specified name is found.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push_constant("x", 42_i64);
    /// assert_eq!(my_scope.is_constant("x"), Some(true));
    /// assert_eq!(my_scope.is_constant("y"), None);
    /// ```
    #[inline]
    pub fn is_constant(&self, name: &str) -> Option<bool> {
        self.get_index(name).and_then(|(_, access)| match access {
            AccessMode::ReadWrite => None,
            AccessMode::ReadOnly => Some(true),
        })
    }
    /// Update the value of the named entry in the [`Scope`] if it already exists and is not constant.
    /// Push a new entry with the value into the [`Scope`] if the name doesn't exist or if the
    /// existing entry is constant.
    ///
    /// Search starts backwards from the last, and only the first entry matching the specified name is updated.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.set_or_push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    /// assert_eq!(my_scope.len(), 1);
    ///
    /// my_scope.set_or_push("x", 0_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 0);
    /// assert_eq!(my_scope.len(), 1);
    ///
    /// my_scope.set_or_push("y", 123_i64);
    /// assert_eq!(my_scope.get_value::<i64>("y").expect("y should exist"), 123);
    /// assert_eq!(my_scope.len(), 2);
    /// ```
    #[inline]
    pub fn set_or_push(
        &mut self,
        name: impl AsRef<str> + Into<Cow<'a, str>>,
        value: impl Variant + Clone,
    ) -> &mut Self {
        match self.get_index(name.as_ref()) {
            None | Some((_, AccessMode::ReadOnly)) => {
                self.push(name, value);
            }
            Some((index, AccessMode::ReadWrite)) => {
                let value_ref = self.values.get_mut(index).expect("index is valid");
                *value_ref = Dynamic::from(value);
            }
        }
        self
    }
    /// Update the value of the named entry in the [`Scope`].
    ///
    /// Search starts backwards from the last, and only the first entry matching the specified name is updated.
    /// If no entry matching the specified name is found, a new one is added.
    ///
    /// # Panics
    ///
    /// Panics when trying to update the value of a constant.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    ///
    /// my_scope.set_value("x", 0_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 0);
    /// ```
    #[inline]
    pub fn set_value(
        &mut self,
        name: impl AsRef<str> + Into<Cow<'a, str>>,
        value: impl Variant + Clone,
    ) -> &mut Self {
        match self.get_index(name.as_ref()) {
            None => {
                self.push(name, value);
            }
            Some((_, AccessMode::ReadOnly)) => panic!("variable {} is constant", name.as_ref()),
            Some((index, AccessMode::ReadWrite)) => {
                let value_ref = self.values.get_mut(index).expect("index is valid");
                *value_ref = Dynamic::from(value);
            }
        }
        self
    }
    /// Get a mutable reference to an entry in the [`Scope`].
    ///
    /// If the entry by the specified name is not found, of if it is read-only,
    /// [`None`] is returned.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 42);
    ///
    /// let ptr = my_scope.get_mut("x").expect("x should exist");
    /// *ptr = 123_i64.into();
    ///
    /// assert_eq!(my_scope.get_value::<i64>("x").expect("x should exist"), 123);
    ///
    /// my_scope.push_constant("Z", 1_i64);
    /// assert!(my_scope.get_mut("Z").is_none());
    /// ```
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Dynamic> {
        self.get_index(name)
            .and_then(move |(index, access)| match access {
                AccessMode::ReadWrite => Some(self.get_mut_by_index(index)),
                AccessMode::ReadOnly => None,
            })
    }
    /// Get a mutable reference to an entry in the [`Scope`] based on the index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    #[inline(always)]
    #[must_use]
    pub(crate) fn get_mut_by_index(&mut self, index: usize) -> &mut Dynamic {
        self.values.get_mut(index).expect("index is out of bounds")
    }
    /// Update the access type of an entry in the [`Scope`].
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    #[cfg(not(feature = "no_module"))]
    #[inline]
    pub(crate) fn add_entry_alias(&mut self, index: usize, alias: Identifier) -> &mut Self {
        let (_, aliases) = self.names.get_mut(index).expect("index is out of bounds");
        match aliases {
            None => {
                let mut list = StaticVec::new();
                list.push(alias);
                *aliases = Some(list.into());
            }
            Some(aliases) if !aliases.iter().any(|a| a == &alias) => aliases.push(alias),
            Some(_) => (),
        }
        self
    }
    /// Clone the [`Scope`], keeping only the last instances of each variable name.
    /// Shadowed variables are omitted in the copy.
    #[inline]
    #[must_use]
    pub(crate) fn clone_visible(&self) -> Self {
        let mut entries = Self::new();

        self.names
            .iter()
            .rev()
            .enumerate()
            .for_each(|(index, (name, alias))| {
                if !entries.names.iter().any(|(key, _)| key == name) {
                    entries.names.push((name.clone(), alias.clone()));
                    entries
                        .values
                        .push(self.values[self.len() - 1 - index].clone());
                }
            });

        entries
    }
    /// Get an iterator to entries in the [`Scope`].
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn into_iter(
        self,
    ) -> impl Iterator<Item = (Cow<'a, str>, Dynamic, Vec<Identifier>)> {
        self.names
            .into_iter()
            .zip(self.values.into_iter())
            .map(|((name, alias), value)| {
                (name, value, alias.map(|a| a.to_vec()).unwrap_or_default())
            })
    }
    /// Get an iterator to entries in the [`Scope`].
    /// Shared values are flatten-cloned.
    ///
    /// # Example
    ///
    /// ```
    /// use rhai::{Dynamic, Scope};
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// my_scope.push_constant("foo", "hello");
    ///
    /// let mut iter = my_scope.iter();
    ///
    /// let (name, is_constant, value) = iter.next().expect("value should exist");
    /// assert_eq!(name, "x");
    /// assert!(!is_constant);
    /// assert_eq!(value.cast::<i64>(), 42);
    ///
    /// let (name, is_constant, value) = iter.next().expect("value should exist");
    /// assert_eq!(name, "foo");
    /// assert!(is_constant);
    /// assert_eq!(value.cast::<String>(), "hello");
    /// ```
    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = (&str, bool, Dynamic)> {
        self.iter_raw()
            .map(|(name, constant, value)| (name, constant, value.flatten_clone()))
    }
    /// Get an iterator to entries in the [`Scope`].
    /// Shared values are not expanded.
    #[inline]
    pub fn iter_raw(&self) -> impl Iterator<Item = (&str, bool, &Dynamic)> {
        self.names
            .iter()
            .zip(self.values.iter())
            .map(|((name, _), value)| (name.as_ref(), value.is_read_only(), value))
    }
}

impl<'a, K: Into<Cow<'a, str>>> Extend<(K, Dynamic)> for Scope<'a> {
    #[inline(always)]
    fn extend<T: IntoIterator<Item = (K, Dynamic)>>(&mut self, iter: T) {
        iter.into_iter().for_each(|(name, value)| {
            self.names.push((name.into(), Default::default()));
            self.values.push(value);
        });
    }
}
