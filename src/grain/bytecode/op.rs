use crate::types::Token;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// `x op= y`
///
/// Both the op-assignment and the plain operator are carried, because Rhai
/// tries the first and falls back to expanding into the second when no
/// op-assignment implementation exists.
///
/// Lives in the program's op-assignment pool rather than in the instruction:
/// four fields including two `Token`s do not fit an operand, and the same
/// `+=` used in ten places is one entry.
#[derive(Debug, Clone, PartialEq)]
pub struct AssignOp {
    /// The `+=` token, for the built-in lookup.
    pub op_assign: Token,
    /// `"+="`, for dispatch and for error messages.
    pub op_assign_name: u32,
    /// The `+` token, for the expansion.
    pub op: Token,
    /// `"+"`.
    pub op_name: u32,
}

impl AssignOp {
    /// Dump the disassembly of the operation.
    #[cfg(feature = "internals")]
    pub fn disassemble(&self, program: &crate::grain::Program) -> String {
        format!(
            "{} ({:?}) / {} ({:?})",
            program.name(self.op_assign_name).unwrap(),
            self.op_assign,
            program.name(self.op_name).unwrap(),
            self.op,
        )
    }
}

/// Where [`Op::CallRef`] finds the variable it calls through.
///
/// They differ in how the variable is reached, not in what happens to it:
/// both take a reference where Rhai would and fall back to a value where it
/// would not, by the same rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receiver {
    /// A local, addressed by slot. Nothing was pushed for it — the call reads
    /// the [`Scope`][crate::Scope] entry itself, and a slot always names one.
    Local(u16),

    /// A variable no slot addresses: the caller's, a module's, or nothing.
    ///
    /// ### Note
    ///
    /// [`Op::LoadNamed`] has already resolved the name and left its value as
    /// argument zero, which is what raises
    /// [`ErrorVariableNotFound`][crate::EvalAltResult::ErrorVariableNotFound]
    /// against the variable rather than against the call — two positions the
    /// table cannot give one instruction.
    ///
    /// The call re-reaches the [`Scope`][crate::Scope] entry for the reference
    /// and falls back to that value when there is no entry to reach:
    /// * a resolver's answer,
    /// * a module's constant,
    /// * a `const`.
    ///
    /// ### Cost
    ///
    /// The by-reference path pays for a clone it discards. Worth removing only
    /// if a profile of a host-heavy script says so; a local, which is the common
    /// receiver by far, never makes one.
    Named(u32),

    /// The frame's receiver, for `f(this, ..)`.
    ///
    /// ### Order Difference
    ///
    /// Rhai applies the same rewrite to `this` as to a variable, but only when
    /// the receiver is neither shared nor curried.
    ///
    /// Shared-ness is a run-time property, so the value arrives on the stack as
    /// argument zero and the call reaches for the register instead when it turns
    /// out to be usable by reference — the deferral [`Receiver::Local`] already
    /// makes for a read-only entry.
    ///
    /// ### Implications
    ///
    /// [`Op::LoadThis`] pushes `this` *before* the remaining arguments.
    ///
    /// Rhai's two arms disagree about when `this` is read: the by-reference one
    /// takes it after arguments, but the fallback that a shared or unbound
    /// receiver lands in reads and flattens it first.
    ///
    /// Reading first is what makes `f(this, { this = 9; 1 })` pass the
    /// pre-mutation value, and an unbound `f(this, no_such)` report
    /// [`ErrorUnboundThis`][crate::EvalAltResult::ErrorUnboundThis] rather than
    /// [`ErrorVariableNotFound`][crate::EvalAltResult::ErrorVariableNotFound].
    This,
}

