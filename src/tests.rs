//! Module containing unit tests.
#![cfg(test)]

/// This test is to make sure no code changes increase the sizes of critical data structures.
#[test]
fn check_struct_sizes() {
    use crate::*;
    use std::mem::size_of;

    const IS_32_BIT: bool = cfg!(target_pointer_width = "32");
    const PACKED: bool = cfg!(all(
        target_pointer_width = "32",
        feature = "only_i32",
        any(feature = "no_float", feature = "f32_float")
    ));
    const WORD_SIZE: usize = size_of::<usize>();

    assert_eq!(size_of::<Dynamic>(), if PACKED { 8 } else { 16 });
    assert_eq!(size_of::<Option<Dynamic>>(), if PACKED { 8 } else { 16 });
    assert_eq!(
        size_of::<Position>(),
        if cfg!(feature = "no_position") { 0 } else { 4 }
    );
    assert_eq!(size_of::<tokenizer::Token>(), 2 * WORD_SIZE);
    assert_eq!(size_of::<ast::Expr>(), if PACKED { 12 } else { 16 });
    assert_eq!(size_of::<Option<ast::Expr>>(), if PACKED { 12 } else { 16 });
    assert_eq!(size_of::<ast::Stmt>(), if IS_32_BIT { 12 } else { 16 });
    assert_eq!(
        size_of::<Option<ast::Stmt>>(),
        if IS_32_BIT { 12 } else { 16 }
    );

    #[cfg(feature = "internals")]
    assert_eq!(size_of::<RhaiFunc>(), 3 * WORD_SIZE);

    // The following only on 64-bit platforms

    if !cfg!(target_pointer_width = "64") {
        return;
    }

    assert_eq!(size_of::<Scope>(), 24);
    assert_eq!(
        size_of::<FnPtr>(),
        32 - if cfg!(feature = "no_function") {
            WORD_SIZE
        } else {
            0
        }
    );
    assert_eq!(
        size_of::<LexError>(),
        if cfg!(feature = "unstable") { 48 } else { 56 }
    );
    assert_eq!(
        size_of::<ParseError>(),
        16 - if cfg!(feature = "no_position") {
            WORD_SIZE
        } else {
            0
        }
    );
    assert_eq!(size_of::<EvalAltResult>(), 64);
    assert_eq!(
        size_of::<NativeCallContext>(),
        56 - if cfg!(feature = "no_position") {
            WORD_SIZE
        } else {
            0
        }
    );
}
