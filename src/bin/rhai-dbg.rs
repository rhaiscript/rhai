use rhai::debugger::{BreakPoint, DebuggerCommand, DebuggerEvent};
use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Position, Scope, INT};

use std::{
    env,
    fs::File,
    io::{stdin, stdout, Read, Write},
    path::Path,
    process::exit,
};

/// Pretty-print source line.
fn print_source(lines: &[String], pos: Position, offset: usize, window: (usize, usize)) {
    if pos.is_none() {
        // No position
        println!();
        return;
    }

    let line = pos.line().unwrap() - 1;
    let start = if line >= window.0 { line - window.0 } else { 0 };
    let end = usize::min(line + window.1, lines.len() - 1);
    let line_no_len = end.to_string().len();

    // Print error position
    if start >= end {
        println!("{}: {}", start + 1, lines[start]);
        if let Some(pos) = pos.position() {
            println!("{0:>1$}", "^", pos + offset + line_no_len + 2);
        }
    } else {
        for (n, s) in lines.iter().enumerate().take(end + 1).skip(start) {
            let marker = if n == line { "> " } else { "  " };

            println!(
                "{0}{1}{2:>3$}{5}│ {0}{4}{5}",
                if n == line { "\x1b[33m" } else { "" },
                marker,
                n + 1,
                line_no_len,
                s,
                if n == line { "\x1b[39m" } else { "" },
            );

            if n == line {
                if let Some(pos) = pos.position() {
                    let shift = offset + line_no_len + marker.len() + 2;
                    println!("{0:>1$}{2:>3$}", "│ ", shift, "\x1b[36m^\x1b[39m", pos + 10);
                }
            }
        }
    }
}

fn print_current_source(
    context: &mut rhai::EvalContext,
    source: Option<&str>,
    pos: Position,
    lines: &[String],
    window: (usize, usize),
) {
    let current_source = &mut *context
        .global_runtime_state_mut()
        .debugger_mut()
        .state_mut()
        .write_lock::<ImmutableString>()
        .unwrap();
    let src = source.unwrap_or("");
    if src != current_source {
        println!(
            "\x1b[34m>>> Source => {}\x1b[39m",
            source.unwrap_or("main script")
        );
        *current_source = src.into();
    }
    if !src.is_empty() {
        // Print just a line number for imported modules
        println!("{src} @ {pos:?}");
    } else {
        // Print the current source line
        print_source(lines, pos, 0, window);
    }
}

/// Pretty-print error.
fn print_error(input: &str, mut err: EvalAltResult) {
    let lines: Vec<_> = input.trim().lines().collect();
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
        println!("\x1b[31m{err}\x1b[39m");
    } else {
        // Specific position - print line text
        println!("{line_no}{}", lines[pos.line().unwrap() - 1]);

        for (i, err_line) in err.to_string().lines().enumerate() {
            // Display position marker
            println!(
                "\x1b[31m{0:>1$}{err_line}\x1b[39m",
                if i > 0 { "| " } else { "^ " },
                line_no.len() + pos.position().unwrap() + 1,
            );
        }
    }
}

