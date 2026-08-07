use crate::grain::bytecode::code::disassemble;
use crate::grain::bytecode::Op;

/// One body of code: the top-level program, or one script function.
///
/// Metadata only. Every chunk in a program shares a single instruction buffer,
/// concatenated in order, and a chunk names its span of it. That keeps one
/// position table and one instruction address across the whole program, so a
/// device reporting where it failed reports one number.
///
/// `max_stack` exists so the VM can size its operand stack rather than growing
/// it. The compiler emits an upper bound it can compute without a depth walk —
/// one per instruction — and then replaces it with the high water the verifier
/// actually measured. On a device that difference is the whole reservation: a
/// chunk of 25 instructions rarely stacks more than three values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Chunk {
    entry: u32,
    end: u32,
    max_stack: u16,
}

impl Chunk {
    pub(crate) fn new(entry: u32, end: u32, max_stack: u16) -> Self {
        Self {
            entry,
            end,
            max_stack,
        }
    }

    /// Where execution starts, as an offset into the program's code.
    #[must_use]
    pub fn entry(&self) -> u32 {
        self.entry
    }

    /// One past the last byte of this chunk.
    #[must_use]
    pub fn end(&self) -> u32 {
        self.end
    }

    #[must_use]
    pub fn max_stack(&self) -> u16 {
        self.max_stack
    }

    pub(crate) fn set_max_stack(&mut self, max_stack: u16) {
        self.max_stack = max_stack;
    }

    /// This chunk's slice of a program's code.
    #[must_use]
    pub fn body<'c>(&self, code: &'c [u8]) -> &'c [u8] {
        code.get(self.entry as usize..self.end as usize)
            .unwrap_or_default()
    }

    /// The instructions, paired with their addresses in the program.
    ///
    /// For reading, not for running — reconstructing an [`Op`] is exactly the
    /// work the byte encoding exists to avoid.
    pub fn ops<'c>(&self, code: &'c [u8]) -> impl Iterator<Item = (usize, Op)> + 'c {
        let entry = self.entry as usize;
        disassemble(self.body(code)).map(move |(at, op)| (at + entry, op))
    }
}
