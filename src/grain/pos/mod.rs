//! Turning a rhaigrain instruction address back into a source position,
//! without `std`.
//!
//! A rhaigrain artifact carries no positions in its instruction stream. They
//! live in a separate table, so a device can be shipped the bytecode alone and
//! the table kept on the host that compiled it — which is the point, because a
//! position on every instruction is a large fraction of the artifact and is
//! never read unless something fails.
//!
//! What comes back is a [`Site`]: a line and a column as plain numbers. This
//! crate does not know what a `rhai::Position` is, and deliberately so, since
//! the reason to keep the table on-device is to report an error without
//! linking the compiler that produced it.
//!
//! ## Where this sits
//!
//! It resolves the first hop only. A script that was minified before it was
//! compiled needs a second: the [`Site`] is a position in the *minified*
//! source, and a Source Map v3 resolver such as `rhaiper-map` takes it the rest
//! of the way to the original.
//!
//! ```text
//! instruction address --[this crate]--> minified line:col --[a source map]--> original
//! ```
//!
//! ## Table format
//!
//! ```text
//! varint          entry count
//! per entry:      varint  address delta from the previous entry
//!                 varint  line, 1-based (0 means the site is unknown)
//!                 varint  column, 1-based (0 means the start of a line)
//! ```
//!
//! Entries are sorted by address, so a lookup walks until it reaches or passes
//! the one it wants and stops. That is a linear scan, which is the right shape
//! for something only consulted when a program has already failed.

pub mod varint;

/// A place in a source file.
///
/// Both fields follow rhai's own convention: `line` counts from 1, and
/// `column` counts characters from 1 with 0 meaning the start of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Site {
    /// 1-based line.
    pub line: u32,
    /// 1-based character column; 0 is the start of a line.
    pub column: u32,
}

/// Why a table could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The table ends mid-entry.
    Truncated,
    /// A number too wide for the field it was read into.
    Overflow,
    /// Addresses are not strictly ascending, so the scan's early exit would be
    /// wrong and a lookup could silently return the wrong site.
    OutOfOrder { at: u32 },
    /// Bytes remain after the last entry, so this is not the table it claims.
    TrailingBytes { count: usize },
}

impl From<varint::Error> for Error {
    fn from(err: varint::Error) -> Self {
        match err {
            varint::Error::Truncated => Self::Truncated,
            varint::Error::Overflow => Self::Overflow,
        }
    }
}

/// The site recorded for instruction `address`, if the table has one.
///
/// `None` means the instruction has no recorded site — not that the table is
/// broken. Most instructions have none: only those that can raise an error
/// against a place in the source are worth recording.
///
/// A malformed table also reads as `None`. Resolving happens while reporting
/// an error, and failing to resolve must not replace the error being reported;
/// call [`check`] at load time to find out whether a table is sound.
#[must_use]
pub fn resolve(table: &[u8], address: u32) -> Option<Site> {
    let mut at = 0usize;
    let count = varint::u32(table, &mut at).ok()?;

    let mut current = 0u32;
    for _ in 0..count {
        current = current.checked_add(varint::u32(table, &mut at).ok()?)?;
        let line = varint::u32(table, &mut at).ok()?;
        let column = varint::u32(table, &mut at).ok()?;

        if current == address {
            return (line != 0).then_some(Site { line, column });
        }
        // Ascending, so nothing past here can match.
        if current > address {
            return None;
        }
    }

    None
}

/// How many entries the table holds, without decoding them.
///
/// # Errors
///
/// [`Error::Truncated`] or [`Error::Overflow`] if the count itself is unreadable.
pub fn count(table: &[u8]) -> Result<u32, Error> {
    let mut at = 0usize;
    Ok(varint::u32(table, &mut at)?)
}

/// Check that a table is well formed, so [`resolve`] can afford not to.
///
/// # Errors
///
/// Names the first thing wrong: a truncated entry, a number too wide, an
/// address that does not ascend, or bytes past the end.
pub fn check(table: &[u8]) -> Result<(), Error> {
    let mut at = 0usize;
    let count = varint::u32(table, &mut at)?;

    let mut previous: Option<u32> = None;
    for _ in 0..count {
        let delta = varint::u32(table, &mut at)?;
        let address = match previous {
            None => delta,
            // A zero delta repeats the previous address, which would shadow it.
            Some(previous) if delta == 0 => return Err(Error::OutOfOrder { at: previous }),
            Some(previous) => previous.checked_add(delta).ok_or(Error::Overflow)?,
        };
        let _line = varint::u32(table, &mut at)?;
        let _column = varint::u32(table, &mut at)?;
        previous = Some(address);
    }

    if at != table.len() {
        return Err(Error::TrailingBytes {
            count: table.len() - at,
        });
    }

    Ok(())
}

