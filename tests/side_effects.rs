///! This test simulates an external command object that is driven by a script.
use rhai::{Engine, Scope, INT};
use std::sync::{Arc, Mutex, RwLock};

/// Simulate a command object.
struct Command {
    /// Simulate an external state.
    state: INT,
}

impl Command {
    /// Do some action.
    pub fn action(&mut self, val: INT) {
        self.state = val;
    }
    /// Get current value.
    pub fn get(&self) -> INT {
        self.state
    }
}

#[allow(clippy::upper_case_acronyms)]
type API = Arc<Mutex<Command>>;

#[cfg(not(feature = "no_object"))]
#[test]
fn test_side_effects_command() {
    let mut engine = Engine::new();
    let mut scope = Scope::new();

    // Create the command object with initial state, handled by an `Arc`.
    let command = Arc::new(Mutex::new(Command { state: 12 }));
    assert_eq!(command.lock().unwrap().get(), 12);

    // Create the API object.
    let api = command.clone(); // Notice this clones the `Arc` only

    // Make the API object a singleton in the script environment.
    scope.push_constant("Command", api);

    // Register type.
    engine.register_type_with_name::<API>("CommandType");
    engine.register_fn("action", |api: &mut API, x: INT| {
        let mut command = api.lock().unwrap();
        let val = command.get();
        command.action(val + x);
    });
    engine.register_get("value", |command: &mut API| command.lock().unwrap().get());

    assert_eq!(
        engine
            .eval_with_scope::<INT>(
                &mut scope,
                "
                    // Drive the command object via the wrapper
                    Command.action(30);
                    Command.value
                "
            )
            .unwrap(),
        42
    );

    // Make sure the actions are properly performed
    assert_eq!(command.lock().unwrap().get(), 42);
}

#[test]
fn test_side_effects_print() {
    let result = Arc::new(RwLock::new(String::new()));

    let mut engine = Engine::new();

    // Override action of 'print' function
    let logger = result.clone();
    engine.on_print(move |s| logger.write().unwrap().push_str(s));

    engine.run("print(40 + 2);").unwrap();

    assert_eq!(*result.read().unwrap(), "42");
}
