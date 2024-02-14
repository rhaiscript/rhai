use spo_rhai::plugin::*;
use spo_rhai::{Engine, EvalAltResult, Scope};

#[export_module]
pub mod general_kenobi {
    /// General Kenobi's Constant.
    pub const CONSTANT: i64 = 42;

    /// Returns a string where "hello there" is repeated `n` times.
    pub fn hello_there(n: i64) -> String {
        use std::convert::TryInto;
        "hello there ".repeat(n.try_into().unwrap())
    }
}

fn main() -> Result<(), Box<EvalAltResult>> {
    let mut engine = Engine::new();
    let mut scope = Scope::new();

    // This variable will also show up in the definitions, since it will be part of the scope.
    scope.push("hello_there", "hello there");

    // This constant will also show up in the definitions, since it will be part of the scope.
    scope.push_constant("HELLO", "hello there");

    #[cfg(not(feature = "no_module"))]
    engine.register_static_module("general_kenobi", exported_module!(general_kenobi).into());

    // Custom operators also show up in definitions.
    #[cfg(not(feature = "no_custom_syntax"))]
    {
        engine.register_custom_operator("minus", 100).unwrap();
        engine.register_fn("minus", |a: i64, b: i64| a - b);
    }

    engine.run_with_scope(
        &mut scope,
        "hello_there = general_kenobi::hello_there(4 minus 2);",
    )?;

    // Generate definitions for the contents of the engine and the scope.
    engine
        .definitions_with_scope(&scope)
        .write_to_dir("examples/definitions/.rhai/definitions")
        .unwrap();

    // Alternatively we can write all of the above to a single file.
    engine
        .definitions_with_scope(&scope)
        .write_to_file("examples/definitions/.rhai/all_in_one.d.rhai")
        .unwrap();

    // Skip standard packages if not needed (e.g. they are provided elsewhere).
    engine
        .definitions_with_scope(&scope)
        .include_standard_packages(false)
        .write_to_file("examples/definitions/.rhai/all_in_one_without_standard.d.rhai")
        .unwrap();

    // Write function definitions as JSON.
    let json = engine
        .definitions()
        .include_standard_packages(false)
        .json()
        .unwrap();

    std::fs::write("examples/definitions/.rhai/defs.json", json).unwrap();

    Ok(())
}
