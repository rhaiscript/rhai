use crate::grain::Program;

use bitflags::bitflags;

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

bitflags! {
    /// Per-step flags for a chain.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct StepFlags: u8 {
        /// The step short-circuits when the receiver is `()`.
        const SKIP_IF_UNIT = 0b_0000_0001;
    }
}

/// One step along `a.b[i].c(x)`.
///
/// Steps live in the program's chain pool rather than in the instruction
/// stream, because a chain is walked by one instruction rather than several.
/// It has to be: the walk holds a `&mut` into the container at every level, and
/// a borrow cannot survive a trip round the dispatch loop. That is also what
/// makes it correct — Rhai holds the same references, so a mutation partway
/// down a chain lands in the same place rather than in a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// `[i]`, where the index was evaluated onto the operand stack before the
    /// chain instruction ran, at `operand` from the first of them.
    ///
    /// Pre-evaluating mirrors Rhai, which collects every index in a chain into
    /// `idx_values` before walking it (`eval/chaining.rs:568`). It has to
    /// happen first: evaluating an index halfway down would need the operand
    /// stack while a borrow of the container is live.
    ///
    /// Rhai reports `a[10]` out of bounds against the `10` rather than against
    /// the chain (`eval/chaining.rs:694`).
    ///
    /// `bracket` is the other position Rhai keeps for a step, and the two are
    /// not interchangeable: `pos` is where the index expression starts and
    /// `bracket` is the `[` in front of it (`op_pos`, `eval/chaining.rs:695`).
    /// An out-of-bounds index is blamed on the first and indexing something
    /// that cannot be indexed on the second, so `a[0][5]` where `a[0]` is not
    /// indexable names the *second* `[` — the step that failed — and a chain
    /// carrying one position between it and its neighbours would name the
    /// wrong one.
    Index {
        /// Where the index sits on the operand stack.
        operand: u16,
        /// Per-step flags, such as null-conditional chaining.
        flags: StepFlags,
        /// Where the index expression starts.
        pos: rhai::Position,
        /// The `[` in front of it.
        bracket: rhai::Position,
    },

    /// `.name`, which is a key lookup on a map and a getter call on anything
    /// else — the distinction Rhai makes at runtime, not at parse time
    /// (`eval/chaining.rs:898`).
    Property {
        /// The bare name, for a map key and for error messages.
        name: u32,
        /// `get$name`.
        getter: u32,
        /// `set$name`, for the write-back.
        setter: u32,
        /// Per-step flags, such as null-conditional chaining.
        flags: StepFlags,
        /// Where the property is in the source.
        pos: rhai::Position,
    },

    /// `.name(args)`, with the receiver as the first argument by reference.
    Method {
        /// The name of the method
        name: u32,
        /// How many arguments, not counting the receiver.
        argc: u8,
        /// Where the first of them sits on the operand stack.
        operand: u16,
        /// Per-step flags, such as null-conditional chaining.
        flags: StepFlags,
        /// Where the call is in the source.
        pos: rhai::Position,
    },
}

impl Step {
    /// Where this step is in the source.
    ///
    /// In the chain pool rather than the position table, because a chain is one
    /// instruction and its single table entry cannot say which of `a.b[i].c()`
    /// failed. Rhai blames the step for all three kinds: an index against its
    /// index expression, a property against the property
    /// (`eval/chaining.rs:1039`), a method against the call (`:904`).
    #[must_use]
    pub fn pos(&self) -> rhai::Position {
        match self {
            Step::Index { pos, .. } | Step::Property { pos, .. } | Step::Method { pos, .. } => *pos,
        }
    }

    /// How many source positions this step carries.
    ///
    /// Two for an index, which keeps the `[` apart from what is inside it.
    #[must_use]
    pub fn position_slots(&self) -> u32 {
        match self {
            Step::Index { .. } => 2,
            Step::Property { .. } | Step::Method { .. } => 1,
        }
    }

    /// Dump the disassembly of the step.
    pub fn disassemble(&self, program: &Program) -> String {
        match self {
            Step::Index { operand, flags, .. } => {
                format!(
                    "{}[{operand}]",
                    if flags.contains(StepFlags::SKIP_IF_UNIT) {
                        "?"
                    } else {
                        ""
                    }
                )
            }
            Step::Property { name, flags, .. } => {
                format!(
                    ".{}{}",
                    if flags.contains(StepFlags::SKIP_IF_UNIT) {
                        "?"
                    } else {
                        ""
                    },
                    program.name(*name).unwrap()
                )
            }
            Step::Method {
                name,
                argc,
                operand,
                flags,
                ..
            } if *argc > 0 => format!(
                ".{}{}({argc} args from {operand})",
                if flags.contains(StepFlags::SKIP_IF_UNIT) {
                    "?"
                } else {
                    ""
                },
                program.name(*name).unwrap()
            ),
            Step::Method { name, flags, .. } => format!(
                ".{}{}()",
                if flags.contains(StepFlags::SKIP_IF_UNIT) {
                    "?"
                } else {
                    ""
                },
                program.name(*name).unwrap()
            ),
        }
    }
}

