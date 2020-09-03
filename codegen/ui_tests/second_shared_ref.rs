use rhai::plugin::*;

#[derive(Clone)]
pub struct Clonable {
    a: f32,
    b: u32,
    c: char,
    d: bool,
}

#[export_fn]
pub fn test_fn(input: Clonable, factor: &bool) -> bool {
    input.d & factor
}

fn main() {
    let n = Clonable {
        a: 0.0,
        b: 10,
        c: 'a',
        d: true,
    };
    if test_fn(n, &true) {
        println!("yes");
    } else {
        println!("no");
    }
}
