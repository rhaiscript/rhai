//! Compile a Rhai script to grain bytecode and print the disassembly.
//!
//! `cargo run --features grain --example grain_dump -- script.rhai`

use rhai::grain::{Compiler, Program};
use rhai::Engine;

fn dump(program: &Program, code: &[u8], name: &str, chunk: &rhai::grain::bytecode::Chunk) {
    println!(
        "\n{name}  [{}..{}]  max_stack {}",
        chunk.entry(),
        chunk.end(),
        chunk.max_stack()
    );

    for (at, op) in chunk.ops(code) {
        let pos = program.position(at);
        let where_ = match (pos.line(), pos.position()) {
            (Some(line), Some(col)) => format!("{line}:{col}"),
            _ => String::new(),
        };
        println!("  {at:>5}  {:<8}  {op:?}", where_);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: grain_dump <script.rhai>")?;
    let source = std::fs::read_to_string(&path)?;

    let engine = Engine::new();
    let ast = engine.compile(&source)?;
    let program = Compiler::new().compile(&ast);

    println!("{program:?}");
    println!(
        "\nresiduals (AST fragments left over): {}",
        program.residual_count()
    );

    let code = program.code();
    dump(&program, code, "main", program.main());

    for f in program.functions() {
        let label = format!("fn #{} ({} params)", f.name, f.params.len());
        dump(&program, code, &label, &f.chunk);
    }

    match program.write_stripped() {
        Ok(s) => println!(
            "\nartifact {} bytes, sidecar {} positions",
            s.artifact.len(),
            s.sidecar.positions.len()
        ),
        Err(e) => println!("\nnot writable: {e:?}"),
    }

    Ok(())
}
