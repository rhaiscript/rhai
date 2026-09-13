//! Compile a Rhai script to Grain bytecodes.
//!
//! `grain-compile script.rhai script.rgrn`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "no_ast"))]
    {
        use rhai::grain::Compiler;
        use rhai::Engine;

        let path = std::env::args()
            .nth(1)
            .ok_or("usage: grain-compile <script.rhai> [<output.rgrn>]")?;
        let source = std::fs::read_to_string(&path)?;

        let path = std::env::args().nth(2).unwrap_or_else(|| {
            let mut path = std::path::PathBuf::from(path);
            path.set_extension("rgrn");
            path.to_string_lossy().into_owned()
        });

        let engine = Engine::new();
        let ast = engine.compile(&source)?;
        let program = Compiler::new().compile(&ast);

        println!("{program:?}");
        println!(
            "\nresiduals (AST fragments left over): {}",
            program.residual_count()
        );

        let buf = program
            .write()
            .map_err(|e| format!("failed to write program: {e}"))?;

        std::fs::write(&path, &buf)?;

        println!("wrote {} bytes to {path}", buf.len());

        return Ok(());
    }

    #[cfg(feature = "no_ast")]
    return Err("the 'no_ast' feature must be disabled to compile a script".into());
}
