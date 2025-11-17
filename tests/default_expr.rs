// Test default parameter expressions

#![cfg(feature = "default-parameters")]

use rhai::{Engine, EvalAltResult, INT};

#[test]
fn test_default_expr_simple() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Simple expression default
    let result = engine.eval::<INT>(
        r#"
        fn add(a, b = a + 1) {
            a + b
        }
        add(5)
        "#,
    )?;
    assert_eq!(result, 11); // 5 + (5 + 1) = 11

    Ok(())
}

#[test]
fn test_default_expr_referencing_previous_params() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Default referencing previous parameters
    let result = engine.eval::<INT>(
        r#"
        fn multiply(a, b = a * 2, c = a + b) {
            a + b + c
        }
        multiply(3)
        "#,
    )?;
    // a = 3, b = 3 * 2 = 6, c = 3 + 6 = 9
    // result = 3 + 6 + 9 = 18
    assert_eq!(result, 18);

    Ok(())
}


// Named arguments use = syntax: func(a, param = value)
#[test]
fn test_default_expr_with_named_args() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Named arguments with expression defaults
    let result = engine.eval::<INT>(
        r#"
        fn calc(a, b = a + 1, c = b * 2) {
            a + b + c
        }
        
        calc(5, c = 20)
        "#,
    )?;
    // a = 5, b = 5 + 1 = 6 (default), c = 20 (named)
    // result = 5 + 6 + 20 = 31
    assert_eq!(result, 31);

    Ok(())
}

#[test]
fn test_default_expr_complex() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Complex expression with function calls
    let result = engine.eval::<INT>(
        r#"
        fn double(x) { x * 2 }
        
        fn calc(a, b = double(a), c = a + b) {
            a + b + c
        }
        
        calc(7)
        "#,
    )?;
    // a = 7, b = double(7) = 14, c = 7 + 14 = 21
    // result = 7 + 14 + 21 = 42
    assert_eq!(result, 42);

    Ok(())
}

#[test]
fn test_default_expr_with_literals_mixed() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Mix of literal and expression defaults
    let result = engine.eval::<INT>(
        r#"
        fn test(a, b = 10, c = a + b) {
            a + b + c
        }
        
        test(5)
        "#,
    )?;
    // a = 5, b = 10 (literal), c = 5 + 10 = 15
    // result = 5 + 10 + 15 = 30
    assert_eq!(result, 30);

    Ok(())
}

// Closures support default parameters and can capture outer scope variables
#[test]
fn test_default_expr_closure() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Closure with expression defaults
    let result = engine.eval::<INT>(
        r#"
        let f = |a, b = a * 2| a + b;
        f.call(6)
        "#,
    )?;
    // a = 6, b = 6 * 2 = 12
    // result = 6 + 12 = 18
    assert_eq!(result, 18);

    // Closure capturing outer variable in default
    let result2 = engine.eval::<INT>(
        r#"
        let multiplier = 10;
        let f = |x, y = x * multiplier| x + y;
        f.call(5)
        "#,
    )?;
    // x = 5, y = 5 * 10 = 50
    // result = 5 + 50 = 55
    assert_eq!(result2, 55);

    Ok(())
}

#[test]
fn test_default_expr_arithmetic() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test various arithmetic operations in defaults
    let result = engine.eval::<INT>(
        r#"
        fn calc(a, b = a + 10, c = a * 2, d = b - c) {
            a + b + c + d
        }
        calc(5)
        "#,
    )?;
    // a = 5, b = 5 + 10 = 15, c = 5 * 2 = 10, d = 15 - 10 = 5
    // result = 5 + 15 + 10 + 5 = 35
    assert_eq!(result, 35);

    Ok(())
}

