use rhai::plugin::*;

#[derive(Clone)]
pub struct Point {
    x: f32,
    y: f32,
}

#[export_module]
pub mod test_module {
    pub use super::Point;
    pub fn test_fn(input: Point) -> bool {
        input.x > input.y
    }

    pub fn test_fn(input: Point) -> bool {
        input.x < input.y
    }
}

fn main() {
    let n = Point {
        x: 0.0,
        y: 10.0,
    };
    if test_module::test_fn(n) {
        println!("yes");
    } else {
        println!("no");
    }
}
