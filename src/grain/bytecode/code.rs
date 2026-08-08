//! The executable form: instructions as bytes, run in place.
//!
//! [`Op`] is what the compiler emits and what a disassembly shows. It is not
//! what runs. A `Vec<Op>` costs sixteen bytes an instruction and has to be
//! built at load, which is most of what this project exists to avoid — so a
//! program's code is a byte slice, and a loaded program borrows it from the
//! artifact rather than decoding it into anything.
//!
//! ## Why fixed fields rather than varints
//!
//! Everything else in an artifact is LEB128, because everything else is read
//! once. Instructions are read on every execution, so their operands are
//! fixed-width little-endian at known offsets: decoding is a match on the tag
//! and a couple of loads, with no loop per field. That costs about one byte per
//! operand against the varint form and buys a dispatch loop that does not
//! decode.
//!
//! ## Jumps are byte offsets
//!
//! Instructions vary in length, so a jump names a byte offset rather than an
//! instruction index. [`assemble`] resolves the compiler's indices into offsets
//! once, and [`verify`](super::verify) proves every one of them lands on an
//! instruction boundary — without which a jump into the middle of an operand
//! would decode whatever the operand's bytes happen to look like.

use alloc::borrow::Cow;

use crate::grain::bytecode::{Op, Receiver};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Instruction tags.
///
/// Written out rather than derived from [`Op`]'s order, so reordering the enum
/// for readability cannot change what a program means. Append only.
///
/// Operators get their own tag rather than an optional field, so the common
/// call pays nothing for the one that carries a token.
pub mod tag {
    /// [`Op::Const`](super::Op::Const).
    pub const CONST: u8 = 0x01;
    /// [`Op::Unit`](super::Op::Unit).
    pub const UNIT: u8 = 0x02;
    /// [`Op::Bool`](super::Op::Bool) holding `false`.
    pub const FALSE: u8 = 0x03;
    /// [`Op::Bool`](super::Op::Bool) holding `true`.
    pub const TRUE: u8 = 0x04;
    /// [`Op::LoadLocal`](super::Op::LoadLocal).
    pub const LOAD_LOCAL: u8 = 0x05;
    /// [`Op::StoreLocal`](super::Op::StoreLocal).
    pub const STORE_LOCAL: u8 = 0x06;
    /// [`Op::AssignLocal`](super::Op::AssignLocal) with a plain `=`.
    pub const ASSIGN_LOCAL: u8 = 0x07;
    /// [`Op::AssignLocal`](super::Op::AssignLocal) through an operator.
    pub const ASSIGN_LOCAL_OP: u8 = 0x08;
    /// [`Op::DeclareLocal`](super::Op::DeclareLocal) for a `let`.
    pub const DECLARE_LOCAL: u8 = 0x09;
    /// [`Op::DeclareLocal`](super::Op::DeclareLocal) for a `const`.
    pub const DECLARE_CONST: u8 = 0x0a;
    /// [`Op::Pop`](super::Op::Pop).
    pub const POP: u8 = 0x0b;
    /// [`Op::Jump`](super::Op::Jump).
    pub const JUMP: u8 = 0x0c;
    /// [`Op::JumpIfTrue`](super::Op::JumpIfTrue).
    pub const JUMP_IF_TRUE: u8 = 0x0d;
    /// [`Op::JumpIfFalse`](super::Op::JumpIfFalse).
    pub const JUMP_IF_FALSE: u8 = 0x0e;
    /// [`Op::Call`](super::Op::Call) to an ordinary function.
    pub const CALL: u8 = 0x0f;
    /// [`Op::Call`](super::Op::Call) to an operator.
    pub const CALL_OP: u8 = 0x10;
    /// [`Op::UnwindTo`](super::Op::UnwindTo).
    pub const UNWIND_TO: u8 = 0x11;
    /// [`Op::Tick`](super::Op::Tick).
    pub const TICK: u8 = 0x12;
    /// [`Op::Return`](super::Op::Return).
    pub const RETURN: u8 = 0x13;
    /// [`Op::EvalAst`](super::Op::EvalAst) that rewinds the scope.
    pub const EVAL_AST: u8 = 0x14;
    /// [`Op::EvalAst`](super::Op::EvalAst) that keeps what it declared.
    pub const EVAL_AST_KEEP: u8 = 0x15;
    /// [`Op::Chain`](super::Op::Chain).
    pub const CHAIN: u8 = 0x16;
    /// [`Op::MakeArray`](super::Op::MakeArray).
    pub const MAKE_ARRAY: u8 = 0x17;
    /// [`Op::Switch`](super::Op::Switch).
    pub const SWITCH: u8 = 0x18;
    /// [`Op::LoadNamed`](super::Op::LoadNamed).
    pub const LOAD_NAMED: u8 = 0x19;
    /// [`Op::AssignNamed`](super::Op::AssignNamed) with a plain `=`.
    pub const ASSIGN_NAMED: u8 = 0x1a;
    /// [`Op::AssignNamed`](super::Op::AssignNamed) through an operator.
    pub const ASSIGN_NAMED_OP: u8 = 0x1b;
    /// [`Op::Throw`](super::Op::Throw).
    pub const THROW: u8 = 0x1c;
    /// [`Op::IterInit`](super::Op::IterInit).
    pub const ITER_INIT: u8 = 0x1d;
    /// [`Op::IterNext`](super::Op::IterNext).
    pub const ITER_NEXT: u8 = 0x1e;
    /// [`Op::IterDrop`](super::Op::IterDrop).
    pub const ITER_DROP: u8 = 0x1f;
    /// [`Op::StoreShared`](super::Op::StoreShared).
    pub const STORE_SHARED: u8 = 0x20;
    /// [`Op::IterNext`](super::Op::IterNext) that also pushes the count.
    pub const ITER_NEXT_INDEXED: u8 = 0x21;
    /// [`Op::PopHandler`](super::Op::PopHandler).
    pub const POP_HANDLER: u8 = 0x22;
    /// [`Op::PushHandler`](super::Op::PushHandler) for a bare `catch`.
    pub const PUSH_HANDLER: u8 = 0x23;
    /// [`Op::PushHandler`](super::Op::PushHandler) binding a catch variable.
    pub const PUSH_HANDLER_VAR: u8 = 0x24;
    /// [`Op::InterpolateStart`](super::Op::InterpolateStart).
    pub const INTERPOLATE_START: u8 = 0x25;
    /// [`Op::InterpolateAppend`](super::Op::InterpolateAppend).
    pub const INTERPOLATE_APPEND: u8 = 0x26;
    /// [`Op::InterpolateEnd`](super::Op::InterpolateEnd).
    pub const INTERPOLATE_END: u8 = 0x27;
    /// [`Op::MakeFnPtr`](super::Op::MakeFnPtr).
    pub const MAKE_FN_PTR: u8 = 0x28;
    /// [`Op::Curry`](super::Op::Curry).
    pub const CURRY: u8 = 0x29;
    /// [`Op::CallFnPtr`](super::Op::CallFnPtr) in call position.
    pub const CALL_FN_PTR: u8 = 0x2a;
    /// [`Op::CallFnPtr`](super::Op::CallFnPtr) in method position.
    pub const CALL_FN_PTR_METHOD: u8 = 0x2b;
    /// [`Op::Share`](super::Op::Share).
    pub const SHARE: u8 = 0x2c;
    /// [`Op::ShareNamed`](super::Op::ShareNamed).
    pub const SHARE_NAMED: u8 = 0x2d;
    /// [`Op::LoadShared`](super::Op::LoadShared).
    pub const LOAD_SHARED: u8 = 0x2e;
    /// [`Op::MakeClosure`](super::Op::MakeClosure).
    pub const MAKE_CLOSURE: u8 = 0x2f;
    /// [`Op::IsShared`](super::Op::IsShared).
    pub const IS_SHARED: u8 = 0x30;
    /// [`Op::Checkpoint`](super::Op::Checkpoint).
    pub const CHECKPOINT: u8 = 0x31;
    /// [`Op::CheckSize`](super::Op::CheckSize) against the array limit.
    pub const CHECK_ARRAY_SIZE: u8 = 0x32;
    /// [`Op::CheckSize`](super::Op::CheckSize) against the map limit.
    pub const CHECK_MAP_SIZE: u8 = 0x33;
    /// [`Op::MakeMap`](super::Op::MakeMap).
    pub const MAKE_MAP: u8 = 0x34;
    /// [`Op::CallRef`](super::Op::CallRef) through [`Receiver::Local`](super::Receiver::Local).
    pub const CALL_LOCAL_REF: u8 = 0x35;
    /// [`Op::Rotate`](super::Op::Rotate).
    pub const ROTATE: u8 = 0x36;
    /// [`Op::CallRef`](super::Op::CallRef) through [`Receiver::Named`](super::Receiver::Named).
    pub const CALL_NAMED_REF: u8 = 0x37;
    /// [`Op::LoadSharedNamed`](super::Op::LoadSharedNamed).
    pub const LOAD_SHARED_NAMED: u8 = 0x38;
}

