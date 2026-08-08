//! Scripts the VM must agree with rhai on.
//!
//! Weighted towards the places a bytecode VM is most likely to drift from a
//! tree walker rather than towards breadth: scope discipline, the error-based
//! unwinding rhai uses for `return`/`break`/`throw`, and the lvalue forms that
//! cannot be expressed as a plain `&mut` and so need explicit write-back.
//!
//! Everything here currently runs as a single `EvalAst` residual, so passing is
//! expected. That is the point — it pins the baseline, and it exercises the
//! runtime-state setup (function library, module resolver, source name,
//! `return`/`exit` mapping) which is real code that can be wrong today.

use rhai::INT;

// Only `tests/fuzz.rs` and the `generated` fuzz target use this; the other
// harnesses take the module for its cases.
#[allow(dead_code)]
pub mod generate;

pub struct Case {
    pub name: &'static str,
    pub source: &'static str,
}

/// A host type with a getter, a setter, an indexer and both kinds of method.
///
/// Registered so the corpus can reach the one part of the chain walker that is
/// conservative rather than exact. Arrays and maps hand out references, so a
/// mutation partway down a chain lands in them and nothing needs writing back.
/// A getter hands back a *value*, so `w.inner.level = 1` mutates a temporary
/// and the setter is the only way home — and rhai decides whether to call it
/// from `func.is_method()`, which the VM cannot see and therefore approximates.
/// Without a host type in the engine, nothing here is ever exercised.
/// Held as `INT` rather than `i64` throughout: `only_i32` narrows the script
/// integer, and a host type registered against the wider one would take a type
/// no script under that build can produce.
#[derive(Debug, Clone, Default)]
pub struct Widget {
    pub level: INT,
    pub cells: Vec<INT>,
}

#[derive(Debug, Clone, Default)]
pub struct Holder {
    pub inner: Widget,
}

fn out_of_range(index: INT, len: usize) -> Box<rhai::EvalAltResult> {
    Box::new(rhai::EvalAltResult::ErrorArrayBounds(len, index, rhai::Position::NONE))
}

/// The engine both sides of the differential run against.
pub fn engine() -> rhai::Engine {
    let mut engine = rhai::Engine::new();

    engine
        .register_type_with_name::<Widget>("Widget")
        .register_fn("widget", |level: INT| Widget { level, cells: vec![10, 20, 30] })
        .register_get_set("level", |w: &mut Widget| w.level, |w: &mut Widget, v: INT| w.level = v)
        // Returning an error rather than panicking, because a panic in a
        // registered function takes the test process with it.
        .register_indexer_get_set(
            |w: &mut Widget, i: INT| -> Result<INT, Box<rhai::EvalAltResult>> { w.cells.get(i as usize).copied().ok_or_else(|| out_of_range(i, w.cells.len())) },
            |w: &mut Widget, i: INT, v: INT| -> Result<(), Box<rhai::EvalAltResult>> {
                let len = w.cells.len();
                *w.cells.get_mut(i as usize).ok_or_else(|| out_of_range(i, len))? = v;
                Ok(())
            },
        )
        // Takes the receiver by reference, so rhai counts it as a method and
        // writes a temporary back afterwards.
        .register_fn("bump", |w: &mut Widget| w.level += 1)
        // Reads only. Whether rhai still writes back after one of these is
        // exactly the question this corpus is here to settle.
        .register_fn("doubled", |w: &mut Widget| w.level * 2)
        // Mutates and *then* fails. Rhai reaches a chain root through a live
        // reference, so the mutation has already landed by the time the error
        // propagates; nothing else here can tell a write-back that happens from
        // one that is skipped because the walk raised.
        .register_fn("bump_then_fail", |w: &mut Widget| -> Result<(), Box<rhai::EvalAltResult>> {
            w.level += 1;
            Err("bump_then_fail".into())
        });

    engine
        .register_type_with_name::<Holder>("Holder")
        .register_fn("holder", |level: INT| Holder { inner: Widget { level, cells: vec![1, 2, 3] } })
        .register_get_set("inner", |h: &mut Holder| h.inner.clone(), |h: &mut Holder, w: Widget| h.inner = w);

    engine
}

const fn case(name: &'static str, source: &'static str) -> Case {
    Case { name, source }
}

/// Whether a corpus case exercises anything on this build.
///
/// A restriction feature removes the syntax outright — rhai will not parse a
/// capturing closure under `no_closure`, or a float literal under `no_float` —
/// so the case tests nothing here, and both sides agreeing on the parse failure
/// would be an empty agreement rather than a passing one.
///
/// Lives here rather than in one harness because every harness that walks
/// [`CASES`] needs the same answer.
#[must_use]
pub fn applies_to_this_build(name: &str) -> bool {
    #[cfg(feature = "no_closure")]
    if name.starts_with("closure_") || name.starts_with("is_shared") {
        return false;
    }
    #[cfg(feature = "no_module")]
    if name.starts_with("import_") || name.starts_with("export_") {
        return false;
    }
    // No shared prefix to key on: a float literal is incidental to most of
    // these, which are about interpolation, ranges and operator errors.
    #[cfg(feature = "no_float")]
    if matches!(
        name,
        "float_arithmetic" | "mixed_numeric" | "interpolation_of_every_type" | "switch_float_in_range" | "error_operator_undefined_for_types" | "error_op_assign_undefined_for_types"
    ) {
        return false;
    }
    let _ = name;
    true
}