/// What a chain does when it gets to the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tail {
    /// Push the value the chain arrived at.
    Read,
    /// Assign the top of the operand stack to it, optionally through an
    /// operator, and push unit.
    Assign {
        /// Index into the op-assignment pool; absent for a plain `=`.
        op: Option<u32>,
    },
}

impl Tail {
    /// Dump the disassembly of the tail.
    pub fn disassemble(&self, program: &Program) -> String {
        match self {
            Tail::Read => String::new(),
            Tail::Assign { op } => match op {
                Some(op) => format!("{}", program.assign_op(*op).unwrap().disassemble(program)),
                None => "=".to_string(),
            },
        }
    }
}

/// Where a chain starts.
///
/// The distinction is whether the root has an identity to write back into.
/// Rhai draws the same line and in the same place: a variable root becomes a
/// `Target` into the scope entry, and anything else is evaluated into a
/// temporary and walked there (`eval/chaining.rs:547-571`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Root {
    /// A local, by slot.
    ///
    /// The only root a chain can write *through*: a mutation partway down has
    /// to land in the scope entry, and walking a copy of it would lose the
    /// mutation.
    Local {
        /// The slot index
        slot: u16,
        /// Names it in `ErrorAssignmentToConstant`.
        name: u32,
    },

    /// A variable no slot addresses: one the caller put in the `Scope`, one a
    /// resolver answers for, or a module's constant.
    ///
    /// Whether it can be written through is not known until it is looked up,
    /// which is the whole difference from [`Root::Local`]. Rhai decides the
    /// same way and at the same moment: `search_namespace` hands back a
    /// `Target`, and a scope entry becomes a reference where a resolver's
    /// answer or a module's constant becomes a read-only temporary
    /// (`eval/expr.rs:120-155`).
    ///
    /// Carries its own position because the lookup can fail and
    /// `ErrorVariableNotFound` is reported against the variable, not the
    /// chain. That costs nothing extra: chain positions already live in this
    /// pool rather than in the stripping table, for the reason [`Step::pos`]
    /// gives.
    Named {
        /// The name of the variable
        name: u32,
        /// Where the variable is in the source.
        pos: rhai::Position,
    },

    /// The frame's receiver.
    ///
    /// Grouped with the two above rather than with [`Root::Temporary`], and the
    /// distinction is the whole reason this variant exists: `this.push(1)` has
    /// to mutate the caller's value, and a temporary would walk a copy and drop
    /// the mutation silently.
    ///
    /// Carries its own position because the chain instruction's table entry is
    /// the `.` or the `[`, while `ErrorUnboundThis` is reported against the
    /// `this` (`eval/chaining.rs:519-527`) — two positions one instruction
    /// cannot give. [`Root::Named`] carries one for the same reason.
    This {
        /// Where the `this` is in the source.
        pos: rhai::Position,
    },

    /// A value the instruction takes off the operand stack, pushed above the
    /// step operands.
    ///
    /// `[1, 2, 3].len()`, `f().x`, `(a + b).to_string()`. Nothing is written
    /// back, because there is nowhere to write it back to — and nothing can be
    /// assigned to one, because Rhai's parser refuses `f().x = 1` before this
    /// ever sees it (`eval/chaining.rs:559`).
    Temporary,
}

impl Root {
    /// How many source positions this root carries.
    ///
    /// Only the two that can fail on their own; a slot and a temporary have
    /// nothing to report against.
    #[must_use]
    pub fn position_slots(&self) -> u32 {
        match self {
            Root::Named { .. } | Root::This { .. } => 1,
            Root::Local { .. } | Root::Temporary => 0,
        }
    }
}

/// A whole `a.b[i].c` chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    /// Where the chain starts.
    pub root: Root,
    /// The steps, in source order.
    pub steps: Vec<Step>,
    /// What happens at the end.
    pub tail: Tail,
    /// How many operand-stack values the *steps* consume, so the VM knows
    /// where they start. Not the whole instruction's appetite — see
    /// [`Chain::consumes`].
    pub operands: u16,
}

