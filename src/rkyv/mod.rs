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

mod archive;
mod de;
mod ser;

pub use de::{
    from_bytes_owned, from_bytes_owned_generic, from_bytes_owned_unchecked,
    from_bytes_owned_unchecked_generic,
};
pub use ser::to_bytes;
