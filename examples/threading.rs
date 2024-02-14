//! An advanced example showing how to communicate with an `Engine` running in a separate thread via
//! an MPSC channel.

use rhai::Engine;

#[cfg(feature = "sync")]
use std::sync::Mutex;

fn main() {
    // Channel: Script -> Master
    let (tx_script, rx_master) = std::sync::mpsc::channel();
    // Channel: Master -> Script
    let (tx_master, rx_script) = std::sync::mpsc::channel();

    #[cfg(feature = "sync")]
    let (tx_script, rx_script) = (Mutex::new(tx_script), Mutex::new(rx_script));

    // Spawn thread with Engine
    std::thread::spawn(move || {
        // Create Engine
        let mut engine = Engine::new();

        // Register API
        // Notice that the API functions are blocking

        #[cfg(not(feature = "sync"))]
        engine
            .register_fn("get", move || rx_script.recv().unwrap_or_default())
            .register_fn("put", move |v: i64| tx_script.send(v).unwrap());

        #[cfg(feature = "sync")]
        engine
            .register_fn("get", move || rx_script.lock().unwrap().recv().unwrap())
            .register_fn("put", move |v: i64| {
                tx_script.lock().unwrap().send(v).unwrap()
            });

        // Run script
        engine
            .run(
                r#"
                    print("Starting script loop...");

                    loop {
                        let x = get();
                        print(`Script Read: ${x}`);
                        x += 1;
                        print(`Script Write: ${x}`);
                        put(x);
                    }
                "#,
            )
            .unwrap();
    });

    // This is the main processing thread

    println!("Starting main loop...");

    let mut value: i64 = 0;

    while value < 10 {
        println!("Value: {value}");
        // Send value to script
        tx_master.send(value).unwrap();
        // Receive value from script
        value = rx_master.recv().unwrap();
    }
}
