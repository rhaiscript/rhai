mod grain {
    // Declared once here rather than in each harness: they are modules of this
    // binary, not crate roots, so a `mod corpus;` of their own would look for
    // `<harness>/corpus.rs`.
    mod corpus;

    // `allocation` is deliberately absent: it owns a counting global allocator
    // and is its own binary, declared in Cargo.toml.
    mod call_fn;
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
    // Prices rhai's own AST nodes, which are exported under `internals` only.
    #[cfg(feature = "internals")]
    mod projection;
    mod scope;
}