/// How wide each tag's instruction is, with 0 for the tags that are not one.
///
/// A table rather than a match because the dispatch loop needs the width of
/// every instruction it executes: matching on the tag twice, once to advance
/// and once to act, is a branch per instruction bought for nothing.
static WIDTHS: [u8; 256] = {
    let mut widths = [0u8; 256];

    widths[tag::UNIT as usize] = 1;
    widths[tag::FALSE as usize] = 1;
    widths[tag::TRUE as usize] = 1;
    widths[tag::POP as usize] = 1;
    widths[tag::TICK as usize] = 1;
    widths[tag::CHECKPOINT as usize] = 1;
    widths[tag::MAKE_MAP as usize] = 3;
    widths[tag::CHECK_ARRAY_SIZE as usize] = 3;
    widths[tag::CHECK_MAP_SIZE as usize] = 3;
    widths[tag::RETURN as usize] = 1;
    widths[tag::THROW as usize] = 1;
    widths[tag::ITER_INIT as usize] = 1;
    widths[tag::ITER_DROP as usize] = 1;

    widths[tag::STORE_SHARED as usize] = 3;

    widths[tag::ITER_NEXT as usize] = 5;
    widths[tag::ITER_NEXT_INDEXED as usize] = 5;
    widths[tag::POP_HANDLER as usize] = 1;
    widths[tag::INTERPOLATE_START as usize] = 1;
    widths[tag::INTERPOLATE_APPEND as usize] = 1;
    widths[tag::INTERPOLATE_END as usize] = 1;
    widths[tag::MAKE_FN_PTR as usize] = 1;
    widths[tag::IS_SHARED as usize] = 1;

    widths[tag::CURRY as usize] = 2;
    widths[tag::ROTATE as usize] = 2;
    widths[tag::CALL_FN_PTR as usize] = 2;
    widths[tag::CALL_FN_PTR_METHOD as usize] = 2;

    widths[tag::SHARE as usize] = 3;
    widths[tag::SHARE_NAMED as usize] = 3;
    widths[tag::LOAD_SHARED as usize] = 3;
    widths[tag::LOAD_SHARED_NAMED as usize] = 3;
    widths[tag::MAKE_CLOSURE as usize] = 3;
    widths[tag::PUSH_HANDLER as usize] = 5;
    widths[tag::PUSH_HANDLER_VAR as usize] = 7;

    widths[tag::CONST as usize] = 3;
    widths[tag::LOAD_LOCAL as usize] = 3;
    widths[tag::STORE_LOCAL as usize] = 3;
    widths[tag::DECLARE_LOCAL as usize] = 3;
    widths[tag::DECLARE_CONST as usize] = 3;
    widths[tag::UNWIND_TO as usize] = 3;
    widths[tag::EVAL_AST as usize] = 3;
    widths[tag::EVAL_AST_KEEP as usize] = 3;
    widths[tag::CHAIN as usize] = 3;
    widths[tag::MAKE_ARRAY as usize] = 3;
    widths[tag::SWITCH as usize] = 3;
    widths[tag::LOAD_NAMED as usize] = 3;
    widths[tag::ASSIGN_NAMED as usize] = 3;

    widths[tag::ASSIGN_NAMED_OP as usize] = 5;

    widths[tag::CALL as usize] = 4;

    widths[tag::ASSIGN_LOCAL as usize] = 5;
    widths[tag::JUMP as usize] = 5;
    widths[tag::JUMP_IF_TRUE as usize] = 5;
    widths[tag::JUMP_IF_FALSE as usize] = 5;

    widths[tag::CALL_OP as usize] = 6;
    widths[tag::CALL_LOCAL_REF as usize] = 6;
    widths[tag::CALL_NAMED_REF as usize] = 6;

    widths[tag::ASSIGN_LOCAL_OP as usize] = 7;

    widths
};

