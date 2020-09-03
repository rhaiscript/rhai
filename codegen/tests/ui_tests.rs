#![cfg(test)]
mod ui_tests {
    #[test]
    fn all() {
        let t = trybuild::TestCases::new();
        t.compile_fail("ui_tests/*.rs");
    }
}
