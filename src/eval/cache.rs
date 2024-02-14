//! System caches.

use crate::func::{RhaiFunc, StraightHashMap};
use crate::types::BloomFilterU64;
use crate::{ImmutableString, StaticVec};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// _(internals)_ An entry in a function resolution cache.
/// Exported under the `internals` feature only.
#[derive(Debug, Clone)]
pub struct FnResolutionCacheEntry {
    /// Function.
    pub func: RhaiFunc,
    /// Optional source.
    pub source: Option<ImmutableString>,
}

/// _(internals)_ A function resolution cache with a bloom filter.
/// Exported under the `internals` feature only.
///
/// The [bloom filter][`BloomFilterU64`] is used to rapidly check whether a function hash has never been encountered.
/// It enables caching a hash only during the second encounter to avoid "one-hit wonders".
#[derive(Debug, Clone, Default)]
pub struct FnResolutionCache {
    /// Hash map containing cached functions.
    pub dict: StraightHashMap<Option<FnResolutionCacheEntry>>,
    /// Bloom filter to avoid caching "one-hit wonders".
    pub filter: BloomFilterU64,
}

impl FnResolutionCache {
    /// Clear the [`FnResolutionCache`].
    #[inline(always)]
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.dict.clear();
        self.filter.clear();
    }
}

/// _(internals)_ A type containing system-wide caches.
/// Exported under the `internals` feature only.
///
/// The following caches are contained inside this type:
/// * A stack of [function resolution caches][FnResolutionCache]
#[derive(Debug, Clone)]
pub struct Caches {
    fn_resolution: StaticVec<FnResolutionCache>,
}

impl Caches {
    /// Create an empty [`Caches`].
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fn_resolution: StaticVec::new_const(),
        }
    }
    /// Get the number of function resolution cache(s) in the stack.
    #[inline(always)]
    #[must_use]
    pub fn fn_resolution_caches_len(&self) -> usize {
        self.fn_resolution.len()
    }
    /// Get a mutable reference to the current function resolution cache.
    ///
    /// A new function resolution cache is pushed onto the stack if the stack is empty.
    #[inline]
    #[must_use]
    pub fn fn_resolution_cache_mut(&mut self) -> &mut FnResolutionCache {
        // Push a new function resolution cache if the stack is empty
        if self.fn_resolution.is_empty() {
            self.push_fn_resolution_cache();
        }
        self.fn_resolution.last_mut().unwrap()
    }
    /// Push an empty function resolution cache onto the stack and make it current.
    #[inline(always)]
    pub fn push_fn_resolution_cache(&mut self) {
        self.fn_resolution.push(<_>::default());
    }
    /// Rewind the function resolution caches stack to a particular size.
    #[inline(always)]
    pub fn rewind_fn_resolution_caches(&mut self, len: usize) {
        self.fn_resolution.truncate(len);
    }
}
