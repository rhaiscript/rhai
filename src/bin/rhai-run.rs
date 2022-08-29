use rhai::{Engine, EvalAltResult, Position};

use std::{env, fs::File, io::Read, path::Path, process::exit};

fn eprint_error(input: &str, mut err: EvalAltResult) {
    fn eprint_line(lines: &[&str], pos: Position, err_msg: &str) {
        let line = pos.line().unwrap();
        let line_no = format!("{line}: ");

        eprintln!("{}{}", line_no, lines[line - 1]);
        eprintln!(
            "{:>1$} {2}",
            "^",
            line_no.len() + pos.position().unwrap(),
            err_msg
        );
        eprintln!();
    }

    let lines: Vec<_> = input.split('\n').collect();

    // Print error
    let pos = err.take_position();

    if pos.is_none() {
        // No position
        eprintln!("{}", err);
    } else {
        // Specific position
        eprint_line(&lines, pos, &err.to_string())
    }
}

fn main() {
    let mut contents = String::new();

    for filename in env::args().skip(1) {
        let filename = match Path::new(&filename).canonicalize() {
            Err(err) => {
                eprintln!("Error script file path: {}\n{}", filename, err);
                exit(1);
            }
            Ok(f) => match f.strip_prefix(std::env::current_dir().unwrap().canonicalize().unwrap())
            {
                Ok(f) => f.into(),
                _ => f,
            },
        };

        // Initialize scripting engine
        let mut engine = Engine::new();

        #[cfg(not(feature = "no_optimize"))]
        engine.set_optimization_level(rhai::OptimizationLevel::Simple);

        let mut f = match File::open(&filename) {
            Err(err) => {
                eprintln!(
                    "Error reading script file: {}\n{}",
                    filename.to_string_lossy(),
                    err
                );
                exit(1);
            }
            Ok(f) => f,
        };

        contents.clear();

        if let Err(err) = f.read_to_string(&mut contents) {
            eprintln!(
                "Error reading script file: {}\n{}",
                filename.to_string_lossy(),
                err
            );
            exit(1);
        }

        let contents = if contents.starts_with("#!") {
            // Skip shebang
            &contents[contents.find('\n').unwrap_or(0)..]
        } else {
            &contents[..]
        };

        if let Err(err) = engine
            .compile(contents)
            .map_err(|err| err.into())
            .and_then(|mut ast| {
                ast.set_source(filename.to_string_lossy().to_string());
                engine.run_ast(&ast)
            })
        {
            let filename = filename.to_string_lossy();

            eprintln!("{:=<1$}", "", filename.len());
            eprintln!("{}", filename);
            eprintln!("{:=<1$}", "", filename.len());
            eprintln!();

            eprint_error(contents, *err);
        }
    }
}