/// How many bytes the instruction at `at` occupies, or `None` if the tag is
/// unknown or the operands run past the end.
#[must_use]
#[inline]
pub fn width(code: &[u8], at: usize) -> Option<usize> {
    let size = WIDTHS[*code.get(at)? as usize] as usize;
    if size == 0 {
        return None;
    }
    // An instruction whose operands are cut off is not an instruction.
    (at + size <= code.len()).then_some(size)
}

/// Read a `u16` operand at `at`.
///
/// Returns `None` past the end rather than panicking. The verifier makes that
/// unreachable for any program the VM will run, but the check is a load's worth
/// of cost and it means nothing has to be trusted.
#[must_use]
#[inline]
pub fn u16_at(code: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(code.get(at..at + 2)?.try_into().ok()?))
}

/// Read a `u32` operand at `at`.
#[must_use]
#[inline]
pub fn u32_at(code: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(code.get(at..at + 4)?.try_into().ok()?))
}

/// Why a lowering could not be turned into bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssembleError {
    /// A pool grew past what a `u16` operand can name. The compiler falls back
    /// to a whole-program fragment rather than emitting a truncated index.
    PoolTooLarge {
        /// Which pool overflowed
        what: &'static str,
        /// How many entries it holds
        entries: usize,
    },
    /// A jump naming an instruction that does not exist. A compiler bug.
    JumpOutOfRange {
        /// Index of the jump instruction
        at: usize,
        /// The instruction index it names
        target: u32,
    },
    /// The same, for a jump that lives in a switch table rather than in the
    /// code.
    SwitchTargetOutOfRange {
        /// Index into the switch pool
        table: usize,
        /// The instruction index the entry names
        target: u32,
    },
    /// A chunk longer than a `u32` of bytes.
    ChunkTooLarge {
        /// How long the chunk is
        bytes: usize,
    },
}

