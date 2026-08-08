use crate::grain::bytecode::code::{self, tag};
use crate::grain::bytecode::{Chain, Chunk, Op, Receiver, Root, Step, Switch, Tail};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// What the pools hold, so an instruction's indices can be checked against
/// something.
///
/// Chains and switches come through whole rather than as a count, because both
/// hold things that have to be checked rather than counted: how much operand
/// stack a chain consumes, and where a switch can send control.
#[derive(Debug, Clone, Copy)]
pub struct Pools<'a> {
    /// How many constants there are.
    pub consts: usize,
    /// How many interned names there are.
    pub names: usize,
    /// How many operator tokens there are.
    pub tokens: usize,
    /// How many op-assignments there are.
    pub assign_ops: usize,
    /// How many residual AST fragments there are.
    pub residuals: usize,
    /// The chain pool.
    pub chains: &'a [Chain],
    /// The switch pool.
    pub switches: &'a [Switch],
}

/// Why a chunk was rejected.
///
/// Every variant names something a correct compiler cannot produce, so a
/// failure here is a bug in the compiler or a corrupted artifact — never
/// anything a script can express.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// A tag with no instruction behind it, or one whose operands run past the
    /// end of the chunk.
    Undecodable {
        /// Byte offset of the offending tag
        at: usize,
    },
    /// The last instruction stops short of the end, so the trailing bytes are
    /// not instructions.
    TrailingBytes {
        /// Where the trailing bytes start
        at: usize,
        /// How long the code is
        len: usize,
    },
    /// A chunk names a span the code does not have.
    ChunkOutOfRange {
        /// The chunk's first byte offset
        entry: u32,
        /// One past its last
        end: u32,
        /// How long the code is
        len: usize,
    },
    /// A jump leaves the chunk it is in — including into another chunk, which
    /// would run that function's instructions against this frame's locals.
    JumpOutOfRange {
        /// Byte offset of the jump
        at: usize,
        /// The byte offset it names
        target: u32,
    },
    /// A jump lands inside an instruction rather than on one. Decoding from
    /// there would read an operand's bytes as a tag.
    JumpIntoAnInstruction {
        /// Byte offset of the jump
        at: usize,
        /// The byte offset it names
        target: u32,
    },
    /// Two paths reach the same instruction with different stack depths, so
    /// the depth at that point is not a static property.
    DepthConflict {
        /// Byte offset of the instruction
        at: usize,
        /// The depth already recorded for it
        expected: usize,
        /// The depth the other path arrives with
        found: usize,
    },
    /// An instruction pops more than is on the stack.
    Underflow {
        /// Byte offset of the instruction
        at: usize,
        /// How many values it pops
        need: usize,
        /// How many are on the stack
        have: usize,
    },
    /// Execution can run past the last instruction.
    FallsOffTheEnd,
    /// The chunk claims less stack than it uses.
    StackExceedsDeclared {
        /// The depth actually reached
        needed: usize,
        /// The depth the chunk declares
        declared: u16,
    },
    /// An iterator is dropped where none was made. The compiler pairs these
    /// up lexically; an artifact off a wire has to be asked.
    IteratorUnderflow {
        /// Byte offset of the instruction
        at: usize,
    },
    /// A handler is disarmed where none was armed. A stale handler is worse
    /// than a missing one: the next unrelated error would be caught into an
    /// already-exited `catch` block.
    HandlerUnderflow {
        /// Byte offset of the instruction
        at: usize,
    },
    /// An index into a pool with nothing behind it.
    BadIndex {
        /// Byte offset of the instruction
        at: usize,
        /// Which pool was indexed
        what: &'static str,
        /// The index it used
        index: u32,
    },
}

