//! Module that defines JSON manipulation functions for [`Engine`].
#![cfg(not(feature = "no_object"))]

use crate::func::native::locked_write;
use crate::parser::{ParseSettingFlags, ParseState};
use crate::tokenizer::{lex_raw, Token};
use crate::types::dynamic::Union;
use crate::types::StringsInterner;
use crate::{Dynamic, Engine, LexError, Map, RhaiResultOf};
use std::fmt::Write;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

impl Engine {
    /// Parse a JSON string into an [object map][Map].
    ///
    /// This is a light-weight alternative to using, say, [`serde_json`](https://crates.io/crates/serde_json)
    /// to deserialize the JSON.
    ///
    /// Not available under `no_object`.
    ///
    /// The JSON string must be an object hash.  It cannot be a simple primitive value.
    ///
    /// Set `has_null` to `true` in order to map `null` values to `()`.
    /// Setting it to `false` causes a syntax error for any `null` value.
    ///
    /// JSON sub-objects are handled transparently.
    ///
    /// This function can be used together with [`format_map_as_json`] to work with JSON texts
    /// without using the [`serde_json`](https://crates.io/crates/serde_json) crate (which is heavy).
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Map};
    ///
    /// let engine = Engine::new();
    ///
    /// let map = engine.parse_json(r#"
    /// {
    ///     "a": 123,
    ///     "b": 42,
    ///     "c": {
    ///         "x": false,
    ///         "y": true,
    ///         "z": '$'
    ///     },
    ///     "d": null
    /// }"#, true)?;
    ///
    /// assert_eq!(map.len(), 4);
    /// assert_eq!(map["a"].as_int().expect("a should exist"), 123);
    /// assert_eq!(map["b"].as_int().expect("b should exist"), 42);
    /// assert_eq!(map["d"].as_unit().expect("d should exist"), ());
    ///
    /// let c = map["c"].read_lock::<Map>().expect("c should exist");
    /// assert_eq!(c["x"].as_bool().expect("x should be bool"), false);
    /// assert_eq!(c["y"].as_bool().expect("y should be bool"), true);
    /// assert_eq!(c["z"].as_char().expect("z should be char"), '$');
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn parse_json(&self, json: impl AsRef<str>, has_null: bool) -> RhaiResultOf<Map> {
        let scripts = [json.as_ref()];

        let (stream, tokenizer_control) = lex_raw(
            self,
            &scripts,
            Some(if has_null {
                &|token, _, _| {
                    match token {
                        // `null` => `()`
                        Token::Reserved(s) if &*s == "null" => Token::Unit,
                        // `{` => `#{`
                        Token::LeftBrace => Token::MapStart,
                        // Disallowed syntax
                        t @ (Token::Unit | Token::MapStart) => Token::LexError(
                            LexError::ImproperSymbol(t.literal_syntax().to_string(), String::new())
                                .into(),
                        ),
                        Token::InterpolatedString(..) => Token::LexError(
                            LexError::ImproperSymbol(
                                "interpolated string".to_string(),
                                String::new(),
                            )
                            .into(),
                        ),
                        // All others
                        _ => token,
                    }
                }
            } else {
                &|token, _, _| {
                    match token {
                        Token::Reserved(s) if &*s == "null" => Token::LexError(
                            LexError::ImproperSymbol("null".to_string(), String::new()).into(),
                        ),
                        // `{` => `#{`
                        Token::LeftBrace => Token::MapStart,
                        // Disallowed syntax
                        t @ (Token::Unit | Token::MapStart) => Token::LexError(
                            LexError::ImproperSymbol(t.literal_syntax().to_string(), String::new())
                                .into(),
                        ),
                        Token::InterpolatedString(..) => Token::LexError(
                            LexError::ImproperSymbol(
                                "interpolated string".to_string(),
                                String::new(),
                            )
                            .into(),
                        ),
                        // All others
                        _ => token,
                    }
                }
            }),
        );

        let ast = {
            let mut interner;
            let mut guard;
            let interned_strings = if let Some(ref interner) = self.interned_strings {
                guard = locked_write(interner);
                &mut *guard
            } else {
                interner = StringsInterner::new();
                &mut interner
            };

            let state = &mut ParseState::new(None, interned_strings, tokenizer_control);

            self.parse_global_expr(
                stream.peekable(),
                state,
                |s| s.flags |= ParseSettingFlags::DISALLOW_UNQUOTED_MAP_PROPERTIES,
                #[cfg(not(feature = "no_optimize"))]
                crate::OptimizationLevel::None,
                #[cfg(feature = "no_optimize")]
                <_>::default(),
            )?
        };

        self.eval_ast(&ast)
    }
}

/// Return the JSON representation of an [object map][Map].
///
/// Not available under `no_std`.
///
/// This function can be used together with [`Engine::parse_json`] to work with JSON texts
/// without using the [`serde_json`](https://crates.io/crates/serde_json) crate (which is heavy).
///
/// # Data types
///
/// Only the following data types should be kept inside the object map: [`INT`][crate::INT],
/// [`FLOAT`][crate::FLOAT], [`ImmutableString`][crate::ImmutableString], `char`, `bool`, `()`,
/// [`Array`][crate::Array], [`Map`].
///
/// # Errors
///
/// Data types not supported by JSON serialize into formats that may invalidate the result.
#[inline]
#[must_use]
pub fn format_map_as_json(map: &Map) -> String {
    let mut result = String::from('{');

    for (key, value) in map {
        if result.len() > 1 {
            result.push(',');
        }

        write!(result, "{key:?}").unwrap();
        result.push(':');

        format_dynamic_as_json(&mut result, value);
    }

    result.push('}');

    result
}

/// Format a [`Dynamic`] value as JSON.
fn format_dynamic_as_json(result: &mut String, value: &Dynamic) {
    match value.0 {
        Union::Unit(..) => result.push_str("null"),
        Union::FnPtr(ref f, _, _) if f.is_curried() => {
            result.push('[');
            write!(result, "{:?}", f.fn_name()).unwrap();
            f.iter_curry().for_each(|value| {
                result.push(',');
                format_dynamic_as_json(result, value);
            });
            result.push(']');
        }
        Union::FnPtr(ref f, _, _) => write!(result, "{:?}", f.fn_name()).unwrap(),
        Union::Map(ref m, ..) => result.push_str(&format_map_as_json(&m)),
        #[cfg(not(feature = "no_index"))]
        Union::Array(ref a, _, _) => {
            result.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                format_dynamic_as_json(result, x);
            }
            result.push(']');
        }
        #[cfg(not(feature = "no_index"))]
        Union::Blob(ref b, _, _) => {
            result.push('[');
            for (i, x) in b.iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                write!(result, "{x}").unwrap();
            }
            result.push(']');
        }
        #[cfg(not(feature = "no_closure"))]
        Union::Shared(ref v, _, _) => format_dynamic_as_json(result, &*crate::func::locked_read(v)),
        _ => write!(result, "{value:?}").unwrap(),
    }
}
