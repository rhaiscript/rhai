#![no_main]
use rhai::{Engine, OptimizationLevel};

use anyhow::Result;
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::hint::black_box;

#[derive(Debug, Clone, Arbitrary)]
struct Ctx<'a> {
    script: &'a str,
    optimization_level: OptimizationLevel,
}

fn fuzz(ctx: Ctx) -> Result<()> {
    let mut engine = Engine::new();

    engine.set_max_string_size(1000);
    engine.set_max_array_size(500);
    engine.set_max_map_size(500);
    engine.set_max_variables(1000);
    engine.set_max_functions(100);
    engine.set_max_modules(100);
    engine.set_max_call_levels(10);
    engine.set_max_expr_depths(50, 5);
    engine.set_optimization_level(ctx.optimization_level);

    // Don't actually print to stdout, but also don't optimise
    // printing code away.
    engine.on_debug(|x, src, pos| _ = black_box((x, src, pos)));
    engine.on_print(move |s| {
        _ = black_box(s);
    });

    // Limit the length of scripts.
    let script = ctx.script.chars().take(32 * 1024).collect::<String>();

    let ast = engine.compile(script)?;
    _ = black_box(format!("{ast:?}"));
    _ = black_box(ast.iter_functions().count());
    _ = black_box(ast.iter_literal_variables(true, true).count());
    _ = black_box(ast.walk(&mut |_| true));
    _ = black_box(engine.gen_fn_metadata_with_ast_to_json(&ast, true));

    let mut function_only_ast = ast.clone_functions_only();
    assert!(function_only_ast.clear_functions().iter_functions().count() == 0);

    let function_only_ast = ast.clone_functions_only_filtered(|_, _, _, _, _| true);
    _ = black_box(function_only_ast.merge(&ast));

    Ok(())
}

fuzz_target!(|ctx: Ctx| {
    if let Err(e) = fuzz(ctx) {
        _ = black_box(format!("{e}"));
    }
});
