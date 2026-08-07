use crate::grain::pos::Site;
use crate::Position;

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
/// In memory this is dense, because the lookup is not always cold. A loop
/// back-edge passes a position to `track_operation` on every iteration, and the
/// built-in operator path builds a `NativeCallContext` around one — both on the
/// hot path, both needing it only if something goes wrong. Indexing an array is
/// what makes that free. The compact delta form in [`rhaigrain_pos`] is the
/// wire form, expanded once at load.
///
/// [`Positions::Stripped`] is not a degraded mode to apologise for: it is what
/// a device runs. Errors come back carrying an instruction address instead of a
/// position, and the host that kept the table resolves it.
#[derive(Debug, Clone, Default)]
pub enum Positions {
    /// The table was never written, or was stripped before shipping.
    #[default]
    Stripped,
    /// One entry per instruction, so a lookup is an index.
    Dense(Box<[Position]>),
}

impl Positions {
    /// Build from one position per instruction, dropping the table entirely if
    /// none of them say anything.
    pub(crate) fn dense(positions: Vec<Position>) -> Self {
        if positions.iter().all(|pos| pos.is_none()) {
            return Self::Stripped;
        }
        Self::Dense(positions.into_boxed_slice())
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
            Self::Dense(positions) => positions.get(pc).copied().unwrap_or(Position::NONE),
        }
    }

    /// Whether the position table is stripped
    #[must_use]
    pub fn is_stripped(&self) -> bool {
        matches!(self, Self::Stripped)
    }

    /// Encode as the compact table [`rhaigrain_pos::resolve`] reads.
    ///
    /// Instructions with no position are skipped, which is most of them.
    #[must_use]
    pub fn to_table(&self) -> Vec<u8> {
        let Self::Dense(positions) = self else {
            return crate::grain::pos::encode(core::iter::empty());
        };

        crate::grain::pos::encode(positions.iter().enumerate().filter_map(|(pc, pos)| {
            let line = pos.line()?;
            Some((
                pc as u32,
                Site {
                    line: line as u32,
                    column: pos.position().unwrap_or(0) as u32,
                },
            ))
        }))
    }

    /// Expand a compact table back to one entry per instruction.
    ///
    /// `instructions` bounds the result, so a table naming an address past the
    /// end of the chunk is refused rather than silently widening the array.
    ///
    /// # Errors
    ///
    /// Whatever [`rhaigrain_pos::check`] found, or [`TableError::PastTheEnd`]
    /// for an address that does not name an instruction.
    pub fn from_table(table: &[u8], instructions: usize) -> Result<Self, TableError> {
        crate::grain::pos::check(table).map_err(TableError::Malformed)?;

        let count = crate::grain::pos::count(table).map_err(TableError::Malformed)?;
        if count == 0 {
            return Ok(Self::Stripped);
        }

        // `check` proved the table sound, so a miss here is an address that
        // does not name an instruction in *this* chunk, which the count
        // comparison below catches.
        let positions: Vec<Position> = (0..instructions)
            .map(|pc| {
                crate::grain::pos::resolve(table, pc as u32)
                    .map_or(Position::NONE, site_to_position)
            })
            .collect();

        let found = positions.iter().filter(|pos| !pos.is_none()).count();
        if found != count as usize {
            return Err(TableError::PastTheEnd {
                entries: count as usize,
                matched: found,
                instructions,
            });
        }

        Ok(Self::dense(positions))
    }
}

/// Why a position table could not be attached to a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// The table is not well formed.
    Malformed(crate::grain::pos::Error),
    /// The table names addresses this chunk does not have, so it belongs to a
    /// different program.
    PastTheEnd {
        entries: usize,
        matched: usize,
        instructions: usize,
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
                "position table has {entries} entries but only {matched} name one of this \
                 chunk's {instructions} instructions, so it is a different program's"
            ),
        }
    }
}

/// A [`Site`] is plain numbers, on purpose — turning it into rhai's own type is
/// this side's job.
fn site_to_position(site: Site) -> Position {
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
        let back = Positions::from_table(&table, 4).expect("the table is this chunk's");

        for pc in 0..4 {
            assert_eq!(back.get(pc), sample().get(pc), "at {pc}");
        }
    }

    /// Column 0 is the start of a line, not the absence of a position.
    #[test]
    fn the_start_of_a_line_survives() {
        let table = sample().to_table();
        let back = Positions::from_table(&table, 4).unwrap();
        assert_eq!(back.get(3), Position::new(3, 0));
    }

    #[test]
    fn stripping_is_what_a_missing_table_reads_as() {
        let stripped = Positions::Stripped;
        assert!(stripped.is_stripped());
        assert_eq!(stripped.get(0), Position::NONE);

        let empty = Positions::from_table(&stripped.to_table(), 4).unwrap();
        assert!(empty.is_stripped());
    }

    /// A chunk with nothing to say should not carry an array of nothing.
    #[test]
    fn positions_that_are_all_absent_collapse() {
        assert!(Positions::dense(vec![Position::NONE; 8]).is_stripped());
    }

    /// Attaching the wrong program's table would silently misreport every
    /// error, which is worse than reporting none.
    #[test]
    fn a_table_from_another_program_is_refused() {
        let table = sample().to_table();
        assert!(matches!(
            Positions::from_table(&table, 2),
            Err(TableError::PastTheEnd { .. }),
        ));
    }

    #[test]
    fn a_malformed_table_is_refused_rather_than_half_applied() {
        let table = sample().to_table();
        assert!(matches!(
            Positions::from_table(&table[..table.len() - 1], 4),
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