/// Print debug help.
fn print_debug_help() {
    println!("help, h                => print this help");
    println!("quit, q, exit, kill    => quit");
    println!("scope                  => print the scope");
    println!("operations             => print the total operations performed");
    println!("source                 => print the current source");
    println!("print, p               => print all variables de-duplicated");
    println!("print/p this           => print the `this` pointer");
    println!("print/p <variable>     => print the current value of a variable");
    #[cfg(not(feature = "no_module"))]
    println!("imports                => print all imported modules");
    println!("node                   => print the current AST node");
    println!("list, l                => print the current source line");
    println!("list/l <line#>         => print a source line");
    println!("backtrace, bt          => print the current call-stack");
    println!("info break, i b        => print all break-points");
    println!("enable/en <bp#>        => enable a break-point");
    println!("disable/dis <bp#>      => disable a break-point");
    println!("delete, d              => delete all break-points");
    println!("delete/d <bp#>         => delete a break-point");
    #[cfg(not(feature = "no_position"))]
    println!("break, b               => set a new break-point at the current position");
    #[cfg(not(feature = "no_position"))]
    println!("break/b <line#>        => set a new break-point at a line number");
    #[cfg(not(feature = "no_object"))]
    println!("break/b .<prop>        => set a new break-point for a property access");
    println!("break/b <func>         => set a new break-point for a function call");
    println!(
        "break/b <func> <#args> => set a new break-point for a function call with #args arguments"
    );
    println!("throw                  => throw a runtime exception");
    println!("throw <message...>     => throw an exception with string data");
    println!("throw <#>              => throw an exception with numeric data");
    println!("run, r                 => restart the script evaluation from beginning");
    println!("step, s                => go to the next expression, diving into functions");
    println!("over, o                => go to the next expression, skipping oer functions");
    println!("next, n, <Enter>       => go to the next statement, skipping over functions");
    println!("finish, f              => continue until the end of the current function call");
    println!("continue, c            => continue normal execution");
    println!();
}

