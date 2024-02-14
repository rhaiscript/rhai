//! An advanced example showing how to pause/resume/stop an `Engine` via an MPSC channel.

#[cfg(feature = "unchecked")]
fn main() {
    panic!("This example does not run under 'unchecked'.");
}

use rhai::{Dynamic, Engine};

#[cfg(feature = "sync")]
use std::sync::Mutex;

#[cfg(not(feature = "unchecked"))]
fn main() {
    let (tx, rx) = std::sync::mpsc::channel::<String>();

    #[cfg(feature = "sync")]
    let rx = Mutex::new(rx);

    // Spawn thread with Engine, capturing the channel
    std::thread::spawn(move || {
        // Create Engine
        let mut engine = Engine::new();

        engine.on_progress(move |_ops| {
            #[cfg(feature = "sync")]
            if _ops % 5 != 0 {
                return None;
            }

            #[cfg(feature = "sync")]
            let rx = &*rx.lock().unwrap();

            let mut paused = false;

            loop {
                match rx.try_recv() {
                    Ok(cmd) => match cmd.as_str() {
                        "pause" => {
                            println!("[Thread] Script paused. Type 'resume' to continue or 'stop' to terminate.");
                            paused = true;
                        }
                        "resume" => {
                            println!("[Thread] Resuming script...");
                            return None;
                        }
                        "stop" => {
                            println!("[Thread] Stopping script...");
                            return Some(Dynamic::UNIT);
                        }
                        cmd if paused => {
                            println!("[Thread] I don't understand '{cmd}'!");
                            println!("Type 'resume' to continue script, or 'stop' to terminate!");
                        }
                        _ => {
                            println!("[Thread] I don't understand '{cmd}'!");
                            return None;
                        }
                    },
                    Err(_) if paused => (),
                    Err(_) => return None,
                }

                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        // Run script
        let _ = engine
            .run(
                r#"
                    let counter = 0;

                    loop {
                        print("[Script] One Potato...");
                        sleep(1);

                        counter += 1;

                        print("[Script] Two Potatoes...");
                        sleep(1);

                        print(`[Script] Boring Counter: ${counter}...`);
                        sleep(1);

                        print("[Script] Three Potatoes...");
                        sleep(1);
                    }
                "#,
            )
            .expect_err("Error expected");

        println!("[Thread] Script stopped!");
    });

    println!("[Main] Type 'pause' or 'stop' to control the script.");

    let mut input = String::new();

    loop {
        input.clear();

        match std::io::stdin().read_line(&mut input) {
            Ok(0) => (),
            Ok(_) => match tx.send(input.trim().to_string()) {
                Ok(_) => (),
                Err(_) => break,
            },
            Err(_) => break,
        }
    }
}
