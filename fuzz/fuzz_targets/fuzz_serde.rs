#![no_main]
use std::{hint::black_box, time::Instant};

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use rhai::{
    serde::{from_dynamic, to_dynamic},
    Dynamic, Engine, OptimizationLevel, Scope,
};
use serde::{Deserialize, Serialize};

#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Enum {
    Normal,
    Newtype(u8),
    StructType { _x: u8, _y: u8 },
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SubStruct {
    _x: f64,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct NewType(u8);

#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TupleStruct(u8, u8);

#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AllTypes {
    _bool: bool,
    _str: String,
    _char: char,

    _i8: i8,
    _i16: i16,
    _i32: i32,
    _i64: i64,
    _i128: i128,

    _u8: u8,
    _u16: u16,
    _u32: u32,
    _u64: u64,
    _u128: u128,

    _f32: f32,
    _f64: f64,

    _unit: (),
    _tuple: (u8, u8),

    _bytes: Vec<u8>,
    _bool_vec: Vec<bool>,
    _option: Option<u8>,

    _struct: SubStruct,
    _new_type: NewType,
    _tuple_struct: TupleStruct,
    _enum: Enum,
}

#[derive(Debug, Clone, Arbitrary)]
struct Ctx<'a> {
    script: &'a str,
    optimization_level: OptimizationLevel,
    all_types: AllTypes,
}

fuzz_target!(|ctx: Ctx| {
    let mut engine = Engine::new();

    engine.set_max_string_size(1000);
    engine.set_max_array_size(500);
    engine.set_max_map_size(500);
    engine.set_max_variables(1000);
    engine.set_max_modules(1000);
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
    if let Ok(dynamic) = to_dynamic(ctx.all_types) {
        scope.push_dynamic("x", dynamic);
    }
    match engine.run_with_scope(&mut scope, &script) {
        Ok(_) => {
            if let Some(value) = scope.get_value::<Dynamic>("x") {
                let _: Result<AllTypes, _> = black_box(from_dynamic(&value));
            }
        }
        Err(e) => _ = black_box(format!("{e}")),
    }
});
