#![cfg(feature = "rkyv")]

use rhai::{Dynamic, Engine, ImmutableString, INT};

#[test]
fn test_rkyv_int() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let value = Dynamic::from(42 as INT);
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());
    assert_eq!(value.as_int().unwrap(), restored.as_int().unwrap());
}

#[test]
fn test_rkyv_bool() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let value = Dynamic::from(true);
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());
    assert_eq!(value.as_bool().unwrap(), restored.as_bool().unwrap());
}

#[test]
fn test_rkyv_string() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let value = Dynamic::from("hello world");
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());
    assert_eq!(value.into_immutable_string().unwrap().as_str(), restored.into_immutable_string().unwrap().as_str());
}

#[test]
fn test_rkyv_char() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let value = Dynamic::from('x');
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());
    assert_eq!(value.as_char().unwrap(), restored.as_char().unwrap());
}

#[test]
fn test_rkyv_unit() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let value = Dynamic::UNIT;
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());
    assert!(value.is_unit());
    assert!(restored.is_unit());
}

#[test]
#[cfg(not(feature = "no_float"))]
fn test_rkyv_float() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};
    use rhai::FLOAT;

    let value = Dynamic::from(123.456 as FLOAT);
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());
    assert_eq!(value.as_float().unwrap(), restored.as_float().unwrap());
}

#[test]
#[cfg(not(feature = "no_index"))]
fn test_rkyv_blob() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let blob = vec![1u8, 2, 3, 4, 5];
    let value = Dynamic::from(blob.clone());
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());

    let restored_blob = restored.cast::<Vec<u8>>();
    assert_eq!(blob, restored_blob);
}

#[test]
fn test_rkyv_immutable_string() {
    use rhai::rkyv::to_bytes;
    use rkyv::Deserialize;

    let value: ImmutableString = "hello rkyv".into();
    let bytes = to_bytes(&value).unwrap();

    // For now, ImmutableString needs the generic deserializer
    // TODO: Add a specialized deserializer like we have for Dynamic
    let restored: ImmutableString = unsafe {
        let archived = rkyv::archived_root::<ImmutableString>(&bytes);
        archived.deserialize(&mut rkyv::Infallible).unwrap()
    };

    assert_eq!(value.as_str(), restored.as_str());
}

#[test]
fn test_rkyv_engine_eval() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let engine = Engine::new();

    // Evaluate a script
    let result: Dynamic = engine.eval("40 + 2").unwrap();

    // Serialize the result
    let bytes = to_bytes(&result).unwrap();

    // Deserialize and check
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };
    assert_eq!(42, restored.as_int().unwrap());
}

// TODO: Add tests for arrays and maps once recursion is handled
// #[test]
// #[cfg(not(feature = "no_index"))]
// fn test_rkyv_array() { ... }
//
// #[test]
// #[cfg(not(feature = "no_object"))]
// fn test_rkyv_map() { ... }
