use rhai::{Engine, EvalAltResult};

use std::collections::HashSet; 

#[test]
fn test_extract_expressions() -> Result<(), Box<EvalAltResult>> {
    let e = Engine::new();
    assert_vars_expr_eq(&e,&[],"1+1")?;
    assert_vars_expr_eq(&e,&["a","b"],"a+b")?;
    Ok(())
}

#[test]
fn test_extract_statements() -> Result<(), Box<EvalAltResult>> {
    let e = Engine::new();
    assert_vars_stmt_eq(&e, &["x"],"if x > 0 { 42 } else { 123 }")?;
    // x is defined inside the script
    assert_vars_stmt_eq(&e, &["y","z"],"let x = 4 + 5 - y + z; y = 1;")?;
    // x and a are defined inside the script
    assert_vars_stmt_eq(&e, &["y","z"],"let x = 4 + 5 - y + z; y = 1;let a = x;")?;
    Ok(())
}


fn assert_vars_expr_eq(e: &Engine, vars:&[&str], expr: &str) -> Result<(), Box<EvalAltResult>> {
    assert_eq!(vars.into_iter().map(|s| s.to_string()).collect::<HashSet<String>>(),e.compile_expression(expr)?.extract_variables());
    Ok(())
}

fn assert_vars_stmt_eq(e: &Engine, vars:&[&str], expr: &str) -> Result<(), Box<EvalAltResult>> {
    assert_eq!(vars.into_iter().map(|s| s.to_string()).collect::<HashSet<String>>(),e.compile(expr)?.extract_variables());
    Ok(())
}