use rhai::plugin::*;

#[derive(Clone)]
pub struct Point {
    x: f32,
    y: f32,
}

#[export_module]
pub mod test_module {
    #[rhai_mod(name = "bar")]
    pub mod test_mod {
        #[rhai_fn(name = "foo")]
        pub fn test_fn(input: Point) -> bool {
            input.x > input.y
        }

        #[rhai_fn(return_raw)]
        pub fn test_fn_raw(input: Point) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
            Ok(Dynamic::from(input.x > input.y))
        }
    }
}

fn main() {
    let n = Point {
        x: 0.0,
        y: 10.0,
    };
    if test_module::test_mod::test_fn(n) {
        println!("yes");
    } else {
        println!("no");
    }
}
