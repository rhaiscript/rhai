mod grain {
    // Declared once here rather than in each harness: they are modules of this
    // binary, not crate roots, so a `mod corpus;` of their own would look for
    // `<harness>/corpus.rs`.
    mod corpus;

    // `allocation` is deliberately absent: it owns a counting global allocator
    // and is its own binary, declared in Cargo.toml.
    // Every one of these calls a script function by name against a map
    // receiver, which is the whole subject; `no_function` leaves nothing to
    // call and `no_object` nothing to bind it to.
    #[cfg(not(any(feature = "no_function", feature = "no_object")))]
    mod call_fn;
    // Every case here hands a function pointer or a closure to a native, which
    // takes a script function to make, an array for the native to walk, and
    // method-call syntax to reach the native by.
    #[cfg(not(any(feature = "no_function", feature = "no_index", feature = "no_object")))]
    mod callback;
    mod differential;
    mod format;
    // Both are about execution staying inside a bound, which `unchecked`
    // removes outright — and the artifact fuzzer needs `max_operations` to stop
    // a corrupted chunk looping forever rather than failing.
    #[cfg(not(feature = "unchecked"))]
    mod fuzz;
    #[cfg(not(feature = "unchecked"))]
    mod limits;
    // Prices rhai's own AST nodes, which are exported under `internals` only,
    // against `follow.rhai` — a checked-in fixture, so a build without the
    // syntax it is written in has nothing to price.
    #[cfg(feature = "internals")]
    #[cfg(not(any(feature = "no_float", feature = "no_function", feature = "no_index", feature = "no_object")))]
    mod projection;
    mod scope;
}