/// Check that a chunk is internally consistent before running it.
///
/// Two passes. The first decodes straight through, recording where each
/// instruction starts; that is what makes it safe to say a jump target is or is
/// not an instruction, which a reachability walk alone cannot — a jump into the
/// middle of an operand would decode the operand's bytes as a tag and look
/// perfectly reasonable.
///
/// The second is an abstract interpretation over stack depth: walk every
/// reachable instruction, and require that all paths into one agree on how deep
/// the operand stack is. That single property catches the whole class of
/// compiler bugs where one branch of a conditional leaves a value and the other
/// does not — which is otherwise invisible until a program takes the unlucky
/// path.
///
/// Together they are what makes an artifact safe to execute in place: a chunk
/// that passes cannot underflow the operand stack, jump outside itself, or
/// decode an operand as an instruction. What it does *not* prove is
/// termination — a jump target inside the chunk is well-formed whether or not
/// it closes a loop — which is why [`Op::Tick`] sits on every back edge and why
/// a host running untrusted bytecode still needs `max_operations`.
///
/// Returns the measured stack high water, which is what the chunk should
/// declare.
pub fn verify(code: &[u8], chunks: &[Chunk], pools: Pools) -> Result<Vec<u16>, VerifyError> {
    // Pass one: where do instructions start?
    //
    // Over the whole buffer at once, because every chunk shares it and an
    // instruction boundary is a property of the bytes, not of who runs them.
    let mut starts = vec![false; code.len() + 1];
    let mut at = 0usize;
    while at < code.len() {
        starts[at] = true;
        let width = code::width(code, at).ok_or(VerifyError::Undecodable { at })?;
        check_indices(at, code, pools)?;
        at += width;
    }
    if at != code.len() {
        return Err(VerifyError::TrailingBytes {
            at,
            len: code.len(),
        });
    }

    chunks
        .iter()
        .map(|chunk| verify_chunk(code, chunk, &starts, pools))
        .collect()
}

/// What every path into an instruction has to agree on.
///
/// The operand stack is the obvious one. The iterator stack is here for the
/// same reason: a `for` loop's iterator lives on a stack of the VM's own, and
/// a chunk that leaves one behind — or drops one it never made — is a chunk
/// whose loops are not the shape the compiler thought. "The compiler balances
/// them" is exactly the sort of claim a verifier for untrusted bytecode exists
/// to check rather than take on trust.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct State {
    operands: usize,
    iters: usize,
    handlers: usize,
}

