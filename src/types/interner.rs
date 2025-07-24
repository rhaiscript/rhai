//! A strings interner type.

use super::BloomFilterU64;
use crate::func::{hashing::get_hasher, StraightHashMap};
use crate::ImmutableString;
use hashbrown::hash_map::Entry;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::AddAssign,
};

/// Maximum length of strings interned.
pub const MAX_STRING_LEN: usize = 32;

/// _(internals)_ A cache for interned strings.
/// Exported under the `internals` feature only.
#[derive(Clone)]
pub struct StringsInterner {
    /// Maximum number of strings to be interned.
    max_strings_interned: usize,
    /// Cached strings.
    cache: StraightHashMap<ImmutableString>,
    /// Bloom filter to avoid caching "one-hit wonders".
    bloom_filter: BloomFilterU64,
}

impl fmt::Debug for StringsInterner {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.cache.values()).finish()
    }
}

impl StringsInterner {
    /// Create a new [`StringsInterner`].
    #[inline(always)]
    #[must_use]
    pub fn new(max_strings_interned: usize) -> Self {
        Self {
            max_strings_interned,
            cache: <_>::default(),
            bloom_filter: BloomFilterU64::new(),
        }
    }

    /// Get an identifier from a text string, adding it to the interner if necessary.
    #[inline(always)]
    #[must_use]
    pub fn get(&mut self, text: impl AsRef<str> + Into<ImmutableString>) -> ImmutableString {
        self.get_with_mapper(0, Into::into, text)
    }

    /// Set the maximum number of strings to be interned.
    #[inline(always)]
    pub fn set_max(&mut self, max: usize) {
        self.max_strings_interned = max;
        self.throttle_cache(None);
    }
    /// The maximum number of strings to be interned.
    #[inline(always)]
    #[must_use]
    pub const fn max(&self) -> usize {
        self.max_strings_interned
    }

    /// Get an identifier from a text string, adding it to the interner if necessary.
    #[inline]
    #[must_use]
    pub fn get_with_mapper<S: AsRef<str>>(
        &mut self,
        category: u8,
        mapper: impl FnOnce(S) -> ImmutableString,
        text: S,
    ) -> ImmutableString {
        let key = text.as_ref();

        if self.max() == 0 {
            return mapper(text);
        }

        let hasher = &mut get_hasher();
        hasher.write_u8(category);
        key.hash(hasher);
        let hash = hasher.finish();

        // Do not cache long strings and avoid caching "one-hit wonders".
        if key.len() > MAX_STRING_LEN || self.bloom_filter.is_absent_and_set(hash) {
            return mapper(text);
        }

        if self.cache.is_empty() {
            // Reserve a good size to kick start the strings interner
            self.cache.reserve(128);
        }

        let result = match self.cache.entry(hash) {
            Entry::Occupied(e) => return e.get().clone(),
            Entry::Vacant(e) => e.insert(mapper(text)).clone(),
        };

        // Throttle the cache upon exit
        self.throttle_cache(Some(hash));

        result
    }

    /// If the interner is over capacity, remove the longest entry that has the lowest count
    #[inline]
    fn throttle_cache(&mut self, skip_hash: Option<u64>) {
        if self.max() == 0 {
            self.clear();
            return;
        }
        if self.cache.len() <= self.max() {
            return;
        }

        // Leave some buffer to grow when shrinking the cache.
        // We leave at least two entries, one for the empty string, and one for the string
        // that has just been inserted.
        while self.cache.len() > self.max() - 3 {
            let mut max_len = 0;
            let mut min_count = usize::MAX;
            let mut index = 0;

            for (&k, v) in &self.cache {
                if skip_hash != Some(k)
                    && (v.strong_count() < min_count
                        || (v.strong_count() == min_count && v.len() > max_len))
                {
                    max_len = v.len();
                    min_count = v.strong_count();
                    index = k;
                }
            }

            self.cache.remove(&index);
        }
    }

    /// Number of strings interned.
    #[inline(always)]
    #[must_use]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Returns `true` if there are no interned strings.
    #[inline(always)]
    #[must_use]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clear all interned strings.
    #[inline(always)]
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.clear();
        self.bloom_filter.clear();
    }
}

impl AddAssign<Self> for StringsInterner {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        self.cache.extend(rhs.cache);
        self.bloom_filter += rhs.bloom_filter;
    }
}

impl AddAssign<&Self> for StringsInterner {
    #[inline(always)]
    fn add_assign(&mut self, rhs: &Self) {
        self.cache
            .extend(rhs.cache.iter().map(|(&k, v)| (k, v.clone())));
        self.bloom_filter += &rhs.bloom_filter;
    }
}
