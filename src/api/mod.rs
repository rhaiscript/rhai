//! Module defining the public API of the Rhai engine.

pub mod eval;

pub mod run;

pub mod compile;

pub mod json;

pub mod files;

pub mod register;

pub mod call_fn;

pub mod options;

pub mod optimize;

pub mod limits;

pub mod events;

pub mod formatting;

pub mod custom_syntax;

pub mod build_type;

#[cfg(feature = "metadata")]
pub mod definitions;

pub mod deprecated;

use crate::{Dynamic, Engine, Identifier};

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Default limits.
pub mod default_limits {
    #[cfg(not(feature = "unchecked"))]
    #[cfg(feature = "internals")]
    pub use super::limits::default_limits::*;

    /// Maximum number of parameters in functions with [`Dynamic`][crate::Dynamic] support.
    pub const MAX_DYNAMIC_PARAMETERS: usize = 16;
}

impl Engine {
    /// The module resolution service used by the [`Engine`].
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    #[must_use]
    pub fn module_resolver(&self) -> &dyn crate::ModuleResolver {
        static DUMMY_RESOLVER: crate::module::resolvers::DummyModuleResolver =
            crate::module::resolvers::DummyModuleResolver;

        self.module_resolver.as_deref().unwrap_or(&DUMMY_RESOLVER)
    }

    /// Set the module resolution service used by the [`Engine`].
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[inline(always)]
    pub fn set_module_resolver(
        &mut self,
        resolver: impl crate::ModuleResolver + 'static,
    ) -> &mut Self {
        self.module_resolver = Some(Box::new(resolver));
        self
    }

    /// Disable a particular keyword or operator in the language.
    ///
    /// # Examples
    ///
    /// The following will raise an error during parsing because the `if` keyword is disabled and is
    /// recognized as a reserved symbol!
    ///
    /// ```rust,should_panic
    /// # fn main() -> Result<(), rhai::ParseError> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// engine.disable_symbol("if");    // disable the 'if' keyword
    ///
    /// engine.compile("let x = if true { 42 } else { 0 };")?;
    /// //                      ^ 'if' is rejected as a reserved symbol
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The following will raise an error during parsing because the `+=` operator is disabled.
    ///
    /// ```rust,should_panic
    /// # fn main() -> Result<(), rhai::ParseError> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// engine.disable_symbol("+=");    // disable the '+=' operator
    ///
    /// engine.compile("let x = 42; x += 1;")?;
    /// //                            ^ unknown operator
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn disable_symbol(&mut self, symbol: impl Into<Identifier>) -> &mut Self {
        self.disabled_symbols.insert(symbol.into());
        self
    }

    /// Is a particular keyword or operator disabled?
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// engine.disable_symbol("if");    // disable the 'if' keyword
    ///
    /// assert!(engine.is_symbol_disabled("if"));
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn is_symbol_disabled(&self, symbol: &str) -> bool {
        self.disabled_symbols.contains(symbol)
    }

    /// Register a custom operator with a precedence into the language.
    ///
    /// Not available under `no_custom_syntax`.
    ///
    /// The operator can be a valid identifier, a reserved symbol, a disabled operator or a disabled keyword.
    ///
    /// The precedence cannot be zero.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register a custom operator called '#' and give it
    /// // a precedence of 160 (i.e. between +|- and *|/).
    /// engine.register_custom_operator("#", 160).expect("should succeed");
    ///
    /// // Register a binary function named '#'
    /// engine.register_fn("#", |x: i64, y: i64| (x * y) - (x + y));
    ///
    /// assert_eq!(
    ///     engine.eval_expression::<i64>("1 + 2 * 3 # 4 - 5 / 6")?,
    ///     15
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "no_custom_syntax"))]
    pub fn register_custom_operator(
        &mut self,
        keyword: impl AsRef<str>,
        precedence: u8,
    ) -> Result<&mut Self, String> {
        use crate::tokenizer::Token;

        let precedence = crate::engine::Precedence::new(precedence)
            .ok_or_else(|| "precedence cannot be zero".to_string())?;

        let keyword = keyword.as_ref();

        match Token::lookup_symbol_from_syntax(keyword) {
            // Standard identifiers and reserved keywords are OK
            None | Some(Token::Reserved(..)) => (),
            // custom keywords are OK
            #[cfg(not(feature = "no_custom_syntax"))]
            Some(Token::Custom(..)) => (),
            // Active standard keywords cannot be made custom
            // Disabled keywords are OK
            Some(token)
                if token.is_standard_keyword()
                    && !self.is_symbol_disabled(token.literal_syntax()) =>
            {
                return Err(format!("'{keyword}' is a reserved keyword"))
            }
            // Active standard symbols cannot be made custom
            Some(token)
                if token.is_standard_symbol()
                    && !self.is_symbol_disabled(token.literal_syntax()) =>
            {
                return Err(format!("'{keyword}' is a reserved operator"))
            }
            // Active standard symbols cannot be made custom
            Some(token) if !self.is_symbol_disabled(token.literal_syntax()) => {
                return Err(format!("'{keyword}' is a reserved symbol"))
            }
            // Disabled symbols are OK
            Some(_) => (),
        }

        // Add to custom keywords
        self.custom_keywords
            .insert(keyword.into(), Some(precedence));

        Ok(self)
    }

    /// Get the default value of the custom state for each evaluation run.
    #[inline(always)]
    pub const fn default_tag(&self) -> &Dynamic {
        &self.def_tag
    }
    /// Get a mutable reference to the default value of the custom state for each evaluation run.
    #[inline(always)]
    pub fn default_tag_mut(&mut self) -> &mut Dynamic {
        &mut self.def_tag
    }
    /// Set the default value of the custom state for each evaluation run.
    #[inline(always)]
    pub fn set_default_tag(&mut self, value: impl Into<Dynamic>) -> &mut Self {
        self.def_tag = value.into();
        self
    }
}
