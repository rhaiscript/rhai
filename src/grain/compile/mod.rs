mod cases;
mod poolable;
mod slots;

#[cfg(not(feature = "no_function"))]
use core::mem;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_function"))]
use crate::ast::ScriptFuncDef;
use crate::ast::{
    ASTFlags, Expr, FlowControl, FnCallExpr, OpAssignment, Stmt, StmtBlock, SwitchCasesCollection,
};
use crate::tokenizer::Token;
use crate::types::Span;
use crate::{Dynamic, ImmutableString, Position, AST};

use crate::grain::bytecode::{
    assemble, resolve_switch_targets, AssignOp, Chain, Chunk, Op, Positions, Receiver, Root, Step,
    Switch, SwitchCase, SwitchRange, Tail,
};
use crate::grain::compile::poolable::is_poolable;
use crate::grain::compile::slots::Slots;
use crate::grain::program::{Function, Parts, Program};

/// Whether a variable reference is module-qualified, as in `foo::bar`.
///
/// `Expr::Variable`'s payload only carries a `Namespace` when modules are
/// compiled in. Under `no_module` the box is two fields rather than four and
/// nothing can be qualified, so the question has a constant answer and the
/// field it would have read does not exist.
#[cfg(not(feature = "no_module"))]
macro_rules! has_namespace {
    ($payload:expr) => {
        !$payload.2.is_empty()
    };
}
#[cfg(feature = "no_module")]
macro_rules! has_namespace {
    ($payload:expr) => {{
        let _ = $payload;
        false
    }};
}

/// The same question for a call: is it `foo::bar()` rather than `bar()`.
/// `FnCallExpr` carries no `namespace` field at all under `no_module`.
#[cfg(not(feature = "no_module"))]
macro_rules! call_has_namespace {
    ($call:expr) => {
        !$call.namespace.is_empty()
    };
}
#[cfg(feature = "no_module")]
macro_rules! call_has_namespace {
    ($call:expr) => {{
        let _ = $call;
        false
    }};
}

/// Lowers a rhai `AST` into a [`Program`].
///
/// Anything not yet lowered is kept as an AST fragment and handed back to
/// rhai's walker at runtime, so the output always means the same as its input.
/// Progress is [`Program::residual_count`] falling.
#[derive(Debug, Default, Clone)]
pub struct Compiler {
    _private: (),
}

impl Compiler {
    /// Create a new [`Compiler`] with default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lower an `AST` into a [`Program`].
    #[must_use]
    pub fn compile(&self, ast: &AST) -> Program<'static> {
        // A bare script-function name used as a value is not a variable read
        // at all — rhai turns it into a function pointer with the calling
        // environment attached (`eval/expr.rs:71-99`) — so those names must
        // not become `LoadNamed`. Carried across every restart below, because
        // function bodies are lowered after one. Under `no_function` an `AST`
        // declares none, so there is nothing to hold back.
        #[cfg(not(feature = "no_function"))]
        let script_fns: Vec<ImmutableString> = ast
            .shared_lib()
            .iter_script_fn_info()
            .map(|(.., def)| def.name.clone())
            .collect();
        #[cfg(feature = "no_function")]
        let script_fns: Vec<ImmutableString> = Vec::new();
        let fresh = || Lowering {
            script_fns: script_fns.clone(),
            ..Lowering::default()
        };

        let mut lowering = fresh();

        // Anything the slot model cannot account for costs the whole program
        // its lowering rather than risking a scope it resolved slots against
        // being a different shape at runtime. Coverage is preserved either way.
        if !lowering.program(ast.statements(), true) {
            lowering = fresh();
            lowering.whole_program_residual(ast.statements());
        }
        let main_ops = lowering.code.len();

        // Each function's body appends to the same instruction list, so the
        // whole program assembles as one address space. A function the slot
        // model cannot handle is simply left out, and rhai's own copy of it
        // stays reachable through the library below.
        #[cfg(not(feature = "no_function"))]
        let (functions, skipped) = lowering.functions(ast);
        #[cfg(feature = "no_function")]
        let (functions, skipped): (Vec<LoweredFn>, usize) = (Vec::new(), 0);

        // Assembly can fail the same way the slot model can, for a script with
        // more distinct names or constants than a `u16` operand can index — so
        // it takes the same exit. The fallback is a single instruction and
        // always assembles, which is what keeps coverage total.
        // Switch targets are instruction indices too, and they live in the
        // pool rather than in the code, so they are resolved separately —
        // failing the same way, into the same fallback.
        let assembled = match assemble(&lowering.code) {
            Ok((code, offsets)) => resolve_switch_targets(&mut lowering.switches, &offsets)
                .ok()
                .map(|()| (code, offsets)),
            Err(..) => None,
        };
        let (code, offsets, main_ops, functions, skipped) = match assembled {
            Some((code, offsets)) => (code, offsets, main_ops, functions, skipped),
            None => {
                lowering = fresh();
                lowering.whole_program_residual(ast.statements());
                let (code, offsets) =
                    assemble(&lowering.code).expect("the fallback is one instruction");
                (code, offsets, lowering.code.len(), Vec::new(), 1)
            }
        };

        // Jump targets and the position table were both keyed on instruction
        // index while lowering; instructions vary in length once assembled.
        let mut positions = vec![rhai::Position::NONE; code.len()];
        for (index, pos) in lowering.positions.iter().enumerate() {
            positions[offsets[index] as usize] = *pos;
        }

        let main = Chunk::new(0, offsets[main_ops], lowering.max_stack);
        let functions: Vec<_> = functions
            .into_iter()
            .map(|f| Function {
                name: f.name,
                params: f.params,
                this_type: f.this_type,
                // Derived from the chunk by `Program::new`, which is the one
                // place that can see the assembled bytes.
                takes_this: false,
                chunk: Chunk::new(
                    offsets[f.first_op],
                    offsets[f.first_op + f.op_count],
                    lowering.max_stack,
                ),
            })
            .collect();

        // rhai's own functions are carried whenever anything might still reach
        // for them: a function this compiler skipped, or a fragment that could
        // call one. With neither, every call resolves in the table above and
        // the library — an `AST`'s whole function tree — can be dropped.
        //
        // The third case is a pointer to a `this`-taking chunk. Rhai reaches a
        // compiled function through a registered wrapper, and a wrapper is
        // registered at one arity — but a native calling a pointer against a
        // receiver decides for itself how many arguments to append beside it,
        // so no single arity is right. Rhai's own pointer carries the body and
        // sizes the call from it, which is what its copy is kept here for. See
        // `callback::wrappers`, which skips exactly these.
        #[cfg(not(feature = "no_function"))]
        let lib = {
            let escapes_as_pointer = crate::grain::program::makes_fn_pointers(&code)
                && functions
                    .iter()
                    .any(|f| crate::grain::program::takes_this(&code, f.chunk, &lowering.chains));
            let needs_walker = skipped > 0 || !lowering.residuals.is_empty() || escapes_as_pointer;
            (needs_walker && !ast.shared_lib().is_empty()).then(|| ast.shared_lib().clone())
        };
        // Under `no_function` there is no function tree to carry, whichever way
        // the fallbacks above went.
        #[cfg(feature = "no_function")]
        let lib = {
            let _ = skipped;
            None
        };

        let mut program = Program::new(
            code.into(),
            main,
            functions,
            Parts {
                positions: Positions::dense(positions),
                residuals: lowering.residuals,
                consts: lowering.consts,
                names: crate::grain::bytecode::Strings::new(&lowering.names),
                tokens: lowering.tokens,
                assign_ops: lowering.assign_ops,
                chains: lowering.chains,
                switches: lowering.switches,
                lib,
                #[cfg(not(feature = "no_module"))]
                resolver: ast.resolver.clone(),
                source: ast.source().map(Into::into),
            },
        );

        // `max_stack` above is an upper bound the lowering can compute without
        // a depth walk. The verifier does the walk anyway, so take its answer.
        program.tighten_stack();
        program
    }
}

/// Where `break` and `continue` jump to, and what they must unwind first.
///
/// Jump targets are backpatched: `break` sites are collected as they are
/// emitted and pointed at the instruction after the loop once that address is
/// known.
struct Loop {
    /// Where `continue` goes — the condition test, or the top of the body.
    continue_target: u32,
    /// Slot depth a `break` unwinds to. For a `for` loop this is *before* the
    /// loop variable, which leaving must drop.
    break_depth: u16,
    /// Slot depth a `continue` unwinds to. Differs from `break_depth` in a
    /// `for`, where the loop variable has to survive into the next iteration —
    /// one field cannot be both.
    continue_depth: u16,
    /// How many iterators are live *inside* this loop, so a jump out of it
    /// can drop whatever was made since. A `break` inside a `try` inside a
    /// `for` skips the straight-line path that would have cleaned up.
    iters: usize,
    /// Whether the loop owns an iterator of its own. `break` drops it and
    /// `continue` must not, which is the other thing one field cannot be.
    owns_iterator: bool,
    /// How many `try` regions were armed when the loop began, so a jump out
    /// of the loop disarms the ones inside it.
    handlers: usize,
    /// `Jump` sites awaiting the address after the loop.
    breaks: Vec<usize>,
}

/// Where a `switch` table entry sends control, before the arms have
/// addresses.
#[derive(Debug, Clone, Copy)]
enum Entry {
    /// Straight to an arm's body: the group has no guard to try first.
    Body(usize),
    /// The head of a guard chain, which is already emitted.
    At(u32),
    /// Nothing in the group can run.
    Default,
}

