use core::mem;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

// Indexing survives either feature alone — a map is indexed by string, an
// array by number — and only goes when both do. `eval/chaining.rs` is gated on
// exactly this, and takes the getter and setter names with it.
// Measuring a value is measuring what an array, a map or a string holds, so
// the same pair of features takes it away — see the note above.
#[cfg(not(feature = "unchecked"))]
#[cfg(not(all(feature = "no_index", feature = "no_object")))]
use crate::eval::calc_data_sizes;
use crate::eval::{Caches, GlobalRuntimeState};
use crate::func::{get_builtin_binary_op_fn, get_builtin_op_assignment_fn};
use crate::packages::string_basic::print_with_func;
use crate::types::dynamic::{AccessMode, DynamicWriteLock};
use crate::types::fn_ptr::FnPtrType;
use crate::types::StringsInterner;
// `Variant` is only re-exported from the crate root under `internals`, so it
// comes from where it is defined.
use crate::ast::Expr;
#[cfg(not(feature = "no_index"))]
use crate::Array;
#[cfg(not(feature = "no_object"))]
use crate::Map;
#[cfg(not(feature = "no_function"))]
use crate::{types::dynamic::Variant, CallFnOptions};
use crate::{
    Dynamic, Engine, EvalAltResult, EvalContext, FnArgsVec, FnPtr, ImmutableString,
    NativeCallContext, Position, Scope, FUNC_TO_STRING, INT,
};

mod callback;

use crate::grain::bytecode::{code, AssignOp, Chain, Chunk, Receiver, Root, Step, StepFlags, Tail};
use crate::grain::program::{Program, SharedModule, SharedProgram};

/// Rhai's own `RhaiResult`, which it does not re-export.
pub type VmResult = Result<Dynamic, Box<EvalAltResult>>;

/// Whether a value is a shared cell.
///
/// Sharing is how closures capture, so under `no_closure` there are no cells,
/// `Dynamic` has no `is_shared` to call, and the answer is a constant. The
/// opcodes that create and read cells are compiled out with it.
#[cfg(not(feature = "no_closure"))]
macro_rules! is_shared {
    ($value:expr) => {
        $value.is_shared()
    };
}
#[cfg(feature = "no_closure")]
macro_rules! is_shared {
    ($value:expr) => {{
        let _ = &$value;
        false
    }};
}

/// Make a function call into Rhai using [`_call_fn_raw`](crate::eval::_call_fn_raw).
#[inline(always)]
fn call_engine(
    engine: &Engine,
    global: &mut GlobalRuntimeState,
    caches: &mut Caches,
    scope: &mut Scope,
    fn_name: &str,
    args: &mut [&mut Dynamic],
    is_ref_mut: bool,
    is_method_call: bool,
    pos: Position,
) -> VmResult {
    let native_only = !crate::tokenizer::is_valid_identifier(fn_name);
    #[cfg(not(feature = "no_function"))]
    let native_only = native_only && !crate::parser::is_anonymous_fn(fn_name);

    crate::eval::_call_fn_raw(
        engine,
        global,
        caches,
        scope,
        fn_name,
        args,
        native_only,
        is_ref_mut,
        is_method_call,
        pos,
    )
}

/// Stamp the call site on an error that passes through a function boundary unwrapped,
/// as Rhai does for exits and system exceptions (`func/script.rs:134`).
fn reposition(mut err: Box<EvalAltResult>, pos: Position) -> Box<EvalAltResult> {
    err.set_position(pos);
    err
}

/// Take back off the scope whatever a debugger callback left on it.
///
/// Slots are positions. An entry a callback declares sits underneath every
/// declaration the chunk has yet to make, so the next `let` would land above it
/// and every slot from there on would name the variable before the one it meant
/// — silently, and in a program that has nothing to do with the debugger. Rhai
/// answers this by searching by name from then on (`eval/debugger.rs:436`); a
/// chunk has no names to search, so the scope goes back to the shape it was
/// compiled against instead, and what a callback declares does not outlive the
/// stop it declared it at.
#[cfg(feature = "debugging")]
fn rewind_after_stop(scope: &mut Scope, before: usize) {
    if scope.len() > before {
        scope.rewind(before);
    }
}

/// Stamp the site on an error that arrived without one.
///
/// Unlike [`reposition`] this never overwrites a position the callee already set.
fn positioned(err: Box<EvalAltResult>, pos: Position) -> Box<EvalAltResult> {
    if err.position().is_none() {
        reposition(err, pos)
    } else {
        err
    }
}

/// Stamp the call site on anything a dispatched call came back with.
///
/// This is `fill_position`, which Rhai applies to the whole of
/// `exec_native_fn_call` — the callee not being found, the call being refused,
/// and the error a native *returned* alike (`func/call.rs:365`, `:406`, `:413`).
///
/// What looks like a counter-example is not one: `1 / 0` reports
/// `ErrorArithmetic` with no position at all, because under `fast_operators` a
/// binary operator returns the built-in's error without going through here
/// (`func/call.rs:1798`). The VM's own fast path skips it for the same reason.
fn dispatch_failure(err: Box<EvalAltResult>, pos: Position) -> Box<EvalAltResult> {
    positioned(err, pos)
}

/// A scope entry, addressed the way whatever wants it was written.
///
/// A slot always names one and a name may name nothing, which is the whole of
/// the difference between the two at run time — for a [`Receiver`] and for a
/// [`Root`] alike.
#[derive(Clone, Copy)]
enum Site<'a> {
    Slot(usize),
    Name(&'a str),
}

/// What a chain turned out to be rooted at, and the value to walk.
///
/// Rhai draws this line in `search_namespace`, which hands back a `Target`: a
/// scope entry becomes a reference to write through, and a resolver's answer or
/// a module's constant becomes a read-only temporary (`eval/expr.rs:120-155`).
/// Which one a [`Root::Named`] is cannot be known until it is looked up.
///
/// Whichever it is, the value keeps the access mode it was found with, and that
/// is not decoration: it is the only thing standing between a `const` and a
/// method that mutates it — `exec_native_fn_call` refuses a non-pure function
/// whose first argument is read-only (`func/call.rs:405`).
enum RootValue<'s> {
    /// A scope entry, walked where it lives.
    ///
    /// Nothing is written back afterwards because nothing was copied: a
    /// mutation partway down the chain landed in the entry itself, which is
    /// what Rhai's `Target::RefMut` does (`eval/chaining.rs:517-563`).
    Entry(&'s mut Dynamic),

    /// A value with a name but no entry behind it — a resolver's answer, or a
    /// module's constant. Nowhere to write back to.
    Detached(Dynamic),

    /// The frame's receiver, moved out of the register for the walk.
    ///
    /// A register cannot lend a `&mut` across the `&mut self` the walk needs,
    /// so this is the one root that still travels: moved out here and moved
    /// back by [`Vm::run_chain`], the same trade [`bind_this`] makes.
    This(Dynamic),

    /// A value taken off the operand stack: `[1, 2].len()`, `f().x`.
    ///
    /// Nothing can be assigned to one, because Rhai's parser refuses it
    /// outright. Moved rather than borrowed for the same reason [`Self::This`]
    /// is — the stack is part of `self`.
    Temporary(Dynamic),
}

impl RootValue<'_> {
    /// The value to walk.
    fn as_mut(&mut self) -> &mut Dynamic {
        match self {
            Self::Entry(value) => value,
            Self::Detached(value) | Self::This(value) | Self::Temporary(value) => value,
        }
    }
}

/// A chain's root, looked up.
struct ChainRoot<'s> {
    value: RootValue<'s>,
    /// Where to blame a refusal: the variable for a name, the chain otherwise.
    pos: Position,
}

/// What one indexing step managed.
#[cfg(not(all(feature = "no_index", feature = "no_object")))]
enum Indexed {
    /// Taken through a reference: the value, and whether anything wrote.
    Done(Dynamic, bool),
    /// There was no reference to take. Carries the value back out, because the
    /// caller cannot touch the container until this borrow has ended.
    NoReference(Dynamic),
}

/// What a chain's root is called, for the two errors that name it.
///
/// `None` for a temporary, which has no name to give — and neither error can
/// reach one: nothing assigns through a temporary, and flattening it on the way
/// in means there is no cell left to contend for.
///
/// `this` *can* reach both and has no name either, so it answers with the empty
/// one rather than with nothing. That is Rhai's own answer: `Expr::ThisPtr`
/// carries no name, so assigning through a read-only receiver is
/// `ErrorAssignmentToConstant("")` (`eval/stmt.rs:118-122`). Answering `None`
/// here would report a malformed chunk instead.
fn root_name<'p>(program: &'p Program, chain: &Chain) -> Option<&'p str> {
    match chain.root {
        Root::Local { name, .. } | Root::Named { name, .. } => program.name(name),
        Root::This { .. } => Some(""),
        Root::Temporary => None,
    }
}

/// Where chain `index`'s slots start in the sidecar's flat stream.
///
/// Walked rather than cached: only a chain that already failed asks.
fn chain_slot_base(program: &Program, index: u32) -> u32 {
    program
        .chains()
        .iter()
        .take(index as usize)
        .map(Chain::position_slots)
        .sum()
}

/// The op-assignment a chain ends with, resolved out of the pool.
fn chain_op<'p>(
    program: &'p Program,
    chain: &Chain,
) -> Result<Option<&'p AssignOp>, Box<EvalAltResult>> {
    let Tail::Assign { op: Some(op) } = &chain.tail else {
        return Ok(None);
    };
    program
        .assign_op(*op)
        .map(Some)
        .ok_or_else(|| malformed(format!("no op-assignment {op}")))
}

/// One `for` loop in progress.
///
/// The count is here rather than in a local because Rhai keeps it outside the
/// scope too, and checks it for overflow before writing it — a loop long
/// enough to wrap the counter is an error rather than a wrap
/// (`eval/stmt.rs:729`).
struct Iteration {
    items: Box<dyn Iterator<Item = VmResult>>,
    /// The index of the item last handed out, starting one below the first.
    count: INT,
}

/// A scope entry as a place to write, seeing through a shared cell.
///
/// Rhai reaches a variable through a `Target`, whose shared arm hands over the
/// cell's guard rather than the cell (`eval/target.rs:409-422`), so an
/// assignment lands where every closure holding that cell can see it. Writing
/// the slot itself would replace the cell and quietly sever them — the value
/// would be right and the aliasing dead.
///
/// For an ordinary value `write_lock` is a downcast to itself, so the common
/// case pays nothing. Rhai's own for-loop does this with `.unwrap()` and
/// panics on a contended cell; a VM that promises errors instead of panics
/// reports `ErrorDataRace`, as `Target` does.
fn place<'a>(
    entry: &'a mut Dynamic,
    name: &str,
    pos: Position,
) -> Result<DynamicWriteLock<'a, Dynamic>, Box<EvalAltResult>> {
    entry
        .write_lock::<Dynamic>()
        .ok_or_else(|| Box::new(EvalAltResult::ErrorDataRace(name.to_string(), pos)))
}

/// Turn the two control-flow errors back into the value they carry.
///
/// Rhai unwinds `return` and `exit` as errors rather than returning them;
/// `eval_global_statements` is where they turn back into values, and anything
/// entering a program from outside has to do the same.
fn unwind_exit(result: VmResult) -> VmResult {
    result.or_else(|err| match *err {
        EvalAltResult::Return(out, ..) | EvalAltResult::Exit(out, ..) => Ok(out),
        _ => Err(err),
    })
}

fn missing(name: &str, pos: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorVariableNotFound(name.to_string(), pos))
}

fn malformed(detail: String) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!("malformed chunk: {detail}").into(),
        Position::NONE,
    ))
}

/// One frame of a failed run, as an address rather than a position.
///
/// What a stripped program reports instead. Chunks share one instruction
/// buffer, so an address names an instruction whichever chunk it is in.
///
/// Travels from the device that failed to the host holding the
/// [`Sidecar`](crate::grain::Sidecar), by whatever the link uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Fault {
    /// Byte offset of the instruction this frame stopped at.
    pub address: usize,
    /// Which chain slot raised, for an
    /// [`Op::Chain`](crate::grain::bytecode::Op::Chain).
    ///
    /// One instruction walks every step of `a.b[i].c` and gets one address
    /// between them, so the slot is what separates them.
    pub slot: Option<u32>,
}

/// Executes a [`Program`] against an `Engine`.
///
/// Holds one `GlobalRuntimeState` and one `Caches` for its whole lifetime, so
/// the function-resolution cache survives across calls. That matters: the
/// reentrant helpers Rhai exposes to native functions build a fresh
/// `Caches::new()` per call (`func/native.rs:519`), which would throw away
/// resolution work on every dispatch.
pub struct Vm<'e> {
    engine: &'e Engine,
    global: GlobalRuntimeState,
    caches: Caches,
    stack: Vec<Dynamic>,
    #[cfg_attr(any(feature = "no_index", feature = "no_object"), allow(unused))]
    strings_interner: StringsInterner,
    /// One entry per `for` loop currently running.
    ///
    /// Not on the operand stack, because an iterator is not a `Dynamic`. A
    /// frame truncates this to what it found on entry, so a `return` or an
    /// escaping error drops whatever its loops were holding without the
    /// compiler emitting anything.
    iterators: Vec<Iteration>,
    /// One entry per `try` region currently armed or catching. Frame-floored
    /// the same way the iterators are, so an error in a called function can
    /// never find its caller's handler and jump into another chunk.
    handlers: Vec<Handler>,
    /// The running data-size total of each literal currently being built,
    /// innermost last.
    ///
    /// One entry per array or map literal under construction, so
    /// `[a, [b, c], d]` keeps the inner total separate from the outer.
    /// Truncated per frame, as the iterators are, so an error part way through
    /// a literal leaves nothing behind.
    sizes: Vec<(usize, usize, usize)>,
    /// Where the scope goes back to if an error escapes the running frame.
    ///
    /// Set by [`Op::Checkpoint`](crate::bytecode::Op::Checkpoint) at each
    /// top-level statement of a chunk, saved and restored per frame. See
    /// [`Vm::execute`].
    unwind_floor: usize,
    /// The receiver bound to the frame currently running.
    ///
    /// Owned rather than borrowed: the value a binder has to hand over always
    /// lives in `stack` or in a caller's `Scope`, and neither can lend a `&mut`
    /// across the `&mut self` call that runs the callee. So the binder moves it
    /// in, the frame owns it, and [`Vm::call_compiled_with_this`] hands it back
    /// for the binder to put where it came from.
    ///
    /// Saved and restored per *call* rather than per frame, which is what makes
    /// `this` never inherited: a plain nested call passes `None` and gets the
    /// caller's back on the way out, reproducing `func/call.rs:669` without a
    /// conditional anywhere.
    this: Option<Dynamic>,
    /// Whether this `Vm` started the run, and so may clear its trace.
    ///
    /// False for a [`Vm::reentrant`]: a callback clearing the trace would
    /// discard frames the run around it already recorded.
    owns_trace: bool,
    /// Which step the innermost chain walk has reached.
    ///
    /// Saved and restored per chain, because a method step can run a body
    /// holding another chain. See [`Vm::run_chain`].
    chain_step: usize,
    /// A failing chain's slot, waiting for the frame it happened in.
    ///
    /// Set by [`Vm::run_chain`], taken by the next [`Vm::record_fault`].
    pending_slot: Option<u32>,
    /// Steps waiting for the statement that asked for them to end.
    ///
    /// Rhai keeps this in a `defer` per AST node (`eval/stmt.rs:271`): a `next`
    /// runs the statement it was asked at with the debugger quiet, and re-arms
    /// once that statement is over. A marker is a point rather than a scope, so
    /// the call level and statement depth it was asked at are recorded here
    /// instead, and the next marker at or outside them is where the statement
    /// has ended. Innermost last.
    #[cfg(feature = "debugging")]
    pending_steps: Vec<(usize, u16, crate::eval::DebuggerStatus)>,
}

/// Take a receiver for a callee to own, and say whether it goes back.
///
/// The rule is `chain_root`'s `walkable` (see [`Vm::chain_root`]) plus the
/// question of ownership. A read-only receiver is *cloned*, because `take`
/// would leave `UNIT` behind and strip the const-ness the caller's slot is
/// carrying; nothing is written back, since Rhai refuses the mutation rather
/// than making it. Anything else is taken, as `call_compiled_body` already
/// takes a call's arguments — a receiver is not shared with anything the callee
/// can reach, and taking it saves a deep clone of an array or a map per call.
///
/// A shared receiver is taken like any other: the taken value *is* the cell, so
/// `is_shared(this)` stays true inside the body and a write lands where every
/// other holder can see it. It still goes back, because taking it left `UNIT`
/// where the cell was.
fn bind_this(receiver: &mut Dynamic) -> (Dynamic, bool) {
    if receiver.is_read_only() {
        (receiver.clone().into_read_only(), false)
    } else {
        (mem::take(receiver), true)
    }
}

/// Put a receiver back where [`bind_this`] took it from.
///
/// Unconditional on how the call ended. Rhai reaches `this` through a pointer
/// into the caller's storage, so a body that mutates and then raises has
/// already written; a binder that only restored on success would discard
/// exactly that.
fn unbind_this(receiver: &mut Dynamic, taken: Option<Dynamic>, write_back: bool) {
    if let (true, Some(value)) = (write_back, taken) {
        *receiver = value;
    }
}

/// A `try` region.
struct Handler {
    target: usize,
    catch_var: Option<u32>,
    /// Where the three stacks were when the region was entered. An error can
    /// be raised at any depth of all three, and the catch block has to begin
    /// where the `try` did.
    operands: usize,
    scope_len: usize,
    iters: usize,
    /// Set once the catch block is running, holding the error it caught. That
    /// is what a bare `throw;` in the catch block re-raises, and its presence
    /// is what tells an escaping error it is leaving a catch rather than
    /// entering one.
    caught: Option<Box<EvalAltResult>>,
}

impl<'e> Vm<'e> {
    /// A VM that dispatches through `engine`.
    #[must_use]
    pub fn new(engine: &'e Engine) -> Self {
        let mut global = engine.new_global_runtime_state();

        // Eagerly, because a callback gets a *clone* of this state and a clone
        // of `None` is a separate `None`. Allocating on first fault would leave
        // every callback recording into a cell of its own.
        global.grain_faults = Some(crate::Shared::new(crate::Locked::new(Vec::new())));

        Self {
            engine,
            global,
            caches: Caches::new(),
            strings_interner: StringsInterner::new(256),
            stack: Vec::new(),
            iterators: Vec::new(),
            handlers: Vec::new(),
            sizes: Vec::new(),
            unwind_floor: 0,
            this: None,
            owns_trace: true,
            chain_step: 0,
            pending_slot: None,
            #[cfg(feature = "debugging")]
            pending_steps: Vec::new(),
        }
    }

    /// A `Vm` for a call arriving from inside a native function.
    ///
    /// Reproduces what Rhai does at every reentrant boundary
    /// (`func/native.rs:516-519`, `types/fn_ptr.rs:451-454`): the caller's
    /// runtime state is *cloned* rather than shared, and the resolution cache
    /// starts empty. The clone is what carries the imported modules, the source
    /// name and — the part that matters here — the function library holding the
    /// wrappers, so a closure reached from a native can hand out a pointer of
    /// its own.
    ///
    /// The empty `Caches` is the cost, and it is the one thing a `Vm` normally
    /// exists to avoid. It cannot be helped: the outer `Vm` is borrowed by the
    /// frame still running beneath this one. Rhai pays the same on its own
    /// callbacks — but it also skips resolution entirely for a pointer that
    /// carries its body, which is why a crossing measures 0.34x. See the
    /// `callback` module.
    ///
    /// Operation counting has the same shape and the same reason: increments
    /// inside the callback land on the clone and are lost when it drops, as
    /// they are for any reentrant call Rhai makes.
    #[must_use]
    pub fn reentrant(context: &'e NativeCallContext<'_>) -> Self {
        Self {
            engine: context.engine(),
            global: context.global_runtime_state().clone(),
            caches: Caches::new(),
            strings_interner: StringsInterner::new(256),
            stack: Vec::new(),
            iterators: Vec::new(),
            handlers: Vec::new(),
            sizes: Vec::new(),
            unwind_floor: 0,
            // A crossing carries no receiver: Rhai binds one only where it
            // dispatches a method, and this arrives through `call_fn_raw`.
            this: None,
            // `global` is a clone, so the trace is shared with the run that
            // called in and is not this call's to clear.
            owns_trace: false,
            chain_step: 0,
            pending_slot: None,
            // A step belongs to the statement that asked for it, and that
            // statement is running in the `Vm` this crossing came from.
            #[cfg(feature = "debugging")]
            pending_steps: Vec::new(),
        }
    }

