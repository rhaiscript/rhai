#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::ast::{ASTFlags, ASTNode};
#[cfg(not(feature = "no_module"))]
use crate::module_resolvers::StaticModuleResolver;
use crate::{ast::Expr, ast::Stmt, tokenizer::Token, Dynamic, ImmutableString, Module, Shared};

use crate::grain::bytecode::{
    AssignOp, Chain, Chunk, Code, Pools, Positions, Strings, Switch, TableError,
};

/// rhai's own `SharedModule`, which it does not re-export.
pub(crate) type SharedModule = Shared<Module>;

/// A program a native function can be handed a way back into.
///
/// [`Vm::run_with_callbacks`](crate::grain::Vm::run_with_callbacks) registers one
/// wrapper per compiled function, and rhai requires a registered function to be
/// `'static` — so the program cannot still be borrowing an artifact, and the
/// wrappers have to share ownership of it rather than borrow it.
pub type SharedProgram = Shared<Program<'static>>;

/// One compiled script function.
///
/// Called by [`Op::Call`](crate::bytecode::Op::Call) directly, without going
/// through rhai's dispatch: the name is already an index into the same pool the
/// call site used, so matching one is two integer comparisons rather than a
/// hash and a module walk.
#[derive(Debug, Clone)]
pub struct Function {
    /// Index into the name pool.
    pub name: u32,
    /// Parameter names, in order, as name-pool indices. They become the
    /// callee's first locals, which is what makes them slot 0 upwards.
    pub params: Vec<u32>,
    pub chunk: Chunk,
}

/// A compiled script, ready to run against an `Engine`.
///
/// Owns everything execution needs that is not the `Engine` itself, so the
/// original `AST` can be dropped after compiling. On a small target that is the
/// whole point: the tree is the part whose cost scales with the program.
///
/// The lifetime is the artifact's. A program read from bytes borrows its
/// instructions from them and allocates only its pools, which are bounded by
/// the distinct constants and names a script actually mentions rather than by
/// how long it is. [`Program::into_owned`] cuts the tie when that is wanted.
///
/// `residuals` is the exception, and the reason a `Program` is not always
/// serializable. Fragments rhai's walker still has to evaluate are held as real
/// `Expr` trees, which is precisely the allocation we are trying to remove. The
/// artifact format refuses to write a `Program` that has any, so nothing
/// reaching a device can depend on them.
pub struct Program<'a> {
    /// Every chunk's instructions, concatenated: main first, then each
    /// function. One buffer means one position table and one instruction
    /// address, so a device that fails reports a single number.
    code: Code<'a>,

    main: Chunk,

    /// Script functions compiled to chunks. Empty when none were compiled,
    /// which is when rhai's own versions are carried in `lib` instead.
    functions: Vec<Function>,

    /// The deepest chunk's operand-stack need, cached.
    max_stack: u16,

    /// Whether the program can hand a function pointer to something that
    /// might call it back. See [`Program::makes_fn_pointers`].
    makes_fn_pointers: bool,

    /// Where each instruction came from, or [`Positions::Stripped`].
    ///
    /// Separable on purpose: a device is shipped the code and the host keeps
    /// the table, so an error arrives as an instruction address and is resolved
    /// where the source is. See [`crate::bytecode::Positions`].
    positions: Positions,

    residuals: Vec<Expr>,

    /// Values `Op::Const` indexes. Deduplicated, so a constant repeated across
    /// the script is stored once.
    consts: Vec<Dynamic>,

    /// Every name the program mentions, as one borrowed blob.
    ///
    /// Nothing needs a `String` of its own. Call names, operators, getters and
    /// property keys go to rhai as `&str`; a `Scope` entry name goes in as an
    /// `Identifier`, which is a `SmartString` and keeps a short name inline
    /// rather than on the heap. So the whole table is two allocations — the
    /// blob and the spans — and neither grows with how many names there are.
    names: Strings<'a>,

    /// Operator tokens the built-in lookup keys on. A `Token` does not fit an
    /// operand, and one script uses a handful of distinct operators however
    /// many times it mentions them.
    tokens: Vec<Token>,

    /// What each `x op= y` site needs, for the same reason.
    assign_ops: Vec<AssignOp>,

    /// The steps of each `a.b[i].c`. Out of the instruction stream because a
    /// chain is one instruction however many steps it has.
    chains: Vec<Chain>,

    /// One dispatch table per `switch`, for the same reason.
    ///
    /// Case hashes are rhai's, and rhai's hasher is seeded per process unless
    /// the host says otherwise — so an artifact carrying any of these carries
    /// a [`probe`](crate::bytecode::probe) too, and refuses to load against a
    /// hasher that would disagree with it.
    switches: Vec<Switch>,

    /// Script functions the compiler did not lower, as rhai's own library, so
    /// a fragment can still call one the ordinary way.
    ///
    /// `None` when the script declared none, which is every program that came
    /// from an artifact. An empty `Module` is 264 bytes and a reference count,
    /// which is a fifth of what loading a small program retains — worth not
    /// allocating for something nothing will look in.
    lib: Option<SharedModule>,

    /// Reinstated on the runtime state at each run, mirroring
    /// `Engine::eval_ast_with_scope_raw`, so `import` resolves as it would have.
    #[cfg(not(feature = "no_module"))]
    resolver: Option<Shared<StaticModuleResolver>>,

    /// Names the script in error messages and `NativeCallContext::call_source`.
    source: Option<ImmutableString>,
}