/// A function body that lowered, before its instruction indices become byte
/// addresses.
struct LoweredFn {
    name: u32,
    params: Vec<u32>,
    /// The declared receiver type, as a name-pool index. See
    /// [`Function::this_type`](crate::grain::program::Function::this_type).
    this_type: Option<u32>,
    first_op: usize,
    op_count: usize,
}

#[derive(Default)]
struct Lowering {
    code: Vec<Op>,
    /// One per instruction, parallel to `code`. Most are `NONE`; the dense
    /// shape is what makes a lookup an index, and it compacts on the way out.
    positions: Vec<Position>,
    residuals: Vec<Expr>,
    consts: Vec<Dynamic>,
    names: Vec<ImmutableString>,
    tokens: Vec<Token>,
    assign_ops: Vec<AssignOp>,
    chains: Vec<Chain>,
    switches: Vec<Switch>,
    slots: Slots,
    max_stack: u16,
    loops: Vec<Loop>,
    /// How many iterators are live at this point in the lowering, so a jump
    /// out of a loop knows how many to drop.
    iters: usize,
    /// The same for `try` regions: a `break` out of one has to disarm it, or
    /// the next unrelated error is caught into a block already left.
    handlers: usize,
    /// Names that are script functions rather than variables.
    script_fns: Vec<ImmutableString>,
    /// Set when something nested inside an expression defeated the slot model.
    ///
    /// [`Lowering::statement`] says so by returning false, but
    /// [`Lowering::expression`] has no way to: it is called from the middle of
    /// building other expressions, and every one of those callers would have
    /// to thread the answer back. So a block used as an expression records the
    /// failure here instead, and [`Lowering::program`] reports it.
    ///
    /// A sticky flag is enough because failure is all or nothing — the caller
    /// throws the whole lowering away and starts again as one fragment — so
    /// instructions emitted after it are discarded rather than run.
    defeated: bool,
}

impl Lowering {
    /// Lower a statement list as a whole chunk. Returns false if something
    /// defeated the slot model and the caller should fall back.
    ///
    /// `keeps_scope` says whether what this chunk declares outlives it, which
    /// is true of the program and false of every function body. Only then is a
    /// [`Op::Checkpoint`] worth emitting: it is what an escaping error unwinds
    /// to, and a function's scope is discarded whole however it ends.
    fn program(&mut self, statements: &[Stmt], keeps_scope: bool) -> bool {
        let Some((last, leading)) = statements.split_last() else {
            self.emit(Op::Unit);
            self.emit(Op::Return);
            return true;
        };

        for stmt in leading {
            if keeps_scope {
                self.emit(Op::Checkpoint);
            }
            if !self.statement(stmt) {
                return false;
            }
            // A statement's value is only the program's value if it is the
            // last one; rhai discards the rest.
            self.emit(Op::Pop);
        }

        if keeps_scope {
            self.emit(Op::Checkpoint);
        }
        if !self.statement(last) {
            return false;
        }

        self.emit(Op::Return);
        !self.defeated
    }

    /// Lower `a.b[i].c`, either reading it or assigning to it.
    ///
    /// Returns false if the chain is not one this can express, in which case
    /// the caller keeps it as a fragment.
    ///
    /// The shape is the awkward part. Rhai does not store a chain as a list:
    /// `a.b[i]` is `Dot { lhs: a, rhs: Index { lhs: b, rhs: i } }`, where each
    /// nested node's `lhs` is the *current* step's operand and its `rhs` is the
    /// continuation. [`flatten_chain`] unpicks that into steps.
    fn chain(&mut self, expr: &Expr, tail: Tail, value: Option<&Expr>) -> bool {
        let Some((root, steps)) = flatten_chain(expr) else {
            return false;
        };

        // A variable root is one the chain can write back into, by slot or by
        // name; anything else has to be both a read and a value rhai would
        // itself have evaluated into a temporary.
        //
        // `this` is deliberately not in the second class. Rhai reaches it
        // through the caller's `&mut` (`eval/chaining.rs:528`), so a method
        // step that mutates lands in the caller's value — walking a copy would
        // drop the write silently. It gets a root of its own instead.
        let root_spec = match root {
            Expr::Variable(v, ..) if !has_namespace!(v) => match self.slots.resolve(&v.1) {
                Some(slot) => Root::Local {
                    slot,
                    name: self.push_name(v.1.clone()),
                },
                // The caller's, or a module's, or nothing — decided at run
                // time, because which of the three it is decides whether the
                // chain can write through it.
                //
                // The guard is load-bearing: a bare script-function name is a
                // function pointer rather than a variable, and turning one
                // into a name lookup would report it missing where rhai hands
                // back a pointer.
                None if self.is_variable_name(&v.1, false) => Root::Named {
                    name: self.push_name(v.1.clone()),
                    pos: root.position(),
                },
                None => return false,
            },
            Expr::ThisPtr(pos) => Root::This { pos: *pos },
            // A qualified root resolves against imported modules, which need
            // `import` — the escape hatch's job.
            Expr::Variable(..) => return false,
            _ if matches!(tail, Tail::Read) => Root::Temporary,
            // Unreachable through the parser, which refuses `f().x = 1`
            // outright (`eval/chaining.rs:559`).
            _ => return false,
        };

        // Index values and method arguments are evaluated first, in step
        // order, exactly as rhai collects them before walking
        // (`eval/chaining.rs:568`). Evaluating one partway down would need the
        // operand stack while a borrow of the container is live.
        let mut lowered = Vec::with_capacity(steps.len());
        let mut operands = 0u16;

        for step in &steps {
            match step {
                ChainStep::Index(index, bracket) => {
                    self.expression(index);
                    lowered.push(Step::Index {
                        operand: operands,
                        pos: index.start_position(),
                        bracket: *bracket,
                    });
                    operands += 1;
                }
                ChainStep::Property(prop, pos) => {
                    let (getter, setter, name) = &**prop;
                    lowered.push(Step::Property {
                        name: self.push_name(name.clone()),
                        getter: self.push_name(getter.0.clone()),
                        setter: self.push_name(setter.0.clone()),
                        pos: *pos,
                    });
                }
                ChainStep::Method(call, pos) => {
                    if !self.is_lowerable_call(call) {
                        return false;
                    }
                    let Ok(argc) = u8::try_from(call.args.len()) else {
                        return false;
                    };
                    let first = operands;
                    for arg in call.args.iter() {
                        self.expression(arg);
                        operands += 1;
                    }
                    lowered.push(Step::Method {
                        name: self.push_name(call.name.clone()),
                        argc,
                        operand: first,
                        pos: *pos,
                    });
                }
            }
        }

        // Then the root, if it is one that has to be evaluated. After the
        // operands rather than before, which is rhai's order and not the
        // reading order: `[f()][g()]` calls `g` first.
        if matches!(root_spec, Root::Temporary) {
            self.expression(root);
        }

        // The value being assigned goes on last, above everything, so the
        // walk can take what it needs before it borrows the container.
        if let Some(value) = value {
            self.expression(value);
        }

        let index = self.push_chain(Chain {
            root: root_spec,
            steps: lowered,
            tail,
            operands,
        });
        self.emit_at(Op::Chain(index), expr.position());
        true
    }

    /// Lower a `switch` into one dispatch table plus the arms it names.
    ///
    /// The layout is: the subject, [`Op::Switch`], the guard chains, the arm
    /// bodies, the default. Every arm leaves one value and jumps to the end,
    /// so the statement's value is the matched arm's — or unit, which is what
    /// an absent `_` compiles to.
    ///
    /// Guards are why the table does not simply hold bodies. Rhai tries the
    /// arms sharing a case value in source order and falls to the *default*
    /// when they all decline — never on to the ranges (`eval/stmt.rs:544`) —
    /// so a group becomes a chain of guards ending in a jump to the default,
    /// and the table points at the chain. Nearly every arm anyone writes has
    /// no guard, and those cost no chain at all.
    fn switch(&mut self, subject: &Expr, sw: &SwitchCasesCollection) -> bool {
        // Sorted because rhai's map iterates in whatever order its hasher put
        // the entries in, and an artifact should not depend on that.
        let mut groups: Vec<(u64, Vec<usize>)> = sw
            .cases
            .iter()
            .map(|(hash, blocks)| (*hash, blocks.to_vec()))
            .collect();
        groups.sort_unstable_by_key(|(hash, ..)| *hash);

        // Overlapping range arms have no single answer at runtime, so they are
        // cut into disjoint pieces here instead. See [`cases::split`].
        let ranges = cases::split(&sw.ranges);

        // Unflattened: a shared subject is not hashable, and rhai's gate on
        // that is what sends it to the default arm. Flattening here would make
        // it match a case the walker skips.
        self.unflattened(subject);
        let table = self.push_switch();
        self.emit(Op::Switch(table));

        // One chain per distinct list of arms, shared by every table entry
        // naming it: `1 | 2 => ..` is two case values and one chain.
        let mut chains: Vec<(&[usize], Entry)> = Vec::new();
        let mut to_body: Vec<(usize, usize)> = Vec::new();
        let mut to_default: Vec<usize> = Vec::new();

        let lists = groups
            .iter()
            .map(|(.., blocks)| blocks.as_slice())
            .chain(ranges.iter().map(|(.., blocks)| blocks.as_slice()));
        for blocks in lists {
            if chains.iter().any(|(existing, ..)| *existing == blocks) {
                continue;
            }
            let entry = self.arm_chain(sw, blocks, &mut to_body, &mut to_default);
            chains.push((blocks, entry));
        }

        // Bodies, one per arm something can reach. An arm behind a constant
        // false guard, or one whose range the parser dropped for being empty,
        // is reachable by nothing and is not emitted.
        let mut wanted: Vec<usize> = to_body.iter().map(|(.., block)| *block).collect();
        wanted.extend(chains.iter().filter_map(|(.., entry)| match entry {
            Entry::Body(block) => Some(*block),
            _ => None,
        }));
        wanted.extend(sw.def_case);
        wanted.sort_unstable();
        wanted.dedup();

        let mut body_at: Vec<(usize, u32)> = Vec::with_capacity(wanted.len());
        let mut to_end: Vec<usize> = Vec::with_capacity(wanted.len());
        for block in wanted {
            body_at.push((block, self.here()));
            // An arm body is an ordinary expression, and a block one goes
            // through the same path as `let y = { .. }`.
            self.expression(&sw.expressions[block].rhs);
            if self.defeated {
                return false;
            }
            to_end.push(self.emit_jump());
        }

        let at = |block: usize| {
            body_at
                .iter()
                .find(|(candidate, ..)| *candidate == block)
                .map(|(.., at)| *at)
                .expect("every reachable arm was emitted above")
        };

        let default_at = match sw.def_case {
            Some(block) => at(block),
            None => {
                let target = self.here();
                self.emit(Op::Unit);
                target
            }
        };
        let end = self.here();

        for site in to_end {
            self.patch_to(site, end);
        }
        for site in to_default {
            self.patch_to(site, default_at);
        }
        for (site, block) in to_body {
            self.patch_to(site, at(block));
        }

        let target = |blocks: &[usize]| {
            let entry = chains
                .iter()
                .find(|(existing, ..)| *existing == blocks)
                .map(|(.., entry)| *entry)
                .expect("every list got a chain above");
            match entry {
                Entry::Body(block) => at(block),
                Entry::At(target) => target,
                Entry::Default => default_at,
            }
        };

        self.switches[table as usize] = Switch {
            cases: groups
                .iter()
                .map(|(hash, blocks)| SwitchCase {
                    hash: *hash,
                    target: target(blocks),
                })
                .collect(),
            ranges: ranges
                .iter()
                .map(|(range, blocks)| SwitchRange {
                    target: target(blocks),
                    ..*range
                })
                .collect(),
            default: default_at,
        };

        true
    }

