use rhai::plugin::*;
use rhai::{Dynamic, Engine, EvalAltResult, Module, Scope, AST, INT};
use rustyline::config::Builder;
use rustyline::error::ReadlineError;
use rustyline::{Cmd, Editor, Event, EventHandler, KeyCode, KeyEvent, Modifiers, Movement};

use std::{env, fs::File, io::Read, path::Path, process::exit};

const HISTORY_FILE: &str = ".rhai-repl-history";

/// Pretty-print error.
fn print_error(input: &str, mut err: EvalAltResult) {
    let lines: Vec<_> = input.split('\n').collect();
    let pos = err.take_position();

    let line_no = if lines.len() > 1 {
        if pos.is_none() {
            String::new()
        } else {
            format!("{}: ", pos.line().unwrap())
        }
    } else {
        String::new()
    };

    // Print error position
    if pos.is_none() {
        // No position
        println!("{}", err);
    } else {
        // Specific position - print line text
        println!("{}{}", line_no, lines[pos.line().unwrap() - 1]);

        // Display position marker
        println!(
            "{0:>1$} {2}",
            "^",
            line_no.len() + pos.position().unwrap(),
            err
        );
    }
}

/// Print help text.
fn print_help() {
    println!("help       => print this help");
    println!("quit, exit => quit");
    println!("keys       => print list of key bindings");
    println!("history    => print lines history");
    println!("!!         => repeat the last history line");
    println!("!<#>       => repeat a particular history line");
    println!("!<text>    => repeat the last history line starting with some text");
    println!("!?<text>   => repeat the last history line containing some text");
    println!("scope      => print all variables in the scope");
    println!("strict     => toggle on/off Strict Variables Mode");
    #[cfg(not(feature = "no_optimize"))]
    println!("optimize   => toggle on/off script optimization");
    #[cfg(feature = "metadata")]
    println!("functions  => print all functions defined");
    #[cfg(feature = "metadata")]
    println!("json       => output all functions to `metadata.json`");
    println!("ast        => print the last AST (optimized)");
    #[cfg(not(feature = "no_optimize"))]
    println!("astu       => print the last raw, un-optimized AST");
    println!();
    println!("press Ctrl-Enter or end a line with `\\`");
    println!("to continue to the next line.");
    println!();
}

/// Print key bindings.
fn print_keys() {
    println!("Home              => move to beginning of line");
    println!("Ctrl-Home         => move to beginning of input");
    println!("End               => move to end of line");
    println!("Ctrl-End          => move to end of input");
    println!("Left              => move left");
    println!("Ctrl-Left         => move left by one word");
    println!("Right             => move right by one word");
    println!("Ctrl-Right        => move right");
    println!("Up                => previous line or history");
    println!("Ctrl-Up           => previous history");
    println!("Down              => next line or history");
    println!("Ctrl-Down         => next history");
    println!("Ctrl-R            => reverse search history");
    println!("                     (Ctrl-S forward, Ctrl-G cancel)");
    println!("Ctrl-L            => clear screen");
    #[cfg(target_family = "windows")]
    println!("Escape            => clear all input");
    println!("Ctrl-C            => exit");
    println!("Ctrl-D            => EOF (when line empty)");
    println!("Ctrl-H, Backspace => backspace");
    println!("Ctrl-D, Del       => delete character");
    println!("Ctrl-U            => delete from start");
    println!("Ctrl-W            => delete previous word");
    println!("Ctrl-T            => transpose characters");
    println!("Ctrl-V            => insert special character");
    println!("Ctrl-Y            => paste yank");
    #[cfg(target_family = "unix")]
    println!("Ctrl-Z            => suspend");
    #[cfg(target_family = "windows")]
    println!("Ctrl-Z            => undo");
    println!("Ctrl-_            => undo");
    println!("Enter             => run code");
    println!("Shift-Ctrl-Enter  => continue to next line");
    println!();
    println!("Plus all standard Emacs key bindings");
    println!();
}

