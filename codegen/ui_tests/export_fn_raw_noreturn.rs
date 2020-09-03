use rhai::plugin::*;

#[derive(Clone)]
struct Point {
    x: f32,
    y: f32,
}

#[export_fn(return_raw)]
pub fn test_fn(input: &mut Point) {
    input.x += 1.0;
}

fn main() {
    let n = Point {
        x: 0.0,
        y: 10.0,
    };
    test_fn(&mut n);
    if n.x >= 10.0 {
        println!("yes");
    } else {
        println!("no");
    }
}