impl Chain {
    /// Whether the chain takes a value off the operand stack to store.
    #[must_use]
    pub fn assigns(&self) -> bool {
        matches!(self.tail, Tail::Assign { .. })
    }

    /// Whether the root itself arrives on the operand stack.
    ///
    /// Only a temporary does. The other two are reached where they live — by
    /// slot or by name — and getting this wrong is not a small mistake:
    /// [`Chain::consumes`] is what the verifier models a chunk's whole operand
    /// depth on, and what the VM finds its operands with.
    #[must_use]
    pub fn roots_on_stack(&self) -> bool {
        match self.root {
            Root::Local { .. } | Root::Named { .. } | Root::This { .. } => false,
            Root::Temporary => true,
        }
    }

    /// Everything the instruction takes off the operand stack.
    ///
    /// Pushed in that order — step operands, then the root, then the value
    /// being assigned — which is Rhai's evaluation order and not the reading
    /// order: it collects a chain's indices and arguments *before* it evaluates
    /// what they are being applied to (`eval/chaining.rs:498-524` then `:562`).
    #[must_use]
    pub fn consumes(&self) -> usize {
        self.operands as usize + usize::from(self.roots_on_stack()) + usize::from(self.assigns())
    }

    /// How many source positions this chain carries.
    ///
    /// Also the stride from one chain's sidecar slots to the next one's.
    #[must_use]
    pub fn position_slots(&self) -> u32 {
        self.root.position_slots() + self.steps.iter().map(Step::position_slots).sum::<u32>()
    }

    /// The slot holding step `index`'s own position.
    ///
    /// An index step owns two — this one, then the `[` — and this names the
    /// first. A fault records the step, not which of its positions raised, so
    /// `a[i]` restores to the index expression even where Rhai blamed the
    /// bracket.
    #[must_use]
    pub fn step_slot(&self, index: usize) -> Option<u32> {
        (index < self.steps.len()).then(|| {
            self.root.position_slots()
                + self.steps[..index]
                    .iter()
                    .map(Step::position_slots)
                    .sum::<u32>()
        })
    }

    /// Every source position this chain carries, in the order they are written.
    ///
    /// The one definition of that order, which the writer, the sidecar and
    /// [`Chain::step_slot`] all follow. They must agree exactly: read back
    /// against a different order, a stream restores plausible positions rather
    /// than none.
    pub fn positions(&self) -> impl Iterator<Item = rhai::Position> + '_ {
        let root = match self.root {
            Root::Named { pos, .. } | Root::This { pos } => Some(pos),
            Root::Local { .. } | Root::Temporary => None,
        };

        root.into_iter().chain(
            self.steps
                .iter()
                .flat_map(|step| match step {
                    Step::Index { pos, bracket, .. } => [Some(*pos), Some(*bracket)],
                    Step::Property { pos, .. } | Step::Method { pos, .. } => [Some(*pos), None],
                })
                .flatten(),
        )
    }

    /// The same positions, to write back into.
    pub fn positions_mut(&mut self) -> impl Iterator<Item = &mut rhai::Position> {
        let Self { root, steps, .. } = self;

        let root = match root {
            Root::Named { pos, .. } | Root::This { pos } => Some(pos),
            Root::Local { .. } | Root::Temporary => None,
        };

        root.into_iter().chain(
            steps
                .iter_mut()
                .flat_map(|step| match step {
                    Step::Index { pos, bracket, .. } => [Some(pos), Some(bracket)],
                    Step::Property { pos, .. } | Step::Method { pos, .. } => [Some(pos), None],
                })
                .flatten(),
        )
    }

    /// Whether walking this chain can change what it walks over.
    ///
    /// A read-only chain needs no write-back at all, which is worth knowing:
    /// write-back on a temporary calls a setter, and calling one where Rhai
    /// would not is an observable difference on a host type.
    #[must_use]
    pub fn mutates(&self) -> bool {
        matches!(self.tail, Tail::Assign { .. })
            || self
                .steps
                .iter()
                .any(|step| matches!(step, Step::Method { .. }))
    }

    /// Dump the disassembly of the entire chain.
    pub fn disassemble(&self, program: &Program) -> String {
        format!(
            "{:?} {} {} {}",
            self.root,
            match self.root {
                Root::Named { name, .. } | Root::Local { name, .. } => program.name(name).unwrap(),
                Root::This { .. } => "this",
                Root::Temporary => "<temp value>",
            },
            self.steps
                .iter()
                .map(|step| step.disassemble(program))
                .collect::<Vec<_>>()
                .join(" "),
            self.tail.disassemble(program)
        )
    }
}
