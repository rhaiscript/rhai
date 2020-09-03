Loadable Configuration
======================

{{#include ../links.md}}


Usage Scenario
--------------

* A system where settings and configurations are complex and logic-driven.

* Where said system is too complex to configure via standard configuration file formats such as `JSON`, `TOML` or `YAML`.

* The system is complex enough to require a full programming language to configure. Essentially _configuration by code_.

* Yet the configuration must be flexible, late-bound and dynamically loadable, just like a configuration file.


Key Concepts
------------

* Leverage the loadable [modules] of Rhai.  The [`no_module`] feature must not be on.

* Expose the configuration API.  Use separate scripts to configure that API.  Dynamically load scripts via the `import` statement.

* Leverage [function overloading] to simplify the API design.

* Since Rhai is _sand-boxed_, it cannot mutate the environment.  To modify the external configuration object via an API, it must be wrapped in a `RefCell` (or `RwLock`/`Mutex` for [`sync`]) and shared to the [`Engine`].


Implementation
--------------

### Configuration Type

```rust
#[derive(Debug, Clone, Default)]
struct Config {
    pub id: String;
    pub some_field: i64;
    pub some_list: Vec<String>;
    pub some_map: HashMap<String, bool>;
}
```

### Make Shared Object

```rust
let config: Rc<RefCell<Config>> = Rc::new(RefCell::new(Default::default()));
```

### Register Config API

```rust
// Notice 'move' is used to move the shared configuration object into the closure.
let cfg = config.clone();
engine.register_fn("config_set_id", move |id: String| *cfg.borrow_mut().id = id);

let cfg = config.clone();
engine.register_fn("config_get_id", move || cfg.borrow().id.clone());

let cfg = config.clone();
engine.register_fn("config_set", move |value: i64| *cfg.borrow_mut().some_field = value);

// Remember Rhai functions can be overloaded when designing the API.

let cfg = config.clone();
engine.register_fn("config_add", move |value: String|
        cfg.borrow_mut().some_list.push(value)
);

let cfg = config.clone();
engine.register_fn("config_add", move |values: &mut Array|
        cfg.borrow_mut().some_list.extend(values.into_iter().map(|v| v.to_string()))
);

let cfg = config.clone();
engine.register_fn("config_add", move |key: String, value: bool|
        cfg.borrow_mut().some_map.insert(key, value)
);

let cfg = config.clone();
engine.register_fn("config_contains", move |value: String|
        cfg.borrow().some_list.contains(&value)
);

let cfg = config.clone();
engine.register_fn("config_is_set", move |value: String|
        cfg.borrow().some_map.get(&value).cloned().unwrap_or(false)
);
```

### Configuration Script

```rust
------------------
| my_config.rhai |
------------------

config_set_id("hello");

config_add("foo");          // add to list
config_add("bar", true);    // add to map

if config_contains("hey") || config_is_set("hey") {
    config_add("baz", false);   // add to map
}
```

### Load the Configuration

```rust
import "my_config";         // run configuration script without creating a module

let id = config_get_id();

id == "hello";
```


Consider a Custom Syntax
------------------------

This is probably one of the few scenarios where a [custom syntax] can be recommended.

A properly-designed [custom syntax] can make the configuration file clean, simple to write,
easy to understand and quick to modify.

For example, the above configuration example may be expressed by this custom syntax:

```rust
------------------
| my_config.rhai |
------------------

// Configure ID
id "hello";

// Add to list
list +"foo"

// Add to map
map "bar" => true;

if config contains "hey" || config is_set "hey" {
    map "baz" => false;
}
```

Notice that `contains` and `is_set` may be implemented as a [custom operator].
