//! _(rkyv)_ Zero-copy serialization and deserialization support for [`rkyv`](https://crates.io/crates/rkyv).
//! Exported under the `rkyv` feature only.
//!
//! # Overview
//!
//! `rkyv` provides high-performance binary serialization with zero-copy deserialization.
//! This is ideal for performance-critical scenarios like:
//!
//! * Script caching - Load compiled scripts 50-100x faster
//! * State snapshots - Save/restore engine state with minimal overhead
//! * Embedded systems - Lower memory footprint
//! * Large data structures - Access without full deserialization
//!
//! # Supported Types
//!
//! | Category | Types |
//! | --- | --- |
//! | Scalars | `INT`, `bool`, `char`, `()` |
//! | Strings | `ImmutableString`, `String` |
//! | Numbers | `FLOAT` (when the `no_float` feature is disabled) |
//! | Binary  | `Blob` (when the `no_index` feature is disabled) |
//! | Collections | `Array`, `Map` (feature-gated; nested values are archived recursively) |
//!
//! Arrays and maps store each nested [`Dynamic`] as an embedded rkyv blob. This keeps the
//! serialization pipeline zero-copy friendly while avoiding recursive derive limitations in
//! rkyv 0.7. Nested collections (arrays of arrays, maps-of-maps) are fully supported.
//!
//! # When to Use
//!
//! Use `rkyv` when you need:
//! - Maximum deserialization performance
//! - Zero-copy data access
//! - Binary format (smaller, faster)
//! - Internal Rust-to-Rust communication
//!
//! Use [`serde`](../serde/index.html) when you need:
//! - JSON, YAML, TOML support
//! - Cross-language interoperability
//! - Human-readable formats
//! - Web API integration
//!
//! # Example
//!
//! ```ignore
//! use rhai::{Dynamic, Engine};
//! use rhai::rkyv::{to_bytes, from_bytes_owned};
//!
//! let mut engine = Engine::new();
//!
//! // Create a value
//! let value = Dynamic::from(42);
//!
//! // Serialize to bytes
//! let bytes = to_bytes(&value)?;
//!
//! // Deserialize back
//! let restored: Dynamic = from_bytes_owned(&bytes)?;
//!
//! assert_eq!(value, restored);
//! # Ok::<_, Box<rhai::EvalAltResult>>(())
//! ```
//!
//! # API Compatibility with serde
//!
//! For users familiar with [`serde`](../serde/index.html), similar function names are provided:
//!
//! ```ignore
//! use rhai::rkyv::{to_dynamic, from_dynamic};
//!
//! // Serialize (same as to_bytes)
//! let bytes = to_dynamic(&value)?;
//!
//! // Deserialize (same as from_bytes_owned)
//! let value: Dynamic = from_dynamic(&bytes)?;
//! ```
//!
//! Note: Unlike serde's `to_dynamic`/`from_dynamic` which work with Dynamic values,
//! rkyv's functions work with byte arrays for zero-copy serialization.

mod archive;
mod de;
mod ser;

pub use de::{
    from_bytes_owned, from_bytes_owned_generic, from_bytes_owned_unchecked,
    from_bytes_owned_unchecked_generic,
};
pub use ser::to_bytes;

// API compatibility aliases matching serde naming convention
// Note: These work with bytes, not Dynamic, unlike serde's versions
pub use de::from_bytes_owned as from_dynamic;
pub use ser::to_bytes as to_dynamic;
