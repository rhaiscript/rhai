//! Collection of custom types.

use crate::Identifier;
use std::{any::type_name, collections::BTreeMap, fmt};

/// _(internals)_ Information for a custom type.
/// Exported under the `internals` feature only.
#[derive(Debug, Eq, PartialEq, Clone, Hash, Default)]
pub struct CustomTypeInfo {
    /// Friendly display name of the custom type.
    pub display_name: Identifier,
}

/// _(internals)_ A collection of custom types.
/// Exported under the `internals` feature only.
#[derive(Clone, Hash, Default)]
pub struct CustomTypesCollection(BTreeMap<Identifier, CustomTypeInfo>);

impl fmt::Debug for CustomTypesCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CustomTypesCollection ")?;
        f.debug_map().entries(self.0.iter()).finish()
    }
}

impl CustomTypesCollection {
    /// Create a new [`CustomTypesCollection`].
    #[inline(always)]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
    /// Clear the [`CustomTypesCollection`].
    #[inline(always)]
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Register a custom type.
    #[inline(always)]
    pub fn add(&mut self, type_name: impl Into<Identifier>, name: impl Into<Identifier>) {
        self.add_raw(
            type_name,
            CustomTypeInfo {
                display_name: name.into(),
            },
        );
    }
    /// Register a custom type.
    #[inline(always)]
    pub fn add_type<T>(&mut self, name: &str) {
        self.add_raw(
            type_name::<T>(),
            CustomTypeInfo {
                display_name: name.into(),
            },
        );
    }
    /// Register a custom type.
    #[inline(always)]
    pub fn add_raw(&mut self, type_name: impl Into<Identifier>, custom_type: CustomTypeInfo) {
        self.0.insert(type_name.into(), custom_type);
    }
    /// Find a custom type.
    #[inline(always)]
    pub fn get(&self, key: &str) -> Option<&CustomTypeInfo> {
        self.0.get(key)
    }
}