/// Walk one chunk's reachable instructions, checking that every path into an
/// instruction agrees on the stack depth.
fn verify_chunk(
    code: &[u8],
    chunk: &Chunk,
    starts: &[bool],
    pools: Pools,
) -> Result<u16, VerifyError> {
    let (entry, end) = (chunk.entry() as usize, chunk.end() as usize);
    if end > code.len() || entry > end {
        return Err(VerifyError::ChunkOutOfRange {
            entry: chunk.entry(),
            end: chunk.end(),
            len: code.len(),
        });
    }

    let mut depth_at: Vec<Option<State>> = vec![None; code.len()];
    let mut worklist = vec![(entry, State::default())];
    let mut high_water = 0usize;

    while let Some((at, state)) = worklist.pop() {
        if at >= end {
            return Err(VerifyError::FallsOffTheEnd);
        }

        // Merge point: either this is the first visit, or every earlier path
        // has to have arrived in the same state.
        match depth_at[at] {
            Some(seen) if seen == state => continue,
            Some(seen) => {
                return Err(VerifyError::DepthConflict {
                    at,
                    expected: seen.operands,
                    found: state.operands,
                })
            }
            None => depth_at[at] = Some(state),
        }

        let depth = state.operands;
        high_water = high_water.max(depth);

        let op = code::decode(code, at).ok_or(VerifyError::Undecodable { at })?;
        let (pops, pushes) = effect(&op, pools);

        if depth < pops {
            return Err(VerifyError::Underflow {
                at,
                need: pops,
                have: depth,
            });
        }
        let next_state = State {
            operands: depth - pops + pushes,
            iters: match op {
                Op::IterInit => state.iters + 1,
                Op::IterDrop => state
                    .iters
                    .checked_sub(1)
                    .ok_or(VerifyError::IteratorUnderflow { at })?,
                _ => state.iters,
            },
            handlers: match op {
                Op::PushHandler { .. } => state.handlers + 1,
                Op::PopHandler => state
                    .handlers
                    .checked_sub(1)
                    .ok_or(VerifyError::HandlerUnderflow { at })?,
                _ => state.handlers,
            },
        };
        let next_depth = next_state.operands;
        high_water = high_water.max(next_depth);

        let width = code::width(code, at).expect("decoded, so it has a width");
        let next = at + width;

        let mut go = |target: u32, state: State| -> Result<(), VerifyError> {
            let target = target as usize;
            // Within this chunk: a jump into another function's body would run
            // its instructions against this frame's locals.
            if target < entry || target >= end {
                return Err(VerifyError::JumpOutOfRange {
                    at,
                    target: target as u32,
                });
            }
            if !starts[target] {
                return Err(VerifyError::JumpIntoAnInstruction {
                    at,
                    target: target as u32,
                });
            }
            worklist.push((target, state));
            Ok(())
        };

        match op {
            // Terminal: nothing follows. A `throw` always fails, so control
            // leaves the chunk here as surely as it does at a `Return`.
            //
            // Neither has to balance the iterator stack: leaving a frame
            // truncates it to what the frame started with.
            Op::Return | Op::Throw => {}

            Op::Jump(target) => go(target, next_state)?,

            // The catch block is entered on the exception path, so it starts
            // where the `try` did: same operand depth, same iterators, and
            // inside the handler it will disarm itself.
            Op::PushHandler { target, .. } => {
                go(
                    target,
                    State {
                        operands: depth,
                        iters: state.iters,
                        handlers: next_state.handlers,
                    },
                )?;
                worklist.push((next, next_state));
            }

            Op::JumpIfFalse { target } | Op::JumpIfTrue { target } => {
                go(target, next_state)?;
                worklist.push((next, next_state));
            }

            // The one instruction whose edges differ in more than where they
            // go: falling through carries the item it pushed and still holds
            // the iterator, while the exit edge has neither.
            Op::IterNext { exit, indexed } => {
                go(
                    exit,
                    State {
                        operands: depth,
                        iters: state
                            .iters
                            .checked_sub(1)
                            .ok_or(VerifyError::IteratorUnderflow { at })?,
                        handlers: state.handlers,
                    },
                )?;
                worklist.push((
                    next,
                    State {
                        // The item, and the count under it when there is one.
                        operands: depth + 1 + usize::from(indexed),
                        iters: state.iters,
                        handlers: state.handlers,
                    },
                ));
            }

            // Terminal like `Jump`, with one successor per arm. A table with
            // no entry behind it is caught by `check_indices`, which has
            // already run over every instruction.
            Op::Switch(index) => {
                if let Some(table) = pools.switches.get(index as usize) {
                    for target in table
                        .cases
                        .iter()
                        .map(|case| case.target)
                        .chain(table.ranges.iter().map(|range| range.target))
                        .chain(core::iter::once(table.default))
                    {
                        go(target, next_state)?;
                    }
                }
            }

            _ => {
                if next >= end {
                    return Err(VerifyError::FallsOffTheEnd);
                }
                worklist.push((next, next_state));
            }
        }
    }

    let Ok(high_water) = u16::try_from(high_water) else {
        return Err(VerifyError::StackExceedsDeclared {
            needed: high_water,
            declared: chunk.max_stack(),
        });
    };
    if high_water > chunk.max_stack() {
        return Err(VerifyError::StackExceedsDeclared {
            needed: high_water as usize,
            declared: chunk.max_stack(),
        });
    }

    Ok(high_water)
}

