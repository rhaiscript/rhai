//! An example showing how to register a Rust type and use it with arrays.

#[cfg(any(feature = "no_index", feature = "no_object"))]
fn main() {
    panic!("This example does not run under 'no_index' or 'no_object'.")
}

use rhai::{Engine, EvalAltResult};

#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
fn main() -> Result<(), Box<EvalAltResult>> {
    #[derive(Debug, Clone)]
    struct TestStruct {
        x: i64,
    }

    impl TestStruct {
        pub fn new() -> Self {
            Self { x: 1 }
        }
        pub fn update(&mut self) {
            self.x += 1000;
        }
    }

    let mut engine = Engine::new();

    engine
        .register_type_with_name::<TestStruct>("TestStruct")
        .register_fn("new_ts", TestStruct::new)
        .register_fn("update", TestStruct::update);

    #[cfg(feature = "metadata")]
    {
        println!("Functions registered:");

        engine
            .gen_fn_signatures(false)
            .into_iter()
            .for_each(|func| println!("{func}"));

        println!();
    }

    let result = engine.eval::<TestStruct>(
        "
            let x = new_ts();
            x.update();
            x
        ",
    )?;

    println!("{result:?}");

    let result = engine.eval::<TestStruct>(
        "
            let x = [ new_ts() ];
            x[0].update();
            x[0]
        ",
    )?;

    println!("{result:?}");

    Ok(())
}
