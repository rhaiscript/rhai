use proc_macro2::TokenStream;

mod custom_type;
mod function;
mod module;

pub fn assert_streams_eq(actual: TokenStream, expected: TokenStream) {
    let actual = actual.to_string();
    let expected = expected.to_string();
    if actual != expected {
        let mut counter = 0;
        let _iter = actual.chars().zip(expected.chars()).skip_while(|(a, e)| {
            if *a == *e {
                counter += 1;
                true
            } else {
                false
            }
        });
        let (_actual_diff, _expected_diff) = {
            let mut actual_diff = String::new();
            let mut expected_diff = String::new();
            for (a, e) in _iter.take(50) {
                actual_diff.push(a);
                expected_diff.push(e);
            }
            (actual_diff, expected_diff)
        };
        eprintln!("actual != expected, diverge at char {counter}");
        // eprintln!("  actual: {}", _actual_diff);
        // eprintln!("expected: {}", _expected_diff);
        // assert!(false);
    }
    assert_eq!(actual, expected);
}
