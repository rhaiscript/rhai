//! An example showing how to register a Rust type and methods/getters/setters using the `CustomType` trait.

#[cfg(feature = "no_object")]
fn main() {
    panic!("This example does not run under 'no_object'.");
}

use rhai::{CustomType, Engine, EvalAltResult, TypeBuilder};

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
        pub fn calculate(&mut self, data: i64) -> i64 {
            self.x * data
        }
        pub fn get_x(&mut self) -> i64 {
            self.x
        }
        pub fn set_x(&mut self, value: i64) {
            self.x = value;
        }
    }

    impl IntoIterator for TestStruct {
        type Item = i64;
        type IntoIter = std::vec::IntoIter<Self::Item>;

        #[inline]
        #[must_use]
        fn into_iter(self) -> Self::IntoIter {
            vec![self.x - 1, self.x, self.x + 1].into_iter()
        }
    }

    impl CustomType for TestStruct {
        fn build(mut builder: TypeBuilder<Self>) {
            #[allow(deprecated)] // The TypeBuilder api is volatile.
            builder
                .with_name("TestStruct")
                .with_fn("new_ts", Self::new)
                .with_fn("update", Self::update)
                .with_fn("calc", Self::calculate)
                .is_iterable()
                .with_get_set("x", Self::get_x, Self::set_x);
        }
    }

    let mut engine = Engine::new();

    engine.build_type::<TestStruct>();

    #[cfg(feature = "metadata")]
    {
        println!("Functions registered:");

        engine
            .gen_fn_signatures(false)
            .into_iter()
            .for_each(|func| println!("{func}"));

        println!();
    }

    let result = engine.eval::<i64>(
        "
            let x = new_ts();

            x.x = 42;

            for n in x {
                x.x += n;
                print(`n = ${n}, total = ${x.x}`);
            }

            x.update();

            x.calc(x.x)
        ",
    )?;

    println!("result: {result}"); // prints 1085764

    Ok(())
}
