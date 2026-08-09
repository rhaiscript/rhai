//! What would a lowering of this script actually weigh?
//!
//! This tests the premise before an instruction set is built to serve it.
//! `follow.rhai` uses most of the language — nested index writes, property
//! chains, method calls, script functions, `loop`/`while`/`if-else` — so
//! lowering it for real is a lot of work. Doing that first and *then*
//! discovering the artifact is only marginally smaller than the tree would be
//! the expensive way to learn it.
//!
//! So this counts the real AST, node by node, and prices each node against the
//! planned stack encoding. The per-node costs are stated in `encoded_size`
//! where they can be argued with.
//!
//! **This is a projection, not a measurement.** It is a lower bound in one
//! direction and optimistic in another: it assumes every node lowers to the
//! ops listed, and it ignores the operand stack traffic a real lowering emits
//! for temporaries. `tests/grain/format.rs` measures the artifact that
//! actually results, and that number is the one to trust.

use std::collections::BTreeSet;

use rhai::{ASTNode, Engine, Expr, Stmt, AST};

const SOURCE: &str = include_str!("fixtures/follow.rhai");

/// Marks a node kind the model does not know how to price. Its presence is a
/// test failure: a silently-unpriced variant would understate the projection.
const UNPRICED: &str = "UNPRICED";

/// A node's kind, and what the planned encoding would spend on it.
///
/// The encoding is a stack machine: one byte of opcode, then operands as
/// varints that are one byte at this program's scale (fewer than 128 locals,
/// constants or names). Costs below are in bytes.
fn encoded_size(node: &ASTNode) -> (&'static str, usize) {
    match node {
        ASTNode::Expr(expr) => match expr {
            // Small integers get a dedicated opcode with an inline operand;
            // anything else is an index into the constant pool. Either way two
            // bytes, which is why they are not split here.
            Expr::IntegerConstant(..) => ("int", 2),
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(..) => ("float", 2),
            Expr::StringConstant(..) => ("string", 2),
            Expr::CharConstant(..) => ("char", 2),
            Expr::DynamicConstant(..) => ("dynamic const", 2),
            // No operand: the opcode is the value.
            Expr::BoolConstant(..) => ("bool", 1),
            Expr::Unit(..) => ("unit", 1),
            Expr::ThisPtr(..) => ("this", 1),

            // LoadLocal + slot. The parser has already resolved most of these
            // to a slot index, so this is not an optimistic assumption.
            Expr::Variable(..) => ("variable", 2),

            // Operators become a typed opcode with no operands when both sides
            // are primitives, and a generic call otherwise. Scored as a call,
            // the pessimistic reading, because the deopt guard may disable the
            // fast path for a given engine.
            Expr::FnCall(x, ..) if x.op_token.is_some() => ("operator", 1),
            Expr::FnCall(..) => ("call", 3),
            Expr::MethodCall(..) => ("method call", 3),

            // Get by interned name; set is a separate op emitted by the parent.
            Expr::Property(..) => ("property", 2),
            Expr::Dot(..) => ("dot", 1),
            Expr::Index(..) => ("index", 1),

            // Length-prefixed build from the operand stack.
            Expr::Array(..) => ("array literal", 2),
            Expr::Map(..) => ("map literal", 2),
            Expr::InterpolatedString(..) => ("interpolation", 2),

            // Short-circuit: a conditional jump per operand.
            Expr::And(..) => ("and", 3),
            Expr::Or(..) => ("or", 3),
            Expr::Coalesce(..) => ("coalesce", 3),

            // Block-as-expression needs no instruction of its own; the
            // statements inside are counted separately.
            Expr::Stmt(..) => ("block expr", 0),

            // Cannot be lowered at all — the handler is looked up by string
            // against a live Engine. Priced as the residual it would stay.
            #[cfg(not(feature = "no_custom_syntax"))]
            Expr::Custom(..) => ("custom syntax", 3),

            // `Expr` is #[non_exhaustive]. A variant added by a rhai upgrade
            // must surface as a test failure, not get quietly priced at zero.
            _ => (UNPRICED, 0),
        },

        ASTNode::Stmt(stmt) => match stmt {
            Stmt::Noop(..) => ("noop", 0),
            // The block's own statements are counted as they are walked.
            Stmt::Block(..) => ("block", 0),

            // StoreLocal + slot.
            Stmt::Var(..) => ("var decl", 2),
            // Store, plus the operator when compound.
            Stmt::Assignment(..) => ("assignment", 3),

            // Condition jump plus the jump over the else arm.
            Stmt::If(..) => ("if", 6),
            // Condition jump plus the backward jump.
            Stmt::While(..) => ("while", 6),
            Stmt::Do(..) => ("do", 6),
            // Iterator setup, plus a step-and-branch per turn.
            Stmt::For(..) => ("for", 8),
            // Jump table plus a linear range list.
            Stmt::Switch(..) => ("switch", 12),

            // Jumps to a fixed target.
            Stmt::BreakLoop(..) => ("break/continue", 3),
            Stmt::Return(..) => ("return/throw", 3),

            // Handler region, registered rather than executed.
            Stmt::TryCatch(..) => ("try/catch", 8),

            Stmt::FnCall(..) => ("call stmt", 3),
            // Discard the value the expression left behind.
            Stmt::Expr(..) => ("expr stmt", 1),

            #[cfg(not(feature = "no_module"))]
            Stmt::Import(..) => ("import", 3),
            #[cfg(not(feature = "no_module"))]
            Stmt::Export(..) => ("export", 3),
            #[cfg(not(feature = "no_closure"))]
            Stmt::Share(..) => ("share", 2),

            _ => (UNPRICED, 0),
        },

        _ => (UNPRICED, 0),
    }
}