pub const CASES: &[Case] = &[
    // --- values and operators -------------------------------------------
    case("int_arithmetic", "let a = 7; let b = 3; a * b - a / b + a % b"),
    case("float_arithmetic", "let a = 7.5; let b = 0.5; a * b + a / b"),
    case("mixed_numeric", "1 + 2.5"),
    case("comparison_chain", "let a = 5; a > 1 && a < 10 || a == 5"),
    case("bitwise", "let a = 0b1010; (a & 0b0110) | (a ^ 0b1111) << 2"),
    case("string_ops", r#"let s = "hello"; s + " " + "world" + s.len"#),
    case("string_interpolation", r#"let n = 42; `answer is ${n} and ${n * 2}`"#),
    // Every segment type goes through a different arm of rhai's rendering:
    // strings skip dispatch entirely, unit renders empty, and a container
    // gets its debug-ish form.
    case("interpolation_of_every_type", r#"let s = "x"; let n = 1; let f = 1.5; let b = true; let u = (); `${s}|${n}|${f}|${b}|${u}|`"#),
    case("interpolation_of_containers", r#"let a = [1, 2]; let m = #{ k: 1 }; `${a}|${m}`"#),
    // A host type with no `to_string` registered falls back to the mapped
    // type name rather than to `Debug`.
    case("interpolation_of_host_type", r#"let w = widget(3); `w=${w}`"#),
    case("char_ops", r#"let c = 'a'; c.to_upper()"#),
    case("unit_value", "()"),
    // --- containers -------------------------------------------------------
    case("array_literal", "let a = [1, 2, 3]; a[0] + a[1] + a[2]"),
    case("array_methods", "let a = [3, 1, 2]; a.sort(); a"),
    case("map_literal", r#"let m = #{ a: 1, b: 2 }; m.a + m.b"#),
    // The one above is all-constant, so the optimizer folds it and the map
    // never gets built at run time. These do get built: rhai keeps a template
    // holding every key and fills the computed ones in afterwards.
    case("map_computed_value", "let v = 7; let m = #{ a: v, b: 2 }; m.a + m.b"),
    case("map_all_computed", "let v = 7; let w = 8; #{ a: v, b: w }"),
    case("map_computed_nested", "let v = 7; #{ outer: #{ inner: v } }.outer.inner"),
    case("map_computed_in_array", "let v = 7; [#{ a: v }, #{ a: 2 }]"),
    // An empty literal inside one that is not. It contributes no size check of
    // its own, so it must not consume the enclosing literal's running total.
    case("empty_literals_nested_in_computed_ones", "let v = 7; [v, [], #{}, v]"),
    case("empty_map_nested_in_a_computed_map", "let v = 7; #{ a: v, b: #{}, c: [] }"),
    // The value is a call, so the order it runs in relative to the rest of the
    // literal is observable.
    case("map_computed_order", r#"let log = ""; fn note(s, c) { s + c } let m = #{ a: note("", "x"), b: note("", "y") }; m.a + m.b"#),
    case("nested_containers", r#"let m = #{ xs: [1, 2, #{ y: 3 }] }; m.xs[2].y"#),
    // --- control flow -----------------------------------------------------
    case("if_else", "let a = 5; if a > 3 { \"big\" } else { \"small\" }"),
    case("while_loop", "let i = 0; let s = 0; while i < 5 { s += i; i += 1; } s"),
    case("do_while", "let i = 0; do { i += 1; } while i < 3; i"),
    case("do_until", "let i = 0; do { i += 1; } until i >= 3; i"),
    case("loop_break_value", "let i = 0; loop { i += 1; if i > 4 { break i * 10; } }"),
    case("continue_skips", "let s = 0; for i in 0..10 { if i % 2 == 0 { continue; } s += i; } s"),
    case("for_range", "let s = 0; for i in 0..5 { s += i; } s"),
    case("for_array", "let s = 0; for x in [10, 20, 30] { s += x; } s"),
    case("for_with_counter", "let s = 0; for (x, i) in [10, 20, 30] { s += x * i; } s"),
    // The loop variable is pushed once and mutated in place rather than
    // re-pushed each iteration (eval/stmt.rs:752); a VM that re-pushes would
    // leave the scope a different depth.
    case("for_loop_var_not_leaked", "let x = 99; for x in 0..3 { } x"),
    case("nested_loops_break", "let s = 0; for i in 0..3 { for j in 0..3 { if j == 2 { break; } s += 1; } } s"),
    // An empty body is a separate path in rhai that never touches the loop
    // variable or the counter (`eval/stmt.rs:719`).
    case("for_empty_body", "let s = 0; for i in 0..5 { } s"),
    // `return` out of a `for` skips the exhaustion path, so the iterator and
    // both loop variables have to go with the frame.
    case("for_return_from_body", "fn find(xs) { for (x, i) in xs { if x > 1 { return i; } } -1 } find([1, 2, 3])"),
    // A `break` out of a `while` nested in a `for` must drop nothing, and out
    // of the `for` must drop one — the two are easy to get the wrong way round.
    case("for_around_while_break", "let s = 0; for i in 0..3 { let j = 0; while true { j += 1; if j > 2 { break; } s += 1; } } s"),
    // Iterating a shared cell walks a snapshot, because rhai flattens the
    // iterable before asking for an iterator (`eval/stmt.rs:677`).
    case("for_over_captured_array", "let a = [1, 2, 3]; { let f = || a; } let s = 0; for x in a { s += x; } s"),
    // --- switch -----------------------------------------------------------
    case("switch_literal", "let x = 2; switch x { 1 => \"one\", 2 => \"two\", _ => \"other\" }"),
    case("switch_range", "let x = 42; switch x { 0..=9 => \"small\", 10..=99 => \"medium\", _ => \"large\" }"),
    // A failing guard must fall through to the next matching case, not to the
    // default, so both single-digit arms are needed to tell those apart.
    case("switch_guard", "let x = 5; switch x { 0..=9 if x % 2 == 1 => \"odd digit\", 0..=9 => \"even digit\", _ => \"big\" }"),
    case("switch_default_only", "switch 999 { 1 => \"a\", _ => \"fallback\" }"),
    // Two case values, one arm: the table has two entries pointing at one
    // body, which a compiler emitting a body per entry would duplicate.
    case("switch_shared_body", "let x = 2; switch x { 1 | 2 => \"low\", 3 => \"three\", _ => \"other\" }"),
    // The rule that reads like a bug and is not: a case value that matched but
    // whose guard declined goes to the *default*, never on to the ranges
    // (eval/stmt.rs:544). Without the range arm here the two are the same
    // answer and the case proves nothing.
    case("switch_declined_case_skips_ranges", "let f = false; let x = 1; switch x { 1 if f => \"guarded\", 0..=5 => \"range\", _ => \"default\" }"),
    // No `_` arm at all, so the miss has to produce unit from nowhere.
    case("switch_no_default", "let x = 9; switch x { 1 => \"a\" }"),
    case("switch_string", "let s = \"b\"; switch s { \"a\" => 1, \"b\" => 2, _ => 0 }"),
    // A range arm covers the reals between its bounds, so a float lands in one
    // even though the bounds are integers.
    case("switch_float_in_range", "let x = 5.5; switch x { 0..10 => \"in\", _ => \"out\" }"),
    // Hashing a host type panics, so the subject has to be checked before it
    // reaches a hasher — and must still find the default.
    case("switch_unhashable_subject", "let w = widget(3); switch w { 1 => \"int\", _ => \"other\" }"),
    // A shared value is not hashable either, so rhai skips the cases *and* the
    // ranges and goes straight to the default — however well the value would
    // otherwise have matched. Reading the subject through its cell would hide
    // that, which is why the subject is loaded unflattened.
    case("switch_on_a_shared_subject_takes_the_default", r#"let v = 0; { let f = || v; } switch v { 0 => "case", _ => "default" }"#),
    case("switch_range_on_a_shared_subject_takes_the_default", r#"let v = 5; { let f = || v; } switch v { 0..=9 => "range", _ => "default" }"#),
    // The same shape before anything shares it, so the pair says the difference
    // is the sharing rather than the switch.
    case("switch_on_an_unshared_subject_matches", r#"let v = 0; switch v { 0 => "case", _ => "default" }"#),
    // An arm body is a block: it declares, and it has to leave the scope the
    // depth it found it — which only shows up in something that reads a local
    // afterwards.
    case("switch_block_body_scope", "let x = 1; let y = 0; switch x { 1 => { let z = 5; y = z * 2 }, _ => () } y"),
    // A jump out of an arm and out of the switch, which is where the operand
    // stack most plausibly ends up a different depth on the two paths.
    // A range is a host type as far as `Dynamic` is concerned, and
    // `is_hashable` says no to those — even though `Hash for Dynamic` would in
    // fact hash a range (types/dynamic.rs:465). So rhai never matches a range
    // *subject* against anything, and neither may the VM: mirroring the gate
    // matters more than being clever about it.
    case("switch_range_subject_never_matches", "let r = 0..5; switch r { 0..5 => \"same\", _ => \"no\" }"),
    case("switch_break_from_loop", "let s = 0; let i = 0; while i < 10 { switch i { 3 => break, _ => () } s += 1; i += 1; } s"),
    // --- blocks used for their value ---------------------------------------
    // Rhai wraps a block in `Expr::Stmt` wherever a value is wanted, so these
    // are one construct in three disguises. Each declares inside the block, so
    // a lowering that forgot to rewind would leave the scope a different depth
    // and every slot after it would name the wrong variable.
    case("let_from_switch", "let x = 2; let y = switch x { 1 => \"one\", 2 => \"two\", _ => \"other\" }; y"),
    case("let_from_if", "let c = true; let y = if c { let a = 1; a } else { let b = 2; b }; y + 10"),
    case("let_from_block", "let a = 3; let y = { let z = a; z * 2 }; y"),
    // A block among a call's arguments, where the scope grows while operands
    // are already on the stack.
    case("block_as_argument", "fn add(a, b) { a + b } let n = 2; add({ let t = n; t + 1 }, 10)"),
    // --- scoping ----------------------------------------------------------
    case("shadowing_nested", "let x = 1; { let x = 2; { let x = 3; } } x"),
    case("block_scope_discarded", "let x = 1; { let y = 2; x += y; } x"),
    case("const_read", "const K = 10; K * 2"),
    // --- functions --------------------------------------------------------
    case("fn_call", "fn add(a, b) { a + b } add(2, 3)"),
    // Kept shallow deliberately: rhai's default call-depth limit is far lower
    // in debug builds than in release, and this case is about recursion working
    // at all, not about the limit. The limit gets its own case.
    case("fn_recursion", "fn fib(n) { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } } fib(6)"),
    case("fn_early_return", "fn f(x) { if x > 0 { return \"pos\"; } \"nonpos\" } f(1) + f(-1)"),
    // A script method mutating its receiver: `this` is bound by reference, so
    // the write has to land back in the caller's variable.
    case("fn_mutating_method", "fn double() { this *= 2; } let v = 21; v.double(); v"),
    case("top_level_return", "let x = 5; if x > 0 { return x * 2; } 0"),
    // A script function this compiler could not lower still runs — rhai finds
    // it in `global.lib` — and it must run in a scope of its own. Handing it
    // this frame's would let the body read the caller's locals, where rhai
    // gives it an empty one (`func/call.rs:1476`), and the read is the whole
    // difference: the walker cannot find `secret` and a VM that leaked its
    // scope answers with 42.
    //
    // `this` is what leaves the body unlowerable, and calling it by name
    // rather than as a method is what routes it through generic dispatch.
    case("error_a_skipped_function_cannot_see_the_caller", "fn peek(k) { let seen = secret; this } let secret = 42; peek(1)"),
    // --- a variable in first-argument position ------------------------------
    // Rhai rewrites `f(x, ..)` into `x.f(..)` so a `&mut` first parameter
    // mutates the variable (`func/call.rs:1434`). These are the same calls the
    // chain cases make in method syntax, and they have to mean the same thing.
    case("call_style_mutating_native", "let a = [1]; push(a, 2); a"),
    case("call_style_mutating_host_type", "let w = widget(4); bump(w); w.level"),
    case("call_style_pure_native", "let a = [1, 2]; len(a)"),
    // Rhai reads the variable *after* the other arguments, so an argument that
    // writes to it is seen. Two shapes of write, because one goes through the
    // rewrite itself and the other does not.
    case("call_style_argument_writes_the_receiver", "let a = [1]; push(a, { push(a, 9); 2 }); a"),
    case("call_style_argument_replaces_the_receiver", "let a = [1]; push(a, { a = [7]; 2 }); a"),
    // The receiver appearing again among the arguments, which is where a live
    // reference into the scope would be most likely to show. It does not: the
    // later argument was read and flattened before the reference was taken, so
    // it is a copy of what the receiver held then.
    case("call_style_receiver_is_also_an_argument", "let a = [1]; push(a, a); a"),
    case("call_style_receiver_twice_over", "let a = [1]; insert(a, 0, a); a"),
    // Neither of these can be handed out by reference, so both are passed by
    // value and the mutation is discarded (`func/call.rs:1449-1454`).
    case("call_style_constant_receiver", "const a = [1]; push(a, 2); a"),
    // The closure is made in a block so the scope the two sides are compared on
    // does not end up holding a pointer, which they render differently on
    // purpose — see `a_closure_pointer_is_late_bound` in `tests/scope.rs`.
    case("call_style_shared_receiver", "let a = [1]; { let f = || a.len(); } push(a, 2); a"),
    // A script function copies its first argument whichever way it arrives, so
    // the rewrite is invisible here — which is the thing to pin.
    case("call_style_script_fn", "fn bump_it(x) { x += 1; x } let n = 3; bump_it(n); n"),
    // The receiver resolves last, so a missing one is reported after a missing
    // argument rather than before it.
    case("error_receiver_resolves_after_arguments", "nosuch(receiver, argument)"),
    case("error_receiver_not_found", "let ok = 1; nosuch(missing, ok)"),
    // Dispatch still fails at the call, not at the variable that reached it.
    case("error_no_function_for_the_receiver", "let a = [1]; nosuch(a, 2)"),
    // An error a native *returned*, which rhai positions at the call site like
    // everything else dispatch produces (`func/call.rs:413`). Both argument
    // shapes, because one goes through the rewrite and the other does not.
    case("error_returned_by_a_native", r#"parse_int("zz")"#),
    case("error_returned_by_a_native_by_reference", r#"let s = "zz"; parse_int(s)"#),
    // --- closures ---------------------------------------------------------
    // Function pointers are invoked via `.call()`; `f(5)` would look for a
    // function literally named `f`.
    // The closure is kept inside a block in all of these. Not incidental: the
    // pointer we build is late-bound where rhai's is early-bound, so rhai
    // renders it `Fn*+("anon$..")` and we render it `Fn("anon$..")`. That
    // difference is the price of not shipping an AST body, it is script-
    // visible, and `a_closure_pointer_is_late_bound` is where it is pinned —
    // so these cases test the capture rather than re-testing the rendering.
    case("closure_capture_read", "let n = 10; let r = 0; { let f = |x| x + n; r = f.call(5); } r"),
    // Capture is by shared cell, so the mutation must be visible outside.
    case("closure_capture_mutate", "let n = 0; { let f = || n += 1; f.call(); f.call(); } n"),
    // In a block for the same reason the closure cases are: what the pointer
    // *does* matches, what it renders as does not.
    case("fn_ptr_call", "fn triple(x) { x * 3 } let r = 0; { let f = Fn(\"triple\"); r = f.call(4); } r"),
    // The same through a name that is not a constant, so rhai's optimizer
    // cannot fold it into a pointer carrying an environment.
    case("fn_ptr_from_dynamic_name", "fn triple(x) { x * 3 } let n = \"trip\" + \"le\"; let f = Fn(n); f.call(4)"),
    case("fn_ptr_curried", "fn add(a, b) { a + b } let n = \"a\" + \"dd\"; let f = Fn(n).curry(10); f.call(5)"),
    // A pointer to a native function goes to rhai's own dispatch rather than
    // to a chunk of ours.
    case("fn_ptr_to_native", "let n = \"ab\" + \"s\"; let f = Fn(n); f.call(-7)"),
    // Deliberately absent: `let x = 1; x.call(2)`. That is not an error in
    // rhai — a non-pointer target means the *argument* is the pointer and the
    // target is `this` — and the VM reproduces the behaviour, but not the
    // position. Rhai blames the argument there and the call everywhere else,
    // and one instruction has one position-table entry; using the argument's
    // was measured to move the divergence onto the common path instead of
    // removing it. A pool of positions would fix it and would not be
    // strippable.
    case("error_fn_ptr_unknown_name", "let n = \"no\" + \"pe\"; let f = Fn(n); f.call(1)"),
    // `Fn` and `curry` read their first argument and blame everything they can
    // then complain about on *it* rather than on the call — a name that is not
    // a string, a string that is not an identifier, a first argument that is
    // not a pointer (`func/call.rs:1217`, `:1220`, `:1232`).
    case("error_fn_ptr_from_a_non_string", "Fn(())"),
    case("error_fn_ptr_from_an_unusable_name", r#"Fn("not an identifier!")"#),
    case("error_curry_of_a_non_pointer", "curry(1, 2)"),
    // Capturing a variable turns its slot into a shared cell, which changes
    // what every later read and write of that slot means. Writing the slot
    // instead of writing *through* it severs the closure silently — the value
    // is right and the aliasing is dead — so this needs a write after the
    // capture to catch it.
    case("closure_shared_write", "let x = 1; let r = 0; { let f = || x; x = 42; r = f.call(); } r"),
    case("closure_shared_op_assign", "let x = 1; let r = 0; { let f = || x; x += 41; r = f.call(); } r"),
    // The same cell as the root of a chain. `get_indexed_mut` refuses a shared
    // value outright, so walking one takes the host down rather than returning
    // an error (`eval/chaining.rs:461`).
    case("closure_shared_chain_root", "let a = [1, 2, 3]; { let f = || a[0]; } a[1] = 20; a[1]"),
    // rhai answers this syntactically and registers no function for it, so a
    // lowered call would fail to resolve where the walker returns a bool.
    case("is_shared_after_capture", "let x = 1; let r = false; { let f = || x; r = is_shared(f); } [is_shared(x), r]"),
    case("closure_in_map", "[1, 2, 3].map(|x| x * 2)"),
    case("closure_in_filter", "[1, 2, 3, 4].filter(|x| x % 2 == 0)"),
    // --- chained lvalues --------------------------------------------------
    // Each of these needs a different `Target` variant and its write-back.
    case("index_assign_array", "let a = [1, 2, 3]; a[1] = 99; a"),
    case("index_assign_nested", "let m = #{ xs: [1, 2, 3] }; m.xs[2] = 42; m.xs"),
    case("property_assign_deep", "let m = #{ a: #{ b: #{ c: 1 } } }; m.a.b.c = 7; m.a.b.c"),
    case("map_autovivify", "let m = #{}; m.fresh = 1; m"),
    // The other half of it: only a write creates a key. Reading one that is
    // not there gives unit and must leave the map alone — the map is returned
    // so the test can see whether it grew.
    case("map_read_of_absent_key_does_not_create_it", "let m = #{ a: 1 }; let r = m.b; [m, r]"),
    case("map_read_absent_through_a_chain", "let m = #{ a: #{} }; let r = m.a.b; [m, r]"),
    // Walking through an absent key reaches a detached unit, so the write has
    // nowhere to land and rhai says so rather than creating the path.
    case("error_map_write_through_an_absent_key", "let m = #{}; m.a.b = 1; m"),
    // A closure holds the same cell, so a key invented by a read would be
    // visible from outside the expression that invented it.
    case("map_read_of_absent_key_is_not_visible_to_a_closure", "let m = #{}; let r = 0; { let f = || m; r = m.zz; } [m, r]"),
    case("op_assign_indexed", "let a = [1, 2, 3]; a[0] += 10; a"),
    case("bitfield_assign", "let x = 0; x[2] = true; x"),
    case("string_char_assign", r#"let s = "hello"; s[0] = 'H'; s"#),
    case("string_slice_read", r#"let s = "hello world"; s[0..5]"#),
    // The inclusive form is a different `TypeId` and a different pool tag, so
    // one does not cover the other. A string rather than an array, because
    // rhai indexes arrays with integers only and slices them with `extract`.
    case("string_slice_inclusive", r#"let s = "hello world"; s[6..=9]"#),
    // --- chains rooted at something that is not a variable ------------------
    // Rhai evaluates the root into a temporary and walks that
    // (`eval/chaining.rs:561-571`), so there is no scope entry behind it and
    // nothing is written back. One case per root shape, because each reaches a
    // different `Target`.
    case("temp_root_array_method", "[3, 1, 2].len()"),
    case("temp_root_array_index", "[10, 20, 30][1]"),
    case("temp_root_string", r#""hello".to_upper()"#),
    case("temp_root_map_property", "#{ a: 1, b: 2 }.b"),
    case("temp_root_call", "fn make() { [1, 2, 3] } make().len()"),
    case("temp_root_parenthesised", "let a = 1; let b = 2; (a + b).to_string()"),
    case("temp_root_nested", "[[1, 2], [3, 4]][1][0]"),
    // A mutating method on a temporary. The mutation has nowhere to land, and
    // the point is that both sides agree it is discarded rather than one of
    // them inventing a place to put it.
    case("temp_root_mutating_method", "let a = [1, 2, 3]; [a.len()].push(9)"),
    case("temp_root_host_mutates", "widget(4).bump()"),
    case("temp_root_host_pure", "widget(4).doubled()"),
    // Order, which is the part that is not obvious: rhai collects a chain's
    // indices *before* it evaluates what they apply to. Both halves fail, so
    // the position in the reported error is which one ran first.
    // An operator with no implementation for the types it got. The corpus
    // reaches `ErrorFunctionNotFound` through a named call elsewhere, which is
    // a different dispatch path and positions itself differently.
    case("error_operator_undefined_for_types", "let a = 1.0; a + #{ b: 1 }"),
    // A chain step that fails, one per kind. Rhai blames the step rather than
    // the chain, and a chain is one instruction with one position-table entry,
    // so these are what make each step carry its own.
    case("error_property_on_a_variable", "let x = 1; x.a"),
    case("error_property_on_a_temporary", "[1, 2].a"),
    case("error_method_on_a_variable", "let x = 1; x.to_upper()"),
    case("error_property_deep_in_a_chain", "let m = #{ a: #{} }; m.a.b.c"),
    // The op-assign form, which falls back to the plain operator when no
    // `+=` is registered and used to lose the position on the way.
    case("error_op_assign_undefined_for_types", "let a = 1.0; a += #{ b: 1 }; a"),
    case("error_temp_root_index_runs_first", "let z = 0; [1 / z][9 / z]"),
    // Two positions belong to an index step, not one: the index expression,
    // which an out-of-bounds is blamed on, and the `[` in front of it, which
    // indexing something unindexable is blamed on. They only come apart in a
    // chain of more than one step — here `n[0]` bit-indexes an integer and
    // yields a bool, and rhai names the *second* `[`.
    case("error_index_into_an_unindexable_step", "let n = 0; n[0][5]"),
    case("error_index_into_an_unindexable_step_deep", "let m = #{ a: 1 }; m.a[0][5]"),
    // --- what an escaping error leaves in the scope -------------------------
    // Rhai rewinds a block whether it is left normally or by a throw, and
    // rewinds nothing at the top level. The comparison that matters in all of
    // these is the leftover scope rather than the error.
    case("throw_from_a_block_rewinds_it", "let a = 1; { let b = 2; throw 3; }"),
    case("throw_from_a_for_body_drops_the_loop_var", "let a = 1; for i in 0..3 { throw i; }"),
    case("throw_from_a_while_body_drops_its_locals", "let a = 1; let n = 0; while n < 3 { let b = n; throw b; }"),
    // A catch block is a block too, and its variable is the one rhai pushes
    // rather than the script.
    case("throw_from_a_catch_drops_the_catch_var", "let a = 1; try { throw 2; } catch (e) { throw e; }"),
    case("throw_from_a_nested_block_drops_every_level", "let a = 1; { let b = 2; { let c = 3; for i in 0..2 { throw i; } } }"),
    // The frame boundary: a function's own locals go with its scope, and the
    // caller's top-level ones stay.
    case("throw_from_a_function_leaves_the_caller_top_level_alone", "fn boom() { let inner = 9; throw inner; } let a = 1; { let b = 2; boom(); }"),
    // Nothing to rewind, which is the case a floor set too low would break.
    case("throw_at_the_top_level_keeps_what_ran", "let a = 1; let b = 2; throw 3;"),
    case("error_temp_root_out_of_bounds", "[1, 2, 3][99]"),
    // --- errors -----------------------------------------------------------
    // Compared by variant and position, so a VM that reports the right failure
    // at the wrong place still fails the test.
    case("error_unknown_variable", "let a = 1; a + nonexistent"),
    // Positions on call failures are set by different code paths depending on
    // whether the callee was found, so both need pinning.
    case("error_unknown_function", "let a = 1; no_such_function(a)"),
    case("error_wrong_arity", "fn f(a, b) { a } f(1)"),
    case("error_array_bounds", "let a = [1, 2]; a[10]"),
    // Rhai maps the *expected* type through its registered names and leaves the
    // *actual* one raw, so a range guard reports `core::ops::range::Range<INT>`
    // rather than the `range` the same engine prints everywhere else. Mapping
    // both is the obvious mistake, and only a type with a registered name shows
    // it up.
    case("error_condition_is_a_range", "if 0..1 { 1 } else { 2 }"),
    case("error_condition_is_a_host_type", "let w = widget(1); while w { 1 }"),
    case("error_type_mismatch", r#"let a = 1; a + "string" + [1]"#),
    case("error_divide_by_zero", "let a = 1; a / 0"),
    case("throw_value", "throw 42"),
    case("throw_in_fn", "fn f() { throw \"boom\"; } f()"),
    // --- try / catch ------------------------------------------------------
    case("try_catch_value", "try { throw 7; } catch (e) { e * 2 }"),
    case("try_catch_native_error", "try { let a = [1]; a[9] } catch (e) { e.message != () }"),
    case("try_catch_rethrow", "try { try { throw 1; } catch { throw; } } catch (e) { e }"),
    // `return` unwinds as an error but must pass straight through a catch.
    case("try_catch_does_not_swallow_return", "fn f() { try { return 1; } catch { return 2; } } f()"),
    case("try_catch_no_error", "try { 5 } catch { 6 }"),
    // The catch block's value is discarded — the statement is unit on the
    // caught path and the try block's value otherwise (`eval/stmt.rs:863`).
    case("try_catch_discards_its_value", "try { throw 1; } catch { 99 }"),
    // A jump out of a `try` skips the `PopHandler` the straight-line path
    // would have run. Left armed, the next error is caught into a block that
    // has already been left — so the second failure here must not be caught.
    case("break_out_of_try_disarms_it", "let s = 0; while true { try { throw 1; } catch { break; } } try { throw 2; } catch (e) { s = e; } s"),
    case("break_out_of_for_inside_try", "let s = 0; for i in 0..5 { try { if i == 2 { break; } s += i; } catch { s = -1; } } s"),
    case("continue_out_of_try_inside_for", "let s = 0; for i in 0..5 { try { if i % 2 == 0 { continue; } s += i; } catch { s = -1; } } s"),
    // An error out of a called function arrives wrapped in
    // `ErrorInFunctionCall`, which is catchable, and `unwrap_inner` is what
    // still binds the bare thrown value.
    case("try_around_a_compiled_call", "fn boom() { throw 7; } try { boom(); } catch (e) { e }"),
    // `return` is a pseudo error and must pass straight through a handler.
    case("try_does_not_catch_return", "fn f() { try { return 1; } catch { 2 } } f()"),
    // --- host types -------------------------------------------------------
    //
    // The one part of the chain walker that approximates rather than
    // reproduces. A getter hands back a value, so anything below it mutates a
    // temporary that only the setter can put back — and rhai decides whether
    // to call the setter from `func.is_method()`, which is not visible from
    // outside the crate.
    case("host_get", "let w = widget(4); w.level"),
    case("host_set", "let w = widget(4); w.level = 9; w.level"),
    case("host_op_assign", "let w = widget(4); w.level += 5; w.level"),
    case("host_index_get", "let w = widget(1); w[1]"),
    case("host_index_set", "let w = widget(1); w[1] = 99; w[1]"),
    case("host_method_mutates", "let w = widget(4); w.bump(); w.level"),
    case("host_method_pure", "let w = widget(4); w.doubled()"),
    // Two levels, so the middle one is a temporary.
    case("host_temp_set", "let h = holder(3); h.inner.level = 8; h.inner.level"),
    case("host_temp_index_set", "let h = holder(3); h.inner[0] = 7; h.inner[0]"),
    // A mutating call on a temporary: rhai writes it back, so the change
    // survives.
    case("host_temp_mutates", "let h = holder(3); h.inner.bump(); h.inner.level"),
    // A read-only call on a temporary, which is where rhai's own flag decides
    // whether a setter runs at all.
    case("host_temp_pure", "let h = holder(3); h.inner.doubled()"),
    case("error_host_index_bounds", "let w = widget(1); w[99]"),
    // A step that mutates and then raises. The error is caught, so what is
    // being compared is whether the mutation reached the variable — rhai's does,
    // because it never walked a copy.
    case("host_mutation_before_a_failure_survives", "let w = widget(1); try { w.bump_then_fail(); } catch(e) {} w.level"),
    case("host_mutation_before_a_failure_survives_in_a_map", "let m = #{ w: widget(1) }; try { m.w.bump_then_fail(); } catch(e) {} m.w.level"),
    case("host_mutation_before_a_failure_survives_in_an_array", "let a = [widget(1)]; try { a[0].bump_then_fail(); } catch(e) {} a[0].level"),
    // `this`, which is a register rather than a scope entry and so is reached
    // by instructions of its own.
    case("this_read", "fn get() { this } let v = 7; v.get()"),
    case("this_in_an_expression", "fn double() { this * 2 } let v = 21; v.double()"),
    case("this_assign", "fn set() { this = 9; } let v = 1; v.set(); v"),
    case("this_op_assign", "fn bump(n) { this += n; } let v = 1; v.bump(4); v"),
    case("this_op_assign_on_a_string", "fn add(s) { this += s; } let v = \"a\"; v.add(\"b\"); v"),
    case("this_is_the_bodys_value", "fn twice() { this + this } let v = 4; v.twice()"),
    // Never inherited: a plain call from a bound body gets no receiver.
    case("error_this_is_not_inherited", "fn outer() { inner() } fn inner() { this } let v = 1; v.outer()"),
    case("error_this_unbound_in_call_style", "fn get() { this } get()"),
    // The check precedes the right-hand side, unlike the variable arm.
    case("error_this_assign_unbound_beats_a_bad_value", "fn set() { this = nosuch; } set()"),
    // Chains rooted at `this`, which must write back into the caller's value.
    case("this_property", "fn count() { this.n } let m = #{ n: 5 }; m.count()"),
    case("this_property_assign", "fn set() { this.n = 9; } let m = #{ n: 1 }; m.set(); m.n"),
    case("this_index", "fn first() { this[0] } let a = [3, 4]; a.first()"),
    case("this_index_assign", "fn set() { this[0] = 9; } let a = [1, 2]; a.set(); a"),
    case("this_method_step", "fn grow() { this.push(3); } let a = [1, 2]; a.grow(); a"),
    case("this_host_method", "fn raise() { this.bump(); } let w = widget(4); w.raise(); w.level"),
    case("this_host_property", "fn read() { this.level } let w = widget(4); w.read()"),
    // A method on `this` that reaches another compiled function.
    case("this_nested_method", "fn outer() { this.inner() } fn inner() { this * 2 } let v = 5; v.outer()"),
    // `f(this, ..)`, which rhai rewrites to `this.f(..)` by reference.
    case("this_as_first_argument", "fn grow() { push(this, 3); } let a = [1, 2]; a.grow(); a"),
    case("this_as_first_argument_pure", "fn size() { len(this) } let a = [1, 2]; a.size()"),
    case("this_as_a_later_argument", "fn plus(n) { n + this } let v = 1; v.plus(2)"),
    // Arity excludes the receiver, so these are two different functions.
    case("this_method_arity", "fn f() { 1 } fn f(x) { this + x } let v = 10; [v.f(), v.f(5)]"),
    // `obj.call(f)` binds `obj` as the closure's `this` by reference, so a
    // write inside the closure reaches `obj`. The operand stack only ever holds
    // a copy of it, which is why the instruction carries where it came from.
    //
    // The pointer is scoped to a block throughout, as the other closure cases
    // are: a compiled closure's `FnPtr` carries a name where rhai's carries the
    // body and its environment, so one left in the scope compares unequal for a
    // reason that has nothing to do with the call.
    case("closure_call_on_a_local_writes_back", "let v = 21; { let f = || { this *= 2; }; v.call(f); } v"),
    case("closure_call_on_a_local_inline", "let v = 21; v.call(|| { this *= 2; }); v"),
    case("closure_call_on_a_local_reads", "let v = 21; let r = 0; { let f = || this * 2; r = v.call(f); } r"),
    case("closure_call_mutates_an_array", "let a = [1]; { let f = || { this.push(2); }; a.call(f); } a"),
    // And the receiver can be the frame's own receiver.
    case("closure_call_on_this", "fn twice() { let f = || { this *= 2; }; this.call(f); } let v = 21; v.twice(); v"),
    // A temporary receiver has nowhere to write back to, and rhai mutates a
    // copy of it too.
    case("closure_call_on_a_temporary", "let r = 0; { let f = || { this *= 2; }; r = (20 + 1).call(f); } r"),
];