// Load script to debug.
fn load_script(engine: &Engine) -> (rhai::AST, String) {
    if let Some(filename) = env::args().nth(1) {
        let mut contents = String::new();

        let filename = match Path::new(&filename).canonicalize() {
            Err(err) => {
                eprintln!("\x1b[31mError script file path: {filename}\n{err}\x1b[39m");
                exit(1);
            }
            Ok(f) => {
                match f.strip_prefix(std::env::current_dir().unwrap().canonicalize().unwrap()) {
                    Ok(f) => f.into(),
                    _ => f,
                }
            }
        };

        let mut f = match File::open(&filename) {
            Err(err) => {
                eprintln!(
                    "\x1b[31mError reading script file: {}\n{}\x1b[39m",
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

        let script = if contents.starts_with("#!") {
            // Skip shebang
            &contents[contents.find('\n').unwrap_or(0)..]
        } else {
            &contents[..]
        };

        let ast = match engine
            .compile(script)
            .map_err(Into::<Box<EvalAltResult>>::into)
        {
            Err(err) => {
                print_error(script, *err);
                exit(1);
            }
            Ok(ast) => ast,
        };

        println!("Script '{}' loaded.", filename.to_string_lossy());

        (ast, contents)
    } else {
        eprintln!("\x1b[31mNo script file specified.\x1b[39m");
        exit(1);
    }
}

// Main callback for debugging.
fn debug_callback(
    mut context: rhai::EvalContext,
    event: DebuggerEvent,
    node: rhai::ASTNode,
    source: Option<&str>,
    pos: Position,
    lines: &[String],
) -> Result<DebuggerCommand, Box<EvalAltResult>> {
    // Check event
    match event {
        DebuggerEvent::Start if source.is_some() => {
            println!("\x1b[32m! Script '{}' start\x1b[39m", source.unwrap())
        }
        DebuggerEvent::Start => println!("\x1b[32m! Script start\x1b[39m"),
        DebuggerEvent::End if source.is_some() => {
            println!("\x1b[31m! Script '{}' end\x1b[39m", source.unwrap())
        }
        DebuggerEvent::End => println!("\x1b[31m! Script end\x1b[39m"),
        DebuggerEvent::Step => (),
        DebuggerEvent::BreakPoint(n) => {
            match context.global_runtime_state().debugger().break_points()[n] {
                #[cfg(not(feature = "no_position"))]
                BreakPoint::AtPosition { .. } => (),
                BreakPoint::AtFunctionName { ref name, .. }
                | BreakPoint::AtFunctionCall { ref name, .. } => {
                    println!("! Call to function {name}.")
                }
                #[cfg(not(feature = "no_object"))]
                BreakPoint::AtProperty { ref name, .. } => {
                    println!("! Property {name} accessed.")
                }
                _ => unreachable!(),
            }
        }
        DebuggerEvent::FunctionExitWithValue(r) => {
            println!(
                "! Return from function call '{}' => {:?}",
                context
                    .global_runtime_state()
                    .debugger()
                    .call_stack()
                    .last()
                    .unwrap()
                    .fn_name,
                r
            )
        }
        DebuggerEvent::FunctionExitWithError(err) => {
            println!(
                "! Return from function call '{}' with error: {}",
                context
                    .global_runtime_state()
                    .debugger()
                    .call_stack()
                    .last()
                    .unwrap()
                    .fn_name,
                err
            )
        }
        _ => unreachable!(),
    }

    // Print current source line
    print_current_source(&mut context, source, pos, lines, (0, 0));

    // Read stdin for commands
    let mut input = String::new();

    loop {
        print!("dbg> ");

        stdout().flush().expect("couldn't flush stdout");

        input.clear();

        match stdin().read_line(&mut input) {
            Ok(0) => break Ok(DebuggerCommand::Continue),
            Ok(_) => match input.split_whitespace().collect::<Vec<_>>().as_slice() {
                ["help" | "h"] => print_debug_help(),
                ["exit" | "quit" | "q" | "kill", ..] => {
                    println!("Script terminated. Bye!");
                    exit(0);
                }
                ["node"] => {
                    if pos.is_none() {
                        println!("{node:?}");
                    } else {
                        match source {
                            Some(source) => println!("{node:?} {source} @ {pos:?}"),
                            None => println!("{node:?} @ {pos:?}"),
                        }
                    }
                    println!();
                }
                ["operations"] => {
                    println!("{}", context.global_runtime_state().num_operations)
                }
                ["source"] => {
                    println!("{}", context.global_runtime_state().source().unwrap_or(""))
                }
                ["list" | "l"] => print_current_source(&mut context, source, pos, lines, (3, 6)),
                ["list" | "l", n] if n.parse::<usize>().is_ok() => {
                    let num = n.parse::<usize>().unwrap();
                    if num == 0 || num > lines.len() {
                        eprintln!("\x1b[31mInvalid line: {num}\x1b[39m");
                    } else {
                        let pos = Position::new(num as u16, 0);
                        print_current_source(&mut context, source, pos, lines, (3, 6));
                    }
                }
                ["continue" | "c"] => break Ok(DebuggerCommand::Continue),
                ["finish" | "f"] => break Ok(DebuggerCommand::FunctionExit),
                [] | ["step" | "s"] => break Ok(DebuggerCommand::StepInto),
                ["over" | "o"] => break Ok(DebuggerCommand::StepOver),
                ["next" | "n"] => break Ok(DebuggerCommand::Next),
                ["scope"] => println!("{}", context.scope()),
                ["print" | "p", "this"] => match context.this_ptr() {
                    Some(value) => println!("=> {value:?}"),
                    None => println!("`this` pointer is unbound."),
                },
                ["print" | "p", var_name] => match context.scope().get_value::<Dynamic>(var_name) {
                    Some(value) => println!("=> {value:?}"),
                    None => eprintln!("Variable not found: {var_name}"),
                },
                ["print" | "p"] => {
                    println!("{}", context.scope().clone_visible());
                    if let Some(value) = context.this_ptr() {
                        println!("this = {value:?}");
                    }
                }
                #[cfg(not(feature = "no_module"))]
                ["imports"] => {
                    for (i, (name, module)) in context
                        .global_runtime_state()
                        .scan_imports_raw()
                        .enumerate()
                    {
                        println!(
                            "[{}] {} = {}",
                            i + 1,
                            name,
                            module.id().unwrap_or("<unknown>")
                        );
                    }

                    println!();
                }
                #[cfg(not(feature = "no_function"))]
                ["backtrace" | "bt"] => {
                    for frame in context
                        .global_runtime_state()
                        .debugger()
                        .call_stack()
                        .iter()
                        .rev()
                    {
                        println!("{frame}")
                    }
                }
                ["info" | "i", "break" | "b"] => Iterator::for_each(
                    context
                        .global_runtime_state()
                        .debugger()
                        .break_points()
                        .iter()
                        .enumerate(),
                    |(i, bp)| match bp {
                        #[cfg(not(feature = "no_position"))]
                        rhai::debugger::BreakPoint::AtPosition { pos, .. } => {
                            let line_num = format!("[{}] line ", i + 1);
                            print!("{line_num}");
                            print_source(lines, *pos, line_num.len(), (0, 0));
                        }
                        _ => println!("[{}] {bp}", i + 1),
                    },
                ),
                ["enable" | "en", n] => {
                    if let Ok(n) = n.parse::<usize>() {
                        let range = 1..=context
                            .global_runtime_state_mut()
                            .debugger()
                            .break_points()
                            .len();
                        if range.contains(&n) {
                            context
                                .global_runtime_state_mut()
                                .debugger_mut()
                                .break_points_mut()
                                .get_mut(n - 1)
                                .unwrap()
                                .enable(true);
                            println!("Break-point #{n} enabled.")
                        } else {
                            eprintln!("\x1b[31mInvalid break-point: {n}\x1b[39m");
                        }
                    } else {
                        eprintln!("\x1b[31mInvalid break-point: '{n}'\x1b[39m");
                    }
                }
                ["disable" | "dis", n] => {
                    if let Ok(n) = n.parse::<usize>() {
                        let range = 1..=context
                            .global_runtime_state_mut()
                            .debugger()
                            .break_points()
                            .len();
                        if range.contains(&n) {
                            context
                                .global_runtime_state_mut()
                                .debugger_mut()
                                .break_points_mut()
                                .get_mut(n - 1)
                                .unwrap()
                                .enable(false);
                            println!("Break-point #{n} disabled.")
                        } else {
                            eprintln!("\x1b[31mInvalid break-point: {n}\x1b[39m");
                        }
                    } else {
                        eprintln!("\x1b[31mInvalid break-point: '{n}'\x1b[39m");
                    }
                }
                ["delete" | "d", n] => {
                    if let Ok(n) = n.parse::<usize>() {
                        let range = 1..=context
                            .global_runtime_state_mut()
                            .debugger()
                            .break_points()
                            .len();
                        if range.contains(&n) {
                            context
                                .global_runtime_state_mut()
                                .debugger_mut()
                                .break_points_mut()
                                .remove(n - 1);
                            println!("Break-point #{n} deleted.")
                        } else {
                            eprintln!("\x1b[31mInvalid break-point: {n}\x1b[39m");
                        }
                    } else {
                        eprintln!("\x1b[31mInvalid break-point: '{n}'\x1b[39m");
                    }
                }
                ["delete" | "d"] => {
                    context
                        .global_runtime_state_mut()
                        .debugger_mut()
                        .break_points_mut()
                        .clear();
                    println!("All break-points deleted.");
                }
                ["break" | "b", fn_name, args] => {
                    if let Ok(args) = args.parse::<usize>() {
                        let bp = rhai::debugger::BreakPoint::AtFunctionCall {
                            name: fn_name.trim().into(),
                            args,
                            enabled: true,
                        };
                        println!("Break-point added for {bp}");
                        context
                            .global_runtime_state_mut()
                            .debugger_mut()
                            .break_points_mut()
                            .push(bp);
                    } else {
                        eprintln!("\x1b[31mInvalid number of arguments: '{args}'\x1b[39m");
                    }
                }
                // Property name
                #[cfg(not(feature = "no_object"))]
                ["break" | "b", param] if param.starts_with('.') && param.len() > 1 => {
                    let bp = rhai::debugger::BreakPoint::AtProperty {
                        name: param[1..].into(),
                        enabled: true,
                    };
                    println!("Break-point added for {bp}");
                    context
                        .global_runtime_state_mut()
                        .debugger_mut()
                        .break_points_mut()
                        .push(bp);
                }
                // Numeric parameter
                #[cfg(not(feature = "no_position"))]
                ["break" | "b", param] if param.parse::<usize>().is_ok() => {
                    let n = param.parse::<usize>().unwrap();
                    let range = if source.is_none() {
                        1..=lines.len()
                    } else {
                        1..=(u16::MAX as usize)
                    };

                    if range.contains(&n) {
                        let bp = rhai::debugger::BreakPoint::AtPosition {
                            source: source.map(|s| s.into()),
                            pos: Position::new(n as u16, 0),
                            enabled: true,
                        };
                        println!("Break-point added {bp}");
                        context
                            .global_runtime_state_mut()
                            .debugger_mut()
                            .break_points_mut()
                            .push(bp);
                    } else {
                        eprintln!("\x1b[31mInvalid line number: '{n}'\x1b[39m");
                    }
                }
                // Function name parameter
                ["break" | "b", param] => {
                    let bp = rhai::debugger::BreakPoint::AtFunctionName {
                        name: param.trim().into(),
                        enabled: true,
                    };
                    println!("Break-point added for {bp}");
                    context
                        .global_runtime_state_mut()
                        .debugger_mut()
                        .break_points_mut()
                        .push(bp);
                }
                #[cfg(not(feature = "no_position"))]
                ["break" | "b"] => {
                    let bp = rhai::debugger::BreakPoint::AtPosition {
                        source: source.map(|s| s.into()),
                        pos,
                        enabled: true,
                    };
                    println!("Break-point added {bp}");
                    context
                        .global_runtime_state_mut()
                        .debugger_mut()
                        .break_points_mut()
                        .push(bp);
                }
                ["throw"] => break Err(EvalAltResult::ErrorRuntime(Dynamic::UNIT, pos).into()),
                ["throw", num] if num.trim().parse::<INT>().is_ok() => {
                    let value = num.trim().parse::<INT>().unwrap().into();
                    break Err(EvalAltResult::ErrorRuntime(value, pos).into());
                }
                #[cfg(not(feature = "no_float"))]
                ["throw", num] if num.trim().parse::<rhai::FLOAT>().is_ok() => {
                    let value = num.trim().parse::<rhai::FLOAT>().unwrap().into();
                    break Err(EvalAltResult::ErrorRuntime(value, pos).into());
                }
                ["throw", ..] => {
                    let msg = input.trim().split_once(' ').map(|(_, x)| x).unwrap_or("");
                    break Err(EvalAltResult::ErrorRuntime(msg.trim().into(), pos).into());
                }
                ["run" | "r"] => {
                    println!("Terminating current run...");
                    break Err(EvalAltResult::ErrorTerminated(Dynamic::UNIT, pos).into());
                }
                _ => eprintln!(
                    "\x1b[31mInvalid debugger command: '{}'\x1b[39m",
                    input.trim()
                ),
            },
            Err(err) => panic!("input error: {}", err),
        }
    }
}

fn main() {
    let title = format!("Rhai Debugger (version {})", env!("CARGO_PKG_VERSION"));
    println!("{title}");
    println!("{0:=<1$}", "", title.len());

    // Initialize scripting engine
    let mut engine = Engine::new();

    #[cfg(not(feature = "no_optimize"))]
    engine.set_optimization_level(rhai::OptimizationLevel::None);

    let (ast, script) = load_script(&engine);

    // Hook up debugger
    let lines: Vec<_> = script.trim().lines().map(|s| s.to_string()).collect();

    #[allow(deprecated)]
    engine.register_debugger(
        // Store the current source in the debugger state
        |engine, mut debugger| {
            debugger.set_state(engine.const_empty_string());
            debugger
        },
        // Main debugging interface
        move |context, event, node, source, pos| {
            debug_callback(context, event, node, source, pos, &lines)
        },
    );

    // Set a file module resolver without caching
    #[cfg(not(feature = "no_module"))]
    #[cfg(not(feature = "no_std"))]
    {
        let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
        resolver.enable_cache(false);
        engine.set_module_resolver(resolver);
    }

    println!("Type 'help' for commands list.");
    println!();

    // Evaluate
    while let Err(err) = engine.run_ast_with_scope(&mut Scope::new(), &ast) {
        match *err {
            // Loop back to restart
            EvalAltResult::ErrorTerminated(..) => {
                println!("Restarting script...");
            }
            // Break evaluation
            _ => {
                print_error(&script, *err);
                println!();
                break;
            }
        }
    }

    println!("Script terminated. Bye!");
}