/// A summary rather than a dump: the library alone would render every script
/// function's whole AST, which is never what someone printing a `Program`
/// wants to read.
impl core::fmt::Debug for Program<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Program")
            .field("source", &self.source)
            .field("bytes", &self.code.len())
            .field("max_stack", &self.main.max_stack())
            .field("consts", &self.consts.len())
            .field("names", &self.names.len())
            .field("residuals", &self.residuals.len())
            .field("compiled_fns", &self.functions.len())
            .field(
                "walked_fns",
                &self.lib.as_ref().map_or(0, |lib| lib.count().1),
            )
            .field("positions", &!self.positions.is_stripped())
            .finish()
    }
}

/// What a script author would call the construct at this node, for the
/// constructs the compiler does not lower yet.
///
/// Only the ones worth naming: an author can act on "switch at line 42", not
/// on "Expr::Dot". Anything else falls through to the generic message.
fn unsupported_kind(node: &ASTNode) -> Option<&'static str> {
    Some(match node {
        ASTNode::Stmt(stmt) => match stmt {
            Stmt::Switch(..) => "switch",
            Stmt::For(..) => "for",
            Stmt::TryCatch(..) => "try/catch",
            #[cfg(not(feature = "no_module"))]
            Stmt::Import(..) => "import",
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(..) => "export",
            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(..) => "a closure capture",
            Stmt::Return(_, flags, ..) if flags.intersects(ASTFlags::BREAK) => "throw",
            _ => return None,
        },
        ASTNode::Expr(expr) => match expr {
            Expr::InterpolatedString(..) => "string interpolation",
            Expr::Custom(..) => "custom syntax",
            Expr::Map(..) => "a non-constant map literal",
            _ => return None,
        },
    })
}

fn node_position(node: &ASTNode) -> rhai::Position {
    match node {
        ASTNode::Stmt(stmt) => stmt.position(),
        ASTNode::Expr(expr) => expr.start_position(),
    }
}

/// Whether any instruction in `code` produces a function pointer.
///
/// Read off the bytes rather than tracked while lowering, because a program
/// read from an artifact has no lowering to have tracked it — and the answer
/// has to be the same either way or one of the two paths silently loses its
/// callbacks.
///
/// Deliberately over-broad: it says yes to a pointer that is only ever called
/// directly. Narrowing it would mean deciding where a pointer *goes*, which is
/// a dataflow question over values that outlive the instruction that made
/// them. Being wrong the other way loses a call at run time.
fn makes_fn_pointers(code: &[u8]) -> bool {
    use crate::grain::bytecode::code::tag;

    crate::grain::bytecode::disassemble(code)
        .any(|(at, ..)| matches!(code[at], tag::MAKE_CLOSURE | tag::MAKE_FN_PTR | tag::CURRY))
}

