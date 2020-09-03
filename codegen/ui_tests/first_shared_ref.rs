use rhai::plugin::*;

struct NonClonable {
    a: f32,
    b: u32,
    c: char,
    d: bool,
}

#[export_fn]
pub fn test_fn(input: &NonClonable) -> bool {
    input.d
}

fn main() {
    let n = NonClonable {
        a: 0.0,
        b: 10,
        c: 'a',
        d: true,
    };
    if test_fn(n) {
        println!("yes");
    } else {
        println!("no");
    }
}
