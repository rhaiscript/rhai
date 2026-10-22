use crate::grain::pos::Site;
use crate::Position;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Where each instruction came from, or nothing.
///
/// Instructions carry no position of their own. Diagnostics are the one part of
/// an artifact that is never read unless something has already failed, so they
/// are the part worth being able to leave behind — and an instruction that is
/// pure payload is one that can be executed straight out of a borrowed byte
/// slice, which is the larger reason.
///
/// Moving them out is close to free on bytes: they were only ever stored on the
/// instructions that had one. What it buys is that they can now be removed.
/// `tests/format.rs` measures how much that removes.
///
/// In memory this holds only the instructions that have a position, in
/// address order. The lookup is not always cold — a loop back-edge passes a
/// position to `track_operation` on every iteration, and the built-in operator
/// path builds a `NativeCallContext` around one — so it is a binary search,
/// a dozen comparisons for a large script. An entry per code byte would make
/// it an index, at four bytes of heap for every byte of code, for a table that
/// is mostly empty: that is what a small device cannot spare. The compact
/// delta form in [`pos`](crate::grain::pos) is the wire form, expanded once at
/// load.
///
/// [`Positions::Stripped`] is not a degraded mode to apologize for: it is what
/// a device runs. Errors come back carrying an instruction address instead of a
/// position, and the host that kept the table resolves it.
#[derive(Debug, Clone, Default)]
pub enum Positions {
    /// The table was never written, or was stripped before shipping.
    #[default]
    Stripped,
    /// The address and position of every instruction that has one, by
    /// address.
    Sparse(Box<[(u32, Position)]>),
}

impl Positions {
    /// Build from one position per code byte, keeping only those that say
    /// something, and dropping the table entirely if none of them do.
    #[cfg(any(test, not(feature = "no_ast")))]
    pub(crate) fn dense(positions: Vec<Position>) -> Self {
        Self::sparse(
            positions
                .into_iter()
                .enumerate()
                .filter(|(_, pos)| !pos.is_none())
                .map(|(pc, pos)| (pc as u32, pos))
                .collect(),
        )
    }

    /// Build from the instructions that have a position, in address order.
    fn sparse(entries: Vec<(u32, Position)>) -> Self {
        if entries.is_empty() {
            return Self::Stripped;
        }
        Self::Sparse(entries.into_boxed_slice())
    }

    /// The position recorded for `pc`, or `Position::NONE`.
    ///
    /// Out of range reads as no position rather than panicking: this runs while
    /// an error is being reported, and losing the position must not replace the
    /// error being reported.
    #[must_use]
    pub fn get(&self, pc: usize) -> Position {
        match self {
            Self::Stripped => Position::NONE,
            Self::Sparse(entries) => u32::try_from(pc)
                .ok()
                .and_then(|pc| entries.binary_search_by_key(&pc, |&(at, _)| at).ok())
                .map_or(Position::NONE, |i| entries[i].1),
        }
    }

    /// Whether the position table is stripped
    #[must_use]
    pub fn is_stripped(&self) -> bool {
        matches!(self, Self::Stripped)
    }

    /// Encode as the compact table [`pos::resolve`](crate::grain::pos::resolve)
    /// reads.
    ///
    /// Instructions with no position are skipped, which is most of them.
    #[must_use]
    pub fn to_table(&self) -> Vec<u8> {
        let Self::Sparse(entries) = self else {
            return crate::grain::pos::encode(core::iter::empty());
        };

        crate::grain::pos::encode(entries.iter().filter_map(|&(pc, pos)| {
            let line = pos.line()?;
            Some((
                pc,
                Site {
                    line: line as u32,
                    column: pos.position().unwrap_or(0) as u32,
                },
            ))
        }))
    }

    /// Expand a compact table back to an entry per instruction it names.
    ///
    /// Every address has to name the *start* of an instruction in `code`.
    /// Counting how many landed in range is not enough: another program's table
    /// could pass that and then misreport every error.
    ///
    /// # Errors
    ///
    /// Whatever [`pos::check`](crate::grain::pos::check) found, or
    /// [`TableError::PastTheEnd`] for an address that does not begin an
    /// instruction.
    pub fn from_table(table: &[u8], code: &[u8]) -> Result<Self, TableError> {
        crate::grain::pos::check(table).map_err(TableError::Malformed)?;

        let count = crate::grain::pos::count(table).map_err(TableError::Malformed)?;
        if count == 0 {
            return Ok(Self::Stripped);
        }

        // Walked, not indexed: only an address the walk arrives at begins an
        // instruction. One landing mid-instruction is as wrong as one past the
        // end, and reads as a plausible position.
        let mut entries = Vec::with_capacity(count as usize);
        let mut matched = 0usize;
        let mut at = 0usize;
        while at < code.len() {
            if let Some(site) = crate::grain::pos::resolve(table, at as u32) {
                let pos = site_to_position(site);
                if !pos.is_none() {
                    entries.push((at as u32, pos));
                }
                matched += 1;
            }
            match crate::grain::bytecode::code::width(code, at) {
                Some(width) => at += width,
                // Not a stream this table can be checked against. Rejecting it
                // is the verifier's job; here the count below reports it.
                None => break,
            }
        }

        if matched != count as usize {
            return Err(TableError::PastTheEnd {
                entries: count as usize,
                matched,
                instructions: code.len(),
            });
        }

        Ok(Self::sparse(entries))
    }
}