// Load script files specified in the command line.
#[cfg(not(feature = "no_module"))]
#[cfg(not(feature = "no_std"))]
fn load_script_files(engine: &mut Engine) {
    // Load init scripts
    let mut contents = String::new();
    let mut has_init_scripts = false;

    for filename in env::args().skip(1) {
        let filename = match Path::new(&filename).canonicalize() {
            Err(err) => {
                eprintln!("Error script file path: {}\n{}", filename, err);
                exit(1);
            }
            Ok(f) => {
                match f.strip_prefix(std::env::current_dir().unwrap().canonicalize().unwrap()) {
                    Ok(f) => f.into(),
                    _ => f,
                }
            }
        };

        contents.clear();

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

        if let Err(err) = f.read_to_string(&mut contents) {
            println!(
                "Error reading script file: {}\n{}",
                filename.to_string_lossy(),
                err
            );
            exit(1);
        }

        let module = match engine
            .compile(&contents)
            .map_err(|err| err.into())
            .and_then(|mut ast| {
                ast.set_source(filename.to_string_lossy().to_string());
                Module::eval_ast_as_new(Scope::new(), &ast, engine)
            }) {
            Err(err) => {
                let filename = filename.to_string_lossy();

                eprintln!("{:=<1$}", "", filename.len());
                eprintln!("{}", filename);
                eprintln!("{:=<1$}", "", filename.len());
                eprintln!();

                print_error(&contents, *err);
                exit(1);
            }
            Ok(m) => m,
        };

        engine.register_global_module(module.into());

        has_init_scripts = true;

        println!("Script '{}' loaded.", filename.to_string_lossy());
    }

    if has_init_scripts {
        println!();
    }
}

// Setup the Rustyline editor.
fn setup_editor() -> Editor<()> {
    //env_logger::init();
    let config = Builder::new()
        .tab_stop(4)
        .indent_size(4)
        .bracketed_paste(true)
        .build();
    let mut rl = Editor::<()>::with_config(config).unwrap();

    // Bind more keys

    // On Windows, Esc clears the input buffer
    #[cfg(target_family = "windows")]
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent(KeyCode::Esc, Modifiers::empty())]),
        EventHandler::Simple(Cmd::Kill(Movement::WholeBuffer)),
    );
    // On Windows, Ctrl-Z is undo
    #[cfg(target_family = "windows")]
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent::ctrl('z')]),
        EventHandler::Simple(Cmd::Undo(1)),
    );
    // Map Ctrl-Enter to insert a new line - bypass need for `\` continuation
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent(KeyCode::Char('J'), Modifiers::CTRL)]),
        EventHandler::Simple(Cmd::Newline),
    );
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent(KeyCode::Enter, Modifiers::CTRL)]),
        EventHandler::Simple(Cmd::Newline),
    );
    // Map Ctrl-Home and Ctrl-End for beginning/end of input
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent(KeyCode::Home, Modifiers::CTRL)]),
        EventHandler::Simple(Cmd::Move(Movement::BeginningOfBuffer)),
    );
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent(KeyCode::End, Modifiers::CTRL)]),
        EventHandler::Simple(Cmd::Move(Movement::EndOfBuffer)),
    );
    // Map Ctrl-Up and Ctrl-Down to skip up/down the history, even through multi-line histories
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent(KeyCode::Down, Modifiers::CTRL)]),
        EventHandler::Simple(Cmd::NextHistory),
    );
    rl.bind_sequence(
        Event::KeySeq(vec![KeyEvent(KeyCode::Up, Modifiers::CTRL)]),
        EventHandler::Simple(Cmd::PreviousHistory),
    );

    // Load the history file
    if rl.load_history(HISTORY_FILE).is_err() {
        eprintln!("! No previous lines history!");
    }

    rl
}

#[export_module]
mod sample_functions {
    /// This is a sample function.
    ///
    /// It takes two numbers and prints them to a string.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let result = test(42, 123);
    ///
    /// print(result);      // prints "42 123"
    /// ```
    pub fn test(x: INT, y: INT) -> String {
        format!("{x} {y}")
    }