/// Everything a program holds besides its code, gathered so the constructor
/// does not take ten positional arguments.
pub(crate) struct Parts<'a> {
    pub positions: Positions,
    pub residuals: Vec<Expr>,
    pub consts: Vec<Dynamic>,
    pub names: Strings<'a>,
    pub tokens: Vec<Token>,
    pub assign_ops: Vec<AssignOp>,
    pub chains: Vec<Chain>,
    pub switches: Vec<Switch>,
    pub lib: Option<SharedModule>,
    #[cfg(not(feature = "no_module"))]
    pub resolver: Option<Shared<StaticModuleResolver>>,
    pub source: Option<ImmutableString>,
}

impl<'a> Program<'a> {
    pub(crate) fn new(
        code: Code<'a>,
        main: Chunk,
        functions: Vec<Function>,
        parts: Parts<'a>,
    ) -> Self {
        let makes_fn_pointers = makes_fn_pointers(&code);
        let mut program = Self {
            code,
            main,
            functions,
            max_stack: 0,
            makes_fn_pointers,
            positions: parts.positions,
            residuals: parts.residuals,
            consts: parts.consts,
            names: parts.names,
            tokens: parts.tokens,
            assign_ops: parts.assign_ops,
            chains: parts.chains,
            switches: parts.switches,
            lib: parts.lib,
            #[cfg(not(feature = "no_module"))]
            resolver: parts.resolver,
            source: parts.source,
        };
        program.recompute_max_stack();
        program
    }

