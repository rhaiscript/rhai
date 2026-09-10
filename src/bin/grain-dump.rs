//! Print the disassembly of a Grain bytecode program.
//!
//! `grain-dump script.rgrn`

use rhai::grain::Program;

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
        println!("  {at:>5}  {where_:<8}  {}", op.disassemble(program));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: grain-dump <script.rgrn>")?;
    let source = std::fs::read(&path)?;
    let program = Program::read(&source).map_err(|e| format!("failed to read program: {e}"))?;

    println!("{program:?}");

    let code = program.code();
    dump(&program, code, "main", program.main());

    for f in program.functions() {
        let label = format!(
            "fn {} : #{} ({} params)",
            f.disassemble(&program),
            f.name,
            f.params.len(),
        );
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