    /// This is a sample method for integers.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 42;
    ///
    /// x.test(123, "hello");
    ///
    /// print(x);       // prints 170
    /// ```
    #[rhai_fn(name = "test")]
    pub fn test2(x: &mut INT, y: INT, z: &str) {
        *x += y + (z.len() as INT);
        println!("{} {} {}", x, y, z);
    }
}

fn main() {
    let title = format!("Rhai REPL tool (version {})", env!("CARGO_PKG_VERSION"));
    println!("{}", title);
    println!("{0:=<1$}", "", title.len());

    #[cfg(not(feature = "no_optimize"))]
    let mut optimize_level = rhai::OptimizationLevel::Simple;

    // Initialize scripting engine
    let mut engine = Engine::new();

    #[cfg(not(feature = "no_module"))]
    #[cfg(not(feature = "no_std"))]
    load_script_files(&mut engine);

    // Setup Engine
    #[cfg(not(feature = "no_optimize"))]
    engine.set_optimization_level(rhai::OptimizationLevel::None);

    // Set a file module resolver without caching
    #[cfg(not(feature = "no_module"))]
    #[cfg(not(feature = "no_std"))]
    {
        let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
        resolver.enable_cache(false);
        engine.set_module_resolver(resolver);
    }

    // Register sample functions
    engine.register_global_module(exported_module!(sample_functions).into());

    // Create scope
    let mut scope = Scope::new();

    // REPL line editor setup
    let mut rl = setup_editor();

    // REPL loop
    let mut input = String::new();
    let mut replacement = None;
    let mut replacement_index = 0;
    let mut history_offset = 1;

    let mut main_ast = AST::empty();
    #[cfg(not(feature = "no_optimize"))]
    let mut ast_u = AST::empty();
    let mut ast = AST::empty();

    print_help();

    'main_loop: loop {
        if let Some(replace) = replacement.take() {
            input = replace;
            if rl.add_history_entry(input.clone()) {
                history_offset += 1;
            }
            if input.contains('\n') {
                println!("[{}] ~~~~", replacement_index);
                println!("{}", input);
                println!("~~~~");
            } else {
                println!("[{}] {}", replacement_index, input);
            }
            replacement_index = 0;
        } else {
            input.clear();

            loop {
                let prompt = if input.is_empty() { "repl> " } else { "    > " };

                match rl.readline(prompt) {
                    // Line continuation
                    Ok(mut line) if line.ends_with('\\') => {
                        line.pop();
                        input += &line;
                        input.push('\n');
                    }
                    Ok(line) => {
                        input += &line;
                        let cmd = input.trim();
                        if !cmd.is_empty()
                            && !cmd.starts_with('!')
                            && cmd.trim() != "history"
                            && rl.add_history_entry(input.clone())
                        {
                            history_offset += 1;
                        }
                        break;
                    }

                    Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break 'main_loop,

                    Err(err) => {
                        eprintln!("Error: {:?}", err);
                        break 'main_loop;
                    }
                }
            }
        }

        let cmd = input.trim();

        if cmd.is_empty() {
            continue;
        }

        // Implement standard commands
        match cmd {
            "help" => {
                print_help();
                continue;
            }
            "keys" => {
                print_keys();
                continue;
            }
            "exit" | "quit" => break, // quit
            "history" => {
                for (i, h) in rl.history().iter().enumerate() {
                    match &h.split('\n').collect::<Vec<_>>()[..] {
                        [line] => println!("[{}] {}", history_offset + i, line),
                        lines => {
                            for (x, line) in lines.iter().enumerate() {
                                let number = format!("[{}]", history_offset + i);
                                if x == 0 {
                                    println!("{} {}", number, line.trim_end());
                                } else {
                                    println!("{0:>1$} {2}", "", number.len(), line.trim_end());
                                }
                            }
                        }
                    }
                }
                continue;
            }
            "strict" if engine.strict_variables() => {
                engine.set_strict_variables(false);
                println!("Strict Variables Mode turned OFF.");
                continue;
            }
            "strict" => {
                engine.set_strict_variables(true);
                println!("Strict Variables Mode turned ON.");
                continue;
            }
            #[cfg(not(feature = "no_optimize"))]
            "optimize" if optimize_level == rhai::OptimizationLevel::Simple => {
                optimize_level = rhai::OptimizationLevel::None;
                println!("Script optimization turned OFF.");
                continue;
            }
            #[cfg(not(feature = "no_optimize"))]
            "optimize" => {
                optimize_level = rhai::OptimizationLevel::Simple;
                println!("Script optimization turned ON.");
                continue;
            }
            "scope" => {
                println!("{}", scope);
                continue;
            }
            #[cfg(not(feature = "no_optimize"))]
            "astu" => {
                // print the last un-optimized AST
                println!("{:#?}\n", ast_u);
                continue;
            }
            "ast" => {
                // print the last AST
                println!("{:#?}\n", ast);
                continue;
            }
            #[cfg(feature = "metadata")]
            "functions" => {
                // print a list of all registered functions
                for f in engine.gen_fn_signatures(false) {
                    println!("{}", f)
                }

                #[cfg(not(feature = "no_function"))]
                for f in main_ast.iter_functions() {
                    println!("{}", f)
                }

                println!();
                continue;
            }
            #[cfg(feature = "metadata")]
            "json" => {
                use std::io::Write;

                let json = engine
                    .gen_fn_metadata_with_ast_to_json(&main_ast, false)
                    .unwrap();
                let mut f = std::fs::File::create("metadata.json")
                    .expect("Unable to create `metadata.json`");
                f.write_all(json.as_bytes()).expect("Unable to write data");
                println!("Functions metadata written to `metadata.json`.");
                continue;
            }
            "!!" => {
                if let Some(line) = rl.history().last() {
                    replacement = Some(line.clone());
                    replacement_index = history_offset + rl.history().len() - 1;
                } else {
                    eprintln!("No lines history!");
                }
                continue;
            }
            _ if cmd.starts_with("!?") => {
                let text = cmd[2..].trim();
                if let Some((n, line)) = rl
                    .history()
                    .iter()
                    .rev()
                    .enumerate()
                    .find(|&(.., h)| h.contains(text))
                {
                    replacement = Some(line.clone());
                    replacement_index = history_offset + (rl.history().len() - 1 - n);
                } else {
                    eprintln!("History line not found: {}", text);
                }
                continue;
            }
            _ if cmd.starts_with('!') => {
                if let Ok(num) = cmd[1..].parse::<usize>() {
                    if num >= history_offset {
                        if let Some(line) = rl.history().get(num - history_offset) {
                            replacement = Some(line.clone());
                            replacement_index = num;
                            continue;
                        }
                    }
                } else {
                    let prefix = cmd[1..].trim();
                    if let Some((n, line)) = rl
                        .history()
                        .iter()
                        .rev()
                        .enumerate()
                        .find(|&(.., h)| h.trim_start().starts_with(prefix))
                    {
                        replacement = Some(line.clone());
                        replacement_index = history_offset + (rl.history().len() - 1 - n);
                        continue;
                    }
                }
                eprintln!("History line not found: {}", &cmd[1..]);
                continue;
            }
            _ => (),
        }

        match engine
            .compile_with_scope(&scope, &input)
            .map_err(Into::into)
            .and_then(|r| {
                #[cfg(not(feature = "no_optimize"))]
                {
                    ast_u = r.clone();

                    ast = engine.optimize_ast(&scope, r, optimize_level);
                }

                #[cfg(feature = "no_optimize")]
                {
                    ast = r;
                }

                // Merge the AST into the main
                main_ast += ast.clone();

                // Evaluate
                engine.eval_ast_with_scope::<Dynamic>(&mut scope, &main_ast)
            }) {
            Ok(result) if !result.is::<()>() => {
                println!("=> {:?}", result);
                println!();
            }
            Ok(_) => (),
            Err(err) => {
                println!();
                print_error(&input, *err);
                println!();
            }
        }

        // Throw away all the statements, leaving only the functions
        main_ast.clear_statements();
    }

    rl.save_history(HISTORY_FILE).unwrap();

    println!("Bye!");
}