/// Pack instructions into their executable form.
///
/// Two passes: the first measures each instruction so jump targets can be
/// turned from indices into byte offsets, the second writes. Also returns the
/// byte offset of each instruction, so the position table can be re-keyed from
/// indices onto addresses.
///
/// # Errors
///
/// [`AssembleError`] for anything that cannot be expressed in the operand
/// widths. All of them are compiler bugs except [`AssembleError::PoolTooLarge`],
/// which a large enough script can reach.
pub fn assemble(ops: &[Op]) -> Result<(Vec<u8>, Vec<u32>), AssembleError> {
    let mut offsets = Vec::with_capacity(ops.len() + 1);
    let mut at = 0u32;
    for op in ops {
        offsets.push(at);
        at = at
            .checked_add(encoded_width(op) as u32)
            .ok_or(AssembleError::ChunkTooLarge { bytes: usize::MAX })?;
    }
    // One past the end, so a jump to "after the last instruction" resolves.
    offsets.push(at);

    let mut code = Vec::with_capacity(at as usize);
    for (index, op) in ops.iter().enumerate() {
        let target = |target: u32| -> Result<u32, AssembleError> {
            offsets
                .get(target as usize)
                .copied()
                .ok_or(AssembleError::JumpOutOfRange { at: index, target })
        };

        let small = |value: usize, what: &'static str| -> Result<u16, AssembleError> {
            u16::try_from(value).map_err(|_| AssembleError::PoolTooLarge {
                what,
                entries: value,
            })
        };

        match op {
            Op::Const(index) => {
                code.push(tag::CONST);
                code.extend_from_slice(&small(*index as usize, "constants")?.to_le_bytes());
            }
            Op::Unit => code.push(tag::UNIT),
            Op::Bool(false) => code.push(tag::FALSE),
            Op::Bool(true) => code.push(tag::TRUE),

            Op::LoadLocal(slot) => {
                code.push(tag::LOAD_LOCAL);
                code.extend_from_slice(&slot.to_le_bytes());
            }
            Op::StoreLocal(slot) => {
                code.push(tag::STORE_LOCAL);
                code.extend_from_slice(&slot.to_le_bytes());
            }

            Op::AssignLocal {
                slot,
                var_name,
                op: None,
            } => {
                code.push(tag::ASSIGN_LOCAL);
                code.extend_from_slice(&slot.to_le_bytes());
                code.extend_from_slice(&small(*var_name as usize, "names")?.to_le_bytes());
            }
            Op::AssignLocal {
                slot,
                var_name,
                op: Some(assign_op),
            } => {
                code.push(tag::ASSIGN_LOCAL_OP);
                code.extend_from_slice(&slot.to_le_bytes());
                code.extend_from_slice(&small(*var_name as usize, "names")?.to_le_bytes());
                code.extend_from_slice(
                    &small(*assign_op as usize, "op-assignments")?.to_le_bytes(),
                );
            }

            Op::LoadNamed(name) => {
                code.push(tag::LOAD_NAMED);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
            }

            Op::AssignNamed { name, op: None } => {
                code.push(tag::ASSIGN_NAMED);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
            }
            Op::AssignNamed {
                name,
                op: Some(assign_op),
            } => {
                code.push(tag::ASSIGN_NAMED_OP);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
                code.extend_from_slice(
                    &small(*assign_op as usize, "op-assignments")?.to_le_bytes(),
                );
            }

            Op::DeclareLocal { name, is_const } => {
                code.push(if *is_const {
                    tag::DECLARE_CONST
                } else {
                    tag::DECLARE_LOCAL
                });
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
            }

            Op::Pop => code.push(tag::POP),

            Op::Jump(to) => {
                code.push(tag::JUMP);
                code.extend_from_slice(&target(*to)?.to_le_bytes());
            }
            Op::JumpIfTrue { target: to } => {
                code.push(tag::JUMP_IF_TRUE);
                code.extend_from_slice(&target(*to)?.to_le_bytes());
            }
            Op::JumpIfFalse { target: to } => {
                code.push(tag::JUMP_IF_FALSE);
                code.extend_from_slice(&target(*to)?.to_le_bytes());
            }

            Op::Call {
                name,
                argc,
                op: None,
            } => {
                code.push(tag::CALL);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
                code.push(*argc);
            }
            Op::Call {
                name,
                argc,
                op: Some(token),
            } => {
                code.push(tag::CALL_OP);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
                code.push(*argc);
                code.extend_from_slice(&small(*token as usize, "operators")?.to_le_bytes());
            }

            Op::CallRef {
                name,
                argc,
                receiver,
            } => {
                let (tag, operand) = match receiver {
                    Receiver::Local(slot) => (tag::CALL_LOCAL_REF, *slot),
                    Receiver::Named(var) => (tag::CALL_NAMED_REF, small(*var as usize, "names")?),
                };
                code.push(tag);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
                code.push(*argc);
                code.extend_from_slice(&operand.to_le_bytes());
            }

            Op::Rotate(under) => {
                code.push(tag::ROTATE);
                code.push(*under);
            }

            Op::Chain(index) => {
                code.push(tag::CHAIN);
                code.extend_from_slice(&small(*index as usize, "chains")?.to_le_bytes());
            }

            Op::MakeArray(len) => {
                code.push(tag::MAKE_ARRAY);
                code.extend_from_slice(&len.to_le_bytes());
            }

            Op::MakeMap(len) => {
                code.push(tag::MAKE_MAP);
                code.extend_from_slice(&len.to_le_bytes());
            }

            Op::CheckSize { index, map } => {
                code.push(if *map {
                    tag::CHECK_MAP_SIZE
                } else {
                    tag::CHECK_ARRAY_SIZE
                });
                code.extend_from_slice(&index.to_le_bytes());
            }

            Op::Share(slot) => {
                code.push(tag::SHARE);
                code.extend_from_slice(&slot.to_le_bytes());
            }
            Op::ShareNamed(name) => {
                code.push(tag::SHARE_NAMED);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
            }
            Op::LoadShared(slot) => {
                code.push(tag::LOAD_SHARED);
                code.extend_from_slice(&slot.to_le_bytes());
            }
            Op::LoadSharedNamed(name) => {
                code.push(tag::LOAD_SHARED_NAMED);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
            }

            Op::MakeClosure(name) => {
                code.push(tag::MAKE_CLOSURE);
                code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
            }
            Op::MakeFnPtr => code.push(tag::MAKE_FN_PTR),
            Op::IsShared => code.push(tag::IS_SHARED),
            Op::Curry(argc) => {
                code.push(tag::CURRY);
                code.push(*argc);
            }
            Op::CallFnPtr { argc, method } => {
                code.push(if *method {
                    tag::CALL_FN_PTR_METHOD
                } else {
                    tag::CALL_FN_PTR
                });
                code.push(*argc);
            }

            Op::InterpolateStart => code.push(tag::INTERPOLATE_START),
            Op::InterpolateAppend => code.push(tag::INTERPOLATE_APPEND),
            Op::InterpolateEnd => code.push(tag::INTERPOLATE_END),

            // The table's own targets are instruction indices too, but they
            // are not in the code — see `resolve_switch_targets`.
            Op::Switch(index) => {
                code.push(tag::SWITCH);
                code.extend_from_slice(&small(*index as usize, "switches")?.to_le_bytes());
            }

            Op::UnwindTo(depth) => {
                code.push(tag::UNWIND_TO);
                code.extend_from_slice(&depth.to_le_bytes());
            }

            Op::Tick => code.push(tag::TICK),
            Op::Checkpoint => code.push(tag::CHECKPOINT),
            Op::Throw => code.push(tag::THROW),
            Op::IterInit => code.push(tag::ITER_INIT),
            Op::IterDrop => code.push(tag::ITER_DROP),
            Op::PopHandler => code.push(tag::POP_HANDLER),

            Op::PushHandler {
                target: to,
                catch_var,
            } => {
                code.push(match catch_var {
                    Some(..) => tag::PUSH_HANDLER_VAR,
                    None => tag::PUSH_HANDLER,
                });
                code.extend_from_slice(&target(*to)?.to_le_bytes());
                if let Some(name) = catch_var {
                    code.extend_from_slice(&small(*name as usize, "names")?.to_le_bytes());
                }
            }

            Op::IterNext { exit, indexed } => {
                code.push(if *indexed {
                    tag::ITER_NEXT_INDEXED
                } else {
                    tag::ITER_NEXT
                });
                code.extend_from_slice(&target(*exit)?.to_le_bytes());
            }

            Op::StoreShared(slot) => {
                code.push(tag::STORE_SHARED);
                code.extend_from_slice(&slot.to_le_bytes());
            }
            Op::Return => code.push(tag::RETURN),

            Op::EvalAst {
                residual,
                rewind_scope,
            } => {
                code.push(if *rewind_scope {
                    tag::EVAL_AST
                } else {
                    tag::EVAL_AST_KEEP
                });
                code.extend_from_slice(&small(*residual as usize, "fragments")?.to_le_bytes());
            }
        }
    }

    Ok((code, offsets))
}

