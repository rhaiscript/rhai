#![cfg(not(feature = "no_index"))]
use rhai::{Blob, Engine, Scope, INT};
use std::iter::FromIterator;

#[test]
fn test_blobs() {
    let a = Blob::from_iter([1, 2, 3]);

    let engine = Engine::new();
    let mut scope = Scope::new();
    scope.push("x", a);

    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x[1]").unwrap(), 2);
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x[0]").unwrap(), 1);
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x[-1]").unwrap(), 3);
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope, "x[-3]").unwrap(), 1);
    assert_eq!(engine.eval_with_scope::<INT>(&mut scope.clone(), "x += 4; x[3]").unwrap(), 4);

    #[cfg(not(feature = "no_object"))]
    {
        assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x.push(4); x").unwrap(), [1, 2, 3, 4]);
        assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x.insert(0, 4); x").unwrap(), [4, 1, 2, 3]);
        assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x.insert(999, 4); x").unwrap(), [1, 2, 3, 4]);
        assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x.insert(-2, 4); x").unwrap(), [1, 4, 2, 3]);
        assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x.insert(-999, 4); x").unwrap(), [4, 1, 2, 3]);
        assert_eq!(engine.eval_with_scope::<INT>(&mut scope.clone(), "let z = [42]; x[z.len]").unwrap(), 2);
        assert_eq!(engine.eval_with_scope::<INT>(&mut scope.clone(), "let z = [2]; x[z[0]]").unwrap(), 3);
    }

    assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x += x; x").unwrap(), [1, 2, 3, 1, 2, 3]);
    assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x + x").unwrap(), [1, 2, 3, 1, 2, 3]);
    assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x += 999; x").unwrap(), [1, 2, 3, 0xe7]);
    assert_eq!(engine.eval_with_scope::<Blob>(&mut scope.clone(), "x[2] = 999; x").unwrap(), [1, 2, 0xe7]);
}

#[cfg(not(feature = "only_i32"))]
#[test]
fn test_blobs_parse() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,2,0)").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,2,9)").unwrap(), 0x0908070605040302);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,2..=11)").unwrap(), 0x0908070605040302);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,2..11)").unwrap(), 0x0908070605040302);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_be_int(x,2,10)").unwrap(), 0x0203040506070809);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_be_int(x,2..12)").unwrap(), 0x0203040506070809);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,-5,99)").unwrap(), 0x0f0e0d0c0b);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,-5,2)").unwrap(), 0x0c0b);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,-99,99)").unwrap(), 0x0706050403020100);
    assert_eq!(
        engine
            .eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } write_be(x, 3, 3, -98765432); parse_be_int(x, 3, 3)")
            .unwrap(),
        0xffffff0000000000_u64 as INT
    );
    assert_eq!(
        engine
            .eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } write_be(x, 3..=5, -98765432); parse_be_int(x, 3..6)")
            .unwrap(),
        0xffffff0000000000_u64 as INT
    );
    assert_eq!(
        engine
            .eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } write_le(x, 3, 3, -98765432); parse_le_int(x, 3, 3)")
            .unwrap(),
        0x1cf588
    );
    assert_eq!(
        engine
            .eval::<Blob>("let x = blob(16, 0); write_be(x, 0, 8, 0x1234567890abcdef); write_be(x, 8, 8, 0xabcdef1234567890); x")
            .unwrap(),
        vec![0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90]
    );
}

#[cfg(feature = "only_i32")]
#[test]
fn test_blobs_parse() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,2,0)").unwrap(), 0);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,2,9)").unwrap(), 0x05040302);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_be_int(x,2,10)").unwrap(), 0x02030405);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,-5,99)").unwrap(), 0x0e0d0c0b);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,-5,2)").unwrap(), 0x0c0b);
    assert_eq!(engine.eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } parse_le_int(x,-99,99)").unwrap(), 0x03020100);
    assert_eq!(
        engine
            .eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } write_be(x, 3, 3, -98765432); parse_be_int(x, 3, 3)")
            .unwrap(),
        0xfa1cf500_u32 as INT
    );
    assert_eq!(
        engine
            .eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } write_be(x, 3..=5, -98765432); parse_be_int(x, 3..6)")
            .unwrap(),
        0xfa1cf500_u32 as INT
    );
    assert_eq!(
        engine
            .eval::<INT>("let x = blob(16, 0); for n in 0..16 { x[n] = n; } write_le(x, 3, 3, -98765432); parse_le_int(x, 3, 3)")
            .unwrap(),
        0x1cf588
    );
}

#[test]
fn test_blobs_write_string() {
    let engine = Engine::new();

    assert_eq!(engine.eval::<Blob>(r#"let x = blob(16, 0); write_ascii(x, 0, 14, "hello, world!"); x"#).unwrap(), "hello, world!\0\0\0".as_bytes());
    assert_eq!(engine.eval::<Blob>(r#"let x = blob(10, 0); write_ascii(x, 3..8, "hello, world!"); x"#).unwrap(), "\0\0\0hello\0\0".as_bytes());
    assert_eq!(engine.eval::<Blob>(r#"let x = blob(10, 0); write_ascii(x, 0..9, "❤ hello, ❤ world! ❤❤❤"); x"#).unwrap(), " hello,  \0".as_bytes());
    assert_eq!(engine.eval::<Blob>(r#"let x = blob(10, 0); write_utf8(x, 3..9, "❤❤❤❤"); x"#).unwrap(), "\0\0\0\u{2764}\u{2764}\0".as_bytes());
    assert_eq!(engine.eval::<Blob>(r#"let x = blob(10, 0); write_utf8(x, 3..7, "❤❤❤❤"); x"#).unwrap(), vec![0, 0, 0, 226, 157, 164, 226, 0, 0, 0]);
}
