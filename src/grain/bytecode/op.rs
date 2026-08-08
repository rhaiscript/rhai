use crate::tokenizer::Token;

/// What `x op= y` needs to reproduce rhai's resolution order.
///
/// Both the op-assignment and the plain operator are carried, because rhai
/// tries the first and falls back to expanding into the second when no
/// op-assignment implementation exists (`eval/stmt.rs:217-236`).
///
/// Lives in the program's op-assignment pool rather than in the instruction:
/// four fields including two `Token`s do not fit an operand, and the same
/// `+=` used in ten places is one entry.
// No `Eq`: `Token` carries float literals, so it is only `PartialEq`.
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

/// Where [`Op::CallRef`] finds the variable it calls through.
///
/// The two differ in how the variable is reached, not in what happens to it:
/// both take a reference where rhai would and fall back to a value where it
/// would not, by the same rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receiver {
    /// A local, addressed by slot. Nothing was pushed for it — the call reads
    /// the scope entry itself, and a slot always names one.
    Local(u16),

    /// A variable no slot addresses: the caller's, a module's, or nothing.
    ///
    /// [`Op::LoadNamed`] has already resolved the name and left its value as
    /// argument zero, which is what raises `ErrorVariableNotFound` against the
    /// variable rather than against the call — two positions the table cannot
    /// give one instruction. The call re-reaches the scope entry for the
    /// reference and falls back to that value when there is no entry to reach:
    /// a resolver's answer, a module's constant, a `const`.
    ///
    /// So the by-reference path pays for a clone it discards. Worth removing
    /// only if a profile of a host-heavy script says so; a local, which is the
    /// common receiver by far, never makes one.
    Named(u32),
}

