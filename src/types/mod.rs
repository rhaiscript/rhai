//! Module defining Rhai data types.

pub mod bloom_filter;
pub mod custom_types;
pub mod dynamic;
pub mod error;
pub mod float;
pub mod fn_ptr;
pub mod ident;
pub mod immutable_string;
pub mod interner;
pub mod parse_error;
pub mod position;
pub mod position_none;
pub mod scope;
pub mod token;
#[cfg(not(feature = "no_ast"))]
pub mod var_def;
pub mod variant;

pub use bloom_filter::BloomFilterU64;
pub use custom_types::{CustomTypeInfo, CustomTypesCollection};
pub use dynamic::Dynamic;
#[cfg(not(feature = "no_time"))]
pub use dynamic::Instant;
pub use error::EvalAltResult;
#[cfg(not(feature = "no_float"))]
pub use float::FloatWrapper;
pub use fn_ptr::FnPtr;
pub use ident::Ident;
pub use immutable_string::ImmutableString;
pub use interner::StringsInterner;
pub use parse_error::{LexError, ParseError, ParseErrorType};
pub use token::Token;
#[cfg(not(feature = "no_ast"))]
pub use var_def::VarDefInfo;

#[cfg(not(feature = "no_position"))]
pub use position::{Position, Span};
#[cfg(feature = "no_position")]
pub use position_none::{Position, Span};

pub use scope::Scope;
pub use variant::Variant;
