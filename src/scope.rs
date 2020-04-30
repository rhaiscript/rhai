//! Module that defines the `Scope` type representing a function call-stack scope.

use crate::any::{Dynamic, Variant};
use crate::parser::{map_dynamic_to_expr, Expr};
use crate::token::Position;

use crate::stdlib::{borrow::Cow, boxed::Box, iter, vec::Vec};

/// Type of an entry in the Scope.
#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub enum EntryType {
    /// Normal value.
    Normal,
    /// Immutable constant value.
    Constant,
}

/// An entry in the Scope.
#[derive(Debug)]
pub struct Entry<'a> {
    /// Name of the entry.
    pub name: Cow<'a, str>,
    /// Type of the entry.
    pub typ: EntryType,
    /// Current value of the entry.
    pub value: Dynamic,
    /// A constant expression if the initial value matches one of the recognized types.
    pub expr: Option<Box<Expr>>,
}

/// A type containing information about the current scope.
/// Useful for keeping state between `Engine` evaluation runs.
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
/// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 41);
/// assert_eq!(my_scope.get_value::<i64>("z").unwrap(), 0);
/// # Ok(())
/// # }
/// ```
///
/// When searching for entries, newly-added entries are found before similarly-named but older entries,
/// allowing for automatic _shadowing_.
///
/// Currently, `Scope` is neither `Send` nor `Sync`. Turn on the `sync` feature to make it `Send + Sync`.
#[derive(Debug)]
pub struct Scope<'a>(Vec<Entry<'a>>);