/// Rewrite switch targets from instruction indices into byte offsets.
///
/// The other half of [`assemble`], and separate only because a table is not in
/// the instruction stream: [`Op::Switch`] carries a pool index, and the jumps
/// are inside the pool entry. Same `offsets` table, same one-past-the-end
/// entry, so a `switch` whose default is the end of the chunk resolves.
///
/// # Errors
///
/// [`AssembleError::SwitchTargetOutOfRange`] for a target naming no
/// instruction, which is a compiler bug.
pub fn resolve_switch_targets(
    switches: &mut [super::Switch],
    offsets: &[u32],
) -> Result<(), AssembleError> {
    for (table, switch) in switches.iter_mut().enumerate() {
        let resolve = |target: &mut u32| -> Result<(), AssembleError> {
            *target =
                *offsets
                    .get(*target as usize)
                    .ok_or(AssembleError::SwitchTargetOutOfRange {
                        table,
                        target: *target,
                    })?;
            Ok(())
        };

        for case in &mut switch.cases {
            resolve(&mut case.target)?;
        }
        for range in &mut switch.ranges {
            resolve(&mut range.target)?;
        }
        resolve(&mut switch.default)?;
    }
    Ok(())
}

/// How many bytes an instruction will take once written.
fn encoded_width(op: &Op) -> usize {
    match op {
        Op::Unit
        | Op::Bool(..)
        | Op::Pop
        | Op::Tick
        | Op::Checkpoint
        | Op::Throw
        | Op::IterInit
        | Op::IterDrop
        | Op::PopHandler
        | Op::InterpolateStart
        | Op::InterpolateAppend
        | Op::InterpolateEnd
        | Op::MakeFnPtr
        | Op::IsShared
        | Op::Return => 1,
        Op::Curry(..) | Op::CallFnPtr { .. } | Op::Rotate(..) => 2,
        Op::Const(..)
        | Op::LoadLocal(..)
        | Op::StoreLocal(..)
        | Op::DeclareLocal { .. }
        | Op::UnwindTo(..)
        | Op::EvalAst { .. }
        | Op::Chain(..)
        | Op::Switch(..)
        | Op::LoadNamed(..)
        | Op::AssignNamed { op: None, .. }
        | Op::StoreShared(..)
        | Op::Share(..)
        | Op::ShareNamed(..)
        | Op::LoadShared(..)
        | Op::LoadSharedNamed(..)
        | Op::MakeClosure(..)
        | Op::MakeArray(..)
        | Op::MakeMap(..)
        | Op::CheckSize { .. } => 3,
        Op::Call { op: None, .. } => 4,
        Op::AssignLocal { op: None, .. }
        | Op::AssignNamed { op: Some(..), .. }
        | Op::Jump(..)
        | Op::JumpIfTrue { .. }
        | Op::JumpIfFalse { .. }
        | Op::IterNext { .. }
        | Op::PushHandler {
            catch_var: None, ..
        } => 5,
        Op::PushHandler {
            catch_var: Some(..),
            ..
        } => 7,
        Op::Call { op: Some(..), .. } | Op::CallRef { .. } => 6,
        Op::AssignLocal { op: Some(..), .. } => 7,
    }
}