#[test]
fn test_default_expr_comparison() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test comparison operations in defaults
    let result = engine.eval::<INT>(
        r#"
        fn test(a, b = if a > 10 { a * 2 } else { a + 5 }) {
            b
        }
        test(15) + test(3)
        "#,
    )?;
    // First call: a=15, b = 15 * 2 = 30
    // Second call: a=3, b = 3 + 5 = 8
    // result = 30 + 8 = 38
    assert_eq!(result, 38);

    Ok(())
}

#[test]
fn test_default_expr_string_ops() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test string operations in defaults
    let result = engine.eval::<String>(
        r#"
        fn greet(name, greeting = "Hello, " + name + "!") {
            greeting
        }
        greet("World")
        "#,
    )?;
    assert_eq!(result, "Hello, World!");

    Ok(())
}

#[test]
fn test_default_expr_multiple_calls() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test that defaults are evaluated fresh for each call
    let result = engine.eval::<INT>(
        r#"
        fn test(a, b = a * 2) {
            a + b
        }
        test(5) + test(10) + test(3)
        "#,
    )?;
    // First: a=5, b=10, result=15
    // Second: a=10, b=20, result=30
    // Third: a=3, b=6, result=9
    // Total: 15 + 30 + 9 = 54
    assert_eq!(result, 54);

    Ok(())
}

#[test]
fn test_default_expr_nested_functions() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test nested function calls in defaults
    let result = engine.eval::<INT>(
        r#"
        fn add(x, y) { x + y }
        fn mul(x, y) { x * y }
        
        fn calc(a, b = add(a, 5), c = mul(b, 2)) {
            c
        }
        calc(10)
        "#,
    )?;
    // a = 10, b = add(10, 5) = 15, c = mul(15, 2) = 30
    assert_eq!(result, 30);

    Ok(())
}

#[test]
fn test_default_expr_override_with_value() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test that providing a value overrides the default
    let result = engine.eval::<INT>(
        r#"
        fn test(a, b = a * 2) {
            a + b
        }
        test(5) + test(5, 100)
        "#,
    )?;
    // First: a=5, b=10 (default), result=15
    // Second: a=5, b=100 (provided), result=105
    // Total: 15 + 105 = 120
    assert_eq!(result, 120);

    Ok(())
}

#[test]
fn test_default_expr_array_ops() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test array operations in defaults
    let result = engine.eval::<INT>(
        r#"
        fn sum_array(arr, multiplier = arr.len()) {
            let total = 0;
            for item in arr {
                total += item;
            }
            total * multiplier
        }
        sum_array([1, 2, 3])
        "#,
    )?;
    // arr = [1, 2, 3], multiplier = 3 (arr.len())
    // sum = 6, result = 6 * 3 = 18
    assert_eq!(result, 18);

    Ok(())
}

#[test]
fn test_default_expr_logical_ops() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test logical operations in defaults
    let result = engine.eval::<INT>(
        r#"
        fn test(a, b = a > 5 && a < 15, c = if b { a * 2 } else { a }) {
            c
        }
        test(10) + test(20)
        "#,
    )?;
    // First: a=10, b=true (10 > 5 && 10 < 15), c=20 (10 * 2)
    // Second: a=20, b=false (20 > 5 but not < 15), c=20 (just a)
    // Total: 20 + 20 = 40
    assert_eq!(result, 40);

    Ok(())
}

#[test]
fn test_default_expr_method_on_param() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test calling methods on parameters in defaults
    let result = engine.eval::<INT>(
        r#"
        fn process(text, length = text.len(), doubled = length * 2) {
            doubled
        }
        process("hello")
        "#,
    )?;
    // text = "hello", length = 5, doubled = 10
    assert_eq!(result, 10);

    Ok(())
}

#[test]
fn test_default_expr_chained_dependencies() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test long chain of parameter dependencies
    let result = engine.eval::<INT>(
        r#"
        fn chain(a, b = a + 1, c = b + 1, d = c + 1, e = d + 1) {
            e
        }
        chain(10)
        "#,
    )?;
    // a=10, b=11, c=12, d=13, e=14
    assert_eq!(result, 14);

    Ok(())
}