    /// Copy the borrowed instructions, so this program outlives the artifact it
    /// was read from.
    ///
    /// The opposite of the point, and only worth it when the buffer has to go.
    #[must_use]
    pub fn into_owned(self) -> Program<'static> {
        Program {
            code: Code::Owned(self.code.into_owned()),
            main: self.main,
            functions: self.functions,
            max_stack: self.max_stack,
            makes_fn_pointers: self.makes_fn_pointers,
            positions: self.positions,
            residuals: self.residuals,
            consts: self.consts,
            names: self.names.into_owned(),
            tokens: self.tokens,
            assign_ops: self.assign_ops,
            chains: self.chains,
            switches: self.switches,
            lib: self.lib,
            #[cfg(not(feature = "no_module"))]
            resolver: self.resolver,
            source: self.source,
        }
    }

    /// Give up the artifact and share the program, so a native can be handed a
    /// way back into it.
    ///
    /// What [`Vm::run_with_callbacks`](crate::grain::Vm::run_with_callbacks) takes.
    /// Worth the copy only when [`makes_fn_pointers`](Self::makes_fn_pointers)
    /// says a pointer can escape.
    #[must_use]
    pub fn into_shared(self) -> SharedProgram {
        Shared::new(self.into_owned())
    }

    /// Check the chunk is internally consistent, returning the stack high water
    /// it measured.
    ///
    /// Cheap enough to run on every compile, and the gate an artifact loaded
    /// from a wire has to pass before the VM will touch it.
    pub fn verify(&self) -> Result<Vec<u16>, crate::grain::bytecode::VerifyError> {
        crate::grain::bytecode::verify(&self.code, &self.chunks(), self.pools())
    }

    /// Every chunk, main first, in the order they sit in the code.
    fn chunks(&self) -> Vec<Chunk> {
        core::iter::once(self.main)
            .chain(self.functions.iter().map(|f| f.chunk))
            .collect()
    }

    pub(crate) fn pools(&self) -> Pools<'_> {
        Pools {
            consts: self.consts.len(),
            names: self.names.len(),
            tokens: self.tokens.len(),
            assign_ops: self.assign_ops.len(),
            residuals: self.residuals.len(),
            chains: &self.chains,
            switches: &self.switches,
        }
    }

    /// Replace the compiler's upper-bound stack estimate with the verified high
    /// water, so the VM reserves what the chunk uses rather than one slot per
    /// instruction.
    ///
    /// A chunk that does not verify keeps its estimate: the VM is still safe
    /// with a value that is too large, and [`Program::verify`] is where the
    /// real failure should surface.
    pub(crate) fn tighten_stack(&mut self) {
        let Ok(high_water) = self.verify() else {
            return;
        };
        let mut measured = high_water.into_iter();
        if let Some(main) = measured.next() {
            self.main.set_max_stack(main);
        }
        for (function, high_water) in self.functions.iter_mut().zip(measured) {
            function.chunk.set_max_stack(high_water);
        }
        self.recompute_max_stack();
    }

    /// Every chunk's instructions, concatenated.
    #[must_use]
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// The compiled script functions.
    #[must_use]
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    /// The compiled function a call site resolves to, if there is one.
    ///
    /// Name and arity only, matching how rhai keys script functions. The name
    /// is an index into the pool the call site also indexes, so equal names
    /// have equal indices and this is two integer comparisons.
    pub(crate) fn function(&self, name: u32, argc: usize) -> Option<&Function> {
        self.functions
            .iter()
            .find(|f| f.name == name && f.params.len() == argc)
    }

    /// The compiled function a *pointer* resolves to.
    ///
    /// By name rather than by pool index, because a `FnPtr` carries a string —
    /// it may have been built from one at run time. A linear scan, which at
    /// these sizes beats a map and keeps the common indexed lookup untouched.
    pub(crate) fn function_named(&self, name: &str, argc: usize) -> Option<&Function> {
        self.functions
            .iter()
            .find(|f| f.params.len() == argc && self.name(f.name) == Some(name))
    }

    /// Whether this program can hand a function pointer to something that
    /// might call it back.
    ///
    /// A compiled function lives in this program's own table and nowhere rhai
    /// can see, so a native that calls a pointer — `map`, `filter` — cannot
    /// reach one. Making it reachable means registering a wrapper, and rhai
    /// requires a registered function to be `'static`, so the wrapper has to
    /// own the program: [`Program::into_owned`] first, at the cost of the
    /// borrowed-from-the-artifact loading that is the point of the format.
    ///
    /// This is how a host decides whether to pay that, without having to read
    /// the script. False is the common answer and costs nothing.
    #[must_use]
    pub fn makes_fn_pointers(&self) -> bool {
        self.makes_fn_pointers
    }

    /// How much operand stack the deepest chunk needs.
    ///
    /// One reservation serves every frame, because a call pushes its operands
    /// above the caller's rather than starting a stack of its own. Cached
    /// rather than recomputed, since entering a frame reads it and entering a
    /// frame is what a call does.
    #[must_use]
    pub fn max_stack(&self) -> u16 {
        self.max_stack
    }

    fn recompute_max_stack(&mut self) {
        self.max_stack = self
            .functions
            .iter()
            .map(|f| f.chunk.max_stack())
            .chain(core::iter::once(self.main.max_stack()))
            .max()
            .unwrap_or(0);
    }

    pub(crate) fn constant(&self, index: u32) -> Option<&Dynamic> {
        self.consts.get(index as usize)
    }

    /// A name, borrowed from the artifact. Never allocates.
    pub(crate) fn name(&self, index: u32) -> Option<&str> {
        self.names.get(index)
    }

    pub(crate) fn token(&self, index: u32) -> Option<&Token> {
        self.tokens.get(index as usize)
    }

    pub(crate) fn assign_op(&self, index: u32) -> Option<&AssignOp> {
        self.assign_ops.get(index as usize)
    }

    pub(crate) fn chain(&self, index: u32) -> Option<&Chain> {
        self.chains.get(index as usize)
    }

    pub(crate) fn chains(&self) -> &[Chain] {
        &self.chains
    }

    pub(crate) fn switch(&self, index: u32) -> Option<&Switch> {
        self.switches.get(index as usize)
    }

    /// The dispatch tables [`Op::Switch`](crate::grain::bytecode::Op::Switch) indexes.
    ///
    /// Public because a disassembly that leaves them out is misleading: a
    /// switch's arms are reached only from its table, so without it they read
    /// as unreachable code.
    #[must_use]
    pub fn switches(&self) -> &[Switch] {
        &self.switches
    }

    /// Where instruction `pc` came from, or `Position::NONE` if the table was
    /// stripped or has nothing for it.
    #[must_use]
    pub fn position(&self, pc: usize) -> rhai::Position {
        self.positions.get(pc)
    }

    /// The whole position table, keyed on instruction address.
    #[must_use]
    pub fn positions(&self) -> &Positions {
        &self.positions
    }

    /// Drop the position table, returning it in its compact wire form.
    ///
    /// This is the separation the debug layer exists for: ship the program to
    /// the device and keep what comes back here. Errors then arrive with no
    /// position, and [`pos::resolve`](crate::grain::pos::resolve) turns the failing instruction
    /// address back into one — see [`Program::attach_positions`] for the
    /// inverse.
    pub fn strip_positions(&mut self) -> Vec<u8> {
        let table = self.positions.to_table();
        self.positions = Positions::Stripped;
        table
    }

    /// Put a stripped table back, so this program reports positions again.
    ///
    /// # Errors
    ///
    /// Refuses a malformed table, and refuses one whose addresses do not fit
    /// this chunk — attaching another program's table would misreport every
    /// error rather than reporting none, which is strictly worse.
    pub fn attach_positions(&mut self, table: &[u8]) -> Result<(), TableError> {
        self.positions = Positions::from_table(table, self.code.len())?;
        Ok(())
    }

    pub(crate) fn consts(&self) -> &[Dynamic] {
        &self.consts
    }

    pub(crate) fn names(&self) -> &Strings<'a> {
        &self.names
    }

    pub(crate) fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub(crate) fn assign_ops(&self) -> &[AssignOp] {
        &self.assign_ops
    }

    /// The top-level chunk, where execution starts.
    #[must_use]
    pub fn main(&self) -> &Chunk {
        &self.main
    }

    /// How many fragments rhai's walker still evaluates.
    ///
    /// Non-zero is the reason a program cannot yet be serialized. As a measure
    /// of progress it is misleading on its own: lowering a statement often
    /// splits one fragment into several smaller ones, so the count rises while
    /// the work left shrinks. Use [`Program::residual_nodes`] for that.
    #[must_use]
    pub fn residual_count(&self) -> usize {
        self.residuals.len()
    }

    /// How many AST nodes are still inside fragments.
    ///
    /// This is the progress metric that only falls: it counts the tree that has
    /// to survive into the artifact.
    #[must_use]
    pub fn residual_nodes(&self) -> usize {
        let mut nodes = 0;
        let path = &mut Vec::new();
        for residual in &self.residuals {
            residual.walk(path, &mut |_| {
                nodes += 1;
                true
            });
        }
        nodes
    }

    pub(crate) fn residual(&self, index: u32) -> Option<&Expr> {
        self.residuals.get(index as usize)
    }

    /// The construct that stopped this program being written, and where.
    ///
    /// A count of fragments is not something anyone can act on. This names the
    /// first thing the compiler could not lower, so a validator can reject an
    /// upload with the line to go and look at — which is what makes falling
    /// back to shipping source a decision rather than a mystery.
    #[must_use]
    pub fn first_unsupported(&self) -> Option<(&'static str, rhai::Position)> {
        let path = &mut Vec::new();
        let mut found: Option<(&'static str, rhai::Position)> = None;

        for residual in &self.residuals {
            residual.walk(path, &mut |path| {
                if found.is_some() {
                    return false;
                }
                if let Some(name) = path.last().and_then(unsupported_kind) {
                    found = Some((name, node_position(path.last().expect("just matched"))));
                    return false;
                }
                true
            });
            if found.is_some() {
                break;
            }
        }

        // A fragment made of nothing this recognises is still a fragment, so
        // say so rather than reporting nothing wrong.
        found.or_else(|| {
            self.residuals
                .first()
                .map(|expr| ("an unlowered expression", expr.start_position()))
        })
    }

    pub(crate) fn lib(&self) -> Option<&SharedModule> {
        self.lib.as_ref()
    }

    #[cfg(not(feature = "no_module"))]
    pub(crate) fn resolver(&self) -> Option<&Shared<StaticModuleResolver>> {
        self.resolver.as_ref()
    }

    pub(crate) fn source(&self) -> Option<&ImmutableString> {
        self.source.as_ref()
    }
}