/// Build a table from sites in ascending address order.
///
/// Entries with a zero line are dropped: they carry no information, and
/// keeping them would cost bytes on a table whose whole purpose is to be small
/// enough to leave behind.
///
/// # Panics
///
/// Panics if the addresses are not strictly ascending. That is a caller bug —
/// a compiler emits in address order by construction — and producing a table
/// that silently resolves wrong is worse than not producing one.
#[must_use]
pub fn encode(sites: impl IntoIterator<Item = (u32, Site)>) -> alloc::vec::Vec<u8> {
    let kept: alloc::vec::Vec<_> = sites
        .into_iter()
        .filter(|(_, site)| site.line != 0)
        .collect();

    let mut out = alloc::vec::Vec::new();
    varint::put_u64(&mut out, kept.len() as u64);

    let mut previous = 0u32;
    for (index, (address, site)) in kept.iter().enumerate() {
        if index > 0 {
            assert!(
                *address > previous,
                "table addresses must strictly ascend: {address} follows {previous}",
            );
        }
        varint::put_u64(&mut out, u64::from(address - previous));
        varint::put_u64(&mut out, u64::from(site.line));
        varint::put_u64(&mut out, u64::from(site.column));
        previous = *address;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn site(line: u32, column: u32) -> Site {
        Site { line, column }
    }

    fn table() -> Vec<u8> {
        encode(vec![
            (0, site(1, 5)),
            (3, site(1, 11)),
            (40, site(7, 2)),
            (41, site(9, 0)),
        ])
    }

    #[test]
    fn a_recorded_address_resolves_to_its_site() {
        let table = table();
        assert_eq!(resolve(&table, 0), Some(site(1, 5)));
        assert_eq!(resolve(&table, 3), Some(site(1, 11)));
        assert_eq!(resolve(&table, 40), Some(site(7, 2)));
    }

    /// Column 0 is the start of a line, which is a real position and must not
    /// be confused with an absent one.
    #[test]
    fn the_start_of_a_line_is_a_position() {
        assert_eq!(resolve(&table(), 41), Some(site(9, 0)));
    }

    #[test]
    fn an_address_with_no_site_resolves_to_nothing() {
        let table = table();
        assert_eq!(resolve(&table, 1), None, "between two entries");
        assert_eq!(resolve(&table, 99), None, "past the last entry");
    }

    #[test]
    fn an_empty_table_resolves_nothing_and_is_sound() {
        let table = encode(Vec::new());
        assert_eq!(check(&table), Ok(()));
        assert_eq!(count(&table), Ok(0));
        assert_eq!(resolve(&table, 0), None);
    }

    /// A site with no line carries nothing, and the table exists to be small.
    #[test]
    fn sites_with_no_line_are_not_stored() {
        let table = encode(vec![(0, site(1, 1)), (1, site(0, 0)), (2, site(3, 3))]);
        assert_eq!(count(&table), Ok(2));
        assert_eq!(resolve(&table, 1), None);
        assert_eq!(resolve(&table, 2), Some(site(3, 3)));
    }

    /// Deltas are the reason the table is worth encoding rather than storing
    /// flat: consecutive instructions cost one byte of address each.
    #[test]
    fn a_dense_run_costs_three_bytes_an_entry() {
        let dense: Vec<_> = (0..100).map(|pc| (pc, site(1, 1))).collect();
        let table = encode(dense);
        assert_eq!(table.len(), 1 + 100 * 3);
    }

    #[test]
    fn a_sound_table_passes_its_own_check() {
        assert_eq!(check(&table()), Ok(()));
    }

    #[test]
    fn a_truncated_table_is_named_as_such() {
        let table = table();
        for cut in 1..table.len() {
            assert!(
                check(&table[..cut]).is_err(),
                "a {cut}-byte prefix passed the check",
            );
        }
    }

    #[test]
    fn a_repeated_address_is_refused() {
        // Hand-built, since `encode` will not produce one.
        let mut bytes = Vec::new();
        varint::put_u64(&mut bytes, 2);
        for _ in 0..2 {
            varint::put_u64(&mut bytes, 0); // delta
            varint::put_u64(&mut bytes, 1); // line
            varint::put_u64(&mut bytes, 1); // column
        }
        assert_eq!(check(&bytes), Err(Error::OutOfOrder { at: 0 }));
    }

    #[test]
    fn bytes_past_the_last_entry_are_refused() {
        let mut bytes = table();
        bytes.push(0);
        assert_eq!(check(&bytes), Err(Error::TrailingBytes { count: 1 }));
    }

    /// Resolution runs while an error is being reported. Whatever the table
    /// says, it may not become the failure.
    #[test]
    fn no_byte_string_can_make_resolution_panic() {
        let table = table();
        for index in 0..table.len() {
            for bit in 0..8 {
                let mut corrupt = table.clone();
                corrupt[index] ^= 1 << bit;
                for address in 0..64 {
                    let _ = resolve(&corrupt, address);
                }
                let _ = check(&corrupt);
            }
        }

        for junk in [&b""[..], &[0xff][..], &[0xff; 32][..], &[0x80; 12][..]] {
            for address in 0..8 {
                let _ = resolve(junk, address);
            }
            let _ = check(junk);
        }
    }

    #[test]
    #[should_panic(expected = "must strictly ascend")]
    fn encoding_out_of_order_sites_is_a_caller_bug() {
        let _ = encode(vec![(5, site(1, 1)), (2, site(1, 1))]);
    }
}
