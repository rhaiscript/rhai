//! The on-the-wire form of a [`Program`].
//!
//! This is what the project is for. A device that loads bytes runs no parser
//! and builds no tree, so neither the nodes a retained rhai `AST` costs nor the
//! parser's higher peak is ever spent. Everything else — the speed, the
//! verifier — is downstream of being able to write a program out and read it
//! back somewhere else.
//!
//! ## Shape
//!
//! ```text
//! "RGRN"          magic
//! u16             format version
//! abi             INT width, FLOAT width, restriction bitmask
//! varint + utf8   source name, empty for none
//! section         names
//! section         constants
//! section         operator tokens
//! section         op-assignments
//! section         chains
//! section         switch tables, prefixed with a hasher probe
//! varint          declared max stack
//! section         code, verbatim
//! section         position table, empty when stripped
//! ```
//!
//! Everything outside the code section is LEB128, signed values zigzagged,
//! because it is read once at load. The code section is not: it is the bytes
//! the VM executes, copied in and sliced back out untouched, with fixed-width
//! operands so dispatch does not decode. See [`crate::grain::bytecode::code`].
//!
//! ## What it refuses to write
//!
//! Residual fragments are real `Expr` trees — precisely the allocation this
//! removes — so a program holding any is rejected rather than partially
//! written. A script function the compiler could not lower is rejected for the
//! same reason: rhai keeps its own copy of that one, as an AST. Both failures
//! name what blocked them, because "cannot serialize" without the construct is
//! not something a script author can act on.

use core::convert::TryFrom;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

mod abi;
mod read;
mod write;

pub use abi::{Abi, AbiMismatch};
pub use read::ReadError;
pub use write::WriteError;

use crate::grain::bytecode::VerifyError;
use crate::grain::program::Program;

/// Identifies the format, so a file that is not one fails immediately rather
/// than as a nonsense opcode.
const MAGIC: [u8; 4] = *b"RGRN";

/// Bumped when an encoding changes in a way an older reader would misread.
/// Additive changes that an older reader would reject anyway — a new op tag,
/// a new constant tag — do not need it.
const VERSION: u16 = 6;

/// Where a chain starts. Append only.
mod root_tag {
    pub const LOCAL: u8 = 0x01;
    pub const TEMPORARY: u8 = 0x02;
    pub const NAMED: u8 = 0x03;
    pub const THIS: u8 = 0x04;
}

/// Chain-step tags. Append only.
mod step_tag {
    pub const INDEX: u8 = 0x01;
    pub const PROPERTY: u8 = 0x02;
    pub const METHOD: u8 = 0x03;
}

/// What a chain does at the end. Append only.
mod tail_tag {
    pub const READ: u8 = 0x01;
    pub const ASSIGN: u8 = 0x02;
    pub const ASSIGN_OP: u8 = 0x03;
}

/// Constant-pool tags, over the subset of `Dynamic` that means the same thing
/// in another process. Append only.
///
/// The whole table is defined on every build even where a restriction feature
/// means nothing can produce a given tag — `no_float` cannot write a `FLOAT`,
/// `no_index` an `ARRAY`. The numbering is the wire format, so it must not
/// shift with the features of whoever compiled the writer.
#[allow(dead_code)]
mod constant {
    pub const UNIT: u8 = 0x00;
    pub const FALSE: u8 = 0x01;
    pub const TRUE: u8 = 0x02;
    pub const INT: u8 = 0x03;
    pub const FLOAT: u8 = 0x04;
    pub const CHAR: u8 = 0x05;
    pub const STRING: u8 = 0x06;
    pub const ARRAY: u8 = 0x07;
    pub const MAP: u8 = 0x08;
    pub const BLOB: u8 = 0x09;
    pub const RANGE: u8 = 0x0a;
    pub const RANGE_INCLUSIVE: u8 = 0x0b;
}

impl<'a> Program<'a> {
    /// Encode this program, diagnostics included.
    ///
    /// # Errors
    ///
    /// Fails if the program still holds anything that cannot cross a process
    /// boundary: an un-lowered fragment, a script function, or a constant
    /// carrying a host type.
    pub fn write(&self) -> Result<Vec<u8>, WriteError> {
        write::write(self, write::Positions::Keep)
    }

    /// Encode this program without its position table, returning the table
    /// separately.
    ///
    /// This is the split the debug layer exists for. Ship the first half to the
    /// device and keep the second: errors then arrive carrying an instruction
    /// address, and [`pos::resolve`](crate::grain::pos::resolve) turns it back into a position
    /// where the source still is. The table can also be sent back later with
    /// [`Program::attach_positions`].
    ///
    /// # Errors
    ///
    /// As [`Program::write`].
    pub fn write_stripped(&self) -> Result<(Vec<u8>, Vec<u8>), WriteError> {
        let bytes = write::write(self, write::Positions::Strip)?;
        Ok((bytes, self.positions().to_table()))
    }

    /// Decode a program written by [`Program::write`], borrowing its
    /// instructions from `bytes`.
    ///
    /// Nothing is allocated for the code — the returned program points into the
    /// buffer and the VM dispatches on it where it lies. What is allocated is
    /// bounded by the distinct constants, names and operators the script
    /// mentions, not by how long it is. Call [`Program::into_owned`] if the
    /// buffer has to go.
    ///
    /// The chunk is verified before this returns, so a program that loads
    /// cannot underflow the operand stack, jump outside itself, jump into the
    /// middle of an instruction, or index a pool entry that is not there. With
    /// the code being executed in place, that check is the only thing between a
    /// corrupt file and an operand read as an opcode.
    ///
    /// # Errors
    ///
    /// Fails on a bad header, an ABI the running build cannot represent,
    /// truncated or malformed input, or a chunk that does not verify.
    pub fn read(bytes: &'a [u8]) -> Result<Self, ReadError> {
        read::read(bytes)
    }
}

