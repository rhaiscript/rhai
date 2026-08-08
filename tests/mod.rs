mod grain {
    // Declared once here rather than in each harness: they are modules of this
    // binary, not crate roots, so a `mod corpus;` of their own would look for
    // `<harness>/corpus.rs`.
    mod corpus;

    // `allocation` is deliberately absent: it owns a counting global allocator
    // and is its own binary, declared in Cargo.toml.
    mod callback;
    mod differential;
    mod format;
    mod fuzz;
    mod limits;
    // Prices rhai's own AST nodes, which are exported under `internals` only.
    #[cfg(feature = "internals")]
    mod projection;
    mod scope;
}