/// Recover the instruction at `at`, for disassembly and tests.
///
/// Jump targets come back as byte offsets, not the instruction indices the
/// compiler used, because that is what the code actually holds.
#[must_use]
pub fn decode(code: &[u8], at: usize) -> Option<Op> {
    width(code, at)?;
    let small = |offset: usize| u16_at(code, at + offset);

    Some(match code[at] {
        tag::CONST => Op::Const(u32::from(small(1)?)),
        tag::UNIT => Op::Unit,
        tag::FALSE => Op::Bool(false),
        tag::TRUE => Op::Bool(true),

        tag::LOAD_LOCAL => Op::LoadLocal(small(1)?),
        tag::STORE_LOCAL => Op::StoreLocal(small(1)?),

        tag::ASSIGN_LOCAL => Op::AssignLocal {
            slot: small(1)?,
            var_name: u32::from(small(3)?),
            op: None,
        },
        tag::ASSIGN_LOCAL_OP => Op::AssignLocal {
            slot: small(1)?,
            var_name: u32::from(small(3)?),
            op: Some(u32::from(small(5)?)),
        },

        tag::LOAD_NAMED => Op::LoadNamed(u32::from(small(1)?)),
        tag::ASSIGN_NAMED => Op::AssignNamed {
            name: u32::from(small(1)?),
            op: None,
        },
        tag::ASSIGN_NAMED_OP => Op::AssignNamed {
            name: u32::from(small(1)?),
            op: Some(u32::from(small(3)?)),
        },

        tag::DECLARE_LOCAL => Op::DeclareLocal {
            name: u32::from(small(1)?),
            is_const: false,
        },
        tag::DECLARE_CONST => Op::DeclareLocal {
            name: u32::from(small(1)?),
            is_const: true,
        },

        tag::POP => Op::Pop,

        tag::JUMP => Op::Jump(u32_at(code, at + 1)?),
        tag::JUMP_IF_TRUE => Op::JumpIfTrue {
            target: u32_at(code, at + 1)?,
        },
        tag::JUMP_IF_FALSE => Op::JumpIfFalse {
            target: u32_at(code, at + 1)?,
        },

        tag::CALL => Op::Call {
            name: u32::from(small(1)?),
            argc: code[at + 3],
            op: None,
        },
        tag::CALL_OP => Op::Call {
            name: u32::from(small(1)?),
            argc: code[at + 3],
            op: Some(u32::from(small(4)?)),
        },

        tag::CALL_LOCAL_REF => Op::CallRef {
            name: u32::from(small(1)?),
            argc: code[at + 3],
            receiver: Receiver::Local(small(4)?),
        },
        tag::CALL_NAMED_REF => Op::CallRef {
            name: u32::from(small(1)?),
            argc: code[at + 3],
            receiver: Receiver::Named(u32::from(small(4)?)),
        },
        tag::ROTATE => Op::Rotate(code[at + 1]),

        tag::CHAIN => Op::Chain(u32::from(small(1)?)),
        tag::SWITCH => Op::Switch(u32::from(small(1)?)),
        tag::MAKE_ARRAY => Op::MakeArray(small(1)?),
        tag::MAKE_MAP => Op::MakeMap(small(1)?),
        tag::CHECK_ARRAY_SIZE => Op::CheckSize {
            index: small(1)?,
            map: false,
        },
        tag::CHECK_MAP_SIZE => Op::CheckSize {
            index: small(1)?,
            map: true,
        },
        tag::SHARE => Op::Share(small(1)?),
        tag::SHARE_NAMED => Op::ShareNamed(u32::from(small(1)?)),
        tag::LOAD_SHARED => Op::LoadShared(small(1)?),
        tag::LOAD_SHARED_NAMED => Op::LoadSharedNamed(u32::from(small(1)?)),
        tag::MAKE_CLOSURE => Op::MakeClosure(u32::from(small(1)?)),
        tag::MAKE_FN_PTR => Op::MakeFnPtr,
        tag::IS_SHARED => Op::IsShared,
        tag::CURRY => Op::Curry(code[at + 1]),
        tag::CALL_FN_PTR => Op::CallFnPtr {
            argc: code[at + 1],
            method: false,
        },
        tag::CALL_FN_PTR_METHOD => Op::CallFnPtr {
            argc: code[at + 1],
            method: true,
        },
        tag::INTERPOLATE_START => Op::InterpolateStart,
        tag::INTERPOLATE_APPEND => Op::InterpolateAppend,
        tag::INTERPOLATE_END => Op::InterpolateEnd,
        tag::UNWIND_TO => Op::UnwindTo(small(1)?),
        tag::TICK => Op::Tick,
        tag::CHECKPOINT => Op::Checkpoint,
        tag::THROW => Op::Throw,
        tag::ITER_INIT => Op::IterInit,
        tag::ITER_DROP => Op::IterDrop,
        tag::ITER_NEXT => Op::IterNext {
            exit: u32_at(code, at + 1)?,
            indexed: false,
        },
        tag::ITER_NEXT_INDEXED => Op::IterNext {
            exit: u32_at(code, at + 1)?,
            indexed: true,
        },
        tag::STORE_SHARED => Op::StoreShared(small(1)?),
        tag::POP_HANDLER => Op::PopHandler,
        tag::PUSH_HANDLER => Op::PushHandler {
            target: u32_at(code, at + 1)?,
            catch_var: None,
        },
        tag::PUSH_HANDLER_VAR => Op::PushHandler {
            target: u32_at(code, at + 1)?,
            catch_var: Some(u32::from(small(5)?)),
        },
        tag::RETURN => Op::Return,

        tag::EVAL_AST => Op::EvalAst {
            residual: u32::from(small(1)?),
            rewind_scope: true,
        },
        tag::EVAL_AST_KEEP => Op::EvalAst {
            residual: u32::from(small(1)?),
            rewind_scope: false,
        },

        _ => return None,
    })
}