// Constants can be accessed in default parameter expressions naturally
#[test]
fn test_default_expr_with_constants() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test defaults that use constants
    let result = engine.eval::<INT>(
        r#"
        const MULTIPLIER = 3;
        
        fn calc(a, b = a * MULTIPLIER) {
            b
        }
        calc(7)
        "#,
    )?;
    // a = 7, b = 7 * 3 = 21
    assert_eq!(result, 21);

    Ok(())
}

#[test]
fn test_default_expr_ternary() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test ternary-like expressions in defaults
    let result = engine.eval::<INT>(
        r#"
        fn clamp(value, min = 0, max = if value < 100 { 100 } else { value * 2 }) {
            if value < min {
                min
            } else if value > max {
                max
            } else {
                value
            }
        }
        clamp(50) + clamp(150)
        "#,
    )?;
    // First: value=50, min=0, max=100, result=50
    // Second: value=150, min=0, max=300 (150*2), result=150
    // Total: 50 + 150 = 200
    assert_eq!(result, 200);

    Ok(())
}

// 'this' is now accessible in closure default parameters
#[test]
fn test_default_expr_with_this() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test defaults that reference 'this' in methods
    let result = engine.eval::<INT>(
        r#"
        let obj = #{
            value: 42,
            process: |multiplier = this.value * 2| multiplier
        };
        obj.process()
        "#,
    )?;
    // multiplier = this.value * 2 = 42 * 2 = 84
    assert_eq!(result, 84);

    Ok(())
}

#[test]
fn test_default_expr_error_in_default() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test that errors in default expressions are properly propagated
    let result = engine.eval::<INT>(
        r#"
        fn divide(a, b = 10 / a) {
            b
        }
        divide(0)
        "#,
    );
    
    // Should error due to division by zero
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_default_expr_partial_override() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test providing some but not all parameters with defaults
    let result = engine.eval::<INT>(
        r#"
        fn test(a, b = a + 1, c = b + 1, d = c + 1) {
            a + b + c + d
        }
        test(10, 20)
        "#,
    )?;
    // a=10, b=20 (provided), c=21 (b+1), d=22 (c+1)
    // result = 10 + 20 + 21 + 22 = 73
    assert_eq!(result, 73);

    Ok(())
}

#[test]
fn test_default_expr_all_defaults() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test function with all parameters having defaults
    let result = engine.eval::<INT>(
        r#"
        fn test(a = 5, b = a * 2, c = a + b) {
            a + b + c
        }
        test()
        "#,
    )?;
    // a=5, b=10, c=15
    // result = 5 + 10 + 15 = 30
    assert_eq!(result, 30);

    Ok(())
}

#[test]
fn test_default_expr_complex_expression() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Test complex multi-operation expression in default
    let result = engine.eval::<INT>(
        r#"
        fn calc(x, result = (x + 5) * 2 - 3) {
            result
        }
        calc(10)
        "#,
    )?;
    // x = 10, result = (10 + 5) * 2 - 3 = 15 * 2 - 3 = 27
    assert_eq!(result, 27);

    Ok(())
}

// NOTE: This test is disabled because regular named functions in Rhai cannot access
// outer scope variables (by design). Only closures can capture outer scope.
// To access outer scope in default parameters, the function must be a closure
#[test]
#[ignore]
fn test_default_expr_with_global_state() -> Result<(), Box<EvalAltResult>> {
    let engine = Engine::new();

    // Default that mutates global state
    let result = engine.eval::<INT>(
        r#"
        let counter = 0;
        
        fn increment_counter() {
            counter += 1;
            counter
        }
        
        fn test(a, b = increment_counter()) {
            a + b
        }
        
        test(10) + test(20)
        "#,
    )?;
    // First call: a=10, b=increment_counter()=1, result=11
    // Second call: a=20, b=increment_counter()=2, result=22
    // Total: 11 + 22 = 33
    assert_eq!(result, 33);

    Ok(())
}