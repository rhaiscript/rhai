//! Example demonstrating rkyv serialization with Rhai

#![cfg(feature = "rkyv")]

use rhai::{Dynamic, Engine, ImmutableString};

#[cfg(not(feature = "no_object"))]
use rhai::Map;

fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    println!("=== Rhai rkyv Serialization Example ===\n");

    // Example 1: Basic types
    println!("Example 1: Serializing basic types");
    {
        use rhai::rkyv::{from_bytes_owned, to_bytes};

        let value = Dynamic::from(42);
        println!("  Original: {:?}", value);

        let bytes = to_bytes(&value)?;
        println!("  Serialized to {} bytes", bytes.len());

        let restored: Dynamic = from_bytes_owned(&bytes)?;
        println!("  Restored: {:?}", restored);
        println!("  Match: {}\n", value.as_int() == restored.as_int());
    }

    // Example 2: Strings
    println!("Example 2: Serializing strings");
    {
        use rhai::rkyv::{from_bytes_owned, to_bytes};

        let value = Dynamic::from("Hello, rkyv!");
        println!("  Original: {:?}", value);

        let bytes = to_bytes(&value)?;
        println!("  Serialized to {} bytes", bytes.len());

        let restored: Dynamic = from_bytes_owned(&bytes)?;
        println!("  Restored: {:?}\n", restored);
    }

    // Example 3: Script evaluation and caching
    println!("Example 3: Script evaluation with result serialization");
    {
        use rhai::rkyv::{from_bytes_owned, to_bytes};

        let mut engine = Engine::new();

        // Evaluate a script
        let script = "let x = 10; let y = 32; x + y";
        let result: Dynamic = engine.eval(script)?;
        println!("  Script: {}", script);
        println!("  Result: {:?}", result);

        // Serialize the result
        let bytes = to_bytes(&result)?;
        println!("  Serialized result to {} bytes", bytes.len());

        // Deserialize and verify
        let restored: Dynamic = from_bytes_owned(&bytes)?;
        println!("  Restored result: {:?}\n", restored);
    }

    // Example 4: Arrays and nested arrays
    #[cfg(not(feature = "no_index"))]
    {
        println!("Example 4: Serializing arrays and nested arrays");
        use rhai::rkyv::{from_bytes_owned, to_bytes};
        use rhai::Array;

        let nested: Array = vec![Dynamic::from(2), Dynamic::from(3)];
        let array: Array = vec![
            Dynamic::from(1),
            Dynamic::from_array(nested.clone()),
            Dynamic::from(4),
        ];

        let value = Dynamic::from_array(array.clone());
        println!("  Original array: {:?}", value);

        let bytes = to_bytes(&value)?;
        println!("  Serialized to {} bytes", bytes.len());

        let restored: Dynamic = from_bytes_owned(&bytes)?;
        println!("  Restored array: {:?}", restored);
        println!(
            "  Nested check -> {}",
            restored.clone().into_array().unwrap()[1]
                .clone()
                .into_array()
                .unwrap()
                .iter()
                .map(|v| v.as_int().unwrap())
                .collect::<Vec<_>>()
                == vec![2, 3]
        );
        println!();
    }

    // Example 5: Complex maps with nested structures
    #[cfg(not(feature = "no_object"))]
    {
        println!("Example 5: Serializing maps with nested data");
        use rhai::rkyv::{from_bytes_owned, to_bytes};
        #[cfg(not(feature = "no_index"))]
        use rhai::Array;

        let mut map = Map::new();
        map.insert("name".into(), Dynamic::from("Alice"));
        map.insert("age".into(), Dynamic::from(30));
        map.insert("active".into(), Dynamic::from(true));

        #[cfg(not(feature = "no_index"))]
        {
            let favorites: Array = vec![Dynamic::from("reading"), Dynamic::from("hiking")];
            map.insert("favorites".into(), Dynamic::from_array(favorites));
        }

        let value = Dynamic::from(map);
        println!("  Original map: {:?}", value);

        let bytes = to_bytes(&value)?;
        println!("  Serialized to {} bytes", bytes.len());

        let restored: Dynamic = from_bytes_owned(&bytes)?;
        println!("  Restored map: {:?}\n", restored);
    }

    // Example 6: ImmutableString
    println!("Example 6: Serializing ImmutableString directly");
    {
        use rhai::rkyv::{from_bytes_owned, to_bytes};

        let value: ImmutableString = "Direct string serialization".into();
        println!("  Original: {}", value);

        let bytes = to_bytes(&value)?;
        println!("  Serialized to {} bytes", bytes.len());

        let restored: ImmutableString = from_bytes_owned(&bytes)?;
        println!("  Restored: {}", restored);
        println!("  Match: {}\n", value == restored);
    }

    println!("=== Performance Note ===");
    println!("rkyv provides:");
    println!("  • 1.5-3x faster serialization than serde");
    println!("  • 50-100x faster deserialization (zero-copy)");
    println!("  • Lower memory footprint");
    println!("  • Perfect for script caching and state snapshots!");

    Ok(())
}