    /// Where the last run failed, innermost frame first.
    ///
    /// What a stripped program reports instead of a position;
    /// [`restore`](crate::grain::restore::restore) turns it back into one.
    ///
    /// Cleared at the start of a run and whenever a `catch` handles an error,
    /// so it describes the failure actually being reported.
    #[must_use]
    pub fn fault_trace(&self) -> Vec<Fault> {
        self.global
            .grain_faults
            .as_ref()
            .and_then(|faults| crate::func::native::locked_read(faults))
            .map(|faults| faults.clone())
            .unwrap_or_default()
    }

    /// Which instruction the last run failed at, if it failed.
    ///
    /// The innermost frame of [`Vm::fault_trace`].
    #[must_use]
    pub fn fault_pc(&self) -> Option<usize> {
        self.fault_trace().first().map(|fault| fault.address)
    }

    /// Note that a frame stopped at `address`, as an error leaves it.
    fn record_fault(&mut self, address: usize) {
        let fault = Fault {
            address,
            slot: self.pending_slot.take(),
        };

        // Normally already there, from `Vm::new` or from a reentrant call's
        // clone. The fallback covers a `Vm` reached some other way.
        let faults = self
            .global
            .grain_faults
            .get_or_insert_with(|| crate::Shared::new(crate::Locked::new(Vec::new())));

        if let Some(mut faults) = crate::func::native::locked_write(faults) {
            faults.push(fault);
        }
    }

    /// Forget where a run failed, because it did not or has not yet.
    ///
    /// A no-op on a reentrant `Vm`: the trace belongs to the run that called it.
    fn clear_faults(&mut self) {
        if !self.owns_trace {
            return;
        }

        self.pending_slot = None;
        if let Some(faults) = self.global.grain_faults.as_ref() {
            if let Some(mut faults) = crate::func::native::locked_write(faults) {
                faults.clear();
            }
        }
    }

    /// Run a program, returning its value.
    ///
    /// Mirrors `Engine::eval_ast_with_scope_raw`: the program's function
    /// library, module resolver and source name are installed for the duration
    /// and restored afterwards, so a `Vm` reused across programs does not leak
    /// one program's definitions into the next.
    /// Call one compiled function by name, with arguments already evaluated.
    ///
    /// The entry point a native needs. `Op::Call` reaches a chunk through the
    /// name *index* it shares with the call site, which a caller from outside
    /// does not have — a `FnPtr` carries a string, and so does Rhai when it
    /// dispatches. This is the same call by the other key.
    ///
    /// `level` is the caller's call depth, so `max_call_levels` still counts
    /// across a boundary that leaves this VM and comes back. Left unthreaded,
    /// a closure calling itself through `map` would recurse until the stack
    /// went rather than until the limit did.
    ///
    /// # Errors
    ///
    /// `ErrorFunctionNotFound` if no compiled function has that name and
    /// arity, and whatever the function itself raises otherwise.
    pub fn call_function(
        &mut self,
        program: &Program,
        name: &str,
        args: FnArgsVec<Dynamic>,
        level: usize,
        pos: Position,
    ) -> VmResult {
        let scope = &mut Scope::new();
        self.call_function_with_this(program, name, args, level, scope, true, pos, None)
            .0
    }

    /// The same, against a receiver the callee owns for the duration.
    ///
    /// The receiver comes back however the call ended, for the caller to put
    /// where it took it from — see [`bind_this`] and [`unbind_this`].
    fn call_function_with_this(
        &mut self,
        program: &Program,
        name: &str,
        args: FnArgsVec<Dynamic>,
        level: usize,
        scope: &mut Scope,
        rewind_scope: bool,
        pos: Position,
        this: Option<Dynamic>,
    ) -> (VmResult, Option<Dynamic>) {
        // An entry point in its own right so the trace starts here rather than at `run_main`.
        self.clear_faults();
        // A step a previous run left waiting is not this one's to honour.
        #[cfg(feature = "debugging")]
        self.pending_steps.clear();

        let Some(function) = program.function_named(name, args.len()) else {
            return (
                Err(Box::new(EvalAltResult::ErrorFunctionNotFound(
                    format!("{name} ({} args)", args.len()),
                    pos,
                ))),
                this,
            );
        };
        let (params, chunk) = (function.params.clone(), function.chunk);

        // `call_compiled` takes its arguments off the operand stack, where a
        // compiled call site would already have put them.
        let first = self.stack.len();
        self.stack.extend(args);

        let restore = mem::replace(&mut self.global.level, level);
        let (result, this) = self.call_compiled_with_this(
            program,
            name,
            &params,
            chunk,
            first,
            scope,
            rewind_scope,
            pos,
            this,
        );
        self.global.level = restore;

        self.stack.truncate(first);
        (result, this)
    }

    /// Call one compiled function by name, instead of running the whole
    /// program.
    ///
    /// Mirrors [`Engine::call_fn`](crate::Engine::call_fn), including that the
    /// program's body runs first — a function usually needs what the top level
    /// declared. [`call_fn_with_options`](Self::call_fn_with_options) turns
    /// that off.
    ///
    /// Not available under `no_function`.
    ///
    /// # Errors
    ///
    /// `ErrorFunctionNotFound` if no compiled function has that name and
    /// arity, `ErrorMismatchOutputType` if the result is not a `T`, and
    /// whatever the function itself raises.
    #[cfg(not(feature = "no_function"))]
    pub fn call_fn<T: Variant + Clone>(
        &mut self,
        scope: &mut Scope,
        program: &Program,
        name: impl AsRef<str>,
        args: impl crate::FuncArgs,
    ) -> Result<T, Box<EvalAltResult>> {
        self.call_fn_with_options(CallFnOptions::new(), scope, program, name, args)
    }

    /// The same, with Rhai's [`CallFnOptions`](crate::CallFnOptions).
    ///
    /// Three of the five options mean something here:
    ///
    /// * `eval_ast` runs the program's main chunk before the call, so what the
    ///   top level declares is in scope for it. On by default, as in Rhai.
    /// * `rewind_scope` truncates the scope back afterwards. On by default.
    /// * `tag` sets the evaluation's custom state.
    ///
    /// `this_ptr` binds the callee's receiver, as it does in Rhai. A write
    /// through `this` lands in the pointer's own `Dynamic` — including when the
    /// call goes on to fail, because Rhai reaches `this` through the caller's
    /// storage and a body that mutates and then raises has already written.
    ///
    /// `in_all_namespaces` is ignored: this looks only in the program's own
    /// compiled functions.
    ///
    /// Not available under `no_function`.
    ///
    /// # Errors
    ///
    /// As [`call_fn`](Self::call_fn).
    #[cfg(not(feature = "no_function"))]
    pub fn call_fn_with_options<T: Variant + Clone>(
        &mut self,
        options: CallFnOptions,
        scope: &mut Scope,
        program: &Program,
        name: impl AsRef<str>,
        args: impl crate::FuncArgs,
    ) -> Result<T, Box<EvalAltResult>> {
        let name = name.as_ref();

        let mut arg_values = FnArgsVec::new();
        args.parse(&mut arg_values);

        let orig_scope_len = scope.len();
        let mut this_ptr = options.this_ptr;
        if let Some(tag) = options.tag {
            self.global.tag = tag;
        }

        // The pointer is the host's and outlives the call, so unlike every
        // other binder this one can hold it across the whole thing.
        let bound = this_ptr.as_deref_mut().map(bind_this);
        let (this, write_back) = bound.map_or((None, false), |(v, w)| (Some(v), w));

        // The program's environment stays installed for the *call*, not only for
        // the main chunk that may precede it: the function being called can
        // reach whatever the compiler left Rhai to interpret, and Rhai looks for
        // it in `global.lib`.
        let mut evaluated = Ok(());
        let (result, returned) = self.with_environment(program, None, |vm| {
            if options.eval_ast {
                // Run for the scope it leaves behind; the body's own value is
                // not what the caller asked for. The main chunk gets no
                // receiver, as `eval_global_statements` does not either.
                evaluated = unwind_exit(vm.run_main(program, scope)).map(|_| ());
            }

            let (result, returned) = if evaluated.is_ok() {
                vm.call_function_with_this(
                    program,
                    name,
                    arg_values,
                    0,
                    scope,
                    options.rewind_scope,
                    Position::NONE,
                    this,
                )
            } else {
                (Ok(Dynamic::UNIT), this)
            };

            // However it went, and whether the call happened at all: Rhai
            // reports the end of a `call_fn` unconditionally
            // (`api/call_fn.rs:302-308`).
            #[cfg(feature = "debugging")]
            let result = vm.at_end(scope).and(result);

            (result, returned)
        });

        // Before the errors below, both of them: Rhai's mutation through `this`
        // survives a failed call, and survives one whose result is the wrong
        // type just as much.
        if let Some(slot) = this_ptr {
            unbind_this(slot, returned, write_back);
        }
        evaluated?;

        if options.rewind_scope {
            scope.rewind(orig_scope_len);
        }

        result?.try_cast_result().map_err(|value| {
            Box::new(EvalAltResult::ErrorMismatchOutputType(
                self.engine
                    .map_type_name(core::any::type_name::<T>())
                    .into(),
                self.engine.map_type_name(value.type_name()).into(),
                Position::NONE,
            ))
        })
    }

    /// Evaluate a program's main chunk against `scope`, yielding its value.
    ///
    /// The scope is the caller's, as it is for
    /// [`Engine::eval_ast_with_scope`](crate::Engine::eval_ast_with_scope):
    /// what the program declares at the top level is left in it, and what the
    /// caller put there beforehand is visible to the program.
    ///
    /// # Errors
    ///
    /// Whatever the program raises, and `ErrorRuntime` for a malformed one.
    pub fn eval_with_scope(&mut self, scope: &mut Scope, program: &Program) -> VmResult {
        self.run_with(program, scope, None)
    }

    /// The same against a scope of its own, for a program that needs none.
    ///
    /// # Errors
    ///
    /// As [`eval_with_scope`](Self::eval_with_scope).
    pub fn eval(&mut self, program: &Program) -> VmResult {
        self.eval_with_scope(&mut Scope::new(), program)
    }

    /// Evaluate a program against `scope` for its effects, discarding its value.
    ///
    /// # Errors
    ///
    /// As [`eval_with_scope`](Self::eval_with_scope).
    pub fn run_with_scope(
        &mut self,
        scope: &mut Scope,
        program: &Program,
    ) -> Result<(), Box<EvalAltResult>> {
        self.eval_with_scope(scope, program).map(|_| ())
    }

    /// The same against a scope of its own.
    ///
    /// # Errors
    ///
    /// As [`eval_with_scope`](Self::eval_with_scope).
    pub fn run(&mut self, program: &Program) -> Result<(), Box<EvalAltResult>> {
        self.run_with_scope(&mut Scope::new(), program)
    }

    /// Evaluate a program that hands function pointers to native functions.
    ///
    /// The same run, plus one native wrapper per compiled function registered
    /// for its duration, so a pointer this program creates resolves when Rhai
    /// dispatches it — `let a = [1, 2]; a.map(|x| x * 2)` is `map` calling us
    /// back, and `map` looks the pointer up its own way. See the `callback`
    /// module.
    ///
    /// Only worth the owned program when [`Program::makes_fn_pointers`] says a
    /// pointer can escape; [`eval_with_scope`](Self::eval_with_scope) is
    /// otherwise identical and copies nothing. A program that needs this and
    /// does not get it still runs — the pointer simply fails to resolve, as
    /// `ErrorFunctionNotFound`, at the point the native tries to call it.
    ///
    /// Read the `callback` module before relying on it: a crossing is slower than the
    /// walker, and a *capturing* closure handed to a native that binds `this`
    /// arrives with its arguments rotated.
    ///
    /// Named `eval_` rather than `run_` because it yields the program's value;
    /// Rhai has no `Engine` method to mirror here, so the crate's own rule is
    /// the one that applies.
    ///
    /// # Errors
    ///
    /// As [`eval_with_scope`](Self::eval_with_scope).
    pub fn eval_with_callbacks(&mut self, scope: &mut Scope, program: &SharedProgram) -> VmResult {
        let wrappers =
            (!program.functions().is_empty()).then(|| callback::wrappers(program).into());
        self.run_with(program, scope, wrappers)
    }

    fn run_with(
        &mut self,
        program: &Program,
        scope: &mut Scope,
        wrappers: Option<SharedModule>,
    ) -> VmResult {
        self.with_environment(program, wrappers, |vm| {
            // `exit` and a top-level `return` become the run's value before the
            // debugger hears that it is over, because Rhai maps them inside
            // `eval_global_statements` (`eval/stmt.rs:1046-1048`) and reports
            // the end after it.
            let value = unwind_exit(vm.run_main(program, scope))?;
            #[cfg(feature = "debugging")]
            vm.at_end(scope)?;
            Ok(value)
        })
    }

    /// What a program contributes to `global`, in place for the duration of `f`.
    ///
    /// Everything entering a program from outside needs this, not only its main
    /// chunk: a function reached through [`call_fn`](Self::call_fn) can call
    /// whatever the compiler left Rhai to interpret, and Rhai looks for it in
    /// `global.lib`.
    fn with_environment<T>(
        &mut self,
        program: &Program,
        wrappers: Option<SharedModule>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let orig_source = mem::replace(&mut self.global.source, program.source().cloned());
        #[cfg(not(feature = "no_function"))]
        let orig_lib_len = self.global.lib.len();
        #[cfg(not(feature = "no_function"))]
        {
            if let Some(lib) = program.lib() {
                self.global.lib.push(lib.clone());
            }
            // Last, so the search — which runs in reverse — reaches a compiled
            // function before whatever the compiler left Rhai to interpret.
            if let Some(wrappers) = wrappers {
                self.global.lib.push(wrappers);
            }
        }
        // Without script functions there is no library to stack them in, and
        // `wrappers` is `None` for want of anything to wrap.
        #[cfg(feature = "no_function")]
        let _ = wrappers;
        #[cfg(not(feature = "no_module"))]
        let orig_resolver = mem::replace(
            &mut self.global.embedded_module_resolver,
            program.resolver().cloned(),
        );

        let result = f(self);

        #[cfg(not(feature = "no_module"))]
        {
            self.global.embedded_module_resolver = orig_resolver;
        }
        #[cfg(not(feature = "no_function"))]
        self.global.lib.truncate(orig_lib_len);
        self.global.source = orig_source;

        result
    }

    /// The main chunk, against an environment the caller has already installed.
    fn run_main(&mut self, program: &Program, scope: &mut Scope) -> VmResult {
        self.clear_faults();
        // A step a previous run left waiting is not this one's to honour.
        #[cfg(feature = "debugging")]
        self.pending_steps.clear();
        let mut pc = program.main().entry() as usize;
        // Slots are indices into the caller's scope, so a caller that arrives
        // with variables already in it shifts every one of them.
        let base = scope.len();
        let result = self.execute(program, scope, *program.main(), base, &mut pc);
        if result.is_err() {
            // Whatever failed deeper already recorded itself,
            // and this is the outermost frame of the same failure.
            self.record_fault(pc);
        }
        result
    }

    fn pop(&mut self) -> Result<Dynamic, Box<EvalAltResult>> {
        self.stack
            .pop()
            .ok_or_else(|| malformed("operand stack underflow".to_string()))
    }

    fn inspect(&mut self) -> Result<&Dynamic, Box<EvalAltResult>> {
        self.stack
            .last()
            .ok_or_else(|| malformed("operand stack underflow".to_string()))
    }

    /// `reached` tracks the instruction being executed, so a failure can be
    /// attributed to one. It is what a stripped program reports in place of a
    /// position.
    /// Call a chunk this compiler produced, reproducing `call_script_fn`
    /// (`func/script.rs:24`) step for step.
    ///
    /// The parts that are not obvious, and that the differential corpus is
    /// what proves: arguments are *taken* out of the caller's stack slots
    /// rather than cloned, the depth check happens after the level is
    /// incremented, and only errors that are neither a `return` nor a system
    /// exception get wrapped in `ErrorInFunctionCall`.
    /// Walk `a.b[i].c`, reading it or assigning to it.
    ///
    /// The reason this is one instruction and a recursion rather than a
    /// sequence: every level holds a `&mut` into the level above, exactly as
    /// Rhai does (`eval/chaining.rs:659`). For a map or an array that borrow is
    /// the whole story — the mutation lands in the container and no write-back
    /// is needed. Doing it on the operand stack instead would mutate a copy.
    ///
    /// That starts at the root: a scope entry is walked where it lives, not
    /// copied out and put back. Copying it would make a chain cost the size of
    /// what it is rooted at rather than the length of the chain, which for
    /// `a[i] = x` in a loop is the difference between linear and quadratic.
    ///
    /// The exceptions are the two roots a `&mut` cannot be had for, because
    /// they live in `self` and the walk needs `&mut self`: the `this` register
    /// and the operand stack. Both are *moved* rather than cloned, and `this`
    /// is moved back.
    ///
    /// Write-back is only for the levels where a borrow was not possible:
    /// a getter on a host type hands back a value, and Rhai calls the setter
    /// afterwards if the sub-chain was a method call. `changed` reproduces
    /// that, and it is deliberately coarse in the same way Rhai's is — Rhai's
    /// flag is `func.is_method()`, "does the resolved function take its
    /// receiver by reference", not "did it actually write".
    fn run_chain(
        &mut self,
        program: &Program,
        chain: &Chain,
        index: u32,
        scope: &mut Scope,
        base: usize,
        pos: Position,
    ) -> VmResult {
        // A method step can run a body holding a chain of its own, which writes
        // the same field. Saved so the step reported is this chain's.
        let outer_step = mem::replace(&mut self.chain_step, 0);
        let result = self.run_chain_steps(program, chain, scope, base, pos);
        let reached = mem::replace(&mut self.chain_step, outer_step);

        if result.is_err() {
            // Not if something deeper already claimed it: the slot belongs to
            // the innermost frame's address.
            if self.pending_slot.is_none() {
                self.pending_slot = chain
                    .step_slot(reached)
                    .map(|slot| chain_slot_base(program, index) + slot);
            }
        }

        result
    }