/// A reader positioned in a byte slice.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        let end = self.pos.checked_add(n).ok_or(ReadError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(ReadError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, ReadError> {
        Ok(self.take(1)?[0])
    }

    /// A count that has to be reserved for before it is read.
    ///
    /// Nothing is encoded in less than a byte, so a count larger than what is
    /// left cannot be honest — and reserving for it first would let a handful
    /// of bytes ask for a terabyte. Reading the entries one at a time runs out
    /// of input safely; `Vec::with_capacity` does not, because it allocates
    /// before the first entry is read. Found by `tests/fuzz.rs`.
    fn count(&mut self) -> Result<usize, ReadError> {
        let count = usize::try_from(self.uvarint()?).map_err(|_| ReadError::Truncated)?;
        if count > self.bytes.len() - self.pos {
            return Err(ReadError::Truncated);
        }
        Ok(count)
    }

    /// LEB128, capped at ten groups so a run of continuation bytes cannot spin.
    fn uvarint(&mut self) -> Result<u64, ReadError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.byte()?;
            let payload = u64::from(byte & 0x7f);
            // The tenth group has a single bit left to land in. Shifting would
            // drop the other six rather than refuse them, so a value too wide
            // for 64 bits would decode as a smaller one.
            if shift == 63 && payload > 1 {
                return Err(ReadError::MalformedVarint);
            }
            value |= payload << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(ReadError::MalformedVarint)
    }

    fn ivarint(&mut self) -> Result<i64, ReadError> {
        let raw = self.uvarint()?;
        Ok(((raw >> 1) as i64) ^ -((raw & 1) as i64))
    }

    fn index(&mut self) -> Result<u32, ReadError> {
        u32::try_from(self.uvarint()?).map_err(|_| ReadError::MalformedVarint)
    }

    fn small(&mut self) -> Result<u16, ReadError> {
        u16::try_from(self.uvarint()?).map_err(|_| ReadError::MalformedVarint)
    }

    fn str(&mut self) -> Result<&'a str, ReadError> {
        let len = usize::try_from(self.uvarint()?).map_err(|_| ReadError::Truncated)?;
        core::str::from_utf8(self.take(len)?).map_err(|_| ReadError::BadUtf8)
    }

    fn at_end(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

fn put_uvarint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_ivarint(out: &mut Vec<u8>, value: i64) {
    put_uvarint(out, ((value << 1) ^ (value >> 63)) as u64);
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    put_uvarint(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

impl From<VerifyError> for ReadError {
    fn from(err: VerifyError) -> Self {
        Self::Unverifiable(err)
    }
}

impl From<crate::grain::bytecode::BadTable> for ReadError {
    fn from(err: crate::grain::bytecode::BadTable) -> Self {
        Self::Names(err)
    }
}

impl From<crate::grain::bytecode::TableError> for ReadError {
    fn from(err: crate::grain::bytecode::TableError) -> Self {
        Self::Positions(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Varints are the whole encoding's foundation; a rounding error here
    /// misreads every index in the file.
    #[test]
    fn unsigned_varints_round_trip_at_the_edges() {
        for value in [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX] {
            let mut buf = Vec::new();
            put_uvarint(&mut buf, value);
            assert_eq!(Cursor::new(&buf).uvarint().unwrap(), value, "at {value}");
        }
    }

    #[test]
    fn signed_varints_round_trip_across_zero() {
        for value in [0i64, -1, 1, -64, 63, i32::MIN as i64, i64::MIN, i64::MAX] {
            let mut buf = Vec::new();
            put_ivarint(&mut buf, value);
            assert_eq!(Cursor::new(&buf).ivarint().unwrap(), value, "at {value}");
        }
    }

    /// Small numbers are most of a chunk, so the encoding only pays for itself
    /// if they cost one byte.
    #[test]
    fn small_indices_cost_one_byte() {
        let mut buf = Vec::new();
        put_uvarint(&mut buf, 127);
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn a_run_of_continuation_bytes_terminates() {
        let never_ends = vec![0xffu8; 64];
        assert_eq!(
            Cursor::new(&never_ends).uvarint(),
            Err(ReadError::MalformedVarint),
        );
    }

    /// The tenth group is the one place a shift could silently lose bits, so a
    /// wide value there must be refused rather than truncated into a small one.
    #[test]
    fn a_tenth_group_wider_than_one_bit_is_refused() {
        let mut ten = [0x80u8; 10];

        // The largest value there is: nine full groups and a final bit.
        let widest = [[0xffu8; 9].as_slice(), &[0x01]].concat();
        assert_eq!(Cursor::new(&widest).uvarint(), Ok(u64::MAX));

        // One past it. Shifting would drop the payload and read this as zero.
        ten[9] = 0x02;
        assert_eq!(Cursor::new(&ten).uvarint(), Err(ReadError::MalformedVarint));

        ten[9] = 0x7f;
        assert_eq!(Cursor::new(&ten).uvarint(), Err(ReadError::MalformedVarint));
    }

    #[test]
    fn reading_past_the_end_is_an_error_not_a_panic() {
        assert_eq!(Cursor::new(&[]).byte(), Err(ReadError::Truncated));
        assert_eq!(Cursor::new(&[1, 2]).take(9), Err(ReadError::Truncated));
    }
}