/// One VM instruction, as the compiler emits it and a disassembly shows it.
///
/// [`Program`]: crate::grain::Program
///
/// ### A Stack Machine
///
/// Operands are pushed and consumed on an operand stack, and locals live in
/// slots addressed directly.
///
/// ### Residual Fragments
///
/// [`Op::EvalAst`] is the escape hatch that hands a fragment back to Rhai's
/// [`AST`][crate::AST] interpreter, so anything the compiler cannot yet lower
/// still runs, and the whole language stays covered.
///
/// Lowering more of it converts residuals into instructions rather than adding
/// coverage.
///
/// ### Not the Executed Form
///
/// A [`Program`]'s code is a byte slice, and dispatched on directly, so a
/// loaded [`Program`] can borrow its instructions from the artifact rather than
/// building sixteen bytes of enum per instruction.
///
/// ### Positions
///
/// Instructions carry no source position.
///
/// Several of them can fail against a place in the source, and the position
/// for that comes from the [`Program`]'s positions table, keyed on the
/// instruction's own address.
///
/// Keeping it out means the diagnostics can be stripped from an artifact
/// without touching the code.
///
/// ### Operand Width
///
/// Anything too wide for an operand is a `u32` index into one of the
/// [`Program`]'s pools, which is also what keeps a repeated operator or name
/// from being stored twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Push constant `.0` from the pool.
    Const(u32),
    /// Push unit.
    Unit,
    /// Push a boolean.
    Bool(bool),

    /// Push the value in local slot `.0`.
    LoadLocal(u16),
    /// Pop and write into local slot `.0`, which must already exist.
    StoreLocal {
        /// The slot index
        slot: u16,
        /// Whether the value should be stored as a constant.
        is_const: bool,
    },

    /// Push the value of the variable named `.0`, found by name.
    ///
    /// For the variables no slot can address: e.g. the ones the caller already
    /// had in its [`Scope`][crate::Scope], which sit below the base every slot
    /// is measured from.
    ///
    /// A reverse scan of the [`Scope`][crate::Scope] is needed — only emitted
    /// for a name that cannot be resolved.
    LoadNamed(u32),

    /// Pop a value and assign it to the variable named `name`, optionally
    /// through an operator.
    ///
    /// [`Op::LoadNamed`]'s counterpart, and resolved the same way.
    ///
    /// # Error
    ///
    /// Assigning it to anything that is not a [`Scope`][crate::Scope] entry
    /// it can take a reference to — a value the resolver produced, a module's
    /// constant, a `const` — throws
    /// [`ErrorAssignmentToConstant`][crate::EvalAltResult::ErrorAssignmentToConstant].
    AssignNamed {
        /// The name of the variable
        name: u32,
        /// Index into the op-assignment pool; absent for a plain `=`.
        op: Option<u32>,
    },

    /// Pop a value and assign it to local slot `slot`, optionally through an
    /// operator.
    ///
    /// Separate from [`Op::StoreLocal`] because `x += y` is not `x = x + y`:
    /// Rhai looks for an op-assignment implementation that mutates in place,
    /// and only expands to the binary form if there is none.
    AssignLocal {
        /// The slot index
        slot: u16,
        /// Names the variable in `ErrorAssignmentToConstant`.
        var_name: u32,
        /// Index into the op-assignment pool; absent for a plain `=`.
        op: Option<u32>,
    },
    /// Pop and declare it as a new local, extending the [`Scope`][crate::Scope]
    /// by one.
    ///
    /// Slots are assigned in declaration order, so the new local always lands
    /// at the top of the [`Scope`][crate::Scope].
    ///
    /// Carries the name because locals live in the caller's [`Scope`][crate::Scope],
    /// where entries are named, and carries `const`-ness because Rhai enforces
    /// it through the value's own access mode.
    DeclareLocal {
        /// The name of the variable
        name: u32,
        /// Whether the variable is declared `const`.
        is_const: bool,
    },

    /// Discard the top of the operand stack.
    Pop,

    /// Jump to `.0`.
    ///
    /// `.0` is an instruction index as the compiler emits it, a byte offset
    /// once assembled — instructions vary in length, so there is nothing else
    /// it could be.
    Jump(u32),
    /// Pop a condition and jump to `.0` if it is `true`, mirroring
    /// [`Op::JumpIfFalse`].
    ///
    /// Both exist so `&&` and `||` lower without an extra negation.
    ///
    /// ### Position
    ///
    /// Its position-table entry is the condition's own position, because Rhai
    /// rejects a non-boolean guard against the guard expression rather than the
    /// statement.
    JumpIfTrue {
        /// Where to jump to
        target: u32,
    },
    /// Pop a condition and jump to `.0` if it is `false`, mirroring
    /// [`Op::JumpIfTrue`].
    ///
    /// Both exist so `&&` and `||` lower without an extra negation.
    ///
    /// ### Position
    ///
    /// Its position-table entry is the condition's own position, because Rhai
    /// rejects a non-boolean guard against the guard expression rather than the
    /// statement.
    JumpIfFalse {
        /// Where to jump to
        target: u32,
    },
    /// Inspects a condition and jump to `.0` if it is not `()`.
    ///
    /// The condition is not popped, so the caller can read it afterwards.
    ///
    /// Exists so short-circuit `??` lower.
    SkipIfNotUnit {
        /// Where to skip to
        target: u32,
    },

    /// Pop `argc` arguments and call the function named by `name`, pushing the
    /// result.
    ///
    /// ### Function Dispatch
    ///
    /// Dispatch goes through Rhai, so every registered function, operator and
    /// script function resolves identically to Rhai.
    ///
    /// ### Syntactic Function Calls
    ///
    /// Calls that Rhai handles syntactically before dispatch — `Fn`, `call`,
    /// `curry`, `eval`, `is_def_var` — are excluded as they are implemented
    /// natively.
    ///
    /// ### Position
    ///
    /// The position table's entry for this instruction is the call site.
    ///
    /// Rhai's dispatch path takes one and reports failures against it so an error
    /// that comes back without a position gets this one.
    ///
    /// ### Operator Call
    ///
    /// `op` indexes the operator pool when the call is an operator, and names
    /// the token the built-in lookup keys on.
    ///
    /// Operator calls on primitive types have built-in short paths.
    Call {
        /// The name of the function
        name: u32,
        /// How many arguments to pop
        argc: u8,
        /// Index into the operator pool; absent unless the call is an operator.
        op: Option<u32>,
        /// This call captures the parent's [`Scope`][crate::Scope].
        capture_parent_scope: bool,
    },

    /// Pop `argc` arguments and call the function named by `name` with a variable
    /// as its first argument, taken by reference, pushing the result.
    ///
    /// ### Rewrite to `&mut`
    ///
    /// Rhai rewrites `f(x, ..)` into `x.f(..)` whenever the first argument is a
    /// plain variable, so that a `&mut` first parameter mutates the variable
    /// rather than a copy.
    ///
    /// For example: `push(a, 2)` and `a.push(2)` are the same call; only the
    /// second reached the mutation through [`Op::Chain`].
    ///
    /// Two things follow from the rewrite, and together they are why this is an
    /// instruction rather than an argument order:
    ///
    /// * the variable is read *after* the other arguments, so an argument that
    ///   writes to it is seen;
    /// * a shared or read-only variable is passed by value instead — Rhai hands
    ///   out a reference to neither.
    CallRef {
        /// The name of the function
        name: u32,
        /// How many arguments to pop, not counting the receiver.
        argc: u8,
        /// Where the first argument is found.
        receiver: Receiver,
        /// This call captures the parent's [`Scope`][crate::Scope].
        capture_parent_scope: bool,
    },

    /// Move the top of the operand stack down past `.0` values.
    ///
    /// ### Uses
    ///
    /// A [`Receiver::Named`] receiver is resolved by [`Op::LoadNamed`] after
    /// the other arguments, and this puts it back in argument order.
    ///
    /// An [`Op::Switch`] instruction needs to clean up the subject value
    /// placed on the operand stack.
    Rotate(u8),

    /// Inspect the subject on the top of the operand stack and jump to wherever
    /// switch table `.0` sends it.
    ///
    /// The subject is not automatically popped due to the need to sometimes
    /// call `Switch` a second time to handle ranges.
    ///
    /// ### Clean Up
    ///
    /// The subject must be manually popped at the end of the statement,
    /// or when control flow jumps out from inside the switch statement,
    /// exiting a loop, or via an error caught by a `try` block.
    ///
    /// ### Always Jumps
    ///
    /// The table's default is where a subject that matches nothing goes,
    /// and an absent `_` arm compiles to a jump past the statement.
    ///
    /// ### Switch Pool
    ///
    /// The table is in the program's switch pool rather than in the
    /// instruction because it is unbounded, and because two arms of one
    /// `switch` share it.
    Switch(u32),

    /// Turn local slot `.0` into a shared cell, so a closure can capture it.
    ///
    /// ### Purpose
    ///
    /// Sharing is what makes closures capture, and the enclosing
    /// [`Scope`][crate::Scope] see the same value afterwards.
    ///
    /// Rhai's parser emits one of these per captured variable ahead of the
    /// `curry` call that binds them; the write-through is the other half.
    Share(u16),

    /// Turn a variable no slot names — one the caller supplied - into a
    /// shared cell, so a closure can capture it.
    ///
    /// ### Purpose
    ///
    /// Sharing is what makes closures capture, and the enclosing
    /// [`Scope`][crate::Scope] see the same value afterwards.
    ShareNamed(u32),

    /// Push local slot `.0` without flattening it.
    ///
    /// ### Purpose
    ///
    /// A read normally hands back what a shared cell contains, which is right
    /// for a value and wrong for a capture.
    ///
    /// Currying a closure has to bind the *cell*, or the closure gets a copy
    /// and stops being a closure.
    LoadShared(u16),

    /// Push a variable no slot names — one the caller supplied - without
    /// flattening it.
    ///
    /// ### Purpose
    ///
    /// A read normally hands back what a shared cell contains, which is right
    /// for a value and wrong for a capture.
    ///
    /// Currying a closure has to bind the *cell*, or the closure gets a copy
    /// and stops being a closure.
    LoadSharedNamed(u32),

    /// Push the receiver bound to the running frame's `this`.
    ///
    /// `this` is not a scope entry and no slot addresses it: Rhai threads it
    /// through evaluation as a parameter and keeps it out of the
    /// [`Scope`][crate::Scope] altogether.
    ///
    /// So it gets a register of its own, and these four instructions are the
    /// only things that reach it.
    ///
    /// # Error
    ///
    /// [`ErrorUnboundThis`][crate::EvalAltResult::ErrorUnboundThis] when the frame
    /// has no receiver.
    LoadThis,

    /// Push the receiver bound to the running frame's `this` without flattening it.
    ///
    /// ### Purpose
    ///
    /// A read normally hands back what a shared cell contains, which is right
    /// for a value and wrong for a capture.
    ///
    /// Currying a closure has to bind the *cell*, or the closure gets a copy
    /// and stops being a closure.
    LoadThisShared,

    /// Raise [`ErrorUnboundThis`][crate::EvalAltResult::ErrorUnboundThis]
    /// if the frame's `this` is not bound to a value.
    ///
    /// ### Order
    ///
    /// `this = v` checks *before* it evaluates `v`, unlike the variable arm,
    /// which evaluates the value first.
    ///
    /// Without this check, `this = no_such` in an unbound frame would report
    /// [`ErrorVariableNotFound`][crate::EvalAltResult::ErrorVariableNotFound] where
    /// Rhai reports [`ErrorUnboundThis`][crate::EvalAltResult::ErrorUnboundThis].
    RequireThis,

    /// Pop a value and assign it to the frame's `this`, optionally through an
    /// operator.
    ///
    /// # Error
    ///
    /// Assigning to a read-only `this` is
    /// [`ErrorAssignmentToConstant("")`][crate::EvalAltResult::ErrorAssignmentToConstant].
    AssignThis {
        /// Index into the op-assignment pool; absent for a plain `=`.
        op: Option<u32>,
    },

    /// Pop a value and push whether it is a shared cell.
    ///
    /// Implements the `is_shared` call which is syntactic and the value has
    /// to arrive unflattened, or the answer is always `false`.
    IsShared,

    /// Push a [function pointer][crate::FnPtr] to the compiled function named `.0`.
    ///
    /// ### Purpose
    ///
    /// A closure, whose name the parser makes up (`anon$…`) and which
    /// [`Op::MakeFnPtr`] would refuse — Rhai only builds pointers to names a
    /// script could have written. The name is known here, so unlike
    /// `MakeFnPtr` it needs no operand on the stack.
    ///
    /// ### Deprecated
    ///
    /// This Op is deprecated and no longer used.
    MakeClosure(u32),

    /// Pop a name and push a [function pointer][crate::FnPtr] to it.
    ///
    /// Implements the `Fn(<name>)` syntactic call which creates a
    /// [function pointer][crate::FnPtr] to the named function.
    MakeFnPtr,

    /// Pop `.0` arguments and a [function pointer][crate::FnPtr], and push
    /// the [function pointer][crate::FnPtr] with those arguments bound to the
    /// front of it.
    ///
    /// Implements the `curry(...)` syntactic call.
    Curry(u8),

    /// Pop `argc` arguments and a target, and call a
    /// [function pointer][crate::FnPtr].
    ///
    /// ### Function Dispatch
    ///
    /// A compiled function of that name and arity is called directly, with the
    /// curried arguments spliced in front.
    ///
    /// Anything else — a native function, a name that resolves elsewhere —
    /// is dispatched by Rhai.
    ///
    /// ### Method Call
    ///
    /// `is_method` distinguishes `f.call(x)` from `call(f, x)`, which are
    /// not the same call.
    ///
    /// In method position a target that is *not* a pointer is not an error:
    /// Rhai takes the first argument as the pointer and binds the target as
    /// `this`, which is how a closure is called against a receiver.
    CallFnPtr {
        /// How many arguments to pop
        argc: u8,
        /// Whether the call is in method position (`f.call(x)`).
        is_method: bool,
        /// This call captures the parent's [`Scope`][crate::Scope].
        capture_parent_scope: bool,
        /// Where the receiver came from, when there is anywhere to put it back.
        ///
        /// `obj.call(f)` binds `obj` as the closure's `this` **by reference**
        /// so a closure that writes to `this` writes to `obj`. The receiver's
        /// *value* is on the operand stack either way — this only says where it
        /// came from, so the write can be carried back there.
        ///
        /// `None` in call position, and for a receiver with nowhere to write
        /// back to: `[1, 2].call(f)` mutates a temporary, as it does in Rhai.
        ///
        /// Only meaningful when `is_method` is set.
        receiver: Option<Receiver>,
    },

    /// Push an empty buffer for an interpolated string to be built in.
    ///
    /// ### Size Checking
    ///
    /// Interpolation is three instructions rather than one because Rhai checks
    /// the size limit after **every** segment and blames the segment that went
    /// over.
    ///
    /// One instruction has one position-table entry, so it could not say which
    /// segment caused the overflow.
    ///
    /// A pool of per-segment positions would say it but would not be
    /// strippable, and diagnostics staying separable is the point of the
    /// table.
    ///
    /// Thus an instruction per segment puts each position exactly where the
    /// rest of them live.
    ///
    /// The buffer is an ordinary operand, so a nested interpolation needs
    /// nothing special.
    InterpolateStart,

    /// Pop an interpolated string segment and append it to the buffer beneath it.
    ///
    /// ### Note
    ///
    /// Not `+`, which is what it looks like: `+` is overridable and
    /// interpolation is not, and the `{string} + {anything}` operator skips
    /// the size check.
    ///
    /// A string segment is written straight out and never reaches dispatch;
    /// anything else goes through Rhai's `to_string` rendering,
    /// which consults native functions only.
    InterpolateAppend,

    /// Replace the buffer with the interpolated string it built.
    InterpolateEnd,

    /// Pop `.0` values and push them as an array.
    ///
    /// ### Purpose
    ///
    /// Only for a literal whose elements are not all constant — one that is
    /// gets folded into the pool by Rhai's own optimizer before this sees it.
    MakeArray(u16),

    /// Build a map from a template and `.0` key/value pairs above it.
    ///
    /// ### Purpose
    ///
    /// An entirely constant map never reaches here: the optimizer has already
    /// folded it into the template alone.
    ///
    /// ### Note
    ///
    /// The operand stack holds `[template, k0, v0, .., k(.0-1), v(.0-1)]`.
    ///
    /// The template is a constant `HashMap` that already carries every key
    /// the literal mentions, with the computed ones holding a placeholder.
    ///
    /// Keys ride on the operand stack as string constants.
    MakeMap(u16),

    /// Measure the value on top of the operand stack into the array or map
    /// literal being built, and raise
    /// [`ErrorDataTooLarge`][crate::EvalAltResult::ErrorDataTooLarge]
    /// if the running total is over.
    ///
    /// ### Note
    ///
    /// The operand is the element's index within its literal: zero starts a
    /// fresh total, and [`Op::MakeArray`] / [`Op::MakeMap`] discards it.
    ///
    /// That is what keeps `[a, [b, c], d]` straight — the inner literal's total
    /// is pushed and popped inside the outer one's.
    ///
    /// ### Purpose
    ///
    /// A separate instruction rather than work inside [`Op::MakeArray`] /
    /// [`Op::MakeMap`] because Rhai blames the *element* that tipped the total
    /// over, and one instruction has only one position-table entry.
    ///
    /// Putting it here rather than in a pool beside the element count is what
    /// keeps those positions strippable, which matters more for a literal than
    /// for a chain: an array or map can have any number of elements.
    CheckSize {
        /// The element's index within its literal
        index: u16,
        /// Whether the element counts towards the map limit rather than the
        /// array one.
        ///
        /// Rhai adds one to a different member of the triple for each,
        /// so the same running total cannot serve both.
        is_map: bool,
    },

    /// Walk `a.b[i].c`, indexing the chain pool.
    ///
    /// Pushes the value for a read, or unit for an assignment.
    ///
    /// ### Note
    ///
    /// One instruction for the whole chain rather than one per step, because
    /// the walk holds a `&mut` into the container at every level and a borrow
    /// cannot survive a trip round the dispatch loop.
    ///
    /// ### Evaluation Order
    ///
    /// Index values and method arguments were pushed before it, in step order.
    Chain(u32),

    /// Truncate the [`Scope`][crate::Scope] back to `.0` slots.
    ///
    /// ### Purpose
    ///
    /// Upon exiting a statements block, everything the block declared must be
    /// dropped.
    UnwindTo(u16),

    /// Count one operation against
    /// [`Engine::max_operations`][crate::Engine::set_max_operations], and give
    /// [`Engine::on_progress`][crate::Engine::on_progress] a chance to terminate.
    ///
    /// Emitted on loops.
    ///
    /// ### Behavioral Differences
    ///
    /// Rhai ticks per [`AST`][crate::AST] node, but the VM ticks only upon
    /// entering a loop, and on jump back-edges, so the operation counts
    /// **will** differ.
    ///
    /// What this preserves is that a *limit* is enforced and an interrupt is
    /// honoured, which is what allows `loop {}` to be killed.
    ///
    /// ### Position
    ///
    /// Its table entry is read on every iteration rather than only on failure,
    /// which is why the in-memory position table is dense.
    Tick,

    /// Record the current [`Scope`][crate::Scope] length as the depth an error
    /// escaping this chunk unwinds to.
    ///
    /// ### Note
    ///
    /// Rhai rewinds a nested block whether it left normally or by a `throw`,
    /// and never rewinds the top level of a chunk.
    ///
    /// The normal path is [`Op::UnwindTo`], which an escaping error jumps
    /// straight past — so the frame needs a floor to fall back to, and the
    /// last top-level statement boundary is exactly it.
    ///
    /// ### Cost
    ///
    /// Emitted once before each top-level statement of a chunk that runs in the
    /// caller's [`Scope`][crate::Scope], so it costs nothing per iteration and
    /// nothing at all to a function body, whose [`Scope`][crate::Scope] is
    /// discarded whole.
    Checkpoint,

    /// A statement begins here, at nesting `depth` within its chunk.
    ///
    /// Where the debugger stops.
    ///
    /// `depth` is how many statements enclose this one. It is what stepping
    /// re-arms against: Rhai restores the stepping state when the statement it
    /// was asked at *ends*, and the next marker at the same depth or shallower
    /// is where that has happened.
    ///
    /// Without it, a `next` over an `if` would step into its body.
    ///
    /// ### Behavioral Differences
    ///
    /// Rhai runs its callback per [`AST`][crate::AST] node and a chunk has
    /// no nodes, so the compiler records where the statements were and the
    /// VM stops there instead.
    ///
    /// Without this, there is nothing for the debugger to stop at, and
    /// break-points and stepping will be inert.
    ///
    /// ### Cost
    ///
    /// Emitted only under the `debugging` feature.
    ///
    /// An instruction per statement is not worth carrying to a device with
    /// no debugger callback to call, so a shipping build has none.
    ///
    /// Decoding is unconditional, so an artifact written by a `debugging`
    /// build still runs anywhere — it simply cannot be stopped by a debugger.
    Statement {
        /// How many statements enclose this one.
        depth: u16,
    },

    /// Evaluate residual [`AST`][crate::AST] fragment `residual` through
    /// Rhai's interpreter, pushing its value.
    ///
    /// `rewind_scope` reaches `eval_stmt_block` when the fragment is a block,
    /// and decides whether locals it declares survive:
    ///
    /// * Statement fragments rewind, so they cannot disturb the scope shape
    ///   slots were resolved against.
    /// * A whole-program fragment does not, because Rhai does not rewind
    ///   top-level statements and callers can see what they declared.
    #[cfg(not(feature = "no_ast"))]
    EvalAst {
        /// Index into the residual pool
        residual: u32,
        /// Whether locals the fragment declares are discarded afterwards.
        rewind_scope: bool,
    },

    /// Dispatch to a registered custom syntax, indexing the custom-syntax
    /// site pool.
    ///
    /// Pushes the value the registered handler returns.
    CustomSyntax(u32),

    /// Arm a handler covering the instructions up to the matching
    /// [`Op::PopHandler`], catching to `target`.
    ///
    /// Records where the operand stack, the [`Scope`][crate::Scope] and the
    /// iterator stack were when it was armed, because an error can be raised
    /// at any depth of all three and the catch block has to start where the
    /// `try` did.
    ///
    /// `catch_var` names the variable the error is bound to.
    ///
    /// ### Catching Errors
    ///
    /// Only errors Rhai considers *catchable* are caught: `return`, `break`,
    /// `continue` and `exit` unwind as errors too and must pass straight
    /// through.
    ///
    /// ### Position
    ///
    /// Its table entry is the `catch_var` variable's position, which is what
    /// Rhai reports
    /// [`ErrorTooManyVariables`][crate::EvalAltResult::ErrorTooManyVariables]
    /// against.
    PushHandler {
        /// Where to jump to when an error is caught.
        target: u32,
        /// The name the error is bound to; absent for a bare `catch`.
        catch_var: Option<u32>,
    },

    /// Disarm the inner-most handler.
    ///
    /// Emitted twice per `try`: once where the body ends normally, and once
    /// where the `catch`` block does — the second ends the region in which a
    /// bare `throw;` means "re-raise the original".
    PopHandler,

    /// Pop an iterable and start iterating it.
    ///
    /// The iterator goes on a stack of the VM's own rather than the operand
    /// stack because it is not a [`Dynamic`][crate::Dynamic].
    ///
    /// Rhai's iterator functions take the iterable **by value** and hand back
    /// something that cannot be re-created, so it is made once here and lives
    /// until the loop ends.
    ///
    /// ### Position
    ///
    /// Its table entry is the iterable's *start* position, which is what
    /// [`ErrorFor`][crate::EvalAltResult::ErrorFor] is reported against —
    /// a different position from the one [`Op::IterNext`] uses.
    IterInit,

    /// Advance the current iterator: push the next item and fall through, or
    /// drop the iterator and jump to `exit`.
    ///
    /// ### Stack Behavior
    ///
    /// The only instruction whose two edges leave different amounts on the
    /// operand stack, which is why the verifier gives it explicit successors.
    ///
    /// ### Position
    ///
    /// Its table entry is the iterable's position — `position`, not
    /// `start_position` — because that is what a fallible iterator's error is
    /// filled in with.
    IterNext {
        /// Where to jump to once the iterator is exhausted.
        exit: u32,
        /// `for (x, i) in seq`: the count is pushed under the item, so the two
        /// `StoreShared`s that follow pop them in declaration order.
        counter_slot: Option<u16>,
    },

    /// Discard the current iterator.
    ///
    /// Emitted where a `break` leaves a loop, since the jump skips the
    /// [`Op::IterNext`] that would have dropped it on exhaustion.
    ///
    /// Leaving a frame drops whatever it left behind without this.
    IterDrop,

    /// Pop a value and write it into local slot `.0`, through a shared cell
    /// rather than over it.
    ///
    /// ### Purpose
    ///
    /// Distinct from [`Op::StoreLocal`] only in intent: a `for` loop's
    /// variables are written once per iteration and a closure in the body
    /// may have shared it, in which case Rhai writes into the cell and every
    /// closure made in the loop sees the last value.
    StoreShared(u16),

    /// Pop a value and raise it as a `throw`.
    ///
    /// Always fails, with [`ErrorRuntime`][crate::EvalAltResult::ErrorRuntime]
    /// carrying the value — Rhai wraps nothing and converts nothing, so any
    /// type can be thrown.
    ///
    /// ### Position
    ///
    /// Its table entry is the `throw` keyword's own position, not the
    /// expression's.
    Throw,

    /// End the chunk, yielding the top of the operand stack, or unit if empty.
    Return,
}