    /// Emit the guard chain for one group of arms, and say where the table
    /// entries naming that group should point.
    fn arm_chain(
        &mut self,
        sw: &SwitchCasesCollection,
        blocks: &[usize],
        to_body: &mut Vec<(usize, usize)>,
        to_default: &mut Vec<usize>,
    ) -> Entry {
        let mut entry: Option<Entry> = None;

        for block in blocks {
            match &sw.expressions[*block].lhs {
                // An arm without an `if` is a literal `true` in the tree
                // (`parser.rs:1187`), so it always runs and everything after
                // it in the group is unreachable.
                Expr::BoolConstant(true, ..) => {
                    return match entry {
                        None => Entry::Body(*block),
                        Some(entry) => {
                            to_body.push((self.emit_jump(), *block));
                            entry
                        }
                    };
                }
                // Nothing can reach this arm, so nothing is emitted for it.
                Expr::BoolConstant(false, ..) => continue,
                guard => {
                    if entry.is_none() {
                        entry = Some(Entry::At(self.here()));
                    }
                    self.expression(guard);
                    let site = self.code.len();
                    // Rhai reports a non-boolean guard against the guard, so
                    // the jump carries the guard's position.
                    self.emit_at(Op::JumpIfTrue { target: u32::MAX }, guard.position());
                    to_body.push((site, *block));
                }
            }
        }

        match entry {
            Some(entry) => {
                to_default.push(self.emit_jump());
                entry
            }
            // Every arm in the group is behind a constant false guard, so the
            // group is the default with extra steps.
            None => Entry::Default,
        }
    }

    /// Reserve a table, to be filled in once its arms have addresses.
    fn push_switch(&mut self) -> u32 {
        self.switches.push(Switch {
            cases: Vec::new(),
            ranges: Vec::new(),
            default: 0,
        });
        (self.switches.len() - 1) as u32
    }

    fn push_chain(&mut self, chain: Chain) -> u32 {
        if let Some(index) = self.chains.iter().position(|existing| *existing == chain) {
            return index as u32;
        }
        self.chains.push(chain);
        (self.chains.len() - 1) as u32
    }

    /// Lower every script function the `AST` declares, and count the ones the
    /// slot model turned down.
    #[cfg(not(feature = "no_function"))]
    fn functions(&mut self, ast: &AST) -> (Vec<LoweredFn>, usize) {
        let mut functions = Vec::new();
        let mut skipped = 0;
        for (.., def) in ast.shared_lib().iter_script_fn_info() {
            match self.function(def) {
                Some(function) => functions.push(function),
                None => skipped += 1,
            }
        }
        (functions, skipped)
    }

    /// Lower one script function's body into the same instruction list.
    ///
    /// Returns `None` if the slot model cannot account for it, in which case
    /// rhai keeps its own copy and calls to it go through dispatch. That is a
    /// per-function decision: one awkward function does not cost the rest
    /// their lowering.
    ///
    /// The body runs in a fresh scope with the parameters already pushed
    /// (`func/script.rs:73`), so the parameters are exactly slots 0 upwards.
    #[cfg(not(feature = "no_function"))]
    fn function(&mut self, def: &ScriptFuncDef) -> Option<LoweredFn> {
        let first_op = self.code.len();
        let saved_slots = mem::take(&mut self.slots);
        let saved_loops = mem::take(&mut self.loops);
        // Per-function, like the slots: one body the model cannot handle must
        // not cost the rest of the program its lowering.
        let saved_defeated = mem::replace(&mut self.defeated, false);

        for param in def.params.iter() {
            self.slots.declare(param.clone());
        }
        let params: Vec<_> = def
            .params
            .iter()
            .map(|p| self.push_name(p.clone()))
            .collect();

        // A body is a statement list whose last value is the return value,
        // which is what `program` already does.
        let lowered = self.program(def.body.statements(), false);

        self.slots = saved_slots;
        self.loops = saved_loops;
        self.defeated = saved_defeated;

        if !lowered {
            // Roll back whatever the attempt emitted, so a function that could
            // not be lowered leaves no unreachable instructions behind.
            self.code.truncate(first_op);
            self.positions.truncate(first_op);
            return None;
        }

        Some(LoweredFn {
            name: self.push_name(def.name.clone()),
            params,
            this_type: def
                .this_type
                .as_ref()
                .map(|typed| self.push_name(typed.clone())),
            first_op,
            op_count: self.code.len() - first_op,
        })
    }

    /// The last-resort fallback: one fragment holding everything, evaluated
    /// without rewinding so top-level declarations still reach the caller.
    fn whole_program_residual(&mut self, statements: &[Stmt]) {
        let body = wrap_statements(statements.to_vec());
        let residual = self.push_residual(body);
        self.emit(Op::EvalAst {
            residual,
            rewind_scope: false,
        });
        self.emit(Op::Return);
    }