/// How many operands an instruction consumes and produces.
fn effect(op: &Op, pools: Pools) -> (usize, usize) {
    match op {
        // A chain eats the indices and arguments its steps named, plus a root
        // that is not a slot, plus the value being assigned, and leaves one
        // behind. An index with no chain behind it reads as consuming nothing;
        // `check_indices` is what rejects it.
        Op::Chain(index) => match pools.chains.get(*index as usize) {
            Some(chain) => (chain.consumes(), 1),
            None => (0, 1),
        },

        Op::Const(..)
        | Op::Unit
        | Op::Bool(..)
        | Op::LoadLocal(..)
        | Op::LoadNamed(..)
        | Op::LoadShared(..)
        | Op::LoadSharedNamed(..)
        | Op::MakeClosure(..)
        | Op::EvalAst { .. } => (0, 1),

        // Sharing is a change to the scope, not to the operand stack.
        Op::Share(..) | Op::ShareNamed(..) => (0, 0),

        Op::StoreLocal(..) | Op::DeclareLocal { .. } | Op::Pop => (1, 0),

        // Pops the value, leaves nothing: the statement's unit value is a
        // separate `Op::Unit`.
        Op::AssignLocal { .. } | Op::AssignNamed { .. } => (1, 0),

        Op::JumpIfFalse { .. } | Op::JumpIfTrue { .. } | Op::Switch(..) => (1, 0),

        Op::Jump(..)
        | Op::UnwindTo(..)
        | Op::Tick
        | Op::Checkpoint
        | Op::PushHandler { .. }
        | Op::PopHandler => (0, 0),

        // Arguments in, result out.
        Op::Call { argc, .. } => (*argc as usize, 1),

        // A named receiver's value is argument zero like any other; a local's
        // is not on the stack at all. An `argc` of zero names no receiver and
        // is rejected when it runs.
        Op::CallRef { argc, receiver, .. } => match receiver {
            Receiver::Local(..) => ((*argc as usize).saturating_sub(1), 1),
            Receiver::Named(..) => (*argc as usize, 1),
        },

        // Reorders what is already there, so the depth is unchanged — but it
        // has to reach every one of them, and saying so is what stops a
        // hand-made artifact reaching under the frame.
        Op::Rotate(under) => (*under as usize + 1, *under as usize + 1),

        Op::MakeArray(len) => (*len as usize, 1),

        // A key and a value per entry, and the template underneath them.
        Op::MakeMap(len) => (2 * *len as usize + 1, 1),

        // Measures the element it is standing on without taking it: the
        // literal is still being built and every element it has so far is
        // still on the stack.
        Op::CheckSize { .. } => (1, 1),

        // The buffer is an ordinary operand: started, appended to, then
        // replaced by the string it built.
        // A name in, a pointer out.
        Op::MakeFnPtr | Op::IsShared => (1, 1),
        // The arguments and the pointer itself, leaving one of each.
        Op::Curry(argc) => (*argc as usize + 1, 1),
        Op::CallFnPtr { argc, .. } => (*argc as usize + 1, 1),

        Op::InterpolateStart => (0, 1),
        Op::InterpolateAppend => (1, 0),
        Op::InterpolateEnd => (1, 1),

        // Pops the thrown value; nothing follows, so what it leaves is moot.
        Op::Throw | Op::StoreShared(..) => (1, 0),

        // The iterable goes onto the iterator stack, not back onto this one.
        Op::IterInit => (1, 0),
        // Its two edges disagree, so the successor match does the work.
        Op::IterNext { .. } | Op::IterDrop => (0, 0),

        // Consumes whatever is left, so depth afterwards is not meaningful.
        Op::Return => (0, 0),
    }
}

/// Check that every pool reference resolves.
///
/// The VM treats these as assertions, and an artifact is the one place they can
/// be wrong without a compiler bug. Reads the operands off the bytes rather
/// than off a decoded `Op`, so it runs in the same pass that measures widths.
fn check_indices(at: usize, code: &[u8], pools: Pools) -> Result<(), VerifyError> {
    let index = |offset: usize| code::u16_at(code, at + offset).map_or(0, u32::from);
    let bounded = |index: u32, what: &'static str, len: usize| {
        if index as usize >= len {
            Err(VerifyError::BadIndex { at, what, index })
        } else {
            Ok(())
        }
    };

    match code[at] {
        tag::CONST => bounded(index(1), "constant", pools.consts),
        tag::DECLARE_LOCAL | tag::DECLARE_CONST => bounded(index(1), "name", pools.names),
        tag::CALL | tag::CALL_LOCAL_REF => bounded(index(1), "name", pools.names),
        // The function's, then the receiver variable's. The slot a local
        // receiver names is not a pool index and is checked against the scope
        // when it runs, as every other slot is.
        tag::CALL_NAMED_REF => {
            bounded(index(1), "name", pools.names)?;
            bounded(index(4), "name", pools.names)
        }
        tag::CALL_OP => {
            bounded(index(1), "name", pools.names)?;
            bounded(index(4), "operator", pools.tokens)
        }
        tag::ASSIGN_LOCAL => bounded(index(3), "name", pools.names),
        tag::LOAD_NAMED
        | tag::LOAD_SHARED_NAMED
        | tag::ASSIGN_NAMED
        | tag::SHARE_NAMED
        | tag::MAKE_CLOSURE => bounded(index(1), "name", pools.names),
        tag::ASSIGN_NAMED_OP => {
            bounded(index(1), "name", pools.names)?;
            bounded(index(3), "op-assignment", pools.assign_ops)
        }
        tag::ASSIGN_LOCAL_OP => {
            bounded(index(3), "name", pools.names)?;
            bounded(index(5), "op-assignment", pools.assign_ops)
        }
        tag::EVAL_AST | tag::EVAL_AST_KEEP => bounded(index(1), "fragment", pools.residuals),
        tag::CHAIN => {
            bounded(index(1), "chain", pools.chains.len())?;
            check_chain_indices(at, &pools.chains[index(1) as usize], pools)
        }
        tag::SWITCH => bounded(index(1), "switch", pools.switches.len()),
        _ => Ok(()),
    }
}