/// Every instruction in a chunk, paired with its address.
///
/// Stops at the first thing it cannot decode, so it is safe to point at
/// anything. For a chunk that verified, it reaches the end.
pub fn disassemble(code: &[u8]) -> impl Iterator<Item = (usize, Op)> + '_ {
    let mut at = 0usize;
    core::iter::from_fn(move || {
        let op = decode(code, at)?;
        let here = at;
        at += width(code, at)?;
        Some((here, op))
    })
}

/// A chunk's instructions, owned when compiled and borrowed when loaded.
///
/// Borrowing is the point. A program read from an artifact holds a slice of
/// those bytes and allocates nothing for its code — which is the difference
/// between retaining sixteen bytes an instruction and retaining three.
pub type Code<'a> = Cow<'a, [u8]>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The property everything else rests on: what the compiler emitted is what
    /// comes back, with jumps rewritten from indices to the addresses those
    /// instructions actually landed at.
    #[test]
    fn instructions_survive_assembly() {
        let ops = vec![
            Op::Const(7),
            Op::Unit,
            Op::Bool(true),
            Op::Bool(false),
            Op::LoadLocal(3),
            Op::StoreLocal(4),
            Op::AssignLocal {
                slot: 1,
                var_name: 2,
                op: None,
            },
            Op::AssignLocal {
                slot: 1,
                var_name: 2,
                op: Some(5),
            },
            Op::DeclareLocal {
                name: 8,
                is_const: false,
            },
            Op::DeclareLocal {
                name: 9,
                is_const: true,
            },
            Op::Pop,
            Op::Call {
                name: 1,
                argc: 2,
                op: None,
            },
            Op::Call {
                name: 1,
                argc: 2,
                op: Some(3),
            },
            Op::CallRef {
                name: 1,
                argc: 2,
                receiver: Receiver::Local(4),
            },
            Op::CallRef {
                name: 1,
                argc: 2,
                receiver: Receiver::Named(5),
            },
            Op::Rotate(3),
            Op::UnwindTo(6),
            Op::Tick,
            Op::EvalAst {
                residual: 0,
                rewind_scope: true,
            },
            Op::EvalAst {
                residual: 1,
                rewind_scope: false,
            },
            Op::Return,
        ];

        let (code, offsets) = assemble(&ops).expect("must assemble");
        let back: Vec<_> = disassemble(&code).map(|(_, op)| op).collect();

        assert_eq!(back, ops);
        assert_eq!(offsets.len(), ops.len() + 1);
        assert_eq!(
            *offsets.last().unwrap() as usize,
            code.len(),
            "the trailing offset must be the end of the chunk",
        );
    }

    #[test]
    fn jumps_become_the_addresses_of_the_instructions_they_named() {
        // Index 3 is `Return`, which lands after Const(3) + Unit + Pop.
        let ops = vec![Op::Const(0), Op::Unit, Op::Pop, Op::Return, Op::Jump(3)];
        let (code, offsets) = assemble(&ops).expect("must assemble");

        assert_eq!(offsets[3], 3 + 1 + 1);
        assert_eq!(decode(&code, offsets[4] as usize), Some(Op::Jump(5)));
    }

    /// The compiler emits a jump to "one past the last instruction" when a
    /// block's exit is the end of the chunk.
    #[test]
    fn a_jump_past_the_last_instruction_resolves_to_the_end() {
        let ops = vec![Op::Unit, Op::Jump(2)];
        let (code, _) = assemble(&ops).expect("must assemble");
        assert_eq!(decode(&code, 1), Some(Op::Jump(code.len() as u32)));
    }

    #[test]
    fn a_jump_to_an_instruction_that_does_not_exist_is_refused() {
        assert_eq!(
            assemble(&[Op::Jump(99)]),
            Err(AssembleError::JumpOutOfRange { at: 0, target: 99 }),
        );
    }

    /// Operand widths are the format's hard limit, and the compiler falls back
    /// rather than writing a truncated index.
    #[test]
    fn a_pool_index_too_wide_for_its_operand_is_refused() {
        assert_eq!(
            assemble(&[Op::Const(70_000)]),
            Err(AssembleError::PoolTooLarge {
                what: "constants",
                entries: 70_000,
            }),
        );
    }

    #[test]
    fn an_unknown_tag_has_no_width_and_does_not_decode() {
        assert_eq!(width(&[0xff, 0, 0], 0), None);
        assert_eq!(decode(&[0xff, 0, 0], 0), None);
    }

    /// A truncated operand must not read whatever follows in memory.
    #[test]
    fn an_instruction_cut_short_does_not_decode() {
        assert_eq!(
            width(&[tag::CONST, 0], 0),
            None,
            "one byte of a u16 operand"
        );
        assert_eq!(decode(&[tag::CONST, 0], 0), None);
        assert_eq!(decode(&[tag::JUMP, 0, 0], 0), None);
    }

    /// Disassembly is pointed at untrusted bytes by `dump`, so it stops rather
    /// than running away.
    #[test]
    fn disassembling_junk_terminates() {
        let junk = [0xff; 32];
        assert_eq!(disassemble(&junk).count(), 0);

        let partly_good = [tag::UNIT, tag::POP, 0xff, tag::UNIT];
        assert_eq!(disassemble(&partly_good).count(), 2);
    }
}
