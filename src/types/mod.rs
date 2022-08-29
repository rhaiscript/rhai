//! Module defining Rhai data types.

pub mod custom_types;
pub mod dynamic;
pub mod error;
pub mod fn_ptr;
pub mod immutable_string;
pub mod interner;
pub mod parse_error;
pub mod scope;

pub use custom_types::{CustomTypeInfo, CustomTypesCollection};
pub use dynamic::Dynamic;
#[cfg(not(feature = "no_std"))]
pub use dynamic::Instant;
pub use error::EvalAltResult;
pub use fn_ptr::FnPtr;
pub use immutable_string::ImmutableString;
pub use interner::StringsInterner;
pub use parse_error::{LexError, ParseError, ParseErrorType};
pub use scope::Scope;