/// Check the pool references *inside* a chain record.
///
/// A chain is one instruction over an unbounded record, so nearly all of what
/// it names lives in the pool rather than in the code. Bounding only the
/// record's own index would leave most of the instruction unverified.
fn check_chain_indices(at: usize, chain: &Chain, pools: Pools) -> Result<(), VerifyError> {
    let bounded = |index: u32, what: &'static str, len: usize| {
        if index as usize >= len {
            Err(VerifyError::BadIndex { at, what, index })
        } else {
            Ok(())
        }
    };

    match chain.root {
        Root::Local { name, .. } | Root::Named { name, .. } => {
            bounded(name, "name", pools.names)?;
        }
        Root::Temporary => {}
    }

    for step in &chain.steps {
        match step {
            // Its operands are stack offsets and its positions are its own.
            Step::Index { .. } => {}
            Step::Property {
                name,
                getter,
                setter,
                ..
            } => {
                bounded(*name, "name", pools.names)?;
                bounded(*getter, "name", pools.names)?;
                bounded(*setter, "name", pools.names)?;
            }
            Step::Method { name, .. } => bounded(*name, "name", pools.names)?,
        }
    }

    match chain.tail {
        Tail::Assign { op: Some(op) } => bounded(op, "op-assignment", pools.assign_ops),
        Tail::Assign { op: None } | Tail::Read => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain::bytecode::assemble;

    fn pools() -> Pools<'static> {
        Pools {
            consts: 0,
            names: 0,
            tokens: 0,
            assign_ops: 0,
            residuals: 0,
            chains: &[],
            switches: &[],
        }
    }

    /// Assemble one chunk spanning the whole buffer, and check it.
    fn check(ops: Vec<Op>) -> Result<Vec<u16>, VerifyError> {
        let (code, _) = assemble(&ops).expect("the test ops must assemble");
        let chunk = Chunk::new(0, code.len() as u32, 8);
        verify(&code, &[chunk], pools())
    }

    /// The same, for bytes `assemble` would refuse to produce — which is what
    /// a corrupt artifact hands the loader.
    fn check_bytes(code: Vec<u8>, max_stack: u16) -> Result<Vec<u16>, VerifyError> {
        let chunk = Chunk::new(0, code.len() as u32, max_stack);
        verify(&code, &[chunk], pools())
    }

    #[test]
    fn accepts_a_well_formed_chunk() {
        assert_eq!(check(vec![Op::Unit, Op::Return]), Ok(vec![1]));
    }

    /// A chain is one instruction over an unbounded record, so almost all of it
    /// is in the pool rather than in the code. Checking only the record's own
    /// index would leave the rest of the instruction unverified.
    #[test]
    fn rejects_a_chain_that_names_something_the_pools_do_not_have() {
        let chain = |root, steps, tail| Chain {
            root,
            steps,
            tail,
            operands: 0,
        };
        let property = |name| Step::Property {
            name,
            getter: 0,
            setter: 0,
            pos: rhai::Position::NONE,
        };

        let past_the_end = [
            chain(
                Root::Named {
                    name: 3,
                    pos: rhai::Position::NONE,
                },
                vec![],
                Tail::Read,
            ),
            chain(Root::Local { slot: 0, name: 3 }, vec![], Tail::Read),
            chain(Root::Temporary, vec![property(3)], Tail::Read),
            chain(
                Root::Temporary,
                vec![Step::Method {
                    name: 3,
                    argc: 0,
                    operand: 0,
                    pos: rhai::Position::NONE,
                }],
                Tail::Read,
            ),
        ];

        for chain in past_the_end {
            let temporary = chain.roots_on_stack();
            let mut ops = vec![Op::Chain(0), Op::Return];
            if temporary {
                ops.insert(0, Op::Unit);
            }
            let (code, _) = assemble(&ops).expect("must assemble");
            let chunk = Chunk::new(0, code.len() as u32, 8);
            let pools = Pools {
                names: 1,
                chains: core::slice::from_ref(&chain),
                ..pools()
            };
            assert!(
                matches!(
                    verify(&code, &[chunk], pools),
                    Err(VerifyError::BadIndex {
                        what: "name",
                        index: 3,
                        ..
                    }),
                ),
                "{chain:?} names name 3 of 1 and must be refused",
            );
        }

        // And the op-assignment a tail can carry.
        let assigning = chain(
            Root::Local { slot: 0, name: 0 },
            vec![],
            Tail::Assign { op: Some(2) },
        );
        let (code, _) = assemble(&[Op::Unit, Op::Chain(0), Op::Return]).expect("must assemble");
        let chunk = Chunk::new(0, code.len() as u32, 8);
        assert!(matches!(
            verify(
                &code,
                &[chunk],
                Pools {
                    names: 1,
                    chains: core::slice::from_ref(&assigning),
                    ..pools()
                },
            ),
            Err(VerifyError::BadIndex {
                what: "op-assignment",
                index: 2,
                ..
            }),
        ));
    }

    /// Chunks share one buffer, so a jump from one into another would run the
    /// callee's instructions against the caller's locals.
    #[test]
    fn rejects_a_jump_from_one_chunk_into_another() {
        // Two chunks: `Unit; Return` twice. The first jumps into the second.
        let (mut code, _) = assemble(&[Op::Unit, Op::Return]).unwrap();
        let boundary = code.len() as u32;
        code.push(tag::JUMP);
        code.extend_from_slice(&0u32.to_le_bytes()); // back into chunk one
        code.push(tag::RETURN);

        let chunks = [
            Chunk::new(0, boundary, 8),
            Chunk::new(boundary, code.len() as u32, 8),
        ];
        assert!(matches!(
            verify(&code, &chunks, pools()),
            Err(VerifyError::JumpOutOfRange { .. }),
        ));
    }

    /// The reason the high water is returned rather than merely checked: the
    /// compiler's estimate is one slot per instruction, and the VM reserves
    /// from it.
    #[test]
    fn the_high_water_is_what_the_chunk_uses_not_what_it_declares() {
        assert_eq!(
            check(vec![
                Op::Unit,
                Op::Unit,
                Op::Pop,
                Op::Pop,
                Op::Unit,
                Op::Return
            ]),
            Ok(vec![2]),
        );
    }

    /// The property the verifier exists for: one branch leaves a value, the
    /// other does not, and nothing notices until a program takes the wrong
    /// path at runtime.
    #[test]
    fn rejects_branches_that_disagree_on_depth() {
        let ops = vec![
            Op::Bool(true),
            Op::JumpIfFalse { target: 3 },
            Op::Unit, // the taken path pushes
            Op::Return,
        ];

        assert!(
            matches!(check(ops.clone()), Err(VerifyError::DepthConflict { .. })),
            "a branch imbalance must be rejected, got {:?}",
            check(ops),
        );
    }

    #[test]
    fn rejects_a_jump_off_the_end() {
        // Assembled by hand: `assemble` refuses an index it cannot resolve, so
        // an out-of-range *address* can only come from a corrupt artifact.
        let mut code = vec![tag::JUMP];
        code.extend_from_slice(&99u32.to_le_bytes());
        code.push(tag::RETURN);

        assert!(matches!(
            check_bytes(code, 8),
            Err(VerifyError::JumpOutOfRange { .. }),
        ));
    }

    /// A jump into an operand would read that operand's bytes as a tag, which
    /// is how a byte-addressed chunk goes wrong in a way an index-addressed one
    /// could not.
    #[test]
    fn rejects_a_jump_into_the_middle_of_an_instruction() {
        let mut code = vec![tag::JUMP];
        code.extend_from_slice(&3u32.to_le_bytes()); // lands inside itself
        code.push(tag::RETURN);

        assert_eq!(
            check_bytes(code, 8),
            Err(VerifyError::JumpIntoAnInstruction { at: 0, target: 3 }),
        );
    }

    /// A switch is a jump with many targets, and every one of them needs the
    /// proof an ordinary jump gets — otherwise the one arm nobody tested is
    /// the one that decodes an operand as an opcode.
    #[test]
    fn every_arm_of_a_switch_is_checked() {
        let ops = vec![
            Op::Unit,
            Op::Switch(0),
            Op::Unit, // index 2: the case arm
            Op::Return,
            Op::Unit, // index 4: the default
            Op::Return,
        ];
        let (code, offsets) = assemble(&ops).expect("must assemble");
        let chunk = Chunk::new(0, code.len() as u32, 8);

        let table = |case: u32, default: u32| Switch {
            cases: vec![crate::grain::bytecode::SwitchCase {
                hash: 7,
                target: case,
            }],
            ranges: Vec::new(),
            default,
        };

        let good = [table(offsets[2], offsets[4])];
        assert_eq!(
            verify(
                &code,
                &[chunk],
                Pools {
                    switches: &good,
                    ..pools()
                }
            ),
            Ok(vec![1]),
        );

        // One byte into the `Switch` instruction's own operand.
        let mid = [table(offsets[1] + 1, offsets[4])];
        assert!(
            matches!(
                verify(
                    &code,
                    &[chunk],
                    Pools {
                        switches: &mid,
                        ..pools()
                    }
                ),
                Err(VerifyError::JumpIntoAnInstruction { .. }),
            ),
            "a case arm landing mid-instruction must be refused",
        );

        // The default is a target like any other, and the easiest to forget.
        let outside = [table(offsets[2], 9999)];
        assert!(
            matches!(
                verify(
                    &code,
                    &[chunk],
                    Pools {
                        switches: &outside,
                        ..pools()
                    }
                ),
                Err(VerifyError::JumpOutOfRange { .. }),
            ),
            "a default outside the chunk must be refused",
        );
    }

    #[test]
    fn rejects_popping_an_empty_stack() {
        assert!(matches!(
            check(vec![Op::Pop, Op::Return]),
            Err(VerifyError::Underflow { .. }),
        ));
    }

    /// A rotate reaches under the operands above it, and one frame's operands
    /// sit on the same stack as its caller's. Nothing at run time knows where
    /// the frame started, so the depth it needs is checked here or nowhere.
    #[test]
    fn rejects_a_rotate_that_reaches_below_the_frame() {
        assert!(matches!(
            check(vec![Op::Unit, Op::Unit, Op::Rotate(2), Op::Return]),
            Err(VerifyError::Underflow {
                need: 3,
                have: 2,
                ..
            }),
        ));
        assert_eq!(
            check(vec![
                Op::Unit,
                Op::Unit,
                Op::Unit,
                Op::Rotate(2),
                Op::Return
            ]),
            Ok(vec![3]),
            "with the third operand there it is in range, and nothing moves",
        );
    }

    #[test]
    fn rejects_running_past_the_last_instruction() {
        assert_eq!(check(vec![Op::Unit]), Err(VerifyError::FallsOffTheEnd));
    }

    #[test]
    fn rejects_an_index_with_nothing_behind_it() {
        let (code, _) = assemble(&[Op::Const(7), Op::Return]).unwrap();
        let chunk = Chunk::new(0, code.len() as u32, 8);
        assert!(matches!(
            verify(
                &code,
                &[chunk],
                Pools {
                    consts: 1,
                    ..pools()
                }
            ),
            Err(VerifyError::BadIndex {
                what: "constant",
                ..
            }),
        ));
    }

    #[test]
    fn rejects_a_chunk_that_outgrows_its_declared_stack() {
        let (code, _) = assemble(&[Op::Unit, Op::Unit, Op::Unit, Op::Return]).unwrap();
        assert!(matches!(
            check_bytes(code, 2),
            Err(VerifyError::StackExceedsDeclared { .. }),
        ));
    }

    /// Bytes that are not instructions must be named as such rather than
    /// executed.
    #[test]
    fn rejects_a_tag_it_does_not_know() {
        assert_eq!(
            check_bytes(vec![0xff], 8),
            Err(VerifyError::Undecodable { at: 0 }),
        );
    }

    #[test]
    fn rejects_an_instruction_whose_operands_are_cut_off() {
        assert_eq!(
            check_bytes(vec![tag::CONST, 0], 8),
            Err(VerifyError::Undecodable { at: 0 }),
        );
    }

    /// A chunk naming a span the code does not have is a corrupt artifact, not
    /// a compiler bug — and must not index out of bounds.
    #[test]
    fn rejects_a_chunk_that_names_code_it_does_not_have() {
        let (code, _) = assemble(&[Op::Unit, Op::Return]).unwrap();
        assert!(matches!(
            verify(&code, &[Chunk::new(0, 9999, 8)], pools()),
            Err(VerifyError::ChunkOutOfRange { .. }),
        ));
    }
}