    /// [`Vm::run_chain`] without the bookkeeping around it.
    fn run_chain_steps(
        &mut self,
        program: &Program,
        chain: &Chain,
        scope: &mut Scope,
        base: usize,
        pos: Position,
    ) -> VmResult {
        // Step operands were pushed first, then the root if it is one that has
        // to be evaluated, then the value being assigned.
        let operands_at = self
            .stack
            .len()
            .checked_sub(chain.consumes())
            .ok_or_else(|| malformed("chain with too few operands".to_string()))?;

        let ChainRoot {
            value: mut root,
            pos: root_pos,
        } = self.chain_root(program, chain, scope, base, operands_at, pos)?;

        // Nothing between here and the restore below may use `?`: a `this` root
        // was moved out of the register, and an early return would drop it on
        // the floor. Everything that can fail travels in `result` instead.
        let read_only = root.as_mut().is_read_only();

        // Read-only is what refuses an assignment, not the absence of a place:
        // a module's constant is neither, so Rhai assigns into the copy and
        // discards it. Only a `const` and a resolver's answer are refused, and
        // both are read-only for that reason.
        //
        // A temporary is separate again — Rhai's parser refuses `f().x = 1`
        // outright, so one reaches here only from a chunk this compiler did
        // not build.
        let value = match (chain.assigns(), &root) {
            (false, _) => Ok(None),
            (true, RootValue::Temporary(..)) => {
                Err(malformed("chain assigns through a temporary root".into()))
            }
            (true, _) if read_only => Err(match root_name(program, chain) {
                Some(name) => Box::new(EvalAltResult::ErrorAssignmentToConstant(
                    name.to_string(),
                    root_pos,
                )),
                None => malformed("no chain root name".to_string()),
            }),
            // Rhai flattens the right-hand side before assigning, so a shared
            // cell is copied out rather than aliased in.
            (true, _) => Ok(Some(self.stack[self.stack.len() - 1].clone().flatten())),
        };

        let result = value.and_then(|value| {
            let mut operands: FnArgsVec<Dynamic> = self.stack
                [operands_at..operands_at + chain.operands as usize]
                .iter()
                .cloned()
                .collect();

            // A shared cell cannot be walked directly. `get_indexed_mut`
            // refuses one outright — `unreachable!("cannot handle shared
            // values")`, `eval/chaining.rs:461` — because Rhai always reaches a
            // root through a `Target`, whose shared arm hands over the guard
            // rather than the cell. Walking the cell would take the host down,
            // so this is a panic-safety fix and not only a correctness one.
            if is_shared!(*root.as_mut()) {
                let mut guard = root.as_mut().write_lock::<Dynamic>().ok_or_else(|| {
                    let name = root_name(program, chain).unwrap_or_default();
                    Box::new(EvalAltResult::ErrorDataRace(name.to_string(), pos))
                })?;
                self.walk_chain(
                    program,
                    chain,
                    &chain.steps,
                    &mut guard,
                    &mut operands,
                    value,
                    pos,
                )
            } else {
                self.walk_chain(
                    program,
                    chain,
                    &chain.steps,
                    root.as_mut(),
                    &mut operands,
                    value,
                    pos,
                )
            }
        });

        // The receiver is the one root that travels rather than being walked
        // where it lives, so it goes back — and it goes back however the walk
        // ended. Rhai reaches `this` through a pointer into the caller's
        // storage, so a body that mutated and then raised has already written;
        // restoring only on success would discard exactly that.
        if let RootValue::This(value) = root {
            self.this = Some(value);
        }

        let (out, _) = result?;
        self.stack.truncate(operands_at);
        Ok(out)
    }

