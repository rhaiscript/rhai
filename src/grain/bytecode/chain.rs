/// One step along `a.b[i].c(x)`.
///
/// Steps live in the program's chain pool rather than in the instruction
/// stream, because a chain is walked by one instruction rather than several.
/// It has to be: the walk holds a `&mut` into the container at every level, and
/// a borrow cannot survive a trip round the dispatch loop. That is also what
/// makes it correct — rhai holds the same references, so a mutation partway
/// down a chain lands in the same place rather than in a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// `[i]`, where the index was evaluated onto the operand stack before the
    /// chain instruction ran, at `operand` from the first of them.
    ///
    /// Pre-evaluating mirrors rhai, which collects every index in a chain into
    /// `idx_values` before walking it (`eval/chaining.rs:568`). It has to
    /// happen first: evaluating an index halfway down would need the operand
    /// stack while a borrow of the container is live.
    ///
    /// Rhai reports `a[10]` out of bounds against the `10` rather than against
    /// the chain (`eval/chaining.rs:694`).
    ///
    /// `bracket` is the other position rhai keeps for a step, and the two are
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
        /// Where the index expression starts.
        pos: rhai::Position,
        /// The `[` in front of it.
        bracket: rhai::Position,
    },

    /// `.name`, which is a key lookup on a map and a getter call on anything
    /// else — the distinction rhai makes at runtime, not at parse time
    /// (`eval/chaining.rs:898`).
    Property {
        /// The bare name, for a map key and for error messages.
        name: u32,
        /// `get$name`.
        getter: u32,
        /// `set$name`, for the write-back.
        setter: u32,
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
        /// Where the call is in the source.
        pos: rhai::Position,
    },
}

impl Step {
    /// Where this step is in the source.
    ///
    /// Every step carries one, and it is the one place diagnostics are not
    /// strippable — four bytes per step, in the chain pool rather than the
    /// position table. That is not an oversight twice over: a chain is a single
    /// instruction, so the one entry the table holds for it cannot say which of
    /// `a.b[i].c()` failed, and rhai blames the step rather than the chain for
    /// all three kinds. An index is reported against its index expression, a
    /// property against the property (`eval/chaining.rs:1039`), a method
    /// against the call (`:904`).
    #[must_use]
    pub fn pos(&self) -> rhai::Position {
        match self {
            Step::Index { pos, .. } | Step::Property { pos, .. } | Step::Method { pos, .. } => *pos,
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
    /// pool rather than in the strippable table, for the reason [`Step::pos`]
    /// gives.
    Named {
        /// The name of the variable
        name: u32,
        /// Where the variable is in the source.
        pos: rhai::Position,
    },

    /// A value the instruction takes off the operand stack, pushed above the
    /// step operands.
    ///
    /// `[1, 2, 3].len()`, `f().x`, `(a + b).to_string()`. Nothing is written
    /// back, because there is nowhere to write it back to — and nothing can be
    /// assigned to one, because rhai's parser refuses `f().x = 1` before this
    /// ever sees it (`eval/chaining.rs:559`).
    Temporary,
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
            Root::Local { .. } | Root::Named { .. } => false,
            Root::Temporary => true,
        }
    }

    /// Everything the instruction takes off the operand stack.
    ///
    /// Pushed in that order — step operands, then the root, then the value
    /// being assigned — which is rhai's evaluation order and not the reading
    /// order: it collects a chain's indices and arguments *before* it evaluates
    /// what they are being applied to (`eval/chaining.rs:498-524` then `:562`).
    #[must_use]
    pub fn consumes(&self) -> usize {
        self.operands as usize + usize::from(self.roots_on_stack()) + usize::from(self.assigns())
    }

    /// Whether walking this chain can change what it walks over.
    ///
    /// A read-only chain needs no write-back at all, which is worth knowing:
    /// write-back on a temporary calls a setter, and calling one where rhai
    /// would not is an observable difference on a host type.
    #[must_use]
    pub fn mutates(&self) -> bool {
        matches!(self.tail, Tail::Assign { .. })
            || self
                .steps
                .iter()
                .any(|step| matches!(step, Step::Method { .. }))
    }
}
