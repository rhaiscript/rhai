#![no_main]
use rhai::{Dynamic, Engine, OptimizationLevel};

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use rhai::Scope;
use std::collections::HashMap;
use std::{hint::black_box, time::Instant};

#[derive(Debug, Clone, Arbitrary)]
struct Ctx<'a> {
    script: &'a str,
    optimization_level: OptimizationLevel,
    scoped_constants: HashMap<&'a str, i64>,
    scoped_vars: HashMap<&'a str, i64>,
}

fuzz_target!(|ctx: Ctx| {
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

    // We need fuzzing to be fast, so we'll stop executing after 1s.
    let start = Instant::now();
    engine.on_progress(move |_| (start.elapsed().as_millis() > 1000).then_some(Dynamic::UNIT));

    let engine = engine;

    let mut scope = Scope::new();
    for (var_name, value) in ctx.scoped_constants.into_iter() {
        scope.push_constant(var_name, value);
    }

    for (var_name, value) in ctx.scoped_vars.into_iter() {
        scope.set_or_push(var_name, value);
    }

    match engine.eval_with_scope::<Dynamic>(&mut scope, &script) {
        Ok(val) => {
            if val.is_array() {
                _ = black_box(val.clone().into_array().unwrap());
            }
            if val.is_blob() {
                _ = black_box(val.clone().into_blob().unwrap());
            }
            if val.is_bool() {
                _ = black_box(val.clone().as_bool().unwrap());
            }
            if val.is_char() {
                _ = black_box(val.clone().as_char().unwrap());
            }
            if val.is_decimal() {
                _ = black_box(val.clone().as_decimal().unwrap());
            }
            if val.is_float() {
                _ = black_box(val.clone().as_float().unwrap());
            }
            if val.is_int() {
                _ = black_box(val.clone().as_int().unwrap());
            }
            if val.is_string() {
                _ = black_box(val.clone().into_string().unwrap());
                _ = black_box(val.clone().into_immutable_string().unwrap());
            }
            if val.is_timestamp() {
                _ = black_box(val.clone().try_cast::<rhai::Instant>().unwrap());
            }
            _ = black_box(val.is_decimal());
            _ = black_box(val.is_locked());
            _ = black_box(val.is_map());
            _ = black_box(val.is_read_only());
            _ = black_box(val.is_shared());
            _ = black_box(val.is_unit());
            _ = black_box(val.is_variant());
            _ = black_box(val.type_name());
            _ = black_box(val.type_id());
            _ = black_box(val.tag());
        }
        Err(e) => _ = black_box(format!("{e}")),
    }
    _ = black_box(format!("{scope}"));
    _ = black_box(scope.clone().into_iter().count());
    _ = black_box(scope.iter().count());
    _ = black_box((&scope).into_iter().count());
    if !scope.is_empty() {
        _ = black_box(scope.pop());
    }
    _ = black_box(scope.clone_visible());
});