    /// Find what a chain is rooted at, resolving a name if that is what it is.
    ///
    /// The search is `load_named`'s and the order is observable: a resolver
    /// registered with `Engine::on_var` sees the name before the scope does,
    /// and a name in no scope is looked for among the global modules before it
    /// is reported missing. It runs exactly once — a chain is one instruction,
    /// so unlike [`Op::CallRef`] there is nothing to resolve twice.
    ///
    /// `ErrorVariableNotFound` is reported against the *variable*, which is why
    /// [`Root::Named`] carries a position of its own.
    fn chain_root<'s>(
        &mut self,
        program: &Program,
        chain: &Chain,
        scope: &'s mut Scope,
        base: usize,
        operands_at: usize,
        pos: Position,
    ) -> Result<ChainRoot<'s>, Box<EvalAltResult>> {
        match chain.root {
            Root::Local { slot, .. } => {
                let index = base + slot as usize;
                if index >= scope.len() {
                    return Err(malformed(format!(
                        "chain root slot {index} is out of scope"
                    )));
                }
                Ok(ChainRoot {
                    value: RootValue::Entry(scope.get_mut_by_index(index)),
                    pos,
                })
            }

            // The `this` position wins over the chain's for the same reason a
            // name's does: this lookup can fail, and Rhai blames the `this`
            // rather than the `.` after it (`eval/chaining.rs:519-527`).
            Root::This { pos: this_pos } => {
                let value = self
                    .this
                    .take()
                    .ok_or_else(|| Box::new(EvalAltResult::ErrorUnboundThis(this_pos)))?;
                Ok(ChainRoot {
                    value: RootValue::This(value),
                    pos: this_pos,
                })
            }

            // A name has a position of its own, and it wins: the lookup below
            // can fail, and Rhai blames the variable rather than the chain.
            Root::Named { name, pos: var_pos } => {
                let name = program
                    .name(name)
                    .ok_or_else(|| malformed(format!("no name {name}")))?;

                // A resolver hands back a value rather than a place, which is
                // what makes writing through it an error.
                if let Some(value) = self.resolve_var(name, scope, var_pos)? {
                    return Ok(ChainRoot {
                        value: RootValue::Detached(value),
                        pos: var_pos,
                    });
                }
                // By index rather than through `Scope::get_mut`, which refuses
                // a constant outright. A constant is walked here rather than
                // refused: the read-only mode the entry carries is what turns
                // away an assignment or a non-pure method, and it is the same
                // mode Rhai's `Target` into the entry would have carried.
                if let Some(index) = scope.search(name) {
                    return Ok(ChainRoot {
                        value: RootValue::Entry(scope.get_mut_by_index(index)),
                        pos: var_pos,
                    });
                }
                // A constant a host published with `Module::set_var`. Not
                // marked read-only, because Rhai does not mark it either
                // (`eval/expr.rs:151` against `:122`) — so a chain assigns
                // into the copy and discards it, where writing to the name
                // directly is refused.
                self.engine
                    .global_modules
                    .iter()
                    .find_map(|module| module.get_var(name))
                    .map(|value| ChainRoot {
                        value: RootValue::Detached(value),
                        pos: var_pos,
                    })
                    .ok_or_else(|| missing(name, var_pos))
            }

            // Moved off the operand stack rather than copied. The slot it
            // leaves behind is unit, and nothing reads it again: `run_chain`
            // truncates past it, and an error unwinds the stack to the frame
            // floor.
            Root::Temporary => Ok(ChainRoot {
                value: RootValue::Temporary(
                    mem::take(&mut self.stack[operands_at + chain.operands as usize]).flatten(),
                ),
                pos,
            }),
        }
    }

    /// One level of the walk. Returns the value and whether anything below may
    /// have written.
    #[allow(clippy::too_many_arguments)]
    fn walk_chain(
        &mut self,
        program: &Program,
        chain: &Chain,
        steps: &[Step],
        target: &mut Dynamic,
        operands: &mut [Dynamic],
        value: Option<Dynamic>,
        pos: Position,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        let Some((step, rest)) = steps.split_first() else {
            // The end of the chain, reached with nothing to do: a bare `a` is
            // not a chain, so this only happens for an empty step list.
            return Ok((target.clone(), false));
        };
        let last = rest.is_empty();
        let coalescing = match step {
            Step::Index { flags, .. }
            | Step::Property { flags, .. }
            | Step::Method { flags, .. } => flags.contains(StepFlags::SKIP_IF_UNIT),
        };
        if coalescing && target.is_unit() {
            return Ok((Dynamic::UNIT, false));
        }

        // Which step a failure gets blamed on. The walk only descends, so the
        // last one written is the one that raised.
        self.chain_step = chain.steps.len() - steps.len();

        match step {
            // Without indexing of either kind there is no `[..]` to compile,
            // so a step that says otherwise came from a corrupted artifact.
            #[cfg(all(feature = "no_index", feature = "no_object"))]
            Step::Index { .. } => Err(malformed(
                "an index step, in a build with no indexing".to_string(),
            )),

            #[cfg(not(all(feature = "no_index", feature = "no_object")))]
            Step::Index {
                operand,
                flags: _,
                pos: idx_pos,
                bracket,
            } => {
                let idx = operands
                    .get_mut(*operand as usize)
                    .ok_or_else(|| malformed("chain index operand missing".to_string()))?;
                let mut idx = idx.clone();
                // Rhai reports an out-of-bounds index against the index and a
                // value that cannot be indexed at all against this step's `[`.
                // Both belong to the step, and neither is the chain's.
                let idx_pos = *idx_pos;
                let bracket = *bracket;

                // Split out so the borrow `get_indexed_mut` takes of `target`
                // ends when it returns: the fallback below needs `target`
                // again, and a `Target` in scope would still be holding it.
                match self.index_by_reference(
                    program, chain, rest, target, &mut idx, idx_pos, operands, value, last,
                    bracket, pos,
                )? {
                    Indexed::Done(out, changed) => Ok((out, changed)),
                    Indexed::NoReference(value) => {
                        self.assign_through_indexer(
                            program, chain, target, &mut idx, value, bracket,
                        )?;
                        Ok((Dynamic::UNIT, true))
                    }
                }
            }

            Step::Property {
                name,
                getter,
                setter,
                flags: _,
                pos: step_pos,
            } => self.walk_property(
                program, chain, rest, target, operands, value, pos, *step_pos, *name, *getter,
                *setter,
            ),

            Step::Method {
                name,
                argc,
                operand,
                flags: _,
                pos: step_pos,
            } => {
                let step_pos = *step_pos;
                let name_index = *name;
                let name = program
                    .name(name_index)
                    .ok_or_else(|| malformed(format!("no name {name_index}")))?;
                let first = *operand as usize;
                let argc = *argc as usize;
                if first + argc > operands.len() {
                    return Err(malformed("chain method arguments missing".to_string()));
                }

                // A method call is where Rhai tries the receiver's type before
                // the plain name (`func/call.rs:614-629`), and the only place it
                // does. `argc` already excludes the receiver, which is the arity
                // the script side is keyed on (`parser.rs:2128-2145`).
                //
                // Consulting our own table first is safe because nothing can get
                // between: `import` pushes onto `global.modules`, not
                // `global.lib` (`eval/stmt.rs:947`), and `global.lib` is only
                // ever pushed where an AST is being run.
                let type_name = target.type_name();
                let compiled = {
                    let typed = self.engine.map_type_name(type_name);
                    program
                        .method(name_index, argc, typed)
                        .map(|f| (f.params.clone(), f.chunk))
                };

                let mut args: FnArgsVec<Dynamic> =
                    operands[first..first + argc].iter().cloned().collect();
                let out = if let Some((params, chunk)) = compiled {
                    // The receiver is moved into the frame and moved back, so a
                    // write through `this` lands in the level above — which for
                    // a chain rooted at a local is the scope entry itself.
                    let (bound, write_back) = bind_this(target);
                    let at = self.stack.len();
                    self.stack.extend(args);
                    // A chained call always starts an empty scope.
                    let new_scope = &mut Scope::new();
                    let (result, returned) = self.call_compiled_with_this(
                        program,
                        name,
                        &params,
                        chunk,
                        at,
                        new_scope,
                        true,
                        step_pos,
                        Some(bound),
                    );
                    self.stack.truncate(at);
                    // Before `?`: a body that mutated and then raised has
                    // already written, as it would through Rhai's pointer.
                    unbind_this(target, returned, write_back);
                    result?
                } else {
                    let mut call_args: FnArgsVec<&mut Dynamic> = core::iter::once(&mut *target)
                        .chain(args.iter_mut())
                        .collect();
                    call_engine(
                        self.engine,
                        &mut self.global,
                        &mut self.caches,
                        &mut Scope::new(),
                        name,
                        &mut call_args,
                        true,
                        true,
                        step_pos,
                    )?
                };

                if last {
                    match value {
                        // `a.f() = x` is not something Rhai parses.
                        Some(_) => Err(malformed("assignment to a method call".to_string())),
                        None => Ok((out, true)),
                    }
                } else {
                    let mut inner = out;
                    let (out, _) =
                        self.walk_chain(program, chain, rest, &mut inner, operands, value, pos)?;
                    // Whatever the sub-chain did, it did to the method's
                    // return value, which nothing owns.
                    Ok((out, true))
                }
            }
        }
    }

    /// One `[i]` step, taken through a reference into the container.
    ///
    /// Returns [`Indexed::NoReference`] when there is no reference to be had —
    /// a custom indexer being assigned through — handing the value back so the
    /// caller can take the long way round once this borrow has ended.
    #[allow(clippy::too_many_arguments)]
    #[cfg(not(all(feature = "no_index", feature = "no_object")))]
    fn index_by_reference(
        &mut self,
        program: &Program,
        chain: &Chain,
        rest: &[Step],
        target: &mut Dynamic,
        idx: &mut Dynamic,
        idx_pos: Position,
        operands: &mut [Dynamic],
        value: Option<Dynamic>,
        last: bool,
        bracket: Position,
        pos: Position,
    ) -> Result<Indexed, Box<EvalAltResult>> {
        let assigning = last && value.is_some();
        let mut detached = Scope::new();

        // Cloned *before* the call, as Rhai clones it (`eval/chaining.rs:706`).
        // `get_indexed_mut` may reach a custom indexer, and a native's by-value
        // parameter is bound by `take` (`func/register.rs:69`) — so afterwards
        // there is nothing left to address the setter with. Only the paths that
        // can write need it; a read returns below without ever looking.
        let index_for_setter = (!last || value.is_some()).then(|| idx.clone());

        let mut item = match self.engine.get_indexed_mut(
            &mut self.global,
            &mut self.caches,
            &mut detached,
            None,
            target,
            idx,
            idx_pos,
            bracket,
            // Auto-vivify a missing map key only when writing, as Rhai does
            // for the assignment case (`eval/chaining.rs:791`).
            assigning,
            // And do not reach for a custom indexer when writing: a value it
            // handed back could not be assigned through. Rhai asks the same
            // way and takes the error as its signal.
            !assigning,
        ) {
            Ok(item) => item,
            Err(err) if assigning && matches!(*err, EvalAltResult::ErrorIndexingType(..)) => {
                return Ok(Indexed::NoReference(value.expect("assigning")));
            }
            Err(err) => return Err(err),
        };

        // A read changes nothing, so it consumes the target and there is
        // nothing to put back.
        if last && value.is_none() {
            return Ok(Indexed::Done(item.take_or_clone(), false));
        }

        let temp = item.is_temp_value();
        let (out, changed) = if last {
            let value = value.expect("checked above");
            self.store(
                program,
                chain_op(program, chain)?,
                item.as_mut(),
                value,
                pos,
            )?;
            (Dynamic::UNIT, true)
        } else {
            // Straight through the borrow: for an array, a map or a blob this
            // *is* the container's element, so a mutation below lands where
            // Rhai's would.
            self.walk_chain(program, chain, rest, item.as_mut(), operands, value, pos)?
        };

        // Bit-fields, string characters and blob bytes cannot be pointed at
        // directly, so `Target` carries a copy and this is what puts it back
        // (`eval/target.rs:282`).
        item.propagate_changed_value(pos)?;

        if temp && changed {
            // The element was a temporary — a custom indexer's — so the setter
            // is the only way back (`eval/chaining.rs:744`).
            let mut updated = item.take_or_clone();
            let mut index = index_for_setter.expect("a read returns before here");
            self.call_indexer_set(target, &mut index, &mut updated, bracket)?;
        }

        Ok(Indexed::Done(out, changed))
    }

    /// Assign through a custom indexer, which cannot hand out a reference.
    ///
    /// An op-assignment has to read the current value back through the getter
    /// first, and Rhai *ignores* a getter that fails here — a write-only
    /// indexer takes the new value as-is (`eval/chaining.rs:812`).
    #[cfg(not(all(feature = "no_index", feature = "no_object")))]
    fn assign_through_indexer(
        &mut self,
        program: &Program,
        chain: &Chain,
        target: &mut Dynamic,
        index: &mut Dynamic,
        value: Dynamic,
        pos: Position,
    ) -> Result<(), Box<EvalAltResult>> {
        let mut new_val = value;

        if matches!(chain.tail, Tail::Assign { op: Some(_) }) {
            let mut probe = index.clone();
            if let Ok(mut current) = self.engine.call_indexer_get(
                &mut self.global,
                &mut self.caches,
                target,
                &mut probe,
                pos,
            ) {
                self.store(
                    program,
                    chain_op(program, chain)?,
                    &mut current,
                    new_val,
                    pos,
                )?;
                new_val = current;
            }
        }

        self.call_indexer_set(target, index, &mut new_val, pos)
    }

    /// Put an element back into a container that had no reference to give.
    ///
    /// A custom indexer returns a value, so a mutation below it landed in a
    /// temporary; this is the replay Rhai does at `eval/chaining.rs:744`,
    /// including swallowing "this type cannot be indexed" the way it does.
    #[cfg(not(all(feature = "no_index", feature = "no_object")))]
    #[inline(always)]
    fn call_indexer_set(
        &mut self,
        target: &mut Dynamic,
        index: &mut Dynamic,
        value: &mut Dynamic,
        pos: Position,
    ) -> Result<(), Box<EvalAltResult>> {
        let result = self.engine.call_indexer_set(
            &mut self.global,
            &mut self.caches,
            target,
            index,
            value,
            true,
            pos,
        );
        match result.map(|_| ()) {
            Ok(()) => Ok(()),
            Err(err) if matches!(*err, EvalAltResult::ErrorIndexingType(..)) => Ok(()),
            Err(mut err) => {
                if err.position().is_none() {
                    err.set_position(pos);
                }
                Err(err)
            }
        }
    }

    // Try to get a property through an indexer.
    //
    // This requires `no_index` and `no_object` to be off,
    // otherwise it just passes the error through.
    fn try_index_get(
        &mut self,
        target: &mut Dynamic,
        key: &str,
        err: Box<EvalAltResult>,
        pos: Position,
    ) -> VmResult {
        #[cfg(not(any(feature = "no_index", feature = "no_object")))]
        return match *err {
            EvalAltResult::ErrorDotExpr(..) => {
                let mut index = self.strings_interner.get(key).into();
                self.engine
                    .call_indexer_get(&mut self.global, &mut self.caches, target, &mut index, pos)
                    .map_err(|err2| match *err2 {
                        EvalAltResult::ErrorIndexingType(..) => err,
                        _ => positioned(err2, pos),
                    })
            }
            _ => Err(err),
        };
        #[cfg(any(feature = "no_index", feature = "no_object"))]
        {
            let _ = (target, key, pos);
            return Err(err);
        }
    }

    // Try to set a property through an index setter.
    //
    // This requires `no_index` and `no_object` to be off,
    // otherwise it just passes the error through.
    fn try_index_set(
        &mut self,
        target: &mut Dynamic,
        key: &str,
        value: &mut Dynamic,
        fail_silently: bool,
        err: Box<EvalAltResult>,
        pos: Position,
    ) -> VmResult {
        #[cfg(not(any(feature = "no_index", feature = "no_object")))]
        return match *err {
            EvalAltResult::ErrorDotExpr(..) => {
                let mut index = self.strings_interner.get(key).into();
                match self
                    .engine
                    .call_indexer_set(
                        &mut self.global,
                        &mut self.caches,
                        target,
                        &mut index,
                        value,
                        true,
                        pos,
                    )
                    .map(|_| ())
                {
                    Ok(()) => Ok(Dynamic::UNIT),
                    Err(err2) if matches!(*err2, EvalAltResult::ErrorIndexingType(..)) => {
                        if fail_silently {
                            Ok(Dynamic::UNIT)
                        } else {
                            Err(err)
                        }
                    }
                    Err(err2) => Err(positioned(err2, pos)),
                }
            }
            _ => Err(err),
        };
        #[cfg(any(feature = "no_index", feature = "no_object"))]
        {
            let _ = (target, key, value, fail_silently, pos);
            return Err(err);
        }
    }

    /// `.name`, which is a key on a map and a getter call on anything else.
    ///
    /// The distinction is Rhai's and it is made at runtime, not at parse time
    /// (`eval/chaining.rs:898`). It matters for more than speed: a map hands
    /// back a reference, so a mutation below lands in the map, while a getter
    /// hands back a value that has to be given to the setter afterwards.
    #[allow(clippy::too_many_arguments)]
    fn walk_property(
        &mut self,
        program: &Program,
        chain: &Chain,
        rest: &[Step],
        target: &mut Dynamic,
        operands: &mut [Dynamic],
        value: Option<Dynamic>,
        pos: Position,
        // The property's own position, which is where Rhai blames a getter or
        // setter that does not exist (`eval/chaining.rs:1039`). `pos` is the
        // chain's, and stays that for everything else.
        step_pos: Position,
        name: u32,
        getter: u32,
        setter: u32,
    ) -> Result<(Dynamic, bool), Box<EvalAltResult>> {
        let last = rest.is_empty();

        // The name is a map key for maps, and the same string is what a host
        // type's fallback string indexer is addressed with.
        let key = program
            .name(name)
            .ok_or_else(|| malformed(format!("no name {name}")))?;

        // A map is the one property holder that is not a host type, and
        // `no_object` removes both it and the syntax that would reach one.
        #[cfg(not(feature = "no_object"))]
        if target.is_map() {
            let mut map = target
                .write_lock::<Map>()
                .ok_or_else(|| malformed("a map that is not a map".to_string()))?;

            // Only a write creates a key. Rhai passes `add_if_not_found` for
            // an assignment (`eval/chaining.rs:930`) and withholds it for a
            // read (`:959`) and for a step on the way through (`:1086`), so
            // reading `m.absent` must leave `m` alone — otherwise a closure
            // holding the map sees a key nobody wrote.
            if last {
                if let Some(value) = value {
                    let entry = map.entry(key.into()).or_insert(Dynamic::UNIT);
                    self.store(program, chain_op(program, chain)?, entry, value, pos)?;
                    return Ok((Dynamic::UNIT, true));
                }
                return match map.get(key) {
                    Some(entry) => Ok((entry.clone(), false)),
                    None => self.absent_key(key, step_pos).map(|unit| (unit, false)),
                };
            }

            return match map.get_mut(key) {
                Some(entry) => self.walk_chain(program, chain, rest, entry, operands, value, pos),
                // Rhai walks on into a detached unit, so whatever the rest of
                // the chain does to it is discarded (`eval/chaining.rs:211`).
                None => {
                    let mut absent = self.absent_key(key, step_pos)?;
                    drop(map);
                    self.walk_chain(program, chain, rest, &mut absent, operands, value, pos)
                }
            };
        }

        // A host type: getter in, setter out.
        let call = |vm: &mut Self, fn_name: u32, args: &mut [&mut Dynamic]| -> VmResult {
            let fn_name = program
                .name(fn_name)
                .ok_or_else(|| malformed(format!("no name {fn_name}")))?;
            call_engine(
                vm.engine,
                &mut vm.global,
                &mut vm.caches,
                &mut Scope::new(),
                fn_name,
                args,
                true,
                true,
                step_pos,
            )
        };

        if last {
            if let Some(value) = value {
                // `x.p += 1` has to read `p` back through the getter before it
                // can add to it — the setter takes a finished value.
                let mut new_val = if matches!(chain.tail, Tail::Assign { op: Some(_) }) {
                    let mut current = call(self, getter, &mut [target])
                        .or_else(|err| self.try_index_get(target, key, err, step_pos))?;
                    self.store(program, chain_op(program, chain)?, &mut current, value, pos)?;
                    current
                } else {
                    value
                };
                // A setter's return value is thrown away, as in Rhai.
                let _ = call(self, setter, &mut [target, &mut new_val]).or_else(|err| {
                    self.try_index_set(target, key, &mut new_val, false, err, step_pos)
                })?;
                return Ok((Dynamic::UNIT, true));
            }
            let out = call(self, getter, &mut [target])
                .or_else(|err| self.try_index_get(target, key, err, step_pos))?;
            return Ok((out, false));
        }

        // A getter returns a value, so the rest of the chain works on a
        // temporary. Rhai puts it back through the setter when the sub-chain
        // was a method call, and skips the setter otherwise.
        let mut temp = call(self, getter, &mut [target])
            .or_else(|err| self.try_index_get(target, key, err, step_pos))?;
        let (out, changed) =
            self.walk_chain(program, chain, rest, &mut temp, operands, value, pos)?;
        if changed {
            let _ = call(self, setter, &mut [target, &mut temp])
                .or_else(|err| self.try_index_set(target, key, &mut temp, true, err, step_pos))?;
        }
        Ok((out, changed))
    }

    /// Store into a slot the walk arrived at, through an operator if there is
    /// one.
    ///
    /// Same resolution order as a plain local assignment, and for the same
    /// reason: `x += y` is not `x = x + y` unless nothing implements `+=`.
    /// The built-in op-assignment for these operands, if Rhai has one.
    ///
    /// Inlined deliberately: `x += 1` in a loop is entirely this, and routing
    /// it through the out-of-line resolution below measured 10% on the
    /// tight-loop benchmark. Inlining the *whole* of `store` instead costs
    /// more than it saves — it took `branch heavy` from 1.59x to 1.41x — so
    /// the split is where the two paths part.
    #[inline]
    fn store_builtin(
        &mut self,
        op: &AssignOp,
        target: &mut Dynamic,
        rhs: &mut Dynamic,
        pos: impl Fn() -> Position,
    ) -> Option<Result<(), Box<EvalAltResult>>> {
        if !self.engine.fast_operators() {
            return None;
        }
        let (func, need_context) = get_builtin_op_assignment_fn(&op.op_assign, target, rhs)?;
        let context = need_context.then(|| (self.engine, "", None, &self.global, pos()).into());
        Some(
            func(context, &mut [target, rhs])
                .map(|_| ())
                .map_err(|mut err| {
                    if err.position().is_none() {
                        err.set_position(pos());
                    }
                    err
                }),
        )
    }

    fn store(
        &mut self,
        program: &Program,
        op: Option<&AssignOp>,
        target: &mut Dynamic,
        mut rhs: Dynamic,
        pos: Position,
    ) -> Result<(), Box<EvalAltResult>> {
        let Some(op) = op else {
            *target = rhs;
            return Ok(());
        };

        if let Some(done) = self.store_builtin(op, target, &mut rhs, || pos) {
            return done;
        }

        let op_assign_name = program
            .name(op.op_assign_name)
            .ok_or_else(|| malformed(format!("no op-assign name {}", op.op_assign_name)))?;
        let op_name = program
            .name(op.op_name)
            .ok_or_else(|| malformed(format!("no operator name {}", op.op_name)))?;

        // The real scope may be borrowed by the target, and dispatch does not
        // read it anyway — operators resolve against the engine.
        let result = call_engine(
            self.engine,
            &mut self.global,
            &mut self.caches,
            &mut Scope::new(),
            op_assign_name,
            &mut [target, &mut rhs],
            true,
            false,
            pos,
        );
        match result {
            Ok(_) => Ok(()),
            Err(err)
                if matches!(&*err,
                    EvalAltResult::ErrorFunctionNotFound(name, ..)
                        if name.starts_with(op_assign_name)) =>
            {
                let value = call_engine(
                    self.engine,
                    &mut self.global,
                    &mut self.caches,
                    &mut Scope::new(),
                    op_name,
                    &mut [target, &mut rhs],
                    true,
                    false,
                    pos,
                )?;
                *target = value;
                Ok(())
            }
            Err(err) => Err(dispatch_failure(err, pos)),
        }
    }

    /// Read a variable no slot names, the way Rhai's `search_scope_only` does
    /// (`eval/expr.rs:107-155`).
    ///
    /// Three places in a fixed order, and the order is observable: a resolver
    /// registered with `Engine::on_var` sees the name before the scope does,
    /// and a name in no scope is looked for among the global modules before it
    /// is reported missing.
    ///
    /// `flatten` is what the two reads differ by, and only for a scope entry:
    /// a value position wants what a shared cell contains, and a capture wants
    /// the cell. The other two places can only ever produce a value.
    ///
    /// Kept out of the dispatch loop for the reason [`Vm::call_compiled`] is.
    fn load_named(
        &mut self,
        name: &str,
        scope: &mut Scope,
        flatten: bool,
        pos: Position,
    ) -> VmResult {
        // A resolver hands back a value, not a place, so it is read-only —
        // which is what makes assigning to one an error.
        if let Some(value) = self.resolve_var(name, scope, pos)? {
            return Ok(value);
        }

        if let Some(value) = scope.get(name) {
            return Ok(if flatten {
                value.flatten_clone()
            } else {
                value.clone()
            });
        }

        // A constant a host published with `Module::set_var`.
        if let Some(value) = self
            .engine
            .global_modules
            .iter()
            .find_map(|module| module.get_var(name))
        {
            return Ok(value);
        }

        Err(missing(name, pos))
    }

    /// Ask the resolver a host registered with `Engine::on_var`, if there is
    /// one.
    ///
    /// `Ok(None)` covers both "no resolver" and "the resolver declined", which
    /// are the same thing to every caller.
    fn resolve_var(
        &mut self,
        name: &str,
        scope: &mut Scope,
        pos: Position,
    ) -> Result<Option<Dynamic>, Box<EvalAltResult>> {
        // Copied out so the borrow is of the engine rather than of `self`,
        // which the context below needs mutably.
        let engine = self.engine;
        let Some(resolver) = &engine.resolve_var else {
            return Ok(None);
        };

        let before = scope.len();
        let context = EvalContext::new(engine, &mut self.global, &mut self.caches, scope, None);
        // Index zero: Rhai passes the slot its parser resolved, and a name
        // that reached here had none.
        let resolved = resolver(name, 0, context);

        // A resolver that pushed onto the scope has moved every entry a
        // parse-time index named, so Rhai stops trusting those from here on.
        // Nothing this compiler emits depends on them — its slots are counted
        // from a base taken before the run — but a fragment's do.
        if scope.len() != before {
            self.global.always_search_scope = true;
        }

        match resolved {
            Ok(Some(value)) => Ok(Some(value.into_read_only())),
            Ok(None) => Ok(None),
            Err(err) => Err(dispatch_failure(err, pos)),
        }
    }

    /// Assign to a variable no slot names.
    ///
    /// Rhai reaches the target through the same search and then refuses
    /// anything that is not a reference it can write through: a value the
    /// resolver produced, a module's constant, a `const` entry
    /// (`eval/stmt.rs:330-344` and `eval/stmt.rs:118-122`). All three are
    /// `ErrorAssignmentToConstant`, so the distinction never reaches a script.
    fn assign_named(
        &mut self,
        program: &Program,
        op: Option<&AssignOp>,
        name: &str,
        rhs: Dynamic,
        scope: &mut Scope,
        pos: Position,
    ) -> Result<(), Box<EvalAltResult>> {
        let constant = || {
            Box::new(EvalAltResult::ErrorAssignmentToConstant(
                name.to_string(),
                pos,
            ))
        };

        // Try the variable resolver first.
        if let Some(mut value) = self.resolve_var(name, scope, pos)? {
            if value.is_read_only() {
                return Err(constant());
            }
            let mut target = place(&mut value, name, pos)?;
            return self.store(program, op, &mut target, rhs, pos);
        }

        // Search the scope.
        match scope.get_mut_raw(name) {
            Some(None) => Err(constant()),
            Some(Some(entry)) => {
                let mut target = place(entry, name, pos)?;
                self.store(program, op, &mut target, rhs, pos)
            }
            // Not a variable at all. A module's is a value rather than a
            // place, so writing to one is the same refusal as writing to a
            // `const`.
            None => Err(
                if self
                    .engine
                    .global_modules
                    .iter()
                    .any(|module| module.get_var(name).is_some())
                {
                    constant()
                } else {
                    missing(name, pos)
                },
            ),
        }
    }

    /// Call a function pointer, preferring a chunk we compiled.
    ///
    /// The pointer sits under its arguments. Rhai's own dispatch would work
    /// for all of this, but it cannot reach our chunks — the compiled function
    /// table is keyed on names from the pool, and a pointer carries a string —
    /// so the name is matched against it first and only the miss goes to
    /// `call_raw`.
    #[allow(clippy::too_many_arguments)]
    fn call_fn_ptr(
        &mut self,
        program: &Program,
        argc: usize,
        method: bool,
        receiver: Option<Receiver>,
        scope: &mut Scope,
        frame_base: usize,
        pos: Position,
    ) -> VmResult {
        let base = self
            .stack
            .len()
            .checked_sub(argc + 1)
            .ok_or_else(|| malformed("function pointer call is missing its target".into()))?;
        let mut at = base;

        // In method position a target that is not a pointer means the *first
        // argument* is one and the target is the receiver — `obj.call(f, ..)`
        // is how a closure is called against a `this`. Rhai reports the
        // mismatch against that argument, not against the target, which is why
        // the position moves with it.
        let mut receiver_at = None;
        if method && !self.stack[at].is::<FnPtr>() {
            receiver_at = Some(at);
            at += 1;
            if at >= self.stack.len() {
                return Err(self.mismatch::<FnPtr>(self.stack[at - 1].type_name(), pos));
            }
        }

        let pointer = self.stack[at]
            .clone()
            .try_cast::<FnPtr>()
            .ok_or_else(|| self.mismatch::<FnPtr>(self.stack[at].type_name(), pos))?;

        let taken = self.stack.len() - at - 1;
        let curried = pointer.curry().len();
        // A receiver does not change which function a pointer names, only what
        // the callee's `this` is: Rhai keys its own script pointers on the
        // declared parameter count alone (`types/fn_ptr.rs:422`), and binds the
        // receiver alongside them.
        let function = program
            .function_named(pointer.fn_name(), curried + taken)
            .map(|f| (f.params.clone(), f.chunk));

        // Bound once, whichever path takes it. Curried values are spliced in
        // above `at`, so the receiver's index is unaffected either way.
        let (mut bound, write_back) = match receiver_at {
            Some(index) => {
                let (value, write_back) = bind_this(&mut self.stack[index]);
                (Some(value), write_back)
            }
            None => (None, false),
        };

        let outcome = if let Some((params, chunk)) = function {
            // Curried arguments go in front of the call's own, which is what
            // currying means and where the callee's parameters expect them.
            let first = at + 1;
            self.stack
                .splice(first..first, pointer.curry().iter().cloned());

            // A function pointer call always starts with an empty scope.
            let new_scope = &mut Scope::new();

            let (result, returned) = self.call_compiled_with_this(
                program,
                pointer.fn_name(),
                &params,
                chunk,
                first,
                new_scope,
                true,
                pos,
                bound.take(),
            );
            bound = returned;
            result
        } else {
            // Anything else is Rhai's: a native function, a name registered
            // elsewhere, or a pointer it built itself.
            let mut args: FnArgsVec<Dynamic> = self.stack.drain(at + 1..).collect();
            let context = (self.engine, pointer.fn_name(), None, &self.global, pos).into();
            pointer
                .call_raw(&context, bound.as_mut(), &mut args)
                .map_err(|mut err| {
                    if err.position().is_none() {
                        err.set_position(pos);
                    }
                    err
                })
        };

        // Before `?`, as everywhere else: Rhai binds the receiver by reference,
        // so a closure that writes and then raises has already written.
        if let Some(index) = receiver_at {
            unbind_this(&mut self.stack[index], bound, write_back);
            if write_back {
                let updated = self.stack[index].clone();
                self.return_receiver(program, receiver, updated, scope, frame_base)?;
            }
        }

        let value = outcome?;
        self.stack.truncate(base);
        Ok(value)
    }

    /// Carry a write through `obj.call(f)`'s `this` back to `obj` itself.
    ///
    /// Rhai binds the receiver by reference (`func/call.rs:862`), so the write
    /// lands in the variable. The operand stack only ever held a copy of it,
    /// and this is what puts the copy back where it came from.
    ///
    /// Nothing to do for a shared receiver: it arrived *as* the cell, so the
    /// write already landed where every holder can see it — `run_chain`'s rule
    /// for a chain root, and for the same reason.
    fn return_receiver(
        &mut self,
        program: &Program,
        receiver: Option<Receiver>,
        value: Dynamic,
        scope: &mut Scope,
        frame_base: usize,
    ) -> Result<(), Box<EvalAltResult>> {
        if is_shared!(value) || value.is_read_only() {
            return Ok(());
        }

        match receiver {
            Some(Receiver::Local(slot)) => {
                let index = frame_base + slot as usize;
                if index >= scope.len() {
                    return Err(malformed(format!("local slot {slot} is out of scope")));
                }
                *scope.get_mut_by_index(index) = value;
            }
            Some(Receiver::Named(var)) => {
                let name = program
                    .name(var)
                    .ok_or_else(|| malformed(format!("no name {var}")))?;
                // A resolver's answer or a module constant has no entry behind
                // it, and Rhai could not have written through one either.
                if let Some(entry) = scope.get_mut(name) {
                    *entry = value;
                }
            }
            Some(Receiver::This) => {
                if let Some(entry) = self.this.as_mut() {
                    *entry = value;
                }
            }
            // A temporary, which Rhai mutates a copy of too.
            None => {}
        }
        Ok(())
    }

    /// Concatenate the segments of an interpolated string, reproducing
    /// `eval/expr.rs:280-304`.
    ///
    /// Every step of it is load-bearing. A **string** segment is written
    /// straight out and never reaches dispatch, so a host's `to_string` for
    /// strings is not consulted here even though `+` would consult it.
    /// Anything else goes through Rhai's own rendering, which calls **native**
    /// functions only — a script `fn to_string` is invisible to it — and
    /// substitutes the mapped type name when the call returns a non-string.
    /// The size limit is checked after every segment against the running
    /// total, not once at the end.
    fn append_segment(
        &mut self,
        segment: Dynamic,
        pos: Position,
    ) -> Result<(), Box<EvalAltResult>> {
        use core::fmt::Write;

        let mut item = segment.flatten();
        let mut rendered = None;

        // A string is written straight out and never reaches dispatch, so a
        // host's `to_string` for strings is not consulted here even though `+`
        // would consult it.
        if !item.is_string() {
            let context = (self.engine, FUNC_TO_STRING, None, &self.global, pos).into();
            rendered = Some(print_with_func(FUNC_TO_STRING, &context, &mut item));
        }

        let mut buffer = self
            .stack
            .last_mut()
            .and_then(|value| value.write_lock::<ImmutableString>())
            .ok_or_else(|| malformed("interpolation lost its buffer".into()))?;

        // `make_mut` is in place while the buffer is uniquely held, which on
        // the operand stack it is — so this is one growing allocation rather
        // than one per segment.
        match rendered {
            Some(text) => write!(buffer.make_mut(), "{text}"),
            None => write!(buffer.make_mut(), "{item}"),
        }
        .expect("writing to a string cannot fail");
        let len = buffer.len();
        drop(buffer);

        // After every segment, against the running total — a script must not
        // be able to build a string past `max_string_size` and hand it over
        // whole.
        #[cfg(not(feature = "unchecked"))]
        {
            self.engine.throw_on_size((0, 0, len)).map_err(|mut err| {
                if err.position().is_none() {
                    err.set_position(pos);
                }
                err
            })
        }
        // `unchecked` removes the limits, and with them the only reason to have
        // measured.
        #[cfg(feature = "unchecked")]
        {
            let _ = (len, pos);
            Ok(())
        }
    }

    /// Start iterating a value, the way Rhai's `for` does
    /// (`eval/stmt.rs:680-703`).
    ///
    /// Three places are searched by `TypeId`, in order, and the order is
    /// Rhai's: the modules in the global namespace, then the imports, then the
    /// statically registered sub-modules. Nothing matching is `ErrorFor`.
    ///
    /// The iterable is flattened first — so iterating a captured array walks a
    /// snapshot rather than the shared cell — and is consumed by value, which
    /// is why the iterator is built once and held for the life of the loop.
    fn iter_init(&mut self, iterable: Dynamic, pos: Position) -> Result<(), Box<EvalAltResult>> {
        let iterable = iterable.flatten();
        let type_id = iterable.type_id();

        let func = self
            .engine
            .global_modules
            .iter()
            .find_map(|module| module.get_iter(type_id));

        // Imported and sub-modules can register iterators too, but neither
        // exists to be searched under `no_module`.
        #[cfg(not(feature = "no_module"))]
        let func = func.or_else(|| self.global.get_iter(type_id)).or_else(|| {
            self.engine
                .global_sub_modules
                .values()
                .find_map(|module| module.get_qualified_iter(type_id))
        });

        let func = func.ok_or_else(|| Box::new(EvalAltResult::ErrorFor(pos)))?;

        self.iterators.push(Iteration {
            items: func(iterable),
            count: -1,
        });
        Ok(())
    }

    /// Called only by [`call_syntactic_or_stacked`] after it has checked for a
    /// syntactic call.
    ///
    /// Some syntactic calls can be self-implemented or short-circuited.
    fn call_syntactic(
        &mut self,
        #[cfg_attr(feature = "no_function", allow(unused))] program: &Program,
        name: &str,
        argc: usize,
        first: usize,
        scope: &mut Scope,
        pos: Position,
    ) -> Result<Option<Dynamic>, Box<EvalAltResult>> {
        match name {
            crate::engine::KEYWORD_IS_DEF_VAR => {
                if argc != 1 {
                    return Err(EvalAltResult::ErrorFunctionNotFound(name.to_string(), pos).into());
                }
                let var_name = self.stack[first].as_immutable_string_ref().map_err(|typ| {
                    self.engine
                        .make_type_mismatch_err::<ImmutableString>(typ, pos)
                })?;
                return Ok(Some(scope.contains(&var_name).into()));
            }
            #[cfg(not(feature = "no_function"))]
            crate::engine::KEYWORD_IS_DEF_FN => {
                let (this_type, fn_name, arity) = match argc {
                    2 => {
                        let var_name = self.stack[first]
                            .as_immutable_string_ref()
                            .as_deref()
                            .cloned()
                            .map_err(|typ| {
                                self.engine
                                    .make_type_mismatch_err::<ImmutableString>(typ, pos)
                            })?;
                        let arity = self.stack[first + 1]
                            .as_int()
                            .map_err(|typ| self.engine.make_type_mismatch_err::<INT>(typ, pos))?;
                        (None, var_name, arity as usize)
                    }
                    3 => {
                        let this_type = self.stack[first]
                            .as_immutable_string_ref()
                            .as_deref()
                            .cloned()
                            .map_err(|typ| {
                                self.engine
                                    .make_type_mismatch_err::<ImmutableString>(typ, pos)
                            })?;
                        let var_name = self.stack[first + 1]
                            .as_immutable_string_ref()
                            .as_deref()
                            .cloned()
                            .map_err(|typ| {
                                self.engine
                                    .make_type_mismatch_err::<ImmutableString>(typ, pos)
                            })?;
                        let arity = self.stack[first + 2]
                            .as_int()
                            .map_err(|typ| self.engine.make_type_mismatch_err::<INT>(typ, pos))?;
                        (Some(this_type), var_name, arity as usize)
                    }
                    _ => {
                        return Err(
                            EvalAltResult::ErrorFunctionNotFound(name.to_string(), pos).into()
                        )
                    }
                };

                // Check if there is a compiled function.
                for f in program.functions() {
                    let local_name = program
                        .name(f.name)
                        .ok_or_else(|| malformed(format!("no name {}", f.name)))?;

                    if local_name == fn_name.as_str() && f.params.len() == arity {
                        if let Some(ref this_type) = this_type {
                            if let Some(local_this_type_index) = f.this_type {
                                let local_this_type_name =
                                    program.name(local_this_type_index).ok_or_else(|| {
                                        malformed(format!("no name {local_this_type_index}"))
                                    })?;

                                if local_this_type_name == this_type {
                                    return Ok(Some(Dynamic::TRUE));
                                }
                            }
                        } else if f.this_type.is_none() {
                            return Ok(Some(Dynamic::TRUE));
                        }
                    }
                }

                // Call into Rhai.
                let mut args = self.stack[first..].iter_mut().collect::<FnArgsVec<_>>();

                return self
                    .engine
                    .exec_syntactic_fn_call(
                        &mut self.global,
                        &mut self.caches,
                        name,
                        &mut args,
                        pos,
                    )
                    .map_err(|err| dispatch_failure(err, pos))?
                    .ok_or_else(|| {
                        EvalAltResult::ErrorFunctionNotFound(name.to_string(), pos).into()
                    })
                    .map(Some);
            }
            _ => {}
        }

        Ok(None)
    }

    /// Called only by [`call_syntactic_or_stacked`] after it has checked for a
    /// syntactic call.
    ///
    /// Call function with `argc` arguments sitting contiguously from `first` up.
    ///
    /// A function this compiler lowered is called directly, with no hash and no
    /// module walk: the call site's name index and the function's come from the
    /// same pool, so equal names have equal indices. Everything else goes to
    /// Rhai's dispatch, and resolves exactly as it would in the walker.
    fn call_stacked(
        &mut self,
        program: &Program,
        name_index: u32,
        name: &str,
        argc: usize,
        first: usize,
        scope: &mut Scope,
        pos: Position,
    ) -> VmResult {
        // Run compiled function if available.
        if let Some(function) = program.function(name_index, argc) {
            return self.call_compiled(
                program,
                name,
                &function.params,
                function.chunk,
                first,
                scope,
                pos,
            );
        }

        // Arguments are already contiguous at the top of the operand stack,
        // which is exactly the shape Rhai's ABI wants (`func/call.rs:36`). It
        // consumes them, replacing each with unit, so the caller truncates
        // afterwards rather than reusing them.
        let mut args: FnArgsVec<&mut Dynamic> = self.stack[first..].iter_mut().collect();

        call_engine(
            self.engine,
            &mut self.global,
            &mut self.caches,
            scope,
            name,
            &mut args,
            false,
            false,
            pos,
        )
    }

    /// This is the main entry-point for function calls.
    ///
    /// First check whether the call is a syntactic one (e.g. `is_def_fn`)
    /// which are self-implemented or directly called into the
    /// corresponding Rhai function.
    ///
    /// If the call is not to a syntactic one, it calls the function
    /// normally, with arguments pushed onto the stack.
    fn call_syntactic_or_stacked(
        &mut self,
        program: &Program,
        name_index: u32,
        name: &str,
        argc: usize,
        first: usize,
        scope: &mut Scope,
        capture: bool,
        pos: Position,
    ) -> VmResult {
        // Check if it is a built-in syntactic function.
        match self.call_syntactic(program, name, argc, first, scope, pos)? {
            Some(value) => Ok(value),
            None => {
                // Detach the scope with a new one if not capturing the parent's.
                let mut detached;
                let scope = if !capture {
                    detached = Scope::new();
                    &mut detached
                } else {
                    scope
                };
                self.call_stacked(program, name_index, name, argc, first, scope, pos)
            }
        }
    }

    /// The same call, with a variable as its first argument and Rhai's
    /// method-call rewrite applied to it (`func/call.rs:1434-1460`).
    ///
    /// The other arguments are already on the operand stack and were evaluated
    /// before the receiver was reached, which is the order Rhai uses and is
    /// observable whenever one of them writes to the receiver.
    #[allow(clippy::too_many_arguments)]
    fn call_by_reference(
        &mut self,
        program: &Program,
        name_index: u32,
        name: &str,
        argc: usize,
        receiver: Receiver,
        scope: &mut Scope,
        base: usize,
        capture: bool,
        pos: Position,
    ) -> VmResult {
        // Every argument count here includes the receiver, so zero of them
        // names no receiver at all and the instruction is nonsense. Only an
        // artifact can say it; the compiler emits one of these for a call that
        // has a first argument.
        if argc == 0 {
            return Err(malformed(
                "a call by reference with no receiver".to_string(),
            ));
        }

        // The register is not a scope entry, so it takes a path of its own
        // rather than a third [`Site`].
        if let Receiver::This = receiver {
            return self.call_by_this(program, name_index, name, argc, scope, capture, pos);
        }

        // A named receiver's value is already argument zero — [`Op::LoadNamed`]
        // put it there. A local's is not on the stack at all.
        let (at, on_stack) = match receiver {
            Receiver::Local(slot) => {
                let index = base + slot as usize;
                if index >= scope.len() {
                    return Err(malformed(format!("local slot {slot} is out of scope")));
                }
                (Site::Slot(index), argc - 1)
            }
            Receiver::Named(var) => {
                let name = program
                    .name(var)
                    .ok_or_else(|| malformed(format!("no name {var}")))?;
                (Site::Name(name), argc)
            }
            Receiver::This => unreachable!("taken above"),
        };
        let first = self
            .stack
            .len()
            .checked_sub(on_stack)
            .ok_or_else(|| malformed("call with too few arguments".to_string()))?;

        let place = match at {
            Site::Slot(index) => Some(scope.get_mut_by_index(index)),
            // A resolver's answer shadows the scope, and `load_named` marks one
            // read-only precisely because it is a value and not a place. Asking
            // the resolver again to find that out would run it twice, which a
            // host can see.
            Site::Name(..) if self.stack[first].is_read_only() => None,
            Site::Name(name) => scope.get_mut(name),
        };

        // Three things rule out a reference, and Rhai rules out the same three:
        // it hands one out for neither a shared cell nor a constant
        // (`func/call.rs:1449-1454`), and a function this compiler lowered
        // copies its first argument whatever it is handed, exactly as Rhai
        // copies it before running a script function (`func/call.rs:661`).
        let by_reference = place.map_or(false, |value| !is_shared!(value) && !value.is_read_only())
            && program.function(name_index, argc).is_none();

        // All three want the ordinary shape, with every argument on the stack.
        if !by_reference {
            // A local's value has not been pushed. A name's already is: it is
            // what carried the lookup's position (see [`Receiver::Named`]), and
            // it is exactly the value Rhai would pass.
            if let Site::Slot(index) = at {
                let value = scope.get_mut_by_index(index).flatten_clone();
                self.stack.insert(first, value);
            }
            let value = self.call_syntactic_or_stacked(
                program, name_index, name, argc, first, scope, capture, pos,
            )?;
            self.stack.truncate(first);
            return Ok(value);
        }

        let value = {
            let (entry, rest) = match at {
                Site::Slot(index) => (scope.get_mut_by_index(index), first),
                // Argument zero is dead weight now that there is an entry to
                // reach, and it is the price of having resolved the name where
                // its position was.
                Site::Name(name) => (
                    scope
                        .get_mut(name)
                        .ok_or_else(|| malformed(format!("`{name}` stopped being writable")))?,
                    first + 1,
                ),
            };
            let mut args: FnArgsVec<&mut Dynamic> = core::iter::once(entry)
                .chain(self.stack[rest..].iter_mut())
                .collect();
            // The scope a dispatched script function runs in, which is never
            // this frame's — see [`Vm::call_stacked`], which has to build one
            // for the same reason and cannot borrow this one because the
            // receiver is holding it.
            call_engine(
                self.engine,
                &mut self.global,
                &mut self.caches,
                &mut Scope::new(),
                name,
                &mut args,
                true,
                false,
                pos,
            )
        };

        self.stack.truncate(first);
        value
    }

    /// The same again, with `this` as the first argument.
    ///
    /// [`Op::LoadThis`] has already pushed a flattened snapshot as argument
    /// zero — *before* the other arguments, unlike either of the other two
    /// receivers, because the path a shared or unbound receiver takes reads
    /// `this` first (`func/call.rs:1462`) where the by-reference path takes a
    /// pointer to it afterwards (`:1417`). Reading first is what makes an
    /// unbound `f(this, no_such)` report `ErrorUnboundThis` rather than the
    /// argument's failure.
    ///
    /// The snapshot is what gets passed when the register cannot be lent out,
    /// and dead weight when it can — the trade [`Receiver::Named`] makes too.
    fn call_by_this(
        &mut self,
        program: &Program,
        name_index: u32,
        name: &str,
        argc: usize,
        scope: &mut Scope,
        capture: bool,
        pos: Position,
    ) -> VmResult {
        let first = self
            .stack
            .len()
            .checked_sub(argc)
            .ok_or_else(|| malformed("call with too few arguments".to_string()))?;

        // Rhai turns `f(this, ..)` into `this.f(..)` for a receiver that is not
        // shared, and read-only is *not* part of that test — unlike the variable
        // arm, which copies a constant before deciding (`func/call.rs:1449`).
        // A function this compiler lowered copies its first argument whatever it
        // is handed, exactly as Rhai copies one before running a script function
        // (`func/call.rs:661`), so a compiled callee rules a reference out too.
        let by_reference = self.this.as_ref().map_or(false, |value| !is_shared!(value))
            && program.function(name_index, argc).is_none();

        if !by_reference {
            let value = self.call_syntactic_or_stacked(
                program, name_index, name, argc, first, scope, capture, pos,
            )?;
            self.stack.truncate(first);
            return Ok(value);
        }

        let value = {
            let entry = self
                .this
                .as_mut()
                .ok_or_else(|| malformed("`this` stopped being bound".to_string()))?;
            // Argument zero is the snapshot, dead now that there is a register
            // to reach through.
            let mut args: FnArgsVec<&mut Dynamic> = core::iter::once(entry)
                .chain(self.stack[first + 1..].iter_mut())
                .collect();
            call_engine(
                self.engine,
                &mut self.global,
                &mut self.caches,
                &mut Scope::new(),
                name,
                &mut args,
                true,
                false,
                pos,
            )
        };

        self.stack.truncate(first);
        value
    }

    fn call_compiled(
        &mut self,
        program: &Program,
        name: &str,
        params: &[u32],
        chunk: Chunk,
        first: usize,
        scope: &mut Scope,
        pos: Position,
    ) -> VmResult {
        self.call_compiled_with_this(program, name, params, chunk, first, scope, true, pos, None)
            .0
    }

    /// The same, against a receiver the callee owns for the duration.
    ///
    /// Hands the receiver back however the call ended, so a body that mutated
    /// `this` and then raised still gives its binder something to write back —
    /// which is what Rhai's pointer into the caller's storage does for free.
    ///
    /// Every compiled call comes through here, and [`Vm::call_compiled`] is this
    /// with no receiver. That is what makes `this` per-call rather than
    /// inherited: an ordinary call installs `None` and gives the caller's back
    /// on the way out, so a callee can never read the receiver of the frame that
    /// called it (`func/call.rs:669`).
    ///
    /// Kept out of the dispatch loop. Inlined, it is enough extra code to change
    /// register allocation across every other instruction — measured as a
    /// uniform slowdown on benchmarks that call no functions at all.
    fn call_compiled_with_this(
        &mut self,
        program: &Program,
        name: &str,
        params: &[u32],
        chunk: Chunk,
        first: usize,
        scope: &mut Scope,
        rewind_scope: bool,
        pos: Position,
        this: Option<Dynamic>,
    ) -> (VmResult, Option<Dynamic>) {
        let saved = mem::replace(&mut self.this, this);

        let result = match self.engine.track_operation(&mut self.global, pos) {
            Ok(()) => {
                self.global.level += 1;
                let result = self.call_compiled_body(
                    program,
                    name,
                    params,
                    chunk,
                    first,
                    scope,
                    rewind_scope,
                    pos,
                );
                self.global.level -= 1;
                result
            }
            Err(err) => Err(err),
        };

        (result, mem::replace(&mut self.this, saved))
    }

    fn call_compiled_body(
        &mut self,
        program: &Program,
        name: &str,
        params: &[u32],
        chunk: Chunk,
        first: usize,
        scope: &mut Scope,
        rewind_scope: bool,
        pos: Position,
    ) -> VmResult {
        #[cfg(not(feature = "unchecked"))]
        {
            // The limit exists only where recursion does — `no_function` leaves
            // nothing to call, so Rhai drops the setting with the functions.
            #[cfg(not(feature = "no_function"))]
            if self.global.level > self.engine.max_call_levels() {
                return Err(Box::new(EvalAltResult::ErrorStackOverflow(pos)));
            }
            if params.len() > self.engine.max_variables() {
                return Err(Box::new(EvalAltResult::ErrorTooManyVariables(pos)));
            }
        }

        let scope_start_len = scope.len();

        #[cfg(feature = "debugging")]
        let orig_call_stack_len = self
            .global
            .debugger
            .as_ref()
            .map_or(0, |dbg| dbg.call_stack().len());

        for (param, slot) in params.iter().zip(first..) {
            let name = program
                .name(*param)
                .ok_or_else(|| malformed(format!("no name {param}")))?;
            // Taken, not cloned — Rhai consumes the caller's argument slots
            // (`func/script.rs:75`), and the caller truncates them away after.
            let value = self
                .stack
                .get_mut(slot)
                .ok_or_else(|| malformed("call with too few arguments".to_string()))?
                .take();
            scope.push_dynamic(name, value);
        }
        let scope_end_len = scope.len();

        // A frame for `back_trace` to see, pushed once the arguments are in the
        // scope so it reports the values the body will run with — the moment
        // Rhai picks (`func/script.rs:78`).
        #[cfg(feature = "debugging")]
        if self.engine.is_debugger_registered() {
            let args = scope
                .iter_inner()
                .skip(scope_start_len)
                .map(|(.., v)| v.flatten_clone());
            let source = self.global.source.clone();

            self.global
                .debugger_mut()
                .push_call_stack_frame(name.into(), args, source, pos);
        }

        // A function's parameters are its first locals, sitting at 0 upwards in
        // a scope that holds nothing else — so slot 0 is index 0.
        let mut reached = chunk.entry() as usize;
        let outcome = self.execute(program, scope, chunk, scope_start_len, &mut reached);
        // Read before the mapping below, which turns the `Return` that carries a
        // body's value into a success.
        let failed = outcome.is_err();

        let result = outcome.or_else(|err| match *err {
            // A `return` inside the body is the body's value.
            EvalAltResult::Return(value, ..) => Ok(value),
            // Exits and system errors pass straight through, positioned at the
            // call rather than at whatever raised them.
            EvalAltResult::Exit(..) => Err(reposition(err, pos)),
            _ if err.is_system_exception() => Err(reposition(err, pos)),
            // Everything else is attributed to the call.
            _ => Err(Box::new(EvalAltResult::ErrorInFunctionCall(
                name.to_string(),
                self.global.source().unwrap_or("").to_string(),
                err,
                pos,
            ))),
        });

        // Mapped first, then reported: Rhai tells the debugger what the *caller*
        // will see (`func/script.rs:157-186`), with the body's locals still in
        // scope for the callback to read.
        #[cfg(feature = "debugging")]
        let result = self.at_function_exit(scope, result, orig_call_stack_len, pos);

        // Rewind scope.
        if rewind_scope {
            scope.rewind(scope_start_len);
        } else if scope_end_len != scope_start_len {
            // Remove arguments only, leaving new variables in the scope
            scope.remove_range(scope_start_len, scope_end_len - scope_start_len);
        }

        if failed {
            self.record_fault(reached);
        }

        result
    }

    /// Report how a body ended and drop its call stack frame.
    ///
    /// The frame goes however the call ended — an error escaping one must not
    /// leave the stack deep, or every later trace is wrong.
    #[cfg(feature = "debugging")]
    fn at_function_exit(
        &mut self,
        scope: &mut Scope,
        result: VmResult,
        orig_call_stack_len: usize,
        pos: Position,
    ) -> VmResult {
        if !self.engine.is_debugger_registered() {
            return result;
        }

        // Only where something is waiting for it: a `FunctionExit` asked for at
        // this level or outside it, or a step that has to stop somewhere and
        // this is where the body ran out (`func/script.rs:159-163`).
        let trigger = match self.global.debugger().status {
            crate::eval::DebuggerStatus::FunctionExit(n) => n >= self.global.level,
            crate::eval::DebuggerStatus::Next(.., true) => true,
            _ => false,
        };

        let result = if trigger {
            // The call site, where Rhai has the body's closing brace: a chunk
            // keeps no position for the end of itself, and the call is the place
            // Rhai falls back to when the body has none either.
            let node = crate::ast::Stmt::Noop(pos);
            let event = match result {
                Ok(ref value) => crate::eval::DebuggerEvent::FunctionExitWithValue(value),
                Err(ref err) => crate::eval::DebuggerEvent::FunctionExitWithError(err),
            };

            let before = scope.len();
            let reported = self.engine.dbg_raw(
                &mut self.global,
                &mut self.caches,
                scope,
                self.this.as_mut(),
                (&node).into(),
                event,
            );
            rewind_after_stop(scope, before);

            match reported {
                Ok(..) => result,
                Err(err) => Err(err),
            }
        } else {
            result
        };

        if let Some(dbg) = self.global.debugger.as_mut() {
            dbg.rewind_call_stack(orig_call_stack_len);
        }

        result
    }

    /// Stop at a statement boundary, as Rhai stops at a statement node
    /// (`eval/stmt.rs:269`).
    ///
    /// `depth` is the marker's, and says which of the steps waiting in
    /// [`Vm::pending_steps`] belong to statements that have now ended.
    #[cfg(feature = "debugging")]
    fn at_statement(
        &mut self,
        scope: &mut Scope,
        depth: u16,
        pos: Position,
    ) -> Result<(), Box<EvalAltResult>> {
        if !self.engine.is_debugger_registered() {
            return Ok(());
        }

        let level = self.global.level;

        // A marker inside the statement a step was asked at is one that step
        // must not stop for — that is what stepping *over* a call or a block
        // means. Anything at the same depth or outside it, or in a frame further
        // out, is past the end of that statement.
        while let Some(&(at_level, at_depth, status)) = self.pending_steps.last() {
            if level > at_level || (level == at_level && depth > at_depth) {
                break;
            }
            self.pending_steps.pop();
            self.global.debugger_mut().reset_status(status);
        }

        let node = crate::ast::Stmt::Noop(pos);
        let before = scope.len();
        let resumed = self.engine.dbg_reset(
            &mut self.global,
            &mut self.caches,
            scope,
            self.this.as_mut(),
            &node,
        );
        rewind_after_stop(scope, before);

        if let Some(status) = resumed? {
            self.pending_steps.push((level, depth, status));
        }

        Ok(())
    }

    /// Tell the debugger the run is over, as Rhai does at the end of an `eval`
    /// (`api/eval.rs:255-260`).
    ///
    /// Only where the run got there: the walker's `?` on the statements means a
    /// failed script never reaches this.
    #[cfg(feature = "debugging")]
    fn at_end(&mut self, scope: &mut Scope) -> Result<(), Box<EvalAltResult>> {
        if !self.engine.is_debugger_registered() {
            return Ok(());
        }

        self.global.debugger_mut().status = crate::eval::DebuggerStatus::Terminate;
        let node = crate::ast::Stmt::Noop(Position::NONE);

        self.engine
            .dbg(&mut self.global, &mut self.caches, scope, None, &node)
    }

    /// Run one frame, cleaning up after it however it leaves.
    ///
    /// Whatever the frame's loops are holding goes when the frame does — a
    /// `return` out of a `for`, or an error escaping one, both skip the
    /// `IterNext` that would have dropped the iterator. Doing it here rather
    /// than at each exit means there is one place to be right.
    fn execute(
        &mut self,
        program: &Program,
        scope: &mut Scope,
        chunk: Chunk,
        base: usize,
        reached: &mut usize,
    ) -> VmResult {
        let iter_base = self.iterators.len();
        let handler_base = self.handlers.len();
        let size_base = self.sizes.len();
        // Each frame's floor is its own. A checkpoint inside a function this
        // one calls must not become what this one unwinds to.
        let outer_floor = mem::replace(&mut self.unwind_floor, base);

        // The dispatch loop uses `?` throughout, so an error leaves it rather
        // than being examined inside it. Catching therefore happens out here:
        // the loop stops, a handler this frame armed gets the error, and the
        // loop restarts at the catch block. `run_frame` keeps `pc` in a
        // register and the fault address arrives through `reached`, which is
        // written every instruction anyway, so none of this costs the common
        // path anything.
        let mut start = chunk.entry() as usize;
        let result = loop {
            match self.run_frame(program, scope, base, reached, start) {
                Ok(value) => break Ok(value),
                Err(err) => match self.catch(program, err, handler_base, scope) {
                    // Metered like a backward jump, and for the same reason:
                    // a catch block that sits before the throw is a cycle the
                    // dispatch loop never sees as one, because control got
                    // there through the error path rather than through a jump.
                    Ok(resume) => {
                        // Handled, so the frames it unwound past are not where
                        // this run failed. Left behind, they would head the
                        // next error's trace.
                        self.clear_faults();
                        self.engine
                            .track_operation(&mut self.global, program.position(resume))?;
                        start = resume;
                    }
                    Err(err) => break Err(err),
                },
            }
        };

        self.iterators.truncate(iter_base);
        self.handlers.truncate(handler_base);
        self.sizes.truncate(size_base);

        if result.is_err() {
            self.unwind_after_error(scope);
        }
        self.unwind_floor = outer_floor;
        result
    }

    /// Rhai's `Engine::make_type_mismatch_err` (`api/formatting.rs:246`).
    ///
    /// The asymmetry is Rhai's and is easy to get wrong in either direction:
    /// the *expected* type goes through the engine's registered names and the
    /// *actual* one does not. So `if 0..1 {}` reports
    /// `core::ops::range::Range<i64>` rather than the `range` the same engine
    /// would print anywhere else. Mapping both — which reads like the obvious
    /// thing — makes every one of these differ from the walker.
    fn mismatch<T>(&self, actual: &str, pos: Position) -> Box<EvalAltResult> {
        Box::new(EvalAltResult::ErrorMismatchDataType(
            self.engine
                .map_type_name(core::any::type_name::<T>())
                .into(),
            actual.into(),
            pos,
        ))
    }

    /// What reading a key a map does not have produces.
    ///
    /// Unit, unless the host asked for the strict reading — which is a whole
    /// engine option (`fail_on_invalid_map_property`) rather than anything the
    /// script says, so it has to be consulted rather than assumed.
    #[cfg(not(feature = "no_object"))]
    fn absent_key(&self, key: &str, pos: Position) -> VmResult {
        if self.engine.fail_on_invalid_map_property() {
            Err(Box::new(EvalAltResult::ErrorPropertyNotFound(
                key.to_string(),
                pos,
            )))
        } else {
            Ok(Dynamic::UNIT)
        }
    }

    /// Fold the element on top of the stack into the literal's running total,
    /// and refuse it if that puts the literal over a configured limit.
    ///
    /// Reproduces `eval/expr.rs:318-329` for an array and `:349-359` for a map.
    /// The two differ in one place — an array element adds one to the array
    /// count, a map entry adds one to the map count — and in nothing else, so
    /// they share this.
    ///
    /// Worth being exact about what is *not* counted: a map's total starts at
    /// zero and only the entries with computed values are added to it, because
    /// Rhai's loop runs over those alone and the constant ones are already
    /// sitting in the template. A literal that is entirely constant never
    /// reaches here at all — the optimizer folded it long before.
    ///
    /// Out of line: it is a handful of instructions in the common case and a
    /// call to Rhai in the rare one, and the dispatch loop is measurably
    /// sensitive to what shares its registers.
    fn check_size(
        &mut self,
        index: u16,
        map: bool,
        pos: Position,
    ) -> Result<(), Box<EvalAltResult>> {
        if index == 0 {
            self.sizes.push((0, 0, 0));
        }

        // `unchecked` removes every limit this could reject against, so the
        // running total is never read and measuring it is pure cost. The push
        // above still happens: the stack is frame-floored either way, and the
        // instruction that drops it does not know which build it is in.
        //
        // Losing both `no_index` and `no_object` is the other way to get here:
        // with no literal to build there is nothing to measure, and the
        // measurement itself goes with the containers.
        #[cfg(any(
            feature = "unchecked",
            all(feature = "no_index", feature = "no_object")
        ))]
        {
            let _ = (map, pos);
            Ok(())
        }

        #[cfg(not(any(
            feature = "unchecked",
            all(feature = "no_index", feature = "no_object")
        )))]
        {
            // Rhai skips the whole measurement when no limit could reject it,
            // and measuring is a walk of the value — so this is the difference
            // between free and proportional to what the literal holds.
            if self.engine.max_string_size() == 0
                && self.engine.max_array_size() == 0
                && self.engine.max_map_size() == 0
            {
                return Ok(());
            }

            let value = self
                .stack
                .last()
                .ok_or_else(|| malformed("size check with no element".to_string()))?;
            let delta = calc_data_sizes(value, true);

            let total = self
                .sizes
                .last_mut()
                .ok_or_else(|| malformed("size check outside a literal".to_string()))?;
            *total = (
                total.0 + delta.0 + usize::from(!map),
                total.1 + delta.1 + usize::from(map),
                total.2 + delta.2,
            );

            self.engine
                .throw_on_size(*total)
                .map_err(|err| dispatch_failure(err, pos))
        }
    }

    /// Drop what an escaping error skipped the unwind for.
    ///
    /// An error leaves a block by jumping over the [`Op::UnwindTo`] that would
    /// have dropped what it declared, so those locals are still in the scope.
    /// Rhai rewinds a block whether it is left normally or by a throw, and
    /// rewinds nothing at a chunk's top level — which is what the floor is: the
    /// last top-level statement boundary. Anything above it belongs to a block
    /// that did not get to finish.
    ///
    /// Guarded rather than unconditional because [`Op::Return`] has already
    /// unwound to `base`, which is below the floor.
    ///
    /// Out of line for the reason [`Vm::catch`] is: it sits on the error edge
    /// of the frame, where nothing is hot and everything competes with the
    /// dispatch loop for the same registers.
    #[cold]
    fn unwind_after_error(&self, scope: &mut Scope) {
        if scope.len() > self.unwind_floor {
            scope.rewind(self.unwind_floor);
        }
    }

    /// Hand an error to the innermost handler this frame armed, if any.
    ///
    /// `Ok` is the address the catch block starts at. `Err` means nothing here
    /// wanted it and it should keep going up.
    ///
    /// Kept out of line for the reason [`Vm::call_compiled`] is: it sits on
    /// the dispatch loop's error edge, and letting it inline there costs every
    /// instruction that never fails.
    #[cold]
    fn catch(
        &mut self,
        program: &Program,
        err: Box<EvalAltResult>,
        handler_base: usize,
        scope: &mut Scope,
    ) -> Result<usize, Box<EvalAltResult>> {
        // Only handlers this frame armed. A callee must never resume into its
        // caller's catch block — that is a jump into another chunk, which the
        // verifier forbids and nothing would catch at run time. The callee's
        // error propagates normally instead, and `ErrorInFunctionCall` is
        // catchable, so the caller's own frame still sees it.
        // Walking outwards, because leaving one region can land the error in
        // the next: `try { try { throw 1 } catch { throw; } } catch (e) { .. }`
        // re-raises from the inner catch and the outer `try` still has to see
        // it.
        let mut err = err;
        let handler = loop {
            if self.handlers.len() <= handler_base {
                return Err(err);
            }
            let handler = self.handlers.last_mut().expect("checked");

            // Leaving a catch block rather than entering one. A bare `throw;`
            // there — an `ErrorRuntime` carrying unit — means "re-raise what
            // was caught, from here" (`eval/stmt.rs:866`).
            let Some(original) = handler.caught.take() else {
                break handler;
            };
            self.handlers.pop();
            let rethrown =
                matches!(&*err, EvalAltResult::ErrorRuntime(value, ..) if value.is_unit());
            if rethrown {
                let pos = err.position();
                err = original;
                err.set_position(pos);
            }
        };

        // `return`, `break`, `continue`, `exit` and the system exceptions
        // unwind as errors and are not exceptions a script may catch.
        if !err.is_catchable() {
            return Err(err);
        }

        let (target, catch_var) = (handler.target, handler.catch_var);
        let (operands, scope_len, iters) = (handler.operands, handler.scope_len, handler.iters);

        let mut err = err;
        let value = self.catch_value(&mut err, catch_var.is_some());

        // Back to where the `try` began, at all three depths.
        self.stack.truncate(operands);
        self.iterators.truncate(iters);
        scope.rewind(scope_len);

        if let Some(index) = catch_var {
            let name = program
                .name(index)
                .ok_or_else(|| malformed(format!("no name {index}")))?;
            #[cfg(not(feature = "unchecked"))]
            if scope.len() >= self.engine.max_variables() {
                return Err(Box::new(EvalAltResult::ErrorTooManyVariables(
                    program.position(target),
                )));
            }
            scope.push_dynamic(name, value);
        }

        self.handlers.last_mut().expect("checked").caught = Some(err);
        Ok(target)
    }

    /// What the catch variable is bound to (`eval/stmt.rs:809-845`).
    ///
    /// Three shapes: nothing at all without a variable, the raw thrown value
    /// for a `throw`, and a map of the error's parts for anything else. The
    /// unwrapping matters — a `throw` inside a called function arrives wrapped
    /// in `ErrorInFunctionCall`, and Rhai still binds the bare value.
    ///
    /// Under `no_object` there is no map to build one in, and Rhai binds the
    /// message alone (`eval/stmt.rs:815`).
    fn catch_value(&self, err: &mut Box<EvalAltResult>, wanted: bool) -> Dynamic {
        if !wanted {
            return Dynamic::UNIT;
        }
        if let EvalAltResult::ErrorRuntime(value, ..) = err.unwrap_inner() {
            return value.clone();
        }

        // Read *and cleared*, as Rhai does, so the message below carries no
        // trailing position and a re-raise starts from the catch site.
        #[cfg(feature = "no_object")]
        {
            let _ = err.take_position();
            err.to_string().into()
        }

        #[cfg(not(feature = "no_object"))]
        {
            let mut map = Map::new();
            let pos = err.take_position();

            map.insert("message".into(), err.to_string().into());
            if let Some(source) = &self.global.source {
                map.insert("source".into(), source.into());
            }
            if !pos.is_none() {
                let line = pos.line().unwrap_or(0) as INT;
                map.insert("line".into(), line.into());
                let column = pos.position().unwrap_or(0) as INT;
                map.insert("position".into(), column.into());
            }
            err.dump_fields(&mut map);
            map.into()
        }
    }

    /// The dispatch loop. `start` is the chunk's entry, or a catch block's
    /// address when [`Vm::execute`] resumes one after an error.
    ///
    /// Inlined into its one caller: splitting the loop out so errors could be
    /// caught outside it cost 1.55x to 1.40x on the tight-loop benchmark until
    /// this was here.
    #[inline(always)]
    fn run_frame(
        &mut self,
        program: &Program,
        scope: &mut Scope,
        base: usize,
        reached: &mut usize,
        start: usize,
    ) -> VmResult {
        // A called function pushes its operands above the caller's rather than
        // starting a stack of its own, so this records where its own begin.
        let stack_base = self.stack.len();
        self.stack.reserve(program.max_stack() as usize);

        // A residual's `Expr::Variable` nodes carry offsets Rhai's parser
        // computed against its own scope discipline, not against ours. Forcing
        // name lookup inside them costs a reverse scan but cannot be wrong.
        // Only programs that still have residuals pay it, which is the point of
        // driving the count to zero.
        if program.residual_count() > 0 {
            self.global.always_search_scope = true;
        }

        let code = program.code();
        // The chunk's entry the first time round, a catch block's address when
        // resumed after one.
        let mut pc = start;

        loop {
            // Nothing inside an iteration moves `pc` except a jump, and a jump
            // only happens after the instruction succeeded — so recording it
            // here names whichever instruction fails.
            *reached = pc;

            // No check against the chunk's end. Verification proves execution
            // cannot leave it — every path reaches a `Return`, no jump goes
            // outside, nothing falls off — so a comparison here would cost
            // every instruction to restate something already established.
            // No address in the message: `reached` is written every iteration,
            // so the fault trace already names this instruction. Read it from
            // `Vm::fault_pc`, corrupt artifact or not.
            let tag = *code.get(pc).ok_or_else(|| {
                Box::new(EvalAltResult::ErrorRuntime(
                    "ran off the end of a chunk".into(),
                    Position::NONE,
                ))
            })?;

            // Every instruction's operands sit at a fixed offset from its tag,
            // so dispatch is a match and a couple of loads with nothing decoded
            // and nothing allocated. The bounds checks are what let this run
            // straight off an artifact without trusting it; the verifier has
            // already made them unreachable for anything that loaded.
            let width = code::width(code, pc)
                .ok_or_else(|| malformed("undecodable instruction".to_string()))?;
            let small = |offset: usize| {
                code::u16_at(code, pc + offset)
                    .ok_or_else(|| malformed("truncated operand".to_string()))
            };
            let wide = |offset: usize| {
                code::u32_at(code, pc + offset)
                    .ok_or_else(|| malformed("truncated operand".to_string()))
            };

            // Instructions carry no position; the table does, keyed on the
            // address. A stripped program answers `NONE` for every one of
            // these, which is what a device runs — the address travels back
            // with the error instead, and the host resolves it.
            //
            // A closure rather than a value: most instructions never ask, and
            // the ones that do mostly ask only on the way to an error.
            let pos = || program.position(pc);

            // Every transfer of control goes through this, and a backward one
            // is charged an operation.
            //
            // A cycle in a chunk always contains a backward edge, so this is
            // what makes `max_operations` and the `on_progress` interrupt cover
            // a chunk *this compiler did not write*. `Op::Tick` covers the
            // loops it does write, positioned where Rhai would report them; a
            // corrupt artifact has no ticks at all and would otherwise spin
            // forever inside a loader that had already accepted it. Found by
            // `mutated_artifacts_load_or_fail_but_never_misbehave`, whose whole
            // claim is that this cannot happen.
            //
            // A macro rather than four open-coded checks because the failure
            // mode of missing one is silent, and because it costs nothing on
            // the straight-line path: only a jump pays the comparison.
            macro_rules! transfer {
                ($target:expr) => {{
                    let target: usize = $target;
                    if target <= pc {
                        self.engine.track_operation(&mut self.global, pos())?;
                    }
                    pc = target;
                }};
            }

            match tag {
                code::tag::CONST => {
                    let index = u32::from(small(1)?);
                    let value = program
                        .constant(index)
                        .ok_or_else(|| malformed(format!("no constant {index}")))?;
                    self.stack.push(value.clone());
                }

                code::tag::UNIT => self.stack.push(Dynamic::UNIT),
                code::tag::FALSE => self.stack.push(Dynamic::from(false)),
                code::tag::TRUE => self.stack.push(Dynamic::from(true)),

                code::tag::LOAD_LOCAL => {
                    let slot = small(1)?;
                    let index = base + slot as usize;
                    if index >= scope.len() {
                        return Err(malformed(format!("local slot {slot} is out of scope")));
                    }
                    // Reads clone out, matching how Rhai's own variable reads
                    // leave the scope entry alone (`eval/expr.rs:276-278`), and
                    // flattening any shared cell the way a read should.
                    self.stack
                        .push(scope.get_mut_by_index(index).flatten_clone());
                }

                code::tag::STORE_LOCAL | code::tag::STORE_CONST => {
                    let slot = small(1)?;
                    let index = base + slot as usize;
                    if index >= scope.len() {
                        return Err(malformed(format!("local slot {slot} is out of scope")));
                    }
                    let mut value = self.pop()?;
                    value.set_access_mode(if tag == code::tag::STORE_CONST {
                        AccessMode::ReadOnly
                    } else {
                        AccessMode::ReadWrite
                    });
                    // Through the cell, not over it — see `place`.
                    *place(scope.get_mut_by_index(index), "", pos())? = value;
                }

                code::tag::LOAD_NAMED | code::tag::LOAD_SHARED_NAMED => {
                    let index = u32::from(small(1)?);
                    let name = program
                        .name(index)
                        .ok_or_else(|| malformed(format!("no name {index}")))?;
                    let flatten = tag == code::tag::LOAD_NAMED;
                    let value = self.load_named(name, scope, flatten, pos())?;
                    self.stack.push(value);
                }

                code::tag::ASSIGN_NAMED | code::tag::ASSIGN_NAMED_OP => {
                    let index = u32::from(small(1)?);
                    let name = program
                        .name(index)
                        .ok_or_else(|| malformed(format!("no name {index}")))?;
                    let op = if tag == code::tag::ASSIGN_NAMED_OP {
                        let index = u32::from(small(3)?);
                        Some(
                            program
                                .assign_op(index)
                                .ok_or_else(|| malformed(format!("no op-assignment {index}")))?,
                        )
                    } else {
                        None
                    };

                    // Flattened before assigning, as Rhai does, so a shared
                    // cell is copied out rather than aliased into the target.
                    let rhs = self.pop()?.flatten();
                    self.assign_named(program, op, name, rhs, scope, pos())?;
                }

                code::tag::DECLARE_LOCAL | code::tag::DECLARE_CONST => {
                    let index = u32::from(small(1)?);
                    // A `Scope` entry name is an `Identifier`, which is a
                    // `SmartString` — short names live inline, so handing it a
                    // borrowed `&str` costs a copy rather than an allocation.
                    let name = program
                        .name(index)
                        .ok_or_else(|| malformed(format!("no name {index}")))?;
                    // Flattened, as Rhai flattens a declaration's initializer
                    // (`eval/stmt.rs:438`). A native can hand back a cell that is
                    // already shared, and sharing must stop at the `let` rather
                    // than becoming a property of the new local.
                    let value = self.pop()?.flatten();
                    if tag == code::tag::DECLARE_CONST {
                        scope.push_constant_dynamic(name, value);
                    } else {
                        scope.push_dynamic(name, value);
                    }
                }

                code::tag::ASSIGN_LOCAL | code::tag::ASSIGN_LOCAL_OP => {
                    let slot = small(1)?;
                    let var_name = u32::from(small(3)?);
                    let op = if tag == code::tag::ASSIGN_LOCAL_OP {
                        let index = u32::from(small(5)?);
                        Some(
                            program
                                .assign_op(index)
                                .ok_or_else(|| malformed(format!("no op-assignment {index}")))?,
                        )
                    } else {
                        None
                    };

                    let index = base + slot as usize;
                    if index >= scope.len() {
                        return Err(malformed(format!("local slot {slot} is out of scope")));
                    }

                    // Rhai flattens the right-hand side before assigning
                    // (`eval/stmt.rs:324`), so a shared cell is copied out
                    // rather than aliased into the target.
                    let rhs = self.pop()?.flatten();

                    if scope.get_mut_by_index(index).is_read_only() {
                        let name = program
                            .name(var_name)
                            .ok_or_else(|| malformed(format!("no name {var_name}")))?;
                        return Err(Box::new(EvalAltResult::ErrorAssignmentToConstant(
                            name.to_string(),
                            pos(),
                        )));
                    }

                    // Written through rather than over: a slot a closure
                    // captured is a shared cell, and replacing it would sever
                    // every holder. `store` is the same path a chain's tail
                    // and a named assignment take, so `x op= y` resolves
                    // identically wherever the target lives.
                    let name = program
                        .name(var_name)
                        .ok_or_else(|| malformed(format!("no name {var_name}")))?;
                    // The guard is only needed for a cell a closure captured,
                    // and `x += 1` in a loop is the hot path — so the check
                    // for one is a discriminant test rather than the downcast
                    // chain `write_lock` walks, and the built-in operator is
                    // reached without leaving the dispatch loop or resolving
                    // the position.
                    //
                    // It is not free even so: the tight-loop benchmark went
                    // 1.63x to 1.55x when locals stopped being written over
                    // and started being written through. That is the price of
                    // a shared cell surviving an assignment, and of a chain
                    // over one not taking the host down.
                    let entry = scope.get_mut_by_index(index);
                    if !is_shared!(entry) {
                        let mut rhs = rhs;
                        if let Some(done) =
                            op.and_then(|op| self.store_builtin(op, entry, &mut rhs, pos))
                        {
                            done?;
                            pc += width;
                            continue;
                        }
                        self.store(program, op, entry, rhs, pos())?;
                        pc += width;
                        continue;
                    }

                    let mut target = place(entry, name, pos())?;
                    self.store(program, op, &mut target, rhs, pos())?;
                }

                code::tag::LOAD_THIS | code::tag::LOAD_THIS_SHARED => {
                    let value = self
                        .this
                        .as_ref()
                        .ok_or_else(|| Box::new(EvalAltResult::ErrorUnboundThis(pos())))?;
                    // Rhai's read is `this_ptr.cloned()` and does not flatten
                    // (`eval/expr.rs:272`); its consumers do. Which tag this is
                    // is which consumer asked.
                    self.stack.push(if tag == code::tag::LOAD_THIS {
                        value.flatten_clone()
                    } else {
                        value.clone()
                    });
                }

                code::tag::REQUIRE_THIS => {
                    if self.this.is_none() {
                        return Err(Box::new(EvalAltResult::ErrorUnboundThis(pos())));
                    }
                }

                code::tag::ASSIGN_THIS | code::tag::ASSIGN_THIS_OP => {
                    let op = if tag == code::tag::ASSIGN_THIS_OP {
                        let index = u32::from(small(1)?);
                        Some(
                            program
                                .assign_op(index)
                                .ok_or_else(|| malformed(format!("no op-assignment {index}")))?,
                        )
                    } else {
                        None
                    };

                    // Flattened before assigning, as everywhere else.
                    let rhs = self.pop()?.flatten();

                    // Taken out of the register rather than borrowed from it:
                    // `store` wants the whole `Vm`, and a write lock into the
                    // field could not outlive that borrow. Put back on both
                    // paths — Rhai's mutation survives an error, and a frame
                    // that lost its receiver would answer `ErrorUnboundThis` to
                    // every read after this one.
                    let mut this = self
                        .this
                        .take()
                        .ok_or_else(|| Box::new(EvalAltResult::ErrorUnboundThis(pos())))?;

                    let outcome = if this.is_read_only() {
                        // Named for an expression that has no name, which is
                        // what Rhai reports too (`eval/stmt.rs:118-122`).
                        Err(Box::new(EvalAltResult::ErrorAssignmentToConstant(
                            String::new(),
                            pos(),
                        )))
                    } else {
                        // Written through, not over: a shared receiver has to
                        // keep its cell, as a captured local does.
                        match place(&mut this, "", pos()) {
                            Ok(mut target) => self.store(program, op, &mut target, rhs, pos()),
                            Err(err) => Err(err),
                        }
                    };

                    self.this = Some(this);
                    outcome?;
                }

                code::tag::POP => {
                    let _ = self.pop()?;
                }

                code::tag::EVAL_AST | code::tag::EVAL_AST_KEEP => {
                    let index = u32::from(small(1)?);
                    let expr = program
                        .residual(index)
                        .ok_or_else(|| malformed(format!("no residual {index}")))?;
                    let rewind_scope = tag == code::tag::EVAL_AST;

                    // Straight to the walker's own entry points rather than
                    // through `EvalContext::eval_expression_tree_raw`, which
                    // is the same two calls behind a shim that only exists
                    // under `custom_syntax`. Total language coverage rests on
                    // this, so it must not depend on a feature.
                    //
                    // The frame's receiver goes with it, by reference. A body
                    // that uses `this` can still hold a fragment — `this?.x`,
                    // or a `this` body containing an `import` — and the walker
                    // has to read and write the same receiver the surrounding
                    // instructions do. The engine is copied out first so the
                    // four borrows below are of disjoint fields.
                    let engine = self.engine;
                    let value = match expr {
                        Expr::Stmt(block) => engine.eval_stmt_block(
                            &mut self.global,
                            &mut self.caches,
                            scope,
                            self.this.as_mut(),
                            block.statements(),
                            rewind_scope,
                        ),
                        expr => engine.eval_expr(
                            &mut self.global,
                            &mut self.caches,
                            scope,
                            self.this.as_mut(),
                            expr,
                        ),
                    }?;

                    self.stack.push(value);
                }

                code::tag::JUMP => {
                    transfer!(wide(1)? as usize);
                    continue;
                }

                code::tag::JUMP_IF_FALSE | code::tag::JUMP_IF_TRUE => {
                    let target = wide(1)? as usize;
                    let condition = self.pop()?;
                    // Rhai requires a boolean guard and reports the mismatch at
                    // the guard's own position (`eval/stmt.rs:487-490`).
                    let holds = condition
                        .as_bool()
                        .map_err(|actual| self.mismatch::<bool>(actual, pos()))?;
                    if holds == (tag == code::tag::JUMP_IF_TRUE) {
                        transfer!(target);
                        continue;
                    }
                }

                code::tag::SKIP_IF_NOT_UNIT => {
                    let target = wide(1)? as usize;
                    let condition = self.inspect()?;
                    if !condition.is_unit() {
                        transfer!(target);
                        continue;
                    }
                }

                code::tag::CALL | code::tag::CALL_CAPTURE | code::tag::CALL_OP => {
                    let name_index = u32::from(small(1)?);
                    let name = program
                        .name(name_index)
                        .ok_or_else(|| malformed(format!("no name {name_index}")))?;
                    let capture = tag == code::tag::CALL_CAPTURE;
                    let argc = code[pc + 3] as usize;
                    let op = if tag == code::tag::CALL_OP {
                        let index = u32::from(small(4)?);
                        Some(
                            program
                                .token(index)
                                .ok_or_else(|| malformed(format!("no operator {index}")))?,
                        )
                    } else {
                        None
                    };

                    let first = self
                        .stack
                        .len()
                        .checked_sub(argc)
                        .ok_or_else(|| malformed("call with too few arguments".to_string()))?;

                    // Reach the same built-in the walker reaches. Gated on
                    // Rhai's own `fast_operators()` rather than a guard of our
                    // own, so an engine that turns it off gets the dispatch
                    // path on both sides, and one that leaves it on gets the
                    // same answer — including for a user-registered operator
                    // on a primitive, which Rhai's fast path also bypasses
                    // (`func/call.rs:1775-1799`).
                    if let (Some(token), 2, true) = (op, argc, self.engine.fast_operators()) {
                        let (lhs, rhs) = self.stack.split_at_mut(first + 1);
                        let lhs = &mut lhs[first];
                        let rhs = &mut rhs[0];

                        // Custom types go to dispatch first, so a registered
                        // function still wins for them.
                        let builtin = (!lhs.is_variant() && !rhs.is_variant())
                            .then(|| get_builtin_binary_op_fn(token, lhs, rhs))
                            .flatten();
                        if let Some((func, need_context)) = builtin {
                            let context = need_context
                                .then(|| (self.engine, name, None, &self.global, pos()).into());
                            let value = func(context, &mut [lhs, rhs])?;
                            self.stack.truncate(first);
                            self.stack.push(value);
                            pc += width;
                            continue;
                        }
                    }

                    // Check if it is a built-in syntactic function.
                    let value = self.call_syntactic_or_stacked(
                        program,
                        name_index,
                        name,
                        argc,
                        first,
                        scope,
                        capture,
                        pos(),
                    )?;
                    self.stack.truncate(first);
                    self.stack.push(value);
                }

                code::tag::CALL_LOCAL_REF
                | code::tag::CALL_LOCAL_REF_CAPTURE
                | code::tag::CALL_NAMED_REF
                | code::tag::CALL_NAMED_REF_CAPTURE
                | code::tag::CALL_THIS_REF
                | code::tag::CALL_THIS_REF_CAPTURE => {
                    let name_index = u32::from(small(1)?);
                    let name = program
                        .name(name_index)
                        .ok_or_else(|| malformed(format!("no name {name_index}")))?;
                    let argc = code[pc + 3] as usize;
                    // `this` is a register, so this one carries no operand for
                    // the receiver and is two bytes shorter.
                    let receiver = match tag {
                        code::tag::CALL_LOCAL_REF | code::tag::CALL_LOCAL_REF_CAPTURE => {
                            Receiver::Local(small(4)?)
                        }
                        code::tag::CALL_NAMED_REF | code::tag::CALL_NAMED_REF_CAPTURE => {
                            Receiver::Named(u32::from(small(4)?))
                        }
                        code::tag::CALL_THIS_REF | code::tag::CALL_THIS_REF_CAPTURE => {
                            Receiver::This
                        }
                        _ => unreachable!(),
                    };
                    let capture = matches!(
                        tag,
                        code::tag::CALL_LOCAL_REF_CAPTURE
                            | code::tag::CALL_NAMED_REF_CAPTURE
                            | code::tag::CALL_THIS_REF_CAPTURE
                    );

                    let value = self.call_by_reference(
                        program,
                        name_index,
                        name,
                        argc,
                        receiver,
                        scope,
                        base,
                        capture,
                        pos(),
                    )?;
                    self.stack.push(value);
                }

                code::tag::ROTATE => {
                    let under = code[pc + 1] as usize;
                    let top = self
                        .stack
                        .len()
                        .checked_sub(1)
                        .ok_or_else(|| malformed("rotate on an empty stack".to_string()))?;
                    let to = top
                        .checked_sub(under)
                        .ok_or_else(|| malformed("rotate past the bottom".to_string()))?;
                    self.stack[to..].rotate_right(1);
                }

                // Emitted only for a literal, which is not syntax under the
                // feature that removes the type.
                #[cfg(not(feature = "no_index"))]
                code::tag::MAKE_ARRAY => {
                    let len = small(1)? as usize;
                    let first = self
                        .stack
                        .len()
                        .checked_sub(len)
                        .ok_or_else(|| malformed("array with too few elements".to_string()))?;

                    // The running total belongs to this literal and goes with
                    // it. `Op::CheckSize` is what filled it in, one element at
                    // a time, and what raised `ErrorDataTooLarge` against the
                    // element that tipped it over (`eval/expr.rs:307-330`).
                    //
                    // Only if there was one: an empty literal emits no
                    // `CheckSize` and pushed nothing, so popping here would
                    // take the *enclosing* literal's total — `[a, [], b]`.
                    if len > 0 {
                        self.sizes.pop();
                    }

                    // Flattened, as Rhai does, so a shared cell is copied in
                    // rather than aliased.
                    let array: Array = self.stack.drain(first..).map(Dynamic::flatten).collect();
                    self.stack.push(Dynamic::from_array(array));
                }

                #[cfg(not(feature = "no_object"))]
                code::tag::MAKE_MAP => {
                    let len = small(1)? as usize;
                    let first = self
                        .stack
                        .len()
                        .checked_sub(2 * len + 1)
                        .ok_or_else(|| malformed("map with too few operands".to_string()))?;
                    // As for `MakeArray`: nothing was pushed for a literal
                    // with no computed entries, so nothing may be popped.
                    if len > 0 {
                        self.sizes.pop();
                    }

                    let mut parts = self.stack.drain(first..);
                    let template = parts.next().expect("checked above");
                    let mut map = template
                        .try_cast::<Map>()
                        .ok_or_else(|| malformed("map literal without a template".to_string()))?;
                    while let Some(key) = parts.next() {
                        let value = parts.next().expect("pairs, checked above");
                        let key = key.into_immutable_string().map_err(|actual| {
                            malformed(format!("map key is a {actual}, not a string"))
                        })?;
                        // Flattened as Rhai does, so a shared cell is copied
                        // in rather than aliased.
                        map.insert(key.as_str().into(), value.flatten());
                    }
                    drop(parts);
                    self.stack.push(Dynamic::from_map(map));
                }

                code::tag::CHECK_ARRAY_SIZE | code::tag::CHECK_MAP_SIZE => {
                    let index = small(1)?;
                    let map = tag == code::tag::CHECK_MAP_SIZE;
                    self.check_size(index, map, pos())?;
                }

                code::tag::SWITCH => {
                    let index = u32::from(small(1)?);
                    let table = program
                        .switch(index)
                        .ok_or_else(|| malformed(format!("no switch {index}")))?;
                    let subject = self.pop()?;
                    // Always a jump: an arm that matched nothing still has the
                    // default to go to.
                    transfer!(table.dispatch(&subject) as usize);
                    continue;
                }

                code::tag::LOAD_SHARED => {
                    let slot = small(1)?;
                    let index = base + slot as usize;
                    if index >= scope.len() {
                        return Err(malformed(format!("local slot {slot} is out of scope")));
                    }
                    // Cloned, not flattened: cloning a shared `Dynamic` clones
                    // the `Rc`, which is the capture.
                    self.stack.push(scope.get_mut_by_index(index).clone());
                }

                // Emitted only for a closure capture, which cannot be parsed
                // under `no_closure`.
                #[cfg(not(feature = "no_closure"))]
                code::tag::SHARE | code::tag::SHARE_NAMED => {
                    let entry = if tag == code::tag::SHARE {
                        let slot = small(1)?;
                        let index = base + slot as usize;
                        if index >= scope.len() {
                            return Err(malformed(format!("local slot {slot} is out of scope")));
                        }
                        Some(index)
                    } else {
                        let name_index = u32::from(small(1)?);
                        let name = program
                            .name(name_index)
                            .ok_or_else(|| malformed(format!("no name {name_index}")))?;
                        // The resolver gets first refusal, and a name it
                        // answers is not shared at all (`eval/stmt.rs:998`).
                        if self.resolve_var(name, scope, pos())?.is_some() {
                            pc += width;
                            continue;
                        }
                        // `iter_raw` walks the scope from the top down, which is
                        // the order shadowing wants — the first match is the
                        // live one — but it counts from the other end than
                        // `get_mut_by_index` does, so the position has to be
                        // turned back round. Rhai reaches the same entry
                        // through `Scope::search`, which is not public
                        // (`eval/stmt.rs:1009`).
                        let depth = scope.len();
                        let found = scope
                            .iter_raw()
                            .position(|(entry, ..)| entry == name)
                            .map(|from_top| depth - 1 - from_top);
                        Some(found.ok_or_else(|| missing(name, pos()))?)
                    };

                    if let Some(index) = entry {
                        let value = scope.get_mut_by_index(index);
                        if !value.is_shared() {
                            *value = value.take().into_shared();
                        }
                    }
                }

                code::tag::MAKE_CLOSURE => {
                    let index = u32::from(small(1)?);
                    let name = program
                        .name(index)
                        .ok_or_else(|| malformed(format!("no name {index}")))?;
                    // Non-validated, because `anon$…` is not a name a script could
                    // have written and the validating constructors refuse it.
                    // Nothing unsound rides on that check — a name that will
                    // not resolve simply fails when the pointer is called.
                    self.stack.push(
                        FnPtr {
                            name: name.into(),
                            curry: Default::default(),
                            #[cfg(not(feature = "no_function"))]
                            env: None,
                            typ: FnPtrType::Normal,
                        }
                        .into(),
                    );
                }

                #[cfg(not(feature = "no_closure"))]
                code::tag::IS_SHARED => {
                    let value = self.pop()?;
                    self.stack.push(value.is_shared().into());
                }

                code::tag::MAKE_FN_PTR => {
                    let name = self.pop()?;
                    let name = name
                        .into_immutable_string()
                        .map_err(|actual| self.mismatch::<ImmutableString>(actual, pos()))?;
                    // Validates that the name is an identifier, as Rhai's own
                    // `Fn(..)` does (`func/call.rs:1215`).
                    let pointer = FnPtr::new(name).map_err(|mut err| {
                        if err.position().is_none() {
                            err.set_position(pos());
                        }
                        err
                    })?;
                    self.stack.push(pointer.into());
                }

                code::tag::CURRY => {
                    let argc = code[pc + 1] as usize;
                    let at = self
                        .stack
                        .len()
                        .checked_sub(argc + 1)
                        .ok_or_else(|| malformed("curry is missing its target".into()))?;
                    let mut pointer = self.stack[at]
                        .clone()
                        .try_cast::<FnPtr>()
                        .ok_or_else(|| self.mismatch::<FnPtr>(self.stack[at].type_name(), pos()))?;
                    for value in self.stack.drain(at + 1..) {
                        pointer.add_curry(value);
                    }
                    self.stack.truncate(at);
                    self.stack.push(pointer.into());
                }

                code::tag::CALL_FN_PTR
                | code::tag::CALL_FN_PTR_METHOD
                | code::tag::CALL_FN_PTR_ON_LOCAL
                | code::tag::CALL_FN_PTR_ON_NAMED
                | code::tag::CALL_FN_PTR_ON_THIS => {
                    let argc = code[pc + 1] as usize;
                    let method = tag != code::tag::CALL_FN_PTR;
                    let receiver = match tag {
                        code::tag::CALL_FN_PTR_ON_LOCAL => Some(Receiver::Local(small(2)?)),
                        code::tag::CALL_FN_PTR_ON_NAMED => {
                            Some(Receiver::Named(u32::from(small(2)?)))
                        }
                        code::tag::CALL_FN_PTR_ON_THIS => Some(Receiver::This),
                        _ => None,
                    };
                    let value =
                        self.call_fn_ptr(program, argc, method, receiver, scope, base, pos())?;
                    self.stack.push(value);
                }

                code::tag::INTERPOLATE_START => {
                    self.stack.push(self.engine.const_empty_string().into());
                }

                code::tag::INTERPOLATE_APPEND => {
                    let segment = self.pop()?;
                    self.append_segment(segment, pos())?;
                }

                code::tag::INTERPOLATE_END => {
                    let buffer = self.pop()?;
                    let text = buffer
                        .into_immutable_string()
                        .map_err(|_| malformed("interpolation lost its buffer".into()))?;
                    // Interned, as Rhai does: the same rendered string in ten
                    // places is one allocation, which is the whole reason the
                    // engine keeps an interner.
                    let value = self.engine.get_interned_string(text.as_str());
                    self.stack.push(value.into());
                }

                code::tag::CHAIN => {
                    let index = u32::from(small(1)?);
                    let chain = program
                        .chain(index)
                        .ok_or_else(|| malformed(format!("no chain {index}")))?;
                    let value = self.run_chain(program, chain, index, scope, base, pos())?;
                    self.stack.push(value);
                }

                code::tag::UNWIND_TO => {
                    let depth = small(1)?;
                    let target = base + depth as usize;
                    if target > scope.len() {
                        return Err(malformed(format!(
                            "unwind to {target} past a scope of {}",
                            scope.len()
                        )));
                    }
                    scope.rewind(target);
                }

                code::tag::TICK => self.engine.track_operation(&mut self.global, pos())?,

                code::tag::CHECKPOINT => self.unwind_floor = scope.len(),

                code::tag::STATEMENT => {
                    // Nothing to stop for without the interface to stop at, and
                    // no marker in the code at all unless it was compiled by a
                    // build that has one.
                    #[cfg(feature = "debugging")]
                    {
                        let depth = small(1)?;
                        self.at_statement(scope, depth, pos())?;
                    }
                }

                code::tag::PUSH_HANDLER | code::tag::PUSH_HANDLER_VAR => {
                    let target = wide(1)? as usize;
                    let catch_var = if tag == code::tag::PUSH_HANDLER_VAR {
                        Some(u32::from(small(5)?))
                    } else {
                        None
                    };
                    self.handlers.push(Handler {
                        target,
                        catch_var,
                        operands: self.stack.len(),
                        scope_len: scope.len(),
                        iters: self.iterators.len(),
                        caught: None,
                    });
                }

                code::tag::POP_HANDLER => {
                    self.handlers.pop();
                }

                code::tag::ITER_INIT => {
                    let iterable = self.pop()?;
                    self.iter_init(iterable, pos())?;
                }

                code::tag::ITER_DROP => {
                    self.iterators.pop();
                }

                code::tag::ITER_NEXT | code::tag::ITER_NEXT_INDEXED => {
                    let exit = wide(1)? as usize;
                    let iteration = self
                        .iterators
                        .last_mut()
                        .ok_or_else(|| malformed("no iterator to advance".to_string()))?;

                    let Some(item) = iteration.items.next() else {
                        self.iterators.pop();
                        transfer!(exit);
                        continue;
                    };

                    // Counted before the item is unwrapped, as Rhai does, so a
                    // loop long enough to wrap the counter is an error rather
                    // than a wrap.
                    iteration.count = iteration.count.checked_add(1).ok_or_else(|| {
                        Box::new(EvalAltResult::ErrorArithmetic(
                            format!("for-loop counter overflow: {}", iteration.count),
                            pos(),
                        ))
                    })?;
                    let count = iteration.count;

                    // A fallible iterator's error is positioned at the
                    // iterable, and only if it brought none of its own
                    // (`eval/stmt.rs:749`).
                    let value = item.map_err(|mut err| {
                        if err.position().is_none() {
                            err.set_position(pos());
                        }
                        err
                    })?;

                    if tag == code::tag::ITER_NEXT_INDEXED {
                        self.stack.push(Dynamic::from(count));
                    }
                    self.stack.push(value.flatten());
                }

                code::tag::STORE_SHARED => {
                    let slot = small(1)?;
                    let index = base + slot as usize;
                    if index >= scope.len() {
                        return Err(malformed(format!("local slot {slot} is out of scope")));
                    }
                    let value = self.pop()?;
                    // Through the cell: a closure made in an earlier iteration
                    // shares this slot, and Rhai writes into it rather than
                    // replacing it (`eval/stmt.rs:752`).
                    *place(scope.get_mut_by_index(index), "", pos())? = value;
                }

                code::tag::THROW => {
                    // Flattened, as Rhai does, so a shared cell is thrown as
                    // its value rather than as the cell.
                    let value = self.pop()?.flatten();
                    return Err(Box::new(EvalAltResult::ErrorRuntime(value, pos())));
                }

                code::tag::RETURN => {
                    let value = self.stack.pop().unwrap_or(Dynamic::UNIT);
                    // Whatever else this frame left behind goes with it, so a
                    // caller's stack is exactly as it was.
                    self.stack.truncate(stack_base);
                    return Ok(value);
                }

                // `code::width` already refused anything it does not know, so
                // this is unreachable — but a wildcard is what stops a new tag
                // from silently falling through to the next instruction.
                _ => return Err(malformed(format!("unknown instruction {tag:#04x}"))),
            }

            pc += width;
        }
    }
}

