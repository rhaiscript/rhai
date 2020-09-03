use rhai::plugin::*;

#[derive(Clone)]
struct Point {
    x: f32,
    y: f32,
}

#[export_fn(rhai::name = "thing")]
pub fn test_fn(input: Point) -> bool {
    input.x > input.y
}

fn main() {
    let n = Point {
        x: 0.0,
        y: 10.0,
    };
    if test_fn(n) {
        println!("yes");
    } else {
        println!("no");
    }
}