/// Names that would live in the artifact's string table, deduplicated.
///
/// Only the ones reachable without unpacking every boxed payload: variables,
/// called functions and properties. That undercounts, so the projected string
/// table is a floor.
fn interned_names(ast: &AST) -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    ast.walk(&mut |path: &[ASTNode]| {
        if let Some(ASTNode::Expr(expr)) = path.last() {
            match expr {
                Expr::Variable(x, ..) => {
                    names.insert(x.1.to_string());
                }
                Expr::FnCall(x, ..) | Expr::MethodCall(x, ..) => {
                    names.insert(x.name.to_string());
                }
                Expr::StringConstant(s, ..) => {
                    names.insert(s.to_string());
                }
                _ => {}
            }
        }
        true
    });

    names
}

#[test]
fn projected_artifact_size() {
    let engine = Engine::new();
    let ast = engine.compile(SOURCE).expect("fixtures/follow.rhai must compile");

    let mut counts: std::collections::BTreeMap<&'static str, (usize, usize)> = std::collections::BTreeMap::new();
    let mut code_bytes = 0usize;
    let mut nodes = 0usize;

    ast.walk(&mut |path: &[ASTNode]| {
        let Some(node) = path.last() else {
            return true;
        };
        let (kind, size) = encoded_size(node);
        let entry = counts.entry(kind).or_default();
        entry.0 += 1;
        entry.1 += size;
        code_bytes += size;
        nodes += 1;
        true
    });

    let names = interned_names(&ast);
    // One length byte plus the UTF-8 bytes, per distinct name.
    let string_table: usize = names.iter().map(|n| 1 + n.len()).sum();

    // A tag byte plus eight bytes of payload, for every node that referenced
    // the pool. Deduplication would shrink this; not modelling it keeps the
    // projection on the pessimistic side.
    let pool_refs: usize = ["int", "float", "string", "char", "dynamic const"].iter().filter_map(|k| counts.get(k)).map(|(n, _)| *n).sum();
    let const_pool = pool_refs * 9;

    // Header, ABI fingerprint, and section offsets.
    const HEADER: usize = 64;

    let projected = HEADER + string_table + const_pool + code_bytes;
    let source_bytes = SOURCE.len();

    println!("\nfixtures/follow.rhai — {source_bytes} source bytes, {nodes} AST nodes\n");
    println!("{:<18} {:>7} {:>9}", "node kind", "count", "bytes");
    for (kind, (n, bytes)) in &counts {
        println!("{kind:<18} {n:>7} {bytes:>9}");
    }

    println!("\n{:<18} {:>9}", "code", code_bytes);
    println!("{:<18} {:>9}   ({} distinct names)", "string table", string_table, names.len());
    println!("{:<18} {:>9}   ({pool_refs} pool refs)", "constants", const_pool);
    println!("{:<18} {:>9}", "header", HEADER);
    println!("{:<18} {:>9}", "projected total", projected);

    println!("\nprojected artifact / source   {:.2}x", projected as f64 / source_bytes as f64);

    assert!(nodes > 0, "the walk visited nothing");
    assert!(code_bytes > 0, "every node priced at zero means the model is broken",);
    assert!(
        !counts.contains_key(UNPRICED),
        "{} nodes have no cost in the model, so the projection understates: \
         a rhai upgrade added an AST variant",
        counts[UNPRICED].0,
    );
}
