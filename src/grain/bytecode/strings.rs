use alloc::borrow::Cow;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Every name a program mentions, as one blob and a list of spans.
///
/// Names are the largest thing a load used to allocate: an `ImmutableString`
/// is a reference-counted box, so a hundred of them is a hundred allocations
/// and several kilobytes. Almost none of them need to be one — a call name, an
/// operator, a getter and a property key are all handed to rhai as `&str`.
///
/// So the table is a byte blob borrowed straight out of the artifact, and a
/// name is a slice of it. Two allocations for the whole table, and neither
/// scales with how many names there are: the spans, and nothing else.
///
/// A name that becomes a scope entry is copied rather than borrowed, because
/// `Scope` stores its own. That is a copy into a `SmartString` and not an
/// allocation, so it costs nothing for the short names a `let` or a parameter
/// actually has.
#[derive(Debug, Clone, Default)]
pub struct Strings<'a> {
    blob: Cow<'a, [u8]>,
    /// Start of each name; the end is the next start, so this is one longer
    /// than the number of names.
    starts: Vec<u32>,
}

impl<'a> Strings<'a> {
    /// Build from names in index order.
    #[must_use]
    pub fn new<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> Strings<'static> {
        let mut blob = Vec::new();
        let mut starts = vec![0u32];

        for name in names {
            blob.extend_from_slice(name.as_ref().as_bytes());
            starts.push(blob.len() as u32);
        }

        Strings {
            blob: Cow::Owned(blob),
            starts,
        }
    }

    /// Wrap a blob and its spans, as read from an artifact.
    ///
    /// # Errors
    ///
    /// [`BadTable`] if the spans do not ascend within the blob, or if any name
    /// is not UTF-8 — both of which would otherwise turn into a slice panic
    /// while the VM was already running.
    pub fn borrowed(blob: &'a [u8], starts: Vec<u32>) -> Result<Self, BadTable> {
        if starts.is_empty() {
            return Err(BadTable::NoTerminator);
        }
        if starts[0] != 0 {
            return Err(BadTable::NoTerminator);
        }

        for pair in starts.windows(2) {
            let (from, to) = (pair[0] as usize, pair[1] as usize);
            if to < from || to > blob.len() {
                return Err(BadTable::SpanOutOfRange { from, to });
            }
            core::str::from_utf8(&blob[from..to]).map_err(|_| BadTable::NotUtf8 { at: from })?;
        }
        if *starts.last().expect("checked") as usize != blob.len() {
            return Err(BadTable::TrailingBytes);
        }

        Ok(Self {
            blob: Cow::Borrowed(blob),
            starts,
        })
    }

    /// How many names the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// Whether the table holds no names.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The name at `index`, or `None`.
    ///
    /// Never allocates: the result points into the artifact.
    #[must_use]
    pub fn get(&self, index: u32) -> Option<&str> {
        let from = *self.starts.get(index as usize)? as usize;
        let to = *self.starts.get(index as usize + 1)? as usize;
        // Checked once on construction, so this cannot fail.
        core::str::from_utf8(self.blob.get(from..to)?).ok()
    }

    /// The concatenated names, without their spans.
    #[must_use]
    pub fn blob(&self) -> &[u8] {
        &self.blob
    }

    /// The span boundaries, one longer than [`Strings::len`].
    #[must_use]
    pub fn starts(&self) -> &[u32] {
        &self.starts
    }

    /// Iterate the names in index order.
    pub fn iter(&self) -> impl Iterator<Item = &str> + '_ {
        (0..self.len() as u32).filter_map(|index| self.get(index))
    }

    /// Take ownership of the blob, so the table outlives the artifact.
    #[must_use]
    pub fn into_owned(self) -> Strings<'static> {
        Strings {
            blob: Cow::Owned(self.blob.into_owned()),
            starts: self.starts,
        }
    }
}

/// Why a string table could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadTable {
    /// The spans are empty, or do not start at zero.
    NoTerminator,
    /// A span runs backwards or past the end of the blob.
    SpanOutOfRange {
        /// Where the span starts
        from: usize,
        /// Where it ends
        to: usize,
    },
    /// A name is not UTF-8.
    NotUtf8 {
        /// Where the name starts
        at: usize,
    },
    /// The blob is longer than the last span accounts for.
    TrailingBytes,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_back_in_order() {
        let table = Strings::new(["alpha", "b", "", "gamma"]);
        assert_eq!(table.len(), 4);
        assert_eq!(table.get(0), Some("alpha"));
        assert_eq!(table.get(1), Some("b"));
        assert_eq!(table.get(2), Some(""), "an empty name is still a name");
        assert_eq!(table.get(3), Some("gamma"));
        assert_eq!(table.get(4), None);
    }

    #[test]
    fn a_table_survives_being_taken_apart_and_borrowed_back() {
        let owned = Strings::new(["one", "two", "three"]);
        let blob = owned.blob().to_vec();

        let borrowed = Strings::borrowed(&blob, owned.starts().to_vec()).expect("sound");
        assert_eq!(borrowed.iter().collect::<Vec<_>>(), ["one", "two", "three"]);
    }

    #[test]
    fn an_empty_table_is_sound() {
        let table = Strings::new(Vec::<&str>::new());
        assert!(table.is_empty());
        assert_eq!(table.get(0), None);

        let borrowed = Strings::borrowed(&[], vec![0]).expect("sound");
        assert!(borrowed.is_empty());
    }

    /// The spans come from an artifact, so they are untrusted and must be
    /// rejected rather than turned into a slice panic mid-run.
    #[test]
    fn spans_that_do_not_fit_the_blob_are_refused() {
        assert_eq!(
            Strings::borrowed(b"abc", vec![]).err(),
            Some(BadTable::NoTerminator),
        );
        assert_eq!(
            Strings::borrowed(b"abc", vec![1, 3]).err(),
            Some(BadTable::NoTerminator),
        );
        assert!(matches!(
            Strings::borrowed(b"abc", vec![0, 99]),
            Err(BadTable::SpanOutOfRange { .. }),
        ));
        assert!(matches!(
            Strings::borrowed(b"abc", vec![0, 2, 1]),
            Err(BadTable::SpanOutOfRange { .. }),
        ));
        assert_eq!(
            Strings::borrowed(b"abcd", vec![0, 3]).err(),
            Some(BadTable::TrailingBytes),
        );
    }

    #[test]
    fn a_name_that_is_not_utf8_is_refused() {
        assert!(matches!(
            Strings::borrowed(&[0xff, 0xfe], vec![0, 2]),
            Err(BadTable::NotUtf8 { .. }),
        ));
    }

    /// The property the whole type exists for.
    #[test]
    fn borrowing_points_into_the_caller_s_buffer() {
        let blob = b"alphabeta".to_vec();
        let table = Strings::borrowed(&blob, vec![0, 5, 9]).expect("sound");

        let name = table.get(0).expect("present");
        assert_eq!(name, "alpha");
        assert!(
            name.as_ptr() >= blob.as_ptr()
                && (name.as_ptr() as usize) < blob.as_ptr() as usize + blob.len(),
            "a borrowed name must point into the blob, not into a copy",
        );
    }
}
