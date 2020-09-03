use rhai::plugin::*;

struct NonClonable {
    a: f32,
    b: u32,
    c: char,
    d: bool,
}

#[export_fn]
pub fn test_fn(a: u32, b: NonClonable) -> bool {
    a == 0 && b.d
}

fn main() {
    let n = NonClonable {
        a: 0.0,
        b: 10,
        c: 'a',
        d: true,
    };
    if test_fn(10, n) {
        println!("yes");
    } else {
        println!("no");
    }
}