/// The `this` register, reached by hand-built chunks.
///
/// The compiler does not emit any of these yet — it still refuses a body that
/// mentions `this` — so this is the only thing that executes them until it does.
/// Worth having on its own account regardless: what a hand-made artifact can say
/// is exactly what a verifier-plus-VM has to survive.
///
/// Every case enters through [`Vm::call_fn_with_options`], which is the only
/// way to bind a receiver from outside — and which `no_function` takes away
/// along with Rhai's `CallFnOptions`.
#[cfg(test)]
#[cfg(not(feature = "no_function"))]
mod tests {
    use super::*;
    use crate::grain::bytecode::{assemble, Chain, Chunk, Op, Positions, Step, Strings, Tail};
    use crate::grain::format::Abi;
    use crate::grain::program::{Function, Parts};
    use crate::{CallFnOptions, Engine, Scope, INT};

    /// A program of phantom functions, named `f` upwards in the order given.
    ///
    /// The main chunk does nothing: everything here is entered through
    /// [`Vm::call_fn_with_options`], which is the only thing that can bind a
    /// receiver.
    fn program_of(bodies: &[&[Op]], consts: Vec<Dynamic>) -> Program<'static> {
        program_with(bodies, consts, Vec::new(), Vec::new())
    }

    /// The same, for the chain instruction, whose record lives in a pool rather
    /// than in the code.
    fn program_with_chains(
        bodies: &[&[Op]],
        consts: Vec<Dynamic>,
        chains: Vec<Chain>,
    ) -> Program<'static> {
        program_with(bodies, consts, chains, Vec::new())
    }

    /// The general form. Name indices are positions in `NAMES`.
    fn program_with(
        bodies: &[&[Op]],
        consts: Vec<Dynamic>,
        chains: Vec<Chain>,
        residuals: Vec<Expr>,
    ) -> Program<'static> {
        /// `f` and `g` are the functions; the rest are for chain steps to name.
        const NAMES: [&str; 4] = ["f", "g", "push", "len"];

        let mut all = vec![Op::Unit, Op::Return];
        let mut spans = Vec::new();
        for body in bodies {
            let start = all.len();
            all.extend_from_slice(body);
            spans.push(start..all.len());
        }

        // Assembling a prefix gives the byte offset that prefix ends at, which
        // is what a chunk is measured in.
        let end_of = |ops: usize| {
            assemble(&all[..ops])
                .expect("the test ops must assemble")
                .0
                .len() as u32
        };
        let (code, _) = assemble(&all).expect("the test ops must assemble");

        let functions = spans
            .iter()
            .enumerate()
            .map(|(index, span)| Function {
                name: index as u32,
                params: Vec::new(),
                this_type: None,
                takes_this: false,
                chunk: Chunk::new(end_of(span.start), end_of(span.end), 8),
            })
            .collect();

        Program::new(
            Abi::host().caps,
            code.into(),
            Chunk::new(0, end_of(2), 8),
            functions,
            Parts {
                positions: Positions::default(),
                debug_id: None,
                residuals,
                consts,
                names: Strings::new(NAMES),
                tokens: Vec::new(),
                assign_ops: Vec::new(),
                chains,
                switches: Vec::new(),
                lib: None,
                #[cfg(not(feature = "no_module"))]
                resolver: None,
                source: None,
            },
        )
    }

    /// The one function `f`, for the cases that need no callee.
    fn one(ops: &[Op], consts: Vec<Dynamic>) -> Program<'static> {
        program_of(&[ops], consts)
    }

    fn call(program: &Program, this: Option<&mut Dynamic>) -> Result<Dynamic, Box<EvalAltResult>> {
        let engine = Engine::new();
        let mut options = CallFnOptions::new().eval_ast(false);
        options.this_ptr = this;
        Vm::new(&engine).call_fn_with_options(options, &mut Scope::new(), program, "f", ())
    }

    #[test]
    fn a_bound_receiver_is_what_load_this_pushes() {
        let program = one(&[Op::LoadThis, Op::Return], Vec::new());
        let mut this = Dynamic::from(7 as INT);
        assert_eq!(call(&program, Some(&mut this)).unwrap().as_int(), Ok(7));
    }

    #[test]
    fn reading_an_unbound_receiver_is_an_error() {
        let program = one(&[Op::LoadThis, Op::Return], Vec::new());
        let err = *call(&program, None).unwrap_err();
        assert!(
            matches!(
                &err,
                EvalAltResult::ErrorInFunctionCall(_, _, inner, _)
                    if matches!(**inner, EvalAltResult::ErrorUnboundThis(..))
            ),
            "expected an unbound `this`, got {err:?}"
        );
    }

    /// `this = v` checks binding before evaluating `v`, which is the whole
    /// reason [`Op::RequireThis`] is a separate instruction.
    #[test]
    fn assigning_to_an_unbound_receiver_is_caught_before_the_value_runs() {
        let program = one(&[Op::RequireThis, Op::Unit, Op::Return], Vec::new());
        let err = *call(&program, None).unwrap_err();
        assert!(
            matches!(
                &err,
                EvalAltResult::ErrorInFunctionCall(_, _, inner, _)
                    if matches!(**inner, EvalAltResult::ErrorUnboundThis(..))
            ),
            "expected an unbound `this`, got {err:?}"
        );
    }

    #[test]
    fn a_write_through_this_reaches_the_hosts_value() {
        let program = one(
            &[
                Op::Const(0),
                Op::AssignThis { op: None },
                Op::Unit,
                Op::Return,
            ],
            vec![Dynamic::from(9 as INT)],
        );
        let mut this = Dynamic::from(1 as INT);
        assert!(call(&program, Some(&mut this)).is_ok());
        assert_eq!(this.as_int(), Ok(9));
    }

    /// Rhai reaches `this` through the caller's storage, so a body that mutates
    /// and then raises has already written.
    #[test]
    fn a_write_through_this_survives_a_failure_after_it() {
        let program = one(
            &[
                Op::Const(0),
                Op::AssignThis { op: None },
                Op::Const(1),
                Op::Throw,
            ],
            vec![Dynamic::from(9 as INT), Dynamic::from("boom")],
        );
        let mut this = Dynamic::from(1 as INT);
        assert!(call(&program, Some(&mut this)).is_err());
        assert_eq!(this.as_int(), Ok(9));
    }

    /// A callee gets `None`, whatever its caller was holding
    /// (`func/call.rs:669`). No conditional makes that true — every ordinary
    /// call goes through `call_compiled`, which installs `None`.
    ///
    /// Worth testing at all because the failure is invisible: a register that
    /// leaked would only show up in a callee that reads `this`, and reading a
    /// value that happens to be there looks like success.
    #[test]
    fn a_receiver_is_not_inherited_by_a_callee() {
        // `f` has a receiver and calls `g`, which reads one it was never given.
        let program = program_of(
            &[
                &[
                    Op::Call {
                        name: 1,
                        argc: 0,
                        op: None,
                        capture_parent_scope: false,
                    },
                    Op::Return,
                ],
                &[Op::LoadThis, Op::Return],
            ],
            Vec::new(),
        );

        let mut this = Dynamic::from(7 as INT);
        let err = *call(&program, Some(&mut this)).unwrap_err();
        assert!(
            format!("{err:?}").contains("ErrorUnboundThis"),
            "expected `g` to have no receiver, got {err:?}"
        );
    }

    /// And the caller still has its own afterwards.
    #[test]
    fn a_callee_frame_does_not_disturb_the_callers_receiver() {
        let program = program_of(
            &[
                &[
                    Op::Call {
                        name: 1,
                        argc: 0,
                        op: None,
                        capture_parent_scope: false,
                    },
                    Op::Pop,
                    Op::LoadThis,
                    Op::Return,
                ],
                &[Op::Unit, Op::Return],
            ],
            Vec::new(),
        );

        let mut this = Dynamic::from(7 as INT);
        assert_eq!(call(&program, Some(&mut this)).unwrap().as_int(), Ok(7));
    }

    /// A chain rooted at `this` whose method is a chunk of ours: the receiver
    /// becomes the callee's `this`, which is the binding Rhai does at
    /// `func/call.rs:649-655` and the only place a method call differs from a
    /// plain one.
    #[test]
    fn a_method_step_reaching_a_chunk_binds_the_receiver() {
        let program = program_with_chains(
            // `f` is `this.g()`; `g` is `this`.
            &[&[Op::Chain(0), Op::Return], &[Op::LoadThis, Op::Return]],
            Vec::new(),
            vec![Chain {
                root: Root::This {
                    pos: Position::NONE,
                },
                steps: vec![Step::Method {
                    name: 1, // `g`
                    argc: 0,
                    operand: 0,
                    flags: Default::default(),
                    pos: Position::NONE,
                }],
                tail: Tail::Read,
                operands: 0,
            }],
        );

        let mut this = Dynamic::from(7 as INT);
        assert_eq!(call(&program, Some(&mut this)).unwrap().as_int(), Ok(7));
    }

    /// And a write inside that callee travels back out through both frames: the
    /// callee's register, the chain's root write-back, then the host's pointer.
    #[test]
    fn a_write_inside_a_method_step_reaches_the_host() {
        let program = program_with_chains(
            // `f` is `this.g()`; `g` is `this = 9`.
            &[
                &[Op::Chain(0), Op::Return],
                &[
                    Op::Const(0),
                    Op::AssignThis { op: None },
                    Op::Unit,
                    Op::Return,
                ],
            ],
            vec![Dynamic::from(9 as INT)],
            vec![Chain {
                root: Root::This {
                    pos: Position::NONE,
                },
                steps: vec![Step::Method {
                    name: 1, // `g`
                    argc: 0,
                    operand: 0,
                    flags: Default::default(),
                    pos: Position::NONE,
                }],
                tail: Tail::Read,
                operands: 0,
            }],
        );

        let mut this = Dynamic::from(1 as INT);
        assert!(call(&program, Some(&mut this)).is_ok());
        assert_eq!(this.as_int(), Ok(9));
    }

    /// A fragment the compiler could not lower still sees the receiver. Without
    /// this the walker would be handed `None` and report `ErrorUnboundThis` for
    /// a `this` the surrounding instructions can read perfectly well.
    #[test]
    fn a_residual_fragment_reads_the_frames_receiver() {
        let program = program_with(
            &[&[
                Op::EvalAst {
                    residual: 0,
                    rewind_scope: false,
                },
                Op::Return,
            ]],
            Vec::new(),
            Vec::new(),
            vec![Expr::ThisPtr(Position::NONE)],
        );

        let mut this = Dynamic::from(7 as INT);
        assert_eq!(call(&program, Some(&mut this)).unwrap().as_int(), Ok(7));
    }

    /// A chain rooted at `this` mutates the caller's value rather than a copy.
    /// That is the whole reason `Root::This` is not `Root::Temporary`.
    #[test]
    #[cfg(not(feature = "no_index"))]
    fn a_chain_rooted_at_this_mutates_the_hosts_value() {
        let program = program_with_chains(
            &[&[Op::Const(0), Op::Chain(0), Op::Return]],
            vec![Dynamic::from(2 as INT)],
            vec![Chain {
                root: Root::This {
                    pos: Position::NONE,
                },
                steps: vec![Step::Method {
                    name: 2, // `push`
                    argc: 1,
                    operand: 0,
                    flags: Default::default(),
                    pos: Position::NONE,
                }],
                tail: Tail::Read,
                operands: 1,
            }],
        );

        let mut this = Dynamic::from(vec![Dynamic::from(1 as INT)]);
        assert!(call(&program, Some(&mut this)).is_ok());

        let array = this.into_array().expect("still an array");
        let items: Vec<INT> = array.iter().map(|v| v.as_int().unwrap()).collect();
        assert_eq!(items, vec![1, 2]);
    }

    /// `f(this, ..)` is Rhai's method-call rewrite, so the receiver goes by
    /// reference and a mutating native reaches the caller's value.
    #[test]
    #[cfg(not(feature = "no_index"))]
    fn this_as_a_first_argument_goes_by_reference() {
        let program = one(
            &[
                Op::LoadThis,
                Op::Const(0),
                Op::CallRef {
                    name: 2, // `push`
                    argc: 2,
                    receiver: Receiver::This,
                    capture_parent_scope: false,
                },
                Op::Return,
            ],
            vec![Dynamic::from(2 as INT)],
        );

        let mut this = Dynamic::from(vec![Dynamic::from(1 as INT)]);
        if let Err(err) = call(&program, Some(&mut this)) {
            panic!("expected the push to succeed, got {err:?}");
        }

        let items: Vec<INT> = this
            .into_array()
            .expect("still an array")
            .iter()
            .map(|v| v.as_int().unwrap())
            .collect();
        assert_eq!(items, vec![1, 2]);
    }

    /// The snapshot is pushed before the other arguments, so an unbound
    /// receiver is what fails — not whatever the arguments would have done.
    #[test]
    fn an_unbound_this_beats_a_failing_argument() {
        let program = one(
            &[
                Op::LoadThis,
                Op::LoadNamed(3), // `len`, which is no variable
                Op::CallRef {
                    name: 2,
                    argc: 2,
                    receiver: Receiver::This,
                    capture_parent_scope: false,
                },
                Op::Return,
            ],
            Vec::new(),
        );

        let err = *call(&program, None).unwrap_err();
        assert!(
            format!("{err:?}").contains("ErrorUnboundThis"),
            "expected the receiver to fail first, got {err:?}"
        );
    }

    #[test]
    fn a_chain_rooted_at_an_unbound_this_is_an_error() {
        let program = program_with_chains(
            &[&[Op::Chain(0), Op::Return]],
            Vec::new(),
            vec![Chain {
                root: Root::This {
                    pos: Position::NONE,
                },
                steps: vec![Step::Method {
                    name: 3, // `len`
                    argc: 0,
                    operand: 0,
                    flags: Default::default(),
                    pos: Position::NONE,
                }],
                tail: Tail::Read,
                operands: 0,
            }],
        );

        let err = *call(&program, None).unwrap_err();
        assert!(
            format!("{err:?}").contains("ErrorUnboundThis"),
            "expected an unbound `this`, got {err:?}"
        );
    }

    #[test]
    fn a_coalescing_step_short_circuits_on_unit() {
        let program = program_with_chains(
            &[&[Op::Unit, Op::Chain(0), Op::Return]],
            Vec::new(),
            vec![Chain {
                root: Root::Temporary,
                steps: vec![Step::Method {
                    name: 3, // `len`
                    argc: 0,
                    operand: 0,
                    flags: crate::grain::bytecode::StepFlags::SKIP_IF_UNIT,
                    pos: Position::NONE,
                }],
                tail: Tail::Read,
                operands: 0,
            }],
        );

        let out = call(&program, None).expect("coalescing should skip method dispatch on unit");
        assert!(out.is_unit(), "expected unit, got {out:?}");
    }

    #[test]
    fn a_read_only_receiver_refuses_the_write_and_is_left_alone() {
        let program = one(
            &[
                Op::Const(0),
                Op::AssignThis { op: None },
                Op::Unit,
                Op::Return,
            ],
            vec![Dynamic::from(9 as INT)],
        );
        let mut this = Dynamic::from(1 as INT).into_read_only();
        let err = *call(&program, Some(&mut this)).unwrap_err();
        assert!(
            matches!(
                &err,
                EvalAltResult::ErrorInFunctionCall(_, _, inner, _)
                    // Named for an expression that has no name.
                    if matches!(&**inner, EvalAltResult::ErrorAssignmentToConstant(name, ..) if name.is_empty())
            ),
            "expected a refused write to a constant, got {err:?}"
        );
        assert_eq!(this.as_int(), Ok(1));
    }
}