impl Op {
    /// Dump the disassembly of the operation.
    #[cfg(feature = "internals")]
    pub fn disassemble(&self, program: &crate::grain::Program) -> String {
        match self {
            Op::Const(idx) => format!("{self:?} = {}", program.constant(*idx).unwrap()),
            Op::LoadNamed(name) => format!("{self:?} : {}", program.name(*name).unwrap()),
            Op::AssignNamed { name, op } => {
                if let Some(op) = op {
                    format!(
                        "{self:?} : {} {}",
                        program.name(*name).unwrap(),
                        program.assign_op(*op).unwrap().disassemble(program)
                    )
                } else {
                    format!("{self:?} : {}", program.name(*name).unwrap(),)
                }
            }

            Op::AssignLocal { var_name, op, .. } => {
                if let Some(op) = op {
                    format!(
                        "{self:?} : {} {}",
                        program.name(*var_name).unwrap(),
                        program.assign_op(*op).unwrap().disassemble(program)
                    )
                } else {
                    format!("{self:?} : {}", program.name(*var_name).unwrap(),)
                }
            }
            Op::DeclareLocal { name, is_const } => {
                format!(
                    "{self:?} : {} {}",
                    if *is_const { "const" } else { "let" },
                    program.name(*name).unwrap()
                )
            }
            Op::Call { name, op, .. } => {
                if let Some(op) = op {
                    format!(
                        "{self:?} : {} ({:?})",
                        program.name(*name).unwrap(),
                        program.token(*op).unwrap()
                    )
                } else {
                    format!("{self:?} : {}", program.name(*name).unwrap(),)
                }
            }

            Op::CallRef { name, .. } => format!("{self:?} : {}", program.name(*name).unwrap(),),
            Op::Switch(idx) => format!(
                "Switch({idx}) {}",
                program.switch(*idx).unwrap().disassemble(program),
            ),
            Op::ShareNamed(name) => format!("{self:?} : {}", program.name(*name).unwrap()),
            Op::LoadSharedNamed(name) => format!("{self:?} : {}", program.name(*name).unwrap()),
            Op::AssignThis { op, .. } if op.is_some() => {
                format!(
                    "{self:?} : this {:?}",
                    program.assign_op(op.unwrap()).unwrap().disassemble(program)
                )
            }
            Op::MakeClosure(name) => format!("{self:?} : {}", program.name(*name).unwrap(),),
            Op::Chain(idx) => {
                let chain = program.chain(*idx).unwrap();
                format!(
                    "Chain({idx}, {} total operands): {}",
                    chain.operands,
                    chain.disassemble(program),
                )
            }
            Op::CustomSyntax(idx) => {
                let custom_syntax = program.custom_syntax_site(*idx).unwrap();
                format!("{self:?} : {}", custom_syntax.disassemble(program))
            }

            _ => format!("{self:?}"),
        }
    }
}