    /// Lower one statement, leaving its value on the stack.
    fn statement(&mut self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Var(payload, flags, ..) => {
                // `export let x = ...` also binds a module alias, which the
                // slot model does not represent.
                if flags.intersects(ASTFlags::EXPORTED) || self.slots.is_full() {
                    return false;
                }

                let (ident, init, ..) = &**payload;
                self.expression(init);

                let name = self.push_name(ident.name.clone());
                self.slots.declare(ident.name.clone());
                self.emit(Op::DeclareLocal {
                    name,
                    is_const: flags.intersects(ASTFlags::CONSTANT),
                });

                // A declaration evaluates to unit.
                self.emit(Op::Unit);
                true
            }

            Stmt::Expr(expr) => {
                self.expression(expr);
                true
            }

            // Rhai gives a call standing alone as a statement its own node
            // rather than wrapping it in `Stmt::Expr`, and an operator is a
            // call — so without this every top-level `a * b` stayed a fragment.
            // A closure's `curry` lands here rather than in `Stmt::Expr`,
            // because rhai gives a call standing alone as a statement its own
            // node.
            Stmt::FnCall(call, pos) if self.fn_ptr_call(call, *pos) => true,

            Stmt::FnCall(call, pos) if self.is_lowerable_call(call) => {
                self.lower_call(call, *pos);
                true
            }

            // Standing alone is the position `eval` is usually written in, and
            // rhai gives it its own node — so this is the arm that catches it,
            // not the `Expr::FnCall` one. See there for why it defeats the
            // lowering rather than becoming a fragment.
            Stmt::FnCall(call, ..) if call.name == crate::engine::KEYWORD_EVAL => false,

            // `this` on the left. Ahead of the two variable arms because rhai's
            // parser puts it there too (`parser.rs:2002`), and because the
            // chain arm below would otherwise take `this.x = 1`'s sibling.
            Stmt::Assignment(payload) if matches!(&payload.1.lhs, Expr::ThisPtr(..)) => {
                let (op_info, binary) = &**payload;

                // Before the right-hand side, not after. Rhai checks that
                // `this` is bound and returns before it evaluates the value
                // (`eval/stmt.rs:300-303`) — unlike the variable arm, which
                // evaluates first — so an unbound `this = nosuch` is
                // `ErrorUnboundThis` and not the value's own failure.
                self.emit_at(Op::RequireThis, binary.lhs.position());

                self.expression(&binary.rhs);
                let op = self.op_assignment(op_info);

                self.emit_at(Op::AssignThis { op }, op_info.position());
                self.emit(Op::Unit);
                true
            }

            // A plain local on the left.
            Stmt::Assignment(payload)
                if matches!(&payload.1.lhs, Expr::Variable(v, ..)
                    if !has_namespace!(v) && self.slots.resolve(&v.1).is_some()) =>
            {
                let (op_info, binary) = &**payload;
                let Expr::Variable(v, ..) = &binary.lhs else {
                    unreachable!("checked by the guard");
                };
                let slot = self.slots.resolve(&v.1).expect("checked by the guard");
                let var_name = self.push_name(v.1.clone());

                self.expression(&binary.rhs);
                let op = self.op_assignment(op_info);

                self.emit_at(Op::AssignLocal { slot, var_name, op }, op_info.position());
                self.emit(Op::Unit);
                true
            }

            // A variable no slot names — the caller's. Same shape as above,
            // and the same op-assignment resolution; only where the target
            // lives differs.
            Stmt::Assignment(payload)
                if matches!(&payload.1.lhs, Expr::Variable(v, ..)
                    if self.is_variable_name(&v.1, has_namespace!(v))) =>
            {
                let (op_info, binary) = &**payload;
                let Expr::Variable(v, ..) = &binary.lhs else {
                    unreachable!("checked by the guard");
                };
                let name = self.push_name(v.1.clone());

                self.expression(&binary.rhs);
                let op = self.op_assignment(op_info);

                // The variable's position, not the operator's — unlike
                // `AssignLocal`. The errors this instruction raises itself are
                // `ErrorAssignmentToConstant` and `ErrorVariableNotFound`, and
                // rhai reports both against the variable (`eval/stmt.rs:340`
                // and `eval/stmt.rs:120`). For a local those are unreachable,
                // because the parser rejects a constant it can see; for a name
                // the caller supplied they are the common failures.
                self.emit_at(Op::AssignNamed { name, op }, binary.lhs.position());
                self.emit(Op::Unit);
                true
            }

            // A chain on the left. The value goes on the stack after the
            // chain's own operands, so the walk has everything it needs before
            // it takes a borrow of the container.
            Stmt::Assignment(payload)
                if matches!(&payload.1.lhs, Expr::Dot(..) | Expr::Index(..)) =>
            {
                let (op_info, binary) = &**payload;
                let op = self.op_assignment(op_info);

                let mark = self.mark();
                if !self.chain(&binary.lhs, Tail::Assign { op }, Some(&binary.rhs)) {
                    self.rewind(mark);
                    let residual = self.push_residual(wrap_statements(vec![stmt.clone()]));
                    self.emit(Op::EvalAst {
                        residual,
                        rewind_scope: true,
                    });
                }
                // The chain leaves unit, which is what an assignment evaluates
                // to, so there is nothing to add here.
                true
            }

            // Emitted by the parser ahead of the `curry` call that binds a
            // closure's captures (`parser.rs:3707`).
            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(names) => {
                for (ident, ..) in names.iter() {
                    match self.slots.resolve(&ident.name) {
                        Some(slot) => self.emit_at(Op::Share(slot), ident.pos),
                        None => {
                            // The caller's — a closure can capture something
                            // no slot addresses.
                            let name = self.push_name(ident.name.clone());
                            self.emit_at(Op::ShareNamed(name), ident.pos);
                        }
                    }
                }
                self.emit(Op::Unit);
                true
            }

            Stmt::Block(block) => self.block(block.statements()),

            // `try { .. } catch (e) { .. }`.
            //
            // The catch block's value is thrown away: rhai's whole statement
            // is the try block's value on the way through and *unit* when
            // something was caught (`.map(|_| Dynamic::UNIT)`,
            // `eval/stmt.rs:863`). So `try { throw 7 } catch (e) { e * 2 }` is
            // unit, not 14.
            Stmt::TryCatch(payload, ..) => {
                let FlowControl { expr, body, branch } = &**payload;

                // An absent catch variable is `Expr::Unit`; a present one is
                // an `Expr::Variable` whose position is what rhai reports
                // `ErrorTooManyVariables` against.
                let catch_var = match expr {
                    Expr::Variable(v, ..) => Some(v.1.clone()),
                    _ => None,
                };

                let catch_name = catch_var.clone().map(|name| self.push_name(name));
                let site = self.code.len();
                self.emit_at(
                    Op::PushHandler {
                        target: u32::MAX,
                        catch_var: catch_name,
                    },
                    expr.position(),
                );
                self.handlers += 1;

                if !self.block(body.statements()) {
                    return false;
                }
                self.emit(Op::PopHandler);
                self.handlers -= 1;
                let past = self.emit_jump();

                // The catch block, entered with the scope back where the `try`
                // began and the variable already pushed on top of it. The
                // handler is still armed here — that is what makes a bare
                // `throw;` in this block a re-raise — so the depth goes back
                // up, and the `PopHandler` below is what ends the region.
                self.patch_to(site, self.here());
                self.handlers += 1;
                let depth = self.slots.depth();
                if let Some(name) = catch_var {
                    self.slots.declare(name);
                }
                if !self.block(branch.statements()) {
                    return false;
                }
                self.emit(Op::Pop);
                self.unwind_to(depth);
                self.emit(Op::PopHandler);
                self.handlers -= 1;
                self.emit(Op::Unit);

                self.patch_here(past);
                true
            }

            // `for x in seq` / `for (x, i) in seq`.
            //
            // The loop variable and counter are pushed once and written each
            // time round, not re-pushed — rhai does the same (`stmt.rs:708`),
            // and it is observable: a closure made in the body captures the
            // cell, so every one of them sees the last value.
            Stmt::For(payload, ..) => {
                let (var, counter, flow) = &**payload;
                let outside = u16::try_from(self.slots.depth()).expect("slot count is bounded");

                self.expression(&flow.expr);
                // `ErrorFor` is reported against the iterable's *start*, which
                // for `a.b` or a call is not its `position`.
                self.emit_at(Op::IterInit, flow.expr.start_position());
                self.iters += 1;

                // Counter first, matching the order rhai pushes them in, so
                // the slots line up with the scope it builds.
                let counter_slot = counter.as_ref().map(|ident| {
                    let name = self.push_name(ident.name.clone());
                    self.emit(Op::Unit);
                    self.emit(Op::DeclareLocal {
                        name,
                        is_const: false,
                    });
                    self.slots.declare(ident.name.clone());
                    self.slots.depth() as u16 - 1
                });
                let var_name = self.push_name(var.name.clone());
                self.emit(Op::Unit);
                self.emit(Op::DeclareLocal {
                    name: var_name,
                    is_const: false,
                });
                self.slots.declare(var.name.clone());
                let var_slot = self.slots.depth() as u16 - 1;

                let top = self.here();
                let exit = self.code.len();
                self.emit_at(
                    Op::IterNext {
                        exit: u32::MAX,
                        indexed: counter_slot.is_some(),
                    },
                    flow.expr.position(),
                );
                // The item is on top, the count under it, so these pop in the
                // order the two locals were declared.
                self.emit(Op::StoreShared(var_slot));
                if let Some(slot) = counter_slot {
                    self.emit(Op::StoreShared(slot));
                }
                self.emit_at(Op::Tick, flow.body.position());

                self.begin_for(top, outside);
                if !self.block_discarding(flow.body.statements()) {
                    return false;
                }
                self.emit(Op::Jump(top));
                let breaks = self.end_loop();

                // Exhausted: `IterNext` dropped the iterator on the way here.
                self.patch_to(exit, self.here());
                self.iters -= 1;
                self.emit(Op::UnwindTo(outside));
                self.slots.unwind_to(outside as usize);

                self.emit(Op::Unit);
                let past = self.emit_jump();
                for site in breaks {
                    self.patch_here(site);
                }
                self.patch_here(past);
                true
            }

            Stmt::Switch(payload, ..) => {
                let (subject, cases) = &**payload;
                self.switch(subject, cases)
            }

            Stmt::If(payload, ..) => {
                let FlowControl { expr, body, branch } = &**payload;

                self.expression(expr);
                let to_else = self.emit_jump_if_false(expr.position());

                if !self.block(body.statements()) {
                    return false;
                }
                let past_else = self.emit_jump();

                self.patch_here(to_else);
                if !self.block(branch.statements()) {
                    return false;
                }
                self.patch_here(past_else);
                true
            }

            // `loop` and `while true` are the same node: rhai marks an
            // unconditional loop with a unit or `true` guard
            // (`eval/stmt.rs:575-576`).
            Stmt::While(payload, ..) => {
                let FlowControl { expr, body, .. } = &**payload;
                let unconditional = matches!(expr, Expr::Unit(..) | Expr::BoolConstant(true, ..));

                let top = self.here();
                self.emit_at(Op::Tick, body.position());

                let exit = if unconditional {
                    None
                } else {
                    self.expression(expr);
                    Some(self.emit_jump_if_false(expr.position()))
                };

                self.begin_loop(top);
                if !self.block_discarding(body.statements()) {
                    return false;
                }
                self.emit(Op::Jump(top));

                let breaks = self.end_loop();
                if let Some(exit) = exit {
                    self.patch_here(exit);
                }
                // A `while` that runs to completion is unit; a `break value`
                // supplies its own. Both arrive here with the stack balanced.
                self.emit(Op::Unit);
                let past = self.emit_jump();
                for site in breaks {
                    self.patch_here(site);
                }
                self.patch_here(past);
                true
            }

            Stmt::Do(payload, flags, ..) => {
                let FlowControl { expr, body, .. } = &**payload;
                let until = flags.intersects(ASTFlags::NEGATED);

                let top = self.here();
                self.emit_at(Op::Tick, body.position());

                self.begin_loop(top);
                if !self.block_discarding(body.statements()) {
                    return false;
                }
                let breaks = self.end_loop();

                self.expression(expr);
                if until {
                    // `do ... until c` loops while `c` is false, which is a
                    // false-jump straight back to the top.
                    self.emit_at(Op::JumpIfFalse { target: top }, expr.position());
                } else {
                    let exit = self.emit_jump_if_false(expr.position());
                    self.emit(Op::Jump(top));
                    self.patch_here(exit);
                }

                self.emit(Op::Unit);
                let past = self.emit_jump();
                for site in breaks {
                    self.patch_here(site);
                }
                self.patch_here(past);
                true
            }

            Stmt::BreakLoop(value, flags, ..) => {
                let Some(active) = self.loops.last() else {
                    // Outside any loop this is a parse error in rhai, so it
                    // should be unreachable; bail rather than emit a jump to
                    // nowhere.
                    return false;
                };
                let continue_target = active.continue_target;
                let loop_iters = active.iters;
                let loop_handlers = active.handlers;
                let owns_iterator = active.owns_iterator;
                let (break_depth, continue_depth) = (active.break_depth, active.continue_depth);
                let is_break = flags.intersects(ASTFlags::BREAK);

                // A jump out of a loop skips whatever the straight-line path
                // would have cleaned up. The nesting is lexical, so how many
                // iterators are live is known here — a `break` inside a `try`
                // inside a `for` has one to drop, and `continue` has none
                // because it re-enters the loop that owns it.
                if is_break {
                    match value {
                        Some(expr) => self.expression(expr),
                        None => self.emit(Op::Unit),
                    }
                    // Out of the loop entirely, so its own iterator goes too —
                    // `loop_iters` counts from inside the loop and therefore
                    // already includes it.
                    self.pop_handlers(loop_handlers);
                    self.drop_iterators(loop_iters - usize::from(owns_iterator));
                    self.emit(Op::UnwindTo(break_depth));
                    let site = self.emit_jump();
                    self.loops.last_mut().expect("checked").breaks.push(site);
                } else {
                    // Back into the same loop, so its iterator and its loop
                    // variable both have to survive.
                    self.pop_handlers(loop_handlers);
                    self.drop_iterators(loop_iters);
                    self.emit(Op::UnwindTo(continue_depth));
                    self.emit(Op::Jump(continue_target));
                }

                // Unreachable, but every statement must leave a value for the
                // caller's `Pop`, and the verifier checks depth on every path.
                self.emit(Op::Unit);
                true
            }

            // `throw` shares this node, flagged, and unwinds as an error
            // rather than returning. The position is the keyword's, not the
            // expression's (`eval/stmt.rs:877`).
            Stmt::Return(value, flags, pos) if flags.intersects(ASTFlags::BREAK) => {
                match value {
                    Some(expr) => self.expression(expr),
                    None => self.emit(Op::Unit),
                }
                self.emit_at(Op::Throw, *pos);
                // Unreachable, but every statement leaves a value for the
                // caller's `Pop` and the verifier checks depth on every path.
                self.emit(Op::Unit);
                true
            }

            Stmt::Return(value, flags, ..) if !flags.intersects(ASTFlags::BREAK) => {
                match value {
                    Some(expr) => self.expression(expr),
                    None => self.emit(Op::Unit),
                }
                self.emit(Op::Return);
                self.emit(Op::Unit);
                true
            }

            // The one statement the fragment fallback below cannot hold.
            //
            // `import` declares into the imports stack rather than the scope,
            // and a fragment that rewinds truncates that stack on the way out
            // (`eval/stmt.rs:55`) — so the alias would be gone before the next
            // statement could name it, and a qualified call is its own
            // fragment. Refusing the lowering hands the body to the walker
            // whole, which is where the alias lives long enough to be used.
            #[cfg(not(feature = "no_module"))]
            Stmt::Import(..) => false,

            // Not lowered yet, and listed rather than matched with `_` on
            // purpose. A wildcard here silently turned `import` and `eval`
            // into fragments that answered differently from the walker; naming
            // every kind means a new one added to rhai's AST stops the build
            // until someone has decided which of the three it is — lowered,
            // fragment, or too scope-shaped to be either.
            //
            // The ones below are fragments because each either declares
            // nothing or rewinds what it declares, so the scope is the same
            // shape afterwards. That is the property to check before adding to
            // this list.
            other @ (Stmt::Noop(..)
            | Stmt::FnCall(..)
            | Stmt::Assignment(..)
            | Stmt::Return(..)) => {
                let residual = self.push_residual(wrap_statements(vec![other.clone()]));
                self.emit(Op::EvalAst {
                    residual,
                    rewind_scope: true,
                });
                true
            }

            #[cfg(not(feature = "no_module"))]
            other @ Stmt::Export(..) => {
                let residual = self.push_residual(wrap_statements(vec![other.clone()]));
                self.emit(Op::EvalAst {
                    residual,
                    rewind_scope: true,
                });
                true
            }
        }
    }

    /// Lower one expression, leaving its value on the stack.
    fn expression(&mut self, expr: &Expr) {
        match expr {
            Expr::BoolConstant(value, ..) => self.emit(Op::Bool(*value)),
            Expr::Unit(..) => self.emit(Op::Unit),

            Expr::IntegerConstant(value, ..) => self.constant(Dynamic::from(*value)),
            Expr::CharConstant(value, ..) => self.constant(Dynamic::from(*value)),
            Expr::StringConstant(value, ..) => self.constant(Dynamic::from(value.clone())),
            // Rhai has no float literal to parse under `no_float`, so there is
            // no variant to match.
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(value, ..) => self.constant(Dynamic::from(**value)),
            // Folded by the optimizer, so it can hold anything a constant call
            // returned — including a function pointer, which must not be
            // copied out of a pool. See `poolable`.
            // A function pointer the optimizer folded — `Fn("f")` with a
            // constant name, or a closure literal. It cannot go in the pool:
            // a closure's carries a `ScriptFuncDef`, which is an AST body and
            // exactly what an artifact must not contain. Rebuilt by name
            // instead, which reaches the chunk we compiled from that same
            // body.
            //
            // A constant function pointer: a closure literal, or what the
            // optimizer folds `Fn("f")` into. Either way it embeds a
            // `ScriptFuncDef` — an AST body, `Fn*` in rhai's own rendering —
            // so it cannot go in the pool. Rebuilt by name instead, reaching
            // the chunk compiled from that same body.
            //
            // There is no version of this that keeps the rendering: the thing
            // that differs *is* the tree, and carrying it is what an artifact
            // must not do. `a_closure_pointer_is_late_bound` pins the
            // difference for both spellings.
            //
            // Curried values are arbitrary `Dynamic`s with the same problem one
            // level down, and are left to the walker.
            Expr::DynamicConstant(value, ..)
                if value
                    .read_lock::<rhai::FnPtr>()
                    .map(|f| f.curry().is_empty())
                    .unwrap_or(false) =>
            {
                let name = value
                    .read_lock::<rhai::FnPtr>()
                    .expect("checked by the guard")
                    .fn_name()
                    .to_string();
                let name = self.push_name(name.into());
                self.emit_at(Op::MakeClosure(name), expr.position());
            }

            Expr::DynamicConstant(value, ..) if is_poolable(value) => {
                self.constant((**value).clone());
            }

            Expr::Variable(payload, ..) => {
                // A qualified name resolves against imported modules, not the
                // scope, so it is not a slot.
                let is_qualified = has_namespace!(payload);

                match self.slots.resolve(&payload.1) {
                    Some(slot) if !is_qualified => self.emit(Op::LoadLocal(slot)),
                    // Not a local this compiler declared, so no slot can name
                    // it: it is the caller's, a module's, or nothing. Looked
                    // up by name at run time, at the cost of a scope scan.
                    _ if self.is_variable_name(&payload.1, is_qualified) => {
                        let name = self.push_name(payload.1.clone());
                        self.emit_at(Op::LoadNamed(name), expr.position());
                    }
                    // A qualified name resolves against imported modules, and
                    // a bare function name is a function pointer. Neither is a
                    // variable read, and both stay rhai's job.
                    _ => self.residual_expr(expr),
                }
            }

            Expr::And(operands, ..) => self.short_circuit(operands, false),
            Expr::Or(operands, ..) => self.short_circuit(operands, true),

            Expr::FnCall(call, pos) if self.fn_ptr_call(call, *pos) => {}

            Expr::FnCall(call, pos) if self.is_lowerable_call(call) => {
                self.lower_call(call, *pos);
            }

            // `eval` evaluates a script in the *caller's* scope, so what it
            // declares outlives it and the next statement can name it. The
            // slot model resolved its indices against a scope that does not
            // have those entries, so a lowered read past an `eval` looks in
            // the wrong place — `eval("let x = 40"); x + 2` found no `x` where
            // the walker found 40. Refusing the lowering hands the body to the
            // walker, which is the only thing that knows the real shape.
            Expr::FnCall(call, ..) if call.name == crate::engine::KEYWORD_EVAL => {
                self.residual_expr(expr);
                self.defeated = true;
            }

            // A literal whose elements are all constant never reaches here —
            // rhai's optimizer folds it into a `DynamicConstant` first — so
            // this is the one that has to be built at run time.
            Expr::Array(elements, ..) if elements.len() <= u16::MAX as usize => {
                for (index, element) in elements.iter().enumerate() {
                    self.expression(element);
                    // Positioned at the element, because that is what rhai
                    // blames when this element is the one that tips the
                    // running total over the limit.
                    self.emit_at(
                        Op::CheckSize {
                            index: index as u16,
                            map: false,
                        },
                        element.position(),
                    );
                }
                self.emit_at(Op::MakeArray(elements.len() as u16), expr.position());
            }

            // The other half of the same shape. Rhai keeps a map literal as a
            // template holding every key — the constant values already in
            // place, the computed ones as placeholders — plus the list of
            // entries still to evaluate (`ast/expr.rs:283`). An all-constant
            // map is folded into a `DynamicConstant` and never arrives here;
            // one with a single computed value does, and used to fragment.
            Expr::Map(entries, ..) if entries.0.len() <= u16::MAX as usize => {
                let (computed, template) = &**entries;
                let template = Dynamic::from_map(template.clone());
                // A template whose constants the pool cannot hold is a program
                // that could not be written to an artifact anyway.
                if !is_poolable(&template) {
                    self.residual_expr(expr);
                    return;
                }

                self.constant(template);
                for (index, (key, value)) in computed.iter().enumerate() {
                    self.constant(key.name.clone().into());
                    self.expression(value);
                    self.emit_at(
                        Op::CheckSize {
                            index: index as u16,
                            map: true,
                        },
                        value.position(),
                    );
                }
                self.emit_at(Op::MakeMap(computed.len() as u16), expr.position());
            }

            // A block used for its value: `let y = if c { 1 } else { 2 }`,
            // `let y = switch ..`, `let y = { let z = 1; z }`. Rhai evaluates
            // it with `restore_orig_state` set (`eval/expr.rs:434`), so it
            // rewinds what it declared — which is what `block` emits.
            Expr::Stmt(block) => {
                if !self.block(block.statements()) {
                    self.defeated = true;
                }
            }

            // The optimizer folds an all-constant interpolation away before
            // this sees it, so what arrives has at least two segments.
            Expr::InterpolatedString(segments, ..) => {
                self.emit(Op::InterpolateStart);
                for segment in segments.iter() {
                    self.expression(segment);
                    // The append carries the segment's own position, because
                    // that is what rhai blames when the size limit goes over.
                    self.emit_at(Op::InterpolateAppend, segment.position());
                }
                self.emit(Op::InterpolateEnd);
            }

            // `f.call(x)` and `f.curry(x)` are the method spellings of the two
            // above. They arrive as chains, so they have to be taken before
            // the chain walker sees them.
            // An `rhs` that is a bare `MethodCall` is the whole chain: a
            // further step would make it a `Dot` or an `Index` instead.
            Expr::Dot(binary, ..)
                if matches!(&binary.rhs, Expr::MethodCall(m, ..)
                    if matches!(m.name.as_str(), "call" | "curry")
                        && m.args.len() <= u8::MAX as usize) =>
            {
                let Expr::MethodCall(method, ..) = &binary.rhs else {
                    unreachable!("checked by the guard");
                };
                // `obj.call(f)` binds `obj` as the closure's `this` by
                // reference (`func/call.rs:862`), so a write inside the closure
                // has to reach `obj`. The value goes on the stack as it always
                // did; the receiver says where to carry a write back to.
                //
                // Unflattened, for the reason `unflattened` gives: a receiver
                // that is a shared cell has to arrive *as* the cell, so a write
                // lands where every holder can see it and no write-back is
                // needed at all.
                let receiver = self.fn_ptr_receiver(&binary.lhs);
                if receiver.is_some() {
                    self.unflattened(&binary.lhs);
                } else {
                    self.expression(&binary.lhs);
                }
                for arg in method.args.iter() {
                    self.expression(arg);
                }
                let argc = method.args.len() as u8;
                // The call's own position, which is what rhai reports for
                // everything the pointer path can raise. The one case it is
                // not is `obj.call(x)` where `obj` is not a pointer and `x` is
                // taken as one: rhai blames `x` (`func/call.rs:838`). Both
                // cannot come from one position-table entry, and using the
                // argument's instead was measured to move the divergence onto
                // the common path rather than remove it.
                //
                // Method style only. `curry(f, ..)` written as a call is a
                // different path in rhai and takes the *argument's* position —
                // see `fn_ptr_call`. The two disagreeing is deliberate.
                let pos = binary.rhs.position();
                if method.name == "call" {
                    self.emit_at(
                        Op::CallFnPtr {
                            argc,
                            method: true,
                            receiver,
                        },
                        pos,
                    );
                } else {
                    self.emit_at(Op::Curry(argc), pos);
                }
            }

            Expr::Dot(..) | Expr::Index(..) => {
                // A chain emits its own operands, so a failed attempt has to
                // leave nothing behind.
                let mark = self.mark();
                if !self.chain(expr, Tail::Read, None) {
                    self.rewind(mark);
                    self.residual_expr(expr);
                }
            }

            // Custom syntax runs host code against an `EvalContext`, which can
            // declare into the caller's scope. What it declares is invisible
            // here, so the slot model would be resolved against a scope shape
            // that is not the one at runtime. Refusing the lowering keeps the
            // walker's answer, as it does for `eval` above.
            #[cfg(not(feature = "no_custom_syntax"))]
            Expr::Custom(..) => {
                self.residual_expr(expr);
                self.defeated = true;
            }

            // Listed rather than matched with `_`, for the reason
            // [`Lowering::statement`] gives: a wildcard is what let `eval`
            // become a fragment that answered differently from the walker.
            //
            // These are fragments because none of them can change the shape of
            // the scope the slot model resolved its indices against. The
            // guarded arms above fall through to here when their guard fails —
            // a pool-defeating constant, a literal too long for its operand, a
            // call rhai resolves syntactically.
            // The frame's receiver, flattened as every consumer but three
            // wants it — see [`Op::LoadThis`] and `unflattened` below. Its own
            // position, because that is what `ErrorUnboundThis` carries.
            Expr::ThisPtr(pos) => self.emit_at(Op::LoadThis, *pos),

            Expr::Coalesce(..)
            | Expr::MethodCall(..)
            | Expr::Property(..)
            | Expr::DynamicConstant(..)
            | Expr::FnCall(..)
            | Expr::Array(..)
            | Expr::Map(..) => self.residual_expr(expr),
        }
    }

    /// Where `obj.call(f)`'s receiver came from, when a write through the
    /// closure's `this` has somewhere to land.
    ///
    /// `None` for anything rhai would evaluate into a temporary — `[1, 2].call(f)`
    /// mutates a copy in the walker too, so there is nothing to carry back.
    fn fn_ptr_receiver(&mut self, receiver: &Expr) -> Option<Receiver> {
        match receiver {
            Expr::ThisPtr(..) => Some(Receiver::This),
            Expr::Variable(payload, ..) if !has_namespace!(payload) => {
                match self.slots.resolve(&payload.1) {
                    Some(slot) => Some(Receiver::Local(slot)),
                    None if self.is_variable_name(&payload.1, false) => {
                        Some(Receiver::Named(self.push_name(payload.1.clone())))
                    }
                    None => None,
                }
            }
            _ => None,
        }
    }

    /// Where rhai's method-call rewrite would take this call's first argument
    /// from, if it applies at all (`func/call.rs:1434`).
    fn receiver(&mut self, call: &FnCallExpr) -> Option<Receiver> {
        // An operator short-circuits before the rewrite is reached, and a call
        // that captures the enclosing scope is excluded from it outright
        // (`func/call.rs:1387` and `:1775`).
        if call.op_token.is_some() || call.capture_parent_scope {
            return None;
        }

        // `f(this, ..)` takes the same rewrite as a variable. Rhai also requires
        // the receiver not to be shared and nothing to be curried
        // (`func/call.rs:1417`), and neither is a question the compiler can
        // answer: sharing is a run-time property, deferred to the VM as it
        // already is for a read-only local, and a curried redirect can never
        // reach this instruction because `call`/`curry` go through
        // `Op::CallFnPtr` and `is_lowerable_call` refuses them here.
        if let Some(Expr::ThisPtr(..)) = call.args.first() {
            return Some(Receiver::This);
        }

        let Some(Expr::Variable(payload, ..)) = call.args.first() else {
            return None;
        };
        let qualified = has_namespace!(payload);

        match self.slots.resolve(&payload.1) {
            Some(slot) if !qualified => Some(Receiver::Local(slot)),
            _ if self.is_variable_name(&payload.1, qualified) => {
                Some(Receiver::Named(self.push_name(payload.1.clone())))
            }
            _ => None,
        }
    }

    /// Push the arguments left to right, then dispatch.
    fn lower_call(&mut self, call: &FnCallExpr, pos: Position) {
        let argc = u8::try_from(call.args.len()).expect("checked by is_lowerable_call");

        // `f(x, ..)` is `x.f(..)`, so the variable is read after the other
        // arguments and by reference. See [`Op::CallRef`].
        if let Some(receiver) = self.receiver(call) {
            // `this` goes on *first*, unlike either of the others. Rhai's two
            // arms disagree about when it is read: the by-reference one takes a
            // pointer after the arguments (`func/call.rs:1417`), but the
            // fallback a shared or unbound receiver lands in reads and flattens
            // it before them (`:1462`). Reading first is what makes an unbound
            // `f(this, nosuch)` report `ErrorUnboundThis`, and what stops an
            // argument that writes to `this` being seen by the value passed.
            if let Receiver::This = receiver {
                self.emit_at(Op::LoadThis, call.args[0].position());
            }
            for arg in call.args.iter().skip(1) {
                self.expression(arg);
            }
            // A name is resolved here, where its own position is the one an
            // `ErrorVariableNotFound` wants, and then moved under the arguments
            // it was read after.
            if let Receiver::Named(var) = receiver {
                self.emit_at(Op::LoadNamed(var), call.args[0].position());
                if argc > 1 {
                    self.emit(Op::Rotate(argc - 1));
                }
            }

            let name = self.push_name(call.name.clone());
            self.emit_at(
                Op::CallRef {
                    name,
                    argc,
                    receiver,
                },
                pos,
            );
            return;
        }

        for arg in call.args.iter() {
            self.expression(arg);
        }
        let name = self.push_name(call.name.clone());
        // Only for a binary operator, which is the only shape the built-in
        // lookup takes. Keeping a unary one would be dead weight and worse:
        // `UnaryMinus` and `Minus` share the syntax `"-"`, so it is a token
        // that cannot be written to an artifact at all.
        let op = (argc == 2)
            .then(|| call.op_token.clone())
            .flatten()
            .map(|token| self.push_token(token));
        self.emit_at(Op::Call { name, argc, op }, pos);
    }

    /// Whether a name read is a variable read at all.
    ///
    /// A qualified name resolves against imported modules rather than the
    /// scope, and a bare script-function name is a function pointer with the
    /// calling environment attached (`eval/expr.rs:71-99`). Neither is
    /// something to look up by name, and both stay fragments.
    fn is_variable_name(&self, name: &ImmutableString, qualified: bool) -> bool {
        !qualified && !self.script_fns.contains(name)
    }

    /// Pool what `x op= y` needs, if there is an operator at all.
    fn op_assignment(&mut self, op_info: &OpAssignment) -> Option<u32> {
        op_info
            .get_op_assignment_info()
            .map(|(_, _, op_assign, op_assign_str, op, op_str)| {
                let entry = AssignOp {
                    op_assign: op_assign.clone(),
                    op_assign_name: self.push_name(op_assign_str.into()),
                    op: op.clone(),
                    op_name: self.push_name(op_str.into()),
                };
                self.push_assign_op(entry)
            })
    }

    /// Read a variable without flattening it, leaving a shared cell shared.
    ///
    /// Rhai's own variable read works this way — `Target::take_or_clone` hands
    /// back the shared value untouched (`eval/target.rs:233`) — and the places
    /// that want the contents flatten for themselves. [`Op::LoadLocal`]
    /// flattens instead, which is right where the value is what matters and
    /// wrong in the two places the cell is:
    ///
    /// * a closure's captured variable, where the aliasing *is* the capture;
    /// * a `switch` subject, which rhai refuses to match on when it is not
    ///   hashable, and a shared value is not — so a shared subject falls to the
    ///   default arm however well it would otherwise have matched.
    fn unflattened(&mut self, expr: &Expr) {
        match expr {
            Expr::Variable(payload, ..) if !has_namespace!(payload) => {
                match self.slots.resolve(&payload.1) {
                    Some(slot) => self.emit(Op::LoadShared(slot)),
                    // The caller's. A closure can capture one of those too, and
                    // reading it flat would bind a copy.
                    None if self.is_variable_name(&payload.1, false) => {
                        let name = self.push_name(payload.1.clone());
                        self.emit_at(Op::LoadSharedNamed(name), expr.position());
                    }
                    None => self.expression(expr),
                }
            }
            // The receiver can be a shared cell too — a closure capturing the
            // variable a method was called on — and the three readers that come
            // through here have to see the cell rather than what it holds.
            Expr::ThisPtr(pos) => self.emit_at(Op::LoadThisShared, *pos),
            other => self.expression(other),
        }
    }

    /// Lower `Fn(name)`, `curry(f, ..)` or `call(f, ..)`, if this is one.
    ///
    /// Rhai resolves these three by name before dispatch, but only at the
    /// arities it recognises (`func/call.rs:1109-1245`); anything else is an
    /// ordinary call that will not find a function. Matching those arities
    /// exactly is what keeps the two agreeing on the failures as well as the
    /// successes.
    fn fn_ptr_call(&mut self, call: &FnCallExpr, pos: Position) -> bool {
        if call_has_namespace!(call) || call.capture_parent_scope {
            return false;
        }
        let argc = call.args.len();

        match (call.name.as_str(), argc) {
            // The argument has to arrive as the cell, not its contents, or the
            // answer is always false.
            //
            // Not lowered under `no_closure`: rhai registers no `is_shared`
            // there, so the call has to reach the walker and fail the way rhai
            // fails it. Lowering it would answer a question rhai refuses.
            #[cfg(not(feature = "no_closure"))]
            ("is_shared", 1) => {
                self.unflattened(&call.args[0]);
                self.emit_at(Op::IsShared, pos);
            }
            // Both of these are reported against the *argument* rather than
            // against the call: rhai reads it, and everything it can then
            // complain about — a name that is not a string, a string that is
            // not an identifier, a first argument that is not a pointer — is
            // filled in with the argument's position (`func/call.rs:1217`,
            // `:1220`, `:1232`).
            ("Fn", 1) => {
                self.expression(&call.args[0]);
                self.emit_at(Op::MakeFnPtr, call.args[0].position());
            }
            ("curry", _) if argc > 1 => {
                let mut args = call.args.iter();
                self.expression(args.next().expect("checked by the arity"));
                for arg in args {
                    // The captured variables. These must bind the *cell* — a
                    // flattening read would hand the closure a copy and it
                    // would stop being one.
                    self.unflattened(arg);
                }
                self.emit_at(Op::Curry((argc - 1) as u8), call.args[0].position());
            }
            ("call", _) if argc >= 1 && argc <= u8::MAX as usize + 1 => {
                for arg in call.args.iter() {
                    self.expression(arg);
                }
                self.emit_at(
                    Op::CallFnPtr {
                        argc: (argc - 1) as u8,
                        method: false,
                        // Call position binds no receiver at all.
                        receiver: None,
                    },
                    pos,
                );
            }
            _ => return false,
        }
        true
    }

    /// Whether a call can go through generic dispatch.
    ///
    /// Rhai resolves a handful of names syntactically in `eval_fn_call_expr`
    /// before dispatch ever happens (`func/call.rs:1109-1340`), so routing
    /// those through `call_fn_raw` would change what they mean. A call that
    /// captures the enclosing scope is closure construction, and a qualified
    /// name resolves against imported modules; neither is a plain call.
    fn is_lowerable_call(&self, call: &FnCallExpr) -> bool {
        // `is_shared` belongs here for a sharper reason than the rest: rhai
        // answers it syntactically in both call positions (`func/call.rs:1240`
        // and `:929`) and registers no function for it anywhere, so a lowered
        // call raises `ErrorFunctionNotFound` where the walker returns a bool.
        const SYNTACTIC: &[&str] = &["eval", "is_def_var", "is_def_fn"];

        // These are handled by `fn_ptr_call` above, but only at the arities
        // rhai treats syntactically — at any other arity it falls through to
        // ordinary dispatch, and so must this.
        if matches!(call.name.as_str(), "Fn" | "call" | "curry" | "is_shared") {
            return false;
        }

        !call.capture_parent_scope
            && !call_has_namespace!(call)
            && call.args.len() <= u8::MAX as usize
            && !SYNTACTIC.contains(&call.name.as_str())
    }

    /// Lower `&&` or `||`: evaluate operands left to right, stopping at the
    /// first that decides the result.
    ///
    /// Each operand is coerced to bool at its own position, which is why the
    /// jumps carry one — rhai reports a non-boolean operand against the
    /// operand, not the expression (`eval/expr.rs:367-399`).
    fn short_circuit(&mut self, operands: &[Expr], stop_on: bool) {
        let mut decided = Vec::new();

        for operand in operands {
            self.expression(operand);
            let pos = operand.position();
            let site = self.code.len();
            self.emit_at(
                if stop_on {
                    Op::JumpIfTrue { target: u32::MAX }
                } else {
                    Op::JumpIfFalse { target: u32::MAX }
                },
                pos,
            );
            decided.push(site);
        }

        self.emit(Op::Bool(!stop_on));
        let past = self.emit_jump();
        for site in decided {
            self.patch_here(site);
        }
        self.emit(Op::Bool(stop_on));
        self.patch_here(past);
    }

    /// Lower a block, leaving its value — the last statement's, or unit if
    /// empty — on the stack, and dropping anything it declared.
    fn block(&mut self, statements: &[Stmt]) -> bool {
        let depth = self.slots.depth();

        let Some((last, leading)) = statements.split_last() else {
            self.emit(Op::Unit);
            return true;
        };

        for stmt in leading {
            if !self.statement(stmt) {
                return false;
            }
            self.emit(Op::Pop);
        }
        if !self.statement(last) {
            return false;
        }

        self.unwind_to(depth);
        true
    }

    /// Lower a block for its effects only, leaving nothing on the stack.
    ///
    /// Loop bodies discard their value: rhai's loops yield unit or whatever a
    /// `break` supplied, never the body's last statement.
    fn block_discarding(&mut self, statements: &[Stmt]) -> bool {
        if !self.block(statements) {
            return false;
        }
        self.emit(Op::Pop);
        true
    }

    /// Emit the scope truncation for leaving a block, and unwind the
    /// compile-time slot model with it.
    ///
    /// The value the block produced is already on the operand stack, so it
    /// survives locals being dropped.
    fn unwind_to(&mut self, depth: usize) {
        if self.slots.depth() > depth {
            let depth = u16::try_from(depth).expect("slot count is bounded");
            self.emit(Op::UnwindTo(depth));
            self.slots.unwind_to(depth as usize);
        }
    }

    /// Where the instruction list currently ends, for [`Lowering::rewind`].
    fn mark(&self) -> usize {
        self.code.len()
    }

    /// Drop everything emitted since `mark`.
    ///
    /// Only safe for an attempt that emitted no jumps out of the rewound
    /// region, which is why it is used for chains and nothing else: a chain
    /// emits its operands and then one instruction, and gives up before
    /// emitting the instruction.
    fn rewind(&mut self, mark: usize) {
        self.code.truncate(mark);
        self.positions.truncate(mark);
    }

    fn here(&self) -> u32 {
        u32::try_from(self.code.len()).expect("chunk length is bounded")
    }

    /// Emit a jump with a placeholder target, returning its site for patching.
    fn emit_jump(&mut self) -> usize {
        let site = self.code.len();
        self.emit(Op::Jump(u32::MAX));
        site
    }

    fn emit_jump_if_false(&mut self, pos: Position) -> usize {
        let site = self.code.len();
        self.emit_at(Op::JumpIfFalse { target: u32::MAX }, pos);
        site
    }

    /// Point a previously emitted jump at the next instruction.
    fn patch_here(&mut self, site: usize) {
        let target = self.here();
        self.patch_to(site, target);
    }

    /// Point a previously emitted jump at an instruction already emitted.
    fn patch_to(&mut self, site: usize, target: u32) {
        match &mut self.code[site] {
            Op::Jump(slot)
            | Op::JumpIfFalse { target: slot, .. }
            | Op::JumpIfTrue { target: slot, .. }
            | Op::IterNext { exit: slot, .. }
            | Op::PushHandler { target: slot, .. } => *slot = target,
            other => unreachable!("patched a {other:?}, which is not a jump"),
        }
    }

    /// Emit an `IterDrop` for every iterator live above `floor`.
    fn drop_iterators(&mut self, floor: usize) {
        for _ in floor..self.iters {
            self.emit(Op::IterDrop);
        }
    }

    /// Disarm every `try` region entered above `floor`.
    ///
    /// A `break` or `continue` jumps over the `PopHandler` the straight-line
    /// path would have run. Left armed, the handler keeps a stale target and a
    /// stale set of depths, and the next error anywhere in the frame is caught
    /// into a `catch` block that has already been left.
    fn pop_handlers(&mut self, floor: usize) {
        for _ in floor..self.handlers {
            self.emit(Op::PopHandler);
        }
    }

    /// Open a loop whose `break` and `continue` unwind to the same place —
    /// `while`, `loop` and `do`, which declare nothing of their own.
    fn begin_loop(&mut self, continue_target: u32) {
        let depth = u16::try_from(self.slots.depth()).expect("slot count is bounded");
        self.loops.push(Loop {
            continue_target,
            break_depth: depth,
            continue_depth: depth,
            iters: self.iters,
            handlers: self.handlers,
            owns_iterator: false,
            breaks: Vec::new(),
        });
    }

    /// Open a `for`, which does declare: the loop variable and any counter
    /// live between the two depths, so leaving drops them and going round
    /// again does not.
    fn begin_for(&mut self, continue_target: u32, break_depth: u16) {
        self.loops.push(Loop {
            continue_target,
            break_depth,
            continue_depth: u16::try_from(self.slots.depth()).expect("slot count is bounded"),
            iters: self.iters,
            handlers: self.handlers,
            owns_iterator: true,
            breaks: Vec::new(),
        });
    }

    fn end_loop(&mut self) -> Vec<usize> {
        self.loops.pop().expect("loop stack is balanced").breaks
    }

    fn residual_expr(&mut self, expr: &Expr) {
        let residual = self.push_residual(expr.clone());
        self.emit(Op::EvalAst {
            residual,
            rewind_scope: true,
        });
    }

    fn constant(&mut self, value: Dynamic) {
        let index = self.push_const(value);
        self.emit(Op::Const(index));
    }

    fn push_const(&mut self, value: Dynamic) -> u32 {
        // Programs at this scale make a linear scan cheaper than a hash map,
        // and it keeps the pool in emission order for readable disassembly.
        let rendered = format!("{value:?}");
        if let Some(index) = self
            .consts
            .iter()
            .position(|existing| format!("{existing:?}") == rendered)
        {
            return index as u32;
        }
        self.consts.push(value);
        (self.consts.len() - 1) as u32
    }

    fn push_name(&mut self, name: ImmutableString) -> u32 {
        if let Some(index) = self.names.iter().position(|existing| *existing == name) {
            return index as u32;
        }
        self.names.push(name);
        (self.names.len() - 1) as u32
    }

    /// A script uses a handful of distinct operators however many times it
    /// mentions them, so the pool stays tiny and a linear scan is right.
    fn push_token(&mut self, token: Token) -> u32 {
        if let Some(index) = self.tokens.iter().position(|existing| *existing == token) {
            return index as u32;
        }
        self.tokens.push(token);
        (self.tokens.len() - 1) as u32
    }

    fn push_assign_op(&mut self, entry: AssignOp) -> u32 {
        if let Some(index) = self
            .assign_ops
            .iter()
            .position(|existing| *existing == entry)
        {
            return index as u32;
        }
        self.assign_ops.push(entry);
        (self.assign_ops.len() - 1) as u32
    }

    fn push_residual(&mut self, expr: Expr) -> u32 {
        self.residuals.push(expr);
        (self.residuals.len() - 1) as u32
    }

    fn emit(&mut self, op: Op) {
        // An upper bound, not the answer: no instruction pushes more than one
        // value, so one slot per instruction cannot be too small. The verifier
        // replaces it with the measured high water once lowering is done.
        self.max_stack = self.max_stack.saturating_add(1);
        self.code.push(op);
        self.positions.push(Position::NONE);
    }

    /// Emit an instruction that can fail against a place in the source.
    ///
    /// The position goes to the side table rather than into the instruction, so
    /// it can be stripped from an artifact without touching the code.
    fn emit_at(&mut self, op: Op, pos: Position) {
        self.emit(op);
        *self.positions.last_mut().expect("just emitted") = pos;
    }
}