/// One VM instruction, as the compiler emits it and a disassembly shows it.
///
/// **Not the executed form.** A program's code is a byte slice, assembled from
/// these by [`assemble`](crate::grain::bytecode::assemble) and dispatched on
/// directly, so a loaded program can borrow its instructions from the artifact
/// rather than building sixteen bytes of enum per instruction. See
/// [`code`](crate::grain::bytecode::code) for the encoding.
///
/// A stack machine: operands are pushed and consumed on an operand stack, and
/// locals live in slots addressed directly. `EvalAst` is the escape hatch that
/// hands a fragment back to rhai's tree walker, so anything the compiler cannot
/// yet lower still runs, and the whole language stays covered. Milestones
/// convert residuals into instructions rather than adding coverage.
///
/// Instructions carry no source position. Several of them can fail against a
/// place in the source, and the position for that comes from the program's
/// [`Positions`](crate::grain::bytecode::Positions) table, keyed on the instruction's
/// own address. Keeping it out means the diagnostics can be stripped from an
/// artifact without touching the code.
///
/// Anything too wide for an operand is a `u32` index into one of the program's
/// pools, which is also what keeps a repeated operator or name from being
/// stored twice.
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
    StoreLocal(u16),

    /// Push the value of the variable named `.0`, found by name.
    ///
    /// For the variables no slot can address: the ones the caller already had
    /// in its `Scope` when the program started, which sit below the base every
    /// slot is measured from. Without this, a script that reads anything its
    /// host supplied is a fragment, and so cannot be written to an artifact at
    /// all.
    ///
    /// Three places are searched, in rhai's order (`eval/expr.rs:107-155`):
    /// the resolver a host may have registered with `Engine::on_var`, then the
    /// scope, then the modules loaded into the global namespace. Missing from
    /// all three is `ErrorVariableNotFound`.
    ///
    /// A reverse scan of the scope per read, where a slot is an index — which
    /// is why the compiler only emits this for a name it could not resolve.
    LoadNamed(u32),

    /// Pop a value and assign it to the variable named `name`, optionally
    /// through an operator.
    ///
    /// [`Op::LoadNamed`]'s counterpart, and resolved the same way. Assigning
    /// to anything that is not a scope entry it can take a reference to — a
    /// value the resolver produced, a module's constant, a `const` — is
    /// `ErrorAssignmentToConstant`, as it is in the walker.
    AssignNamed {
        /// The name of the variable
        name: u32,
        /// Index into the op-assignment pool; absent for a plain `=`.
        op: Option<u32>,
    },

    /// Pop a value and assign it to local slot `slot`, optionally through an
    /// operator.
    ///
    /// Separate from `StoreLocal` because `x += y` is not `x = x + y`: rhai
    /// looks for an op-assignment implementation that mutates in place, and
    /// only expands to the binary form if there is none.
    AssignLocal {
        /// The slot index
        slot: u16,
        /// Names the variable in `ErrorAssignmentToConstant`.
        var_name: u32,
        /// Index into the op-assignment pool; absent for a plain `=`.
        op: Option<u32>,
    },
    /// Pop and declare it as a new local, extending the scope by one.
    ///
    /// Slots are assigned in declaration order, so the new local always lands
    /// at the top of the scope. Carries the name because locals live in the
    /// caller's `Scope`, where entries are named, and carries constness
    /// because rhai enforces it through the value's own access mode.
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
    /// An instruction index as the compiler emits it, a byte offset once
    /// assembled — instructions vary in length, so there is nothing else it
    /// could be.
    Jump(u32),
    /// Pop a condition and jump to `.0` if it is true. Mirrors
    /// [`Op::JumpIfFalse`]; both exist so short-circuit `&&` and `||` lower
    /// without an extra negation.
    JumpIfTrue {
        /// Where to jump to
        target: u32,
    },
    /// Pop a condition and jump to `.0` if it is false.
    ///
    /// Its position-table entry is the condition's own position, because rhai
    /// rejects a non-boolean guard against the guard expression rather than the
    /// statement — and the differential harness compares error positions.
    JumpIfFalse {
        /// Where to jump to
        target: u32,
    },

    /// Pop `argc` arguments and call the function named by `name`, pushing the
    /// result.
    ///
    /// Dispatch goes through rhai, so every registered function, operator and
    /// script function resolves exactly as it would in the walker. Only calls
    /// rhai handles syntactically before dispatch — `Fn`, `call`, `curry`,
    /// `eval`, `is_def_var` — are excluded, and stay fragments.
    ///
    /// The position table's entry for this instruction is the call site. Rhai's
    /// dispatch path takes one and reports failures against it; `call_fn_raw`
    /// does not, so an error that comes back without a position gets this one.
    ///
    /// `op` indexes the operator pool when the call is an operator, and names
    /// the token the built-in lookup keys on. The walker short-circuits these
    /// to a function pointer rather than dispatching, and a VM that did not
    /// would be slower than the tree it replaced.
    Call {
        /// The name of the function
        name: u32,
        /// How many arguments to pop
        argc: u8,
        /// Index into the operator pool; absent unless the call is an operator.
        op: Option<u32>,
    },

    /// Call `name` with a variable as its first argument, taken by reference.
    ///
    /// Rhai rewrites `f(x, ..)` into `x.f(..)` whenever the first argument is a
    /// plain variable, so that a `&mut` first parameter mutates the variable
    /// rather than a copy (`func/call.rs:1434-1460`). `push(a, 2)` and
    /// `a.push(2)` are the same call; only the second reached the mutation
    /// through [`Op::Chain`].
    ///
    /// Two things follow from the rewrite, and together they are why this is an
    /// instruction rather than an argument order:
    ///
    /// * the variable is read *after* the other arguments, so an argument that
    ///   writes to it is seen;
    /// * a shared or read-only variable is passed by value instead — rhai hands
    ///   out a reference to neither (`func/call.rs:1449-1454`).
    ///
    /// Operators never reach here: under `fast_operators` a binary one
    /// short-circuits before the rewrite (`func/call.rs:1775`), so `a + b`
    /// reads `a` first and needs no reference.
    CallRef {
        /// The name of the function
        name: u32,
        /// How many arguments to pop, not counting the receiver.
        argc: u8,
        /// Where the first argument is found.
        receiver: Receiver,
    },

    /// Move the top of the operand stack down past `.0` values.
    ///
    /// A [`Receiver::Named`] receiver is resolved by [`Op::LoadNamed`] after
    /// the other arguments, and this puts it back in argument order.
    Rotate(u8),

    /// Pop a subject and jump to wherever switch table `.0` sends it.
    ///
    /// Always jumps — the table's default is where a subject that matches
    /// nothing goes, and an absent `_` arm compiles to a jump past the
    /// statement. Arms with guards are not table entries: the table sends a
    /// subject to the head of a chain that tries each guard in source order
    /// and falls through to the default, which is what keeps dispatch a
    /// lookup.
    ///
    /// The table is in the program's switch pool rather than in the
    /// instruction because it is unbounded, and because two arms of one
    /// `switch` share it.
    Switch(u32),

    /// Turn local slot `.0` into a shared cell, so a closure can capture it.
    ///
    /// Rhai's parser emits one of these per captured variable ahead of the
    /// `curry` call that binds them (`parser.rs:3707`). Sharing is what makes
    /// the closure and the enclosing scope see the same value afterwards; the
    /// write-through in `place` is the other half.
    ///
    /// The variable resolver gets first refusal, as it does in
    /// `eval/stmt.rs:998`: if a host's `on_var` answers the name, the variable
    /// is *not* shared.
    Share(u16),

    /// The same for a variable no slot names — one the caller supplied.
    ShareNamed(u32),

    /// Push local slot `.0` without flattening it.
    ///
    /// A read normally hands back what a shared cell contains, which is right
    /// for a value and wrong for a capture: currying a closure has to bind the
    /// *cell*, or the closure gets a copy and stops being a closure.
    LoadShared(u16),

    /// The same for a variable no slot names — one the caller supplied.
    ///
    /// [`Op::LoadNamed`] flattens, and a closure capturing a caller's variable
    /// through that read binds a copy: the aliasing is dead, and a write to the
    /// variable afterwards is invisible to the closure. The slot case has had
    /// [`Op::LoadShared`] since closures were lowered at all; this is the half
    /// that was missing.
    LoadSharedNamed(u32),

    /// Pop a value and push whether it is a shared cell.
    ///
    /// Rhai answers `is_shared` before dispatch and registers no function for
    /// it (`func/call.rs:1240`), so there is nothing to call — and the value
    /// has to arrive unflattened, or the answer is always false.
    IsShared,

    /// Push a function pointer to the compiled function named `.0`.
    ///
    /// A closure's, whose name the parser makes up (`anon$…`) and which
    /// [`Op::MakeFnPtr`] would refuse — rhai only builds pointers to names a
    /// script could have written. The name is known here, so unlike
    /// `MakeFnPtr` it needs no operand on the stack.
    MakeClosure(u32),

    /// Pop a name and push a function pointer to it.
    ///
    /// Deliberately the *late-bound* kind, carrying a name and nothing else.
    /// Rhai's other kind embeds a `ScriptFuncDef` — an AST body — which is
    /// both unreachable from outside the crate and exactly the allocation this
    /// project exists to remove. Building our own means a pointer resolves
    /// through the compiled function table like any other call, and a program
    /// holding one can still be written to an artifact.
    ///
    /// The cost is that a `Normal` pointer is late-bound where rhai's is
    /// early-bound: redefining the function after taking a pointer to it is
    /// visible here and not in the walker.
    MakeFnPtr,

    /// Pop `.0` arguments and a function pointer, and push the pointer with
    /// those arguments bound to the front of it.
    Curry(u8),

    /// Pop `argc` arguments and a target, and call a function pointer.
    ///
    /// A compiled function of that name and arity is called directly, with the
    /// curried arguments spliced in front. Anything else — a native function,
    /// a name that resolves elsewhere — goes to rhai's own `call_raw`.
    ///
    /// `method` distinguishes `f.call(x)` from `call(f, x)`, which are not the
    /// same call. In method position a target that is *not* a pointer is not
    /// an error: rhai takes the first argument as the pointer and binds the
    /// target as `this` (`func/call.rs:816-919`), which is how a closure is
    /// called against a receiver.
    CallFnPtr {
        /// How many arguments to pop
        argc: u8,
        /// Whether the call is in method position (`f.call(x)`).
        method: bool,
    },

    /// Push an empty buffer for an interpolated string to be built in.
    ///
    /// Interpolation is three instructions rather than one because rhai checks
    /// the size limit after **every** segment and blames the segment that went
    /// over. One instruction has one position-table entry, so it could not say
    /// which; a pool of per-segment positions would say it but would not be
    /// strippable, and diagnostics staying separable is the point of the
    /// table. An instruction per segment puts each position exactly where the
    /// rest of them live.
    ///
    /// The buffer is an ordinary operand, so a nested interpolation needs
    /// nothing special.
    InterpolateStart,

    /// Pop a segment and append it to the buffer beneath it.
    ///
    /// Not `+`, which is what it looks like: `+` is overridable and
    /// interpolation is not, and the string-plus-anything operator skips this
    /// size check. A string segment is written straight out and never reaches
    /// dispatch; anything else goes through rhai's `to_string` rendering,
    /// which consults native functions only.
    InterpolateAppend,

    /// Replace the buffer with the interned string it built.
    InterpolateEnd,

    /// Pop `.0` values and push them as an array.
    ///
    /// Only for a literal whose elements are not all constant — one that is
    /// gets folded into the pool by rhai's own optimizer before this sees it.
    MakeArray(u16),

    /// Build a map from a template and `n` key/value pairs above it.
    ///
    /// The stack holds `[template, k0, v0, .., k(n-1), v(n-1)]`. The template
    /// is a constant map that already carries every key the literal mentions,
    /// with the computed ones holding a placeholder — that is rhai's own shape
    /// (`ast/expr.rs:283`), and it is why an entirely constant map never
    /// reaches here: the optimizer has already folded it into the template
    /// alone.
    ///
    /// Keys ride on the operand stack as string constants rather than in a
    /// pool of their own. They are constants either way, and the constant pool
    /// already deduplicates them across the program.
    MakeMap(u16),

    /// Measure the value on top of the stack into the array literal being
    /// built, and raise `ErrorDataTooLarge` if the running total is over.
    ///
    /// The operand is the element's index within its literal: zero starts a
    /// fresh total, and [`Op::MakeArray`] discards it. That is what keeps
    /// `[a, [b, c], d]` straight — the inner literal's total is pushed and
    /// popped inside the outer one's.
    ///
    /// A separate instruction rather than work inside `MakeArray` because rhai
    /// blames the *element* that tipped the total over
    /// (`eval/expr.rs:328`), and one instruction has one position-table entry.
    /// Putting it here rather than in a pool beside the element count is what
    /// keeps those positions strippable, which matters more for a literal than
    /// for a chain: an array can have any number of elements.
    CheckSize {
        /// The element's index within its literal
        index: u16,
        /// Whether the element counts towards the map limit rather than the
        /// array one. Rhai adds one to a different member of the triple for
        /// each (`eval/expr.rs:323` against `:354`), so the same running total
        /// cannot serve both.
        map: bool,
    },

    /// Walk `a.b[i].c`, indexing the chain pool.
    ///
    /// One instruction for the whole chain rather than one per step, because
    /// the walk holds a `&mut` into the container at every level and a borrow
    /// cannot survive a trip round the dispatch loop. Index values and method
    /// arguments were pushed before it, in step order.
    ///
    /// Pushes the value for a read, or unit for an assignment.
    Chain(u32),

    /// Truncate the scope back to `.0` locals, dropping everything a block
    /// declared. The compile-time slot model unwinds in step.
    UnwindTo(u16),

    /// Count one operation against `max_operations`, and give `on_progress` a
    /// chance to terminate.
    ///
    /// Emitted on loop back-edges. Rhai ticks per AST node, so counts differ;
    /// what this preserves is that a limit is enforced and an interrupt is
    /// honoured, which is what stops `loop {}` from being unkillable.
    ///
    /// Its table entry is read on every iteration rather than only on failure,
    /// which is why the in-memory position table is dense.
    Tick,

    /// Record the current scope length as the depth an error escaping this
    /// chunk unwinds to.
    ///
    /// Rhai rewinds a nested block whether it is left normally or by a throw,
    /// and never rewinds the top level of a chunk (`eval/stmt.rs`, and
    /// `eval_global_statements` passing `rewind_scope = false`). The normal
    /// path is [`Op::UnwindTo`], which an escaping error jumps straight past —
    /// so the frame needs a floor to fall back to, and the last top-level
    /// statement boundary is exactly it.
    ///
    /// Emitted once before each top-level statement of a chunk that runs in the
    /// caller's scope, so it costs nothing per iteration and nothing at all to
    /// a function body, whose scope is discarded whole.
    Checkpoint,

    /// Evaluate residual AST fragment `residual` through rhai's walker,
    /// pushing its value.
    ///
    /// `rewind_scope` reaches `eval_stmt_block` when the fragment is a block,
    /// and decides whether locals it declares survive. Statement fragments
    /// rewind, so they cannot disturb the scope shape slots were resolved
    /// against. A whole-program fragment does not, because rhai does not
    /// rewind top-level statements and callers can see what they declared.
    EvalAst {
        /// Index into the residual pool
        residual: u32,
        /// Whether locals the fragment declares are discarded afterwards.
        rewind_scope: bool,
    },

    /// Arm a handler covering the instructions up to the matching
    /// [`Op::PopHandler`], catching to `target`.
    ///
    /// Records where the operand stack, the scope and the iterator stack were
    /// when it was armed, because an error can be raised at any depth of all
    /// three and the catch block has to start where the `try` did.
    ///
    /// Only errors rhai considers catchable are caught: `return`, `break`,
    /// `continue` and `exit` unwind as errors too and must pass straight
    /// through (`eval/stmt.rs:806`).
    ///
    /// `catch_var` names the variable the error is bound to. Its table entry
    /// is that variable's position, which is what rhai reports
    /// `ErrorTooManyVariables` against.
    PushHandler {
        /// Where to jump to when an error is caught.
        target: u32,
        /// The name the error is bound to; absent for a bare `catch`.
        catch_var: Option<u32>,
    },

    /// Disarm the innermost handler.
    ///
    /// Emitted twice per `try`: once where the body ends normally, and once
    /// where the catch block does — the second ends the region in which a
    /// bare `throw;` means "re-raise the original".
    PopHandler,

    /// Pop an iterable and start iterating it.
    ///
    /// The iterator goes on a stack of the VM's own rather than the operand
    /// stack, because it is not a `Dynamic`. Rhai's iterator functions take
    /// the iterable **by value** and hand back something that cannot be
    /// re-created, so it is made once here and lives until the loop ends.
    ///
    /// Its table entry is the iterable's *start* position, which is what
    /// `ErrorFor` is reported against (`eval/stmt.rs:703`) — a different
    /// position from the one [`Op::IterNext`] uses.
    IterInit,

    /// Advance the current iterator: push the next item and fall through, or
    /// drop the iterator and jump to `exit`.
    ///
    /// The only instruction whose two edges leave different amounts on the
    /// operand stack, which is why the verifier gives it explicit successors.
    ///
    /// Its table entry is the iterable's position — `position`, not
    /// `start_position` — because that is what a fallible iterator's error is
    /// filled in with (`eval/stmt.rs:749`).
    IterNext {
        /// Where to jump to once the iterator is exhausted.
        exit: u32,
        /// `for (x, i) in seq`: the count is pushed under the item, so the two
        /// `StoreShared`s that follow pop them in declaration order.
        indexed: bool,
    },

    /// Discard the current iterator.
    ///
    /// Emitted where a `break` leaves a loop, since the jump skips the
    /// [`Op::IterNext`] that would have dropped it on exhaustion. Leaving a
    /// frame drops whatever it left behind without this.
    IterDrop,

    /// Pop a value and write it into local slot `.0`, through a shared cell
    /// rather than over it.
    ///
    /// Distinct from [`Op::StoreLocal`] only in intent: the `for` loop
    /// variable is written once per iteration and a closure in the body may
    /// have shared it, in which case rhai writes into the cell and every
    /// closure made in the loop sees the last value (`eval/stmt.rs:752`).
    StoreShared(u16),

    /// Pop a value and raise it as a `throw`.
    ///
    /// Always fails, with `ErrorRuntime` carrying the value — rhai wraps
    /// nothing and converts nothing, so any type can be thrown. Its table
    /// entry is the `throw` keyword's own position, not the expression's
    /// (`eval/stmt.rs:877`).
    Throw,

    /// End the chunk, yielding the top of the operand stack, or unit if empty.
    Return,
}
