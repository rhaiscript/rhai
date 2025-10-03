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
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};

    let value = Dynamic::from("hello rkyv");
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert_eq!(value.type_name(), restored.type_name());
    assert_eq!(value.as_string().unwrap(), restored.as_string().unwrap());
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

#[test]
#[cfg(not(feature = "no_index"))]
fn test_rkyv_array_simple() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};
    use rhai::Array;

    let array: Array = vec![1.into(), 2.into(), 3.into()];
    let value = Dynamic::from_array(array.clone());

    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert!(!restored.is_variant(), "restored array should not be Variant");

    let restored_array = restored.into_array().unwrap();
    assert_eq!(restored_array.len(), array.len());

    for (original, restored) in array.iter().zip(restored_array.iter()) {
        assert_eq!(original.as_int().unwrap(), restored.as_int().unwrap());
    }
}

#[test]
#[cfg(not(feature = "no_index"))]
fn test_rkyv_array_nested() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};
    use rhai::Array;

    let inner: Array = vec![Dynamic::from(2), Dynamic::from(3)];
    let array: Array = vec![Dynamic::from(1), Dynamic::from_array(inner.clone()), Dynamic::from(4)];
    
    // Debug: Check what type Dynamic::from(1) creates
    let test_val = Dynamic::from(1);
    println!("test_val: type={}, is_int={}", test_val.type_name(), test_val.is_int());

    // Ensure a standalone nested array roundtrips without loss
    let inner_bytes = to_bytes(&Dynamic::from_array(inner.clone())).unwrap();
    let inner_restored: Dynamic = unsafe { from_bytes_owned_unchecked(&inner_bytes).unwrap() };
    assert!(inner_restored.is_array());

    let value = Dynamic::from_array(array);
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    let mut restored_array = restored.into_array().unwrap();
    println!("restored_array[0]: type={}, value={:?}", restored_array[0].type_name(), restored_array[0]);
    println!("restored_array[1]: type={}, value={:?}", restored_array[1].type_name(), restored_array[1]);
    println!("restored_array[2]: type={}, value={:?}", restored_array[2].type_name(), restored_array[2]);
    assert_eq!(restored_array[0].as_int().unwrap(), 1);
    assert_eq!(restored_array[2].as_int().unwrap(), 4);

    let second = restored_array.remove(1);
    assert!(second.is_array(), "expected array, got {}", second.type_name());
    let nested = second.into_array().unwrap();
    assert_eq!(nested.len(), inner.len());
    assert_eq!(nested[0].as_int().unwrap(), 2);
    assert_eq!(nested[1].as_int().unwrap(), 3);
}

#[test]
#[cfg(not(feature = "no_object"))]
fn test_rkyv_map_simple() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};
    use rhai::Map;

    let mut map: Map = Map::new();
    map.insert("foo".into(), Dynamic::from(42));
    map.insert("bar".into(), Dynamic::from(true));

    let value = Dynamic::from_map(map.clone());
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert!(!restored.is_variant(), "restored map should not be Variant");

    let restored_map = restored.clone().try_cast::<Map>().unwrap();
    assert_eq!(restored_map.len(), map.len());
    assert_eq!(restored_map["foo"].as_int().unwrap(), 42);
    assert_eq!(restored_map["bar"].as_bool().unwrap(), true);
}

#[test]
#[cfg(all(not(feature = "no_object"), not(feature = "no_index")))]
fn test_rkyv_map_with_nested_structures() {
    use rhai::rkyv::{from_bytes_owned_unchecked, to_bytes};
    use rhai::{Array, Map};

    let mut inner_map: Map = Map::new();
    inner_map.insert("nested".into(), Dynamic::from("value"));

    let nested_array: Array = vec![Dynamic::from(1), Dynamic::from(2)];

    let mut map: Map = Map::new();
    map.insert("numbers".into(), Dynamic::from_array(nested_array.clone()));
    map.insert("inner".into(), Dynamic::from_map(inner_map.clone()));

    let value = Dynamic::from_map(map);
    let bytes = to_bytes(&value).unwrap();
    let restored: Dynamic = unsafe { from_bytes_owned_unchecked(&bytes).unwrap() };

    assert!(!restored.is_variant(), "restored nested map should not be Variant");

    let restored_map = restored.try_cast::<Map>().unwrap();

    let restored_numbers = restored_map["numbers"].clone().into_array().unwrap();
    assert_eq!(restored_numbers.len(), nested_array.len());
    assert_eq!(restored_numbers[0].as_int().unwrap(), 1);
    assert_eq!(restored_numbers[1].as_int().unwrap(), 2);

    let restored_inner = restored_map["inner"].clone().try_cast::<Map>().unwrap();
    assert_eq!(restored_inner["nested"].clone().into_immutable_string().unwrap().as_str(), "value");
}