/// One step, still as AST.
/// A step, and where rhai would blame it.
///
/// The position travels with the step rather than being taken from the chain:
/// rhai reports each kind against its own node, and one chain instruction has
/// only one position-table entry between all of them.
enum ChainStep<'a> {
    /// The index expression, and the `[` it sits behind — see [`Step::Index`].
    Index(&'a Expr, rhai::Position),
    Property(
        &'a (
            (ImmutableString, u64),
            (ImmutableString, u64),
            ImmutableString,
        ),
        rhai::Position,
    ),
    Method(&'a FnCallExpr, rhai::Position),
}

/// Unpick rhai's nested chain encoding into a root and a list of steps.
///
/// `a.b[i]` is `Dot { lhs: a, rhs: Index { lhs: b, rhs: i } }`: each nested
/// node's `lhs` is the current step's operand and its `rhs` is the
/// continuation, so the list is built by walking `rhs` and taking `lhs` at each
/// level. The innermost `rhs` is the last step rather than a continuation,
/// which is what ends the walk.
///
/// `ASTFlags::BREAK` is what ends it, and it carries real information:
/// `a[b[0]]` and `a[b][0]` have the same shape, and the flag is the only thing
/// that says the first one's `b[0]` is an index expression rather than two
/// steps (`eval/chaining.rs:698`).
///
/// Returns `None` for `?.` and `?[]`, which short-circuit on unit rather than
/// stepping, and for a dot onto anything but a property or a method.
fn flatten_chain(expr: &Expr) -> Option<(&Expr, Vec<ChainStep<'_>>)> {
    /// A chain node's parts: operand side, continuation side, and whether the
    /// step it introduces is a property rather than an index.
    fn parts(expr: &Expr) -> Option<(&Expr, &Expr, ASTFlags, bool)> {
        match expr {
            Expr::Dot(binary, flags, ..) => Some((&binary.lhs, &binary.rhs, *flags, true)),
            Expr::Index(binary, flags, ..) => Some((&binary.lhs, &binary.rhs, *flags, false)),
            _ => None,
        }
    }

    let (root, mut rest, mut flags, mut dotted) = parts(expr)?;
    let mut steps = Vec::new();
    // Rhai's `op_pos`, which is the position of the chain node the step is
    // being taken *inside* rather than of the step's operand, and which walks
    // down with the recursion (`eval/chaining.rs:695`).
    let mut bracket = expr.position();

    loop {
        if flags.intersects(ASTFlags::NEGATED) {
            return None;
        }

        // `rest` is the continuation only when it is a chain node *and* this
        // node is not marked as the last one. Otherwise it is this step's own
        // operand — the index expression, or the property being read.
        let next = (!flags.intersects(ASTFlags::BREAK))
            .then(|| parts(rest))
            .flatten();

        let (operand, following) = match next {
            Some((operand, _, _, _)) => (operand, Some(rest)),
            None => (rest, None),
        };

        steps.push(match (dotted, operand) {
            (true, Expr::Property(prop, pos)) => ChainStep::Property(prop, *pos),
            (true, Expr::MethodCall(call, pos)) => ChainStep::Method(call, *pos),
            // `a.(expr)` is not syntax, so a dot onto anything else is a shape
            // the parser only makes for something handled elsewhere.
            (true, _) => return None,
            (false, index) => ChainStep::Index(index, bracket),
        });

        match following {
            Some(node) => {
                let (_, next_rest, next_flags, next_dotted) =
                    parts(node).expect("checked by `next`");
                rest = next_rest;
                flags = next_flags;
                dotted = next_dotted;
                bracket = node.position();
            }
            None => break,
        }
    }

    Some((root, steps))
}

/// Wrap statements as a block expression.
///
/// `Expr::Stmt` is the one shape `eval_expression_tree_raw` routes to
/// `eval_stmt_block` rather than `eval_expr`, which is what lets statements go
/// back through the walker at all.
fn wrap_statements(statements: Vec<Stmt>) -> Expr {
    let span = statements.first().zip(statements.last()).map_or_else(
        || Span::new(Position::NONE, Position::NONE),
        // `crate::types`, not `crate::types::position`: `no_position` swaps the
        // module out for a zero-sized one and re-exports `Span` from whichever
        // is in play.
        |(first, last)| crate::types::Span::new(first.position(), last.position()),
    );

    Expr::Stmt(Box::new(StmtBlock::new_with_span(statements, span)))
}
