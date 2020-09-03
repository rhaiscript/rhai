use rhai::plugin::*;

#[derive(Clone)]
struct Point {
    x: f32,
    y: f32,
}

#[export_module]
pub mod test_mod {
#[rhai_fn(return_raw = "yes")]
pub fn test_fn(input: Point) -> bool {
    input.x > input.y
}
}

fn main() {
    let n = Point {
        x: 0.0,
        y: 10.0,
    };
    if test_mod::test_fn(n) {
        println!("yes");
    } else {
        println!("no");
    }
}
