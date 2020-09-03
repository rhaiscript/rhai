use rhai::{Engine, EvalAltResult, INT, Scope};
use rhai::packages::{Package, StandardPackage};

#[test]
fn test_packages() -> Result<(), Box<EvalAltResult>> {
    let e = Engine::new();
    let ast = e.compile("x")?;
    let std_pkg = StandardPackage::new();

    let make_call = |x: INT| -> Result<INT, Box<EvalAltResult>> {
        // Create a raw Engine - extremely cheap.
        let mut engine = Engine::new_raw();

        // Load packages - cheap.
        engine.load_package(std_pkg.get());

        // Create custom scope - cheap.
        let mut scope = Scope::new();

        // Push variable into scope - relatively cheap.
        scope.push("x", x);

        // Evaluate script.
        engine.eval_ast_with_scope::<INT>(&mut scope, &ast)
    };

    // The following loop creates 10,000 Engine instances!
    for x in 0..10_000 {
        assert_eq!(make_call(x)?, x);
    }

    Ok(())
}