impl<'a> Scope<'a> {
    /// Create a new Scope.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 42);
    /// ```
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Empty the Scope.
    ///
    /// # Examples
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
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Get the number of entries inside the Scope.
    ///
    /// # Examples
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
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Is the Scope empty?
    ///
    /// # Examples
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
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    /// Add (push) a new entry to the Scope.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 42);
    /// ```
    pub fn push<K: Into<Cow<'a, str>>, T: Variant + Clone>(&mut self, name: K, value: T) {
        self.push_dynamic_value(name, EntryType::Normal, Dynamic::from(value), false);
    }

    /// Add (push) a new `Dynamic` entry to the Scope.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Dynamic,  Scope};
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push_dynamic("x", Dynamic::from(42_i64));
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 42);
    /// ```
    pub fn push_dynamic<K: Into<Cow<'a, str>>>(&mut self, name: K, value: Dynamic) {
        self.push_dynamic_value(name, EntryType::Normal, value, false);
    }

    /// Add (push) a new constant to the Scope.
    ///
    /// Constants are immutable and cannot be assigned to.  Their values never change.
    /// Constants propagation is a technique used to optimize an AST.
    ///
    /// However, in order to be used for optimization, constants must be in one of the recognized types:
    /// `INT` (default to `i64`, `i32` if `only_i32`), `f64`, `String`, `char` and `bool`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push_constant("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 42);
    /// ```
    pub fn push_constant<K: Into<Cow<'a, str>>, T: Variant + Clone>(&mut self, name: K, value: T) {
        self.push_dynamic_value(name, EntryType::Constant, Dynamic::from(value), true);
    }

    /// Add (push) a new constant with a `Dynamic` value to the Scope.
    ///
    /// Constants are immutable and cannot be assigned to.  Their values never change.
    /// Constants propagation is a technique used to optimize an AST.
    ///
    /// However, in order to be used for optimization, the `Dynamic` value must be in one of the
    /// recognized types:
    /// `INT` (default to `i64`, `i32` if `only_i32`), `f64`, `String`, `char` and `bool`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::{Dynamic, Scope};
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push_constant_dynamic("x", Dynamic::from(42_i64));
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 42);
    /// ```
    pub fn push_constant_dynamic<K: Into<Cow<'a, str>>>(&mut self, name: K, value: Dynamic) {
        self.push_dynamic_value(name, EntryType::Constant, value, true);
    }

    /// Add (push) a new entry with a `Dynamic` value to the Scope.
    pub(crate) fn push_dynamic_value<K: Into<Cow<'a, str>>>(
        &mut self,
        name: K,
        entry_type: EntryType,
        value: Dynamic,
        map_expr: bool,
    ) {
        let expr = if map_expr {
            map_dynamic_to_expr(value.clone(), Position::none()).map(Box::new)
        } else {
            None
        };

        self.0.push(Entry {
            name: name.into(),
            typ: entry_type,
            value: value.into(),
            expr,
        });
    }

    /// Truncate (rewind) the Scope to a previous size.
    ///
    /// # Examples
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
    pub fn rewind(&mut self, size: usize) {
        self.0.truncate(size);
    }

    /// Does the scope contain the entry?
    ///
    /// # Examples
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
    pub fn contains(&self, name: &str) -> bool {
        self.0
            .iter()
            .rev() // Always search a Scope in reverse order
            .any(|Entry { name: key, .. }| name == key)
    }

    /// Find an entry in the Scope, starting from the last.
    pub(crate) fn get(&self, name: &str) -> Option<(usize, EntryType)> {
        self.0
            .iter()
            .enumerate()
            .rev() // Always search a Scope in reverse order
            .find_map(|(index, Entry { name: key, typ, .. })| {
                if name == key {
                    Some((index, *typ))
                } else {
                    None
                }
            })
    }

    pub(crate) fn get_dynamic(&self, name: &str) -> Option<Dynamic> {
        self.0
            .iter()
            .rev()
            .find(|Entry { name: key, .. }| name == key)
            .map(|Entry{ value, .. }| value.clone())
    }

    /// Get the value of an entry in the Scope, starting from the last.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 42);
    /// ```
    pub fn get_value<T: Variant + Clone>(&self, name: &str) -> Option<T> {
        self.0
            .iter()
            .rev()
            .find(|Entry { name: key, .. }| name == key)
            .and_then(|Entry { value, .. }| value.downcast_ref::<T>().cloned())
    }

    /// Update the value of the named entry.
    /// Search starts backwards from the last, and only the first entry matching the specified name is updated.
    /// If no entry matching the specified name is found, a new one is added.
    ///
    /// # Panics
    ///
    /// Panics when trying to update the value of a constant.
    ///
    /// # Examples
    ///
    /// ```
    /// use rhai::Scope;
    ///
    /// let mut my_scope = Scope::new();
    ///
    /// my_scope.push("x", 42_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 42);
    ///
    /// my_scope.set_value("x", 0_i64);
    /// assert_eq!(my_scope.get_value::<i64>("x").unwrap(), 0);
    /// ```
    pub fn set_value<T: Variant + Clone>(&mut self, name: &'a str, value: T) {
        match self.get(name) {
            Some((_, EntryType::Constant)) => panic!("variable {} is constant", name),
            Some((index, EntryType::Normal)) => {
                self.0.get_mut(index).unwrap().value = Dynamic::from(value)
            }
            None => self.push(name, value),
        }
    }

    /// Get a mutable reference to an entry in the Scope.
    pub(crate) fn get_mut(&mut self, index: usize) -> (&mut Dynamic, EntryType) {
        let entry = self.0.get_mut(index).expect("invalid index in Scope");

        // assert_ne!(
        //     entry.typ,
        //     EntryType::Constant,
        //     "get mut of constant entry"
        // );

        (&mut entry.value, entry.typ)
    }

    /// Get an iterator to entries in the Scope.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.0.iter().rev() // Always search a Scope in reverse order
    }
}

impl Default for Scope<'_> {
    fn default() -> Self {
        Scope::new()
    }
}

impl<'a, K: Into<Cow<'a, str>>> iter::Extend<(K, EntryType, Dynamic)> for Scope<'a> {
    fn extend<T: IntoIterator<Item = (K, EntryType, Dynamic)>>(&mut self, iter: T) {
        self.0
            .extend(iter.into_iter().map(|(name, typ, value)| Entry {
                name: name.into(),
                typ,
                value: value.into(),
                expr: None,
            }));
    }
}