/// Why a sidecar could not be attached to a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// The table is not well formed.
    Malformed(crate::grain::pos::Error),
    /// The table names addresses that do not begin an instruction of this
    /// chunk, so it belongs to a different program.
    PastTheEnd {
        /// How many entries the table holds
        entries: usize,
        /// How many of them landed on an instruction
        matched: usize,
        /// How many code bytes the chunk has
        instructions: usize,
    },
    /// The sidecar was taken from a different program.
    WrongProgram {
        /// The sidecar's debug id
        expected: u128,
        /// This program's debug id
        found: u128,
    },
    /// The chain site stream is not well formed.
    ChainStream(super::sites::StreamError),
    /// The chain site stream is sound but does not describe this program's
    /// chains.
    ChainCount {
        /// How many sites the stream holds
        sites: usize,
        /// How many positions this program's chains carry
        slots: usize,
    },
}

impl core::fmt::Display for TableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Malformed(err) => write!(f, "position table is malformed: {err:?}"),
            Self::PastTheEnd {
                entries,
                matched,
                instructions,
            } => write!(
                f,
                "position table has {entries} entries but only {matched} begin an instruction \
                 of this chunk's {instructions} code bytes, so it is a different program's"
            ),
            Self::WrongProgram { expected, found } => write!(
                f,
                "sidecar belongs to debug id {expected:#034x}, not to this program's {found:#034x}"
            ),
            Self::ChainStream(err) => write!(f, "{err}"),
            Self::ChainCount { sites, slots } => write!(
                f,
                "chain site stream holds {sites} sites but this program's chains carry {slots} \
                 positions, so it is a different program's"
            ),
        }
    }
}

/// A [`Site`] is plain numbers, on purpose — turning it into Rhai's own type is
/// this side's job.
pub(crate) fn site_to_position(site: Site) -> Position {
    let (Ok(line), Ok(column)) = (u16::try_from(site.line), u16::try_from(site.column)) else {
        return Position::NONE;
    };
    if line == 0 {
        return Position::NONE;
    }
    Position::new(line, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain::bytecode::code::assemble;
    use crate::grain::bytecode::Op;

    /// Four one-byte instructions, so each of `sample`'s addresses begins one.
    fn code() -> Vec<u8> {
        let (code, _) = assemble(&[Op::Unit, Op::Unit, Op::Unit, Op::Unit]).expect("must assemble");
        assert_eq!(code.len(), 4, "the sample assumes one byte an instruction");
        code
    }

    fn sample() -> Positions {
        Positions::dense(vec![
            Position::NONE,
            Position::new(1, 5),
            Position::NONE,
            Position::new(3, 0),
        ])
    }

    #[test]
    fn a_table_survives_the_round_trip() {
        let table = sample().to_table();
        let back = Positions::from_table(&table, &code()).expect("the table is this chunk's");

        for pc in 0..4 {
            assert_eq!(back.get(pc), sample().get(pc), "at {pc}");
        }
    }

    /// Column 0 is the start of a line, not the absence of a position.
    #[test]
    fn the_start_of_a_line_survives() {
        let table = sample().to_table();
        let back = Positions::from_table(&table, &code()).unwrap();
        assert_eq!(back.get(3), Position::new(3, 0));
    }

    #[test]
    fn stripping_is_what_a_missing_table_reads_as() {
        let stripped = Positions::Stripped;
        assert!(stripped.is_stripped());
        assert_eq!(stripped.get(0), Position::NONE);

        let empty = Positions::from_table(&stripped.to_table(), &code()).unwrap();
        assert!(empty.is_stripped());
    }

    /// A chunk with nothing to say should not carry an array of nothing.
    #[test]
    fn positions_that_are_all_absent_collapse() {
        assert!(Positions::dense(vec![Position::NONE; 8]).is_stripped());
    }

    /// Attaching the wrong program's table would silently misreport every
    /// error, which is worse than reporting none.
    ///
    /// Under `no_position` every `Position` is `NONE`, so `sample()` collapses
    /// to a stripped table — which any chunk accepts, there being no addresses
    /// in it to fall past the end of.
    #[test]
    #[cfg(not(feature = "no_position"))]
    fn a_table_from_another_program_is_refused() {
        let table = sample().to_table();
        let (short, _) = assemble(&[Op::Unit, Op::Unit]).expect("must assemble");
        assert!(matches!(
            Positions::from_table(&table, &short),
            Err(TableError::PastTheEnd { .. }),
        ));
    }

    /// An address inside an instruction is as wrong as one past the end, and a
    /// count of how many landed in range would not notice.
    #[test]
    #[cfg(not(feature = "no_position"))]
    fn an_address_that_does_not_begin_an_instruction_is_refused() {
        let table = sample().to_table();
        // `Const` is three bytes, so this chunk begins instructions at 0 and 3
        // only — and the sample names 1 and 3.
        let (wide, _) = assemble(&[Op::Const(0), Op::Unit]).expect("must assemble");
        assert_eq!(wide.len(), 4, "the case needs the same four bytes");

        assert!(matches!(
            Positions::from_table(&table, &wide),
            Err(TableError::PastTheEnd { .. }),
        ));
    }

    #[test]
    fn a_malformed_table_is_refused_rather_than_half_applied() {
        let table = sample().to_table();
        assert!(matches!(
            Positions::from_table(&table[..table.len() - 1], &code()),
            Err(TableError::Malformed(..)),
        ));
    }

    /// Reading past the chunk is how an error report finds out there is no
    /// position, and it happens while another error is already in flight.
    #[test]
    fn an_address_past_the_end_reads_as_no_position() {
        assert_eq!(sample().get(9999), Position::NONE);
    }
}
