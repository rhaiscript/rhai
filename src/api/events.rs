//! Module that defines public event handlers for [`Engine`].

use crate::func::SendSync;
use crate::VarDefInfo;
use crate::{Dynamic, Engine, EvalContext, Position, RhaiResultOf};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

impl Engine {
    /// Provide a callback that will be invoked before each variable access.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(name: &str, index: usize, context: EvalContext) -> Result<Option<Dynamic>, Box<EvalAltResult>>`
    ///
    /// where:
    /// * `name`: name of the variable.
    /// * `index`: an offset from the bottom of the current [`Scope`][crate::Scope] that the
    ///   variable is supposed to reside. Offsets start from 1, with 1 meaning the last variable in
    ///   the current [`Scope`][crate::Scope].  Essentially the correct variable is at position
    ///   `scope.len() - index`. If `index` is zero, then there is no pre-calculated offset position
    ///   and a search through the current [`Scope`][crate::Scope] must be performed.
    /// * `context`: the current [evaluation context][`EvalContext`].
    ///
    /// ## Return value
    ///
    /// * `Ok(None)`: continue with normal variable access.
    /// * `Ok(Some(Dynamic))`: the variable's value.
    ///
    /// ## Raising errors
    ///
    /// Return `Err(...)` if there is an error.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register a variable resolver.
    /// engine.on_var(|name, _, _| {
    ///     match name {
    ///         "MYSTIC_NUMBER" => Ok(Some(42_i64.into())),
    ///         _ => Ok(None)
    ///     }
    /// });
    ///
    /// engine.eval::<i64>("MYSTIC_NUMBER")?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[inline(always)]
    pub fn on_var(
        &mut self,
        callback: impl Fn(&str, usize, EvalContext) -> RhaiResultOf<Option<Dynamic>>
            + SendSync
            + 'static,
    ) -> &mut Self {
        self.resolve_var = Some(Box::new(callback));
        self
    }
    /// Provide a callback that will be invoked before the definition of each variable .
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(is_runtime: bool, info: VarInfo, context: EvalContext) -> Result<bool, Box<EvalAltResult>>`
    ///
    /// where:
    /// * `is_runtime`: `true` if the variable definition event happens during runtime, `false` if during compilation.
    /// * `info`: information on the variable.
    /// * `context`: the current [evaluation context][`EvalContext`].
    ///
    /// ## Return value
    ///
    /// * `Ok(true)`: continue with normal variable definition.
    /// * `Ok(false)`: deny the variable definition with an [runtime error][crate::EvalAltResult::ErrorRuntime].
    ///
    /// ## Raising errors
    ///
    /// Return `Err(...)` if there is an error.
    ///
    /// # Example
    ///
    /// ```should_panic
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::Engine;
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register a variable definition filter.
    /// engine.on_def_var(|_, info, _| {
    ///     // Disallow defining MYSTIC_NUMBER as a constant
    ///     if info.name() == "MYSTIC_NUMBER" && info.is_const() {
    ///         Ok(false)
    ///     } else {
    ///         Ok(true)
    ///     }
    /// });
    ///
    /// // The following runs fine:
    /// engine.eval::<i64>("let MYSTIC_NUMBER = 42;")?;
    ///
    /// // The following will cause an error:
    /// engine.eval::<i64>("const MYSTIC_NUMBER = 42;")?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[inline(always)]
    pub fn on_def_var(
        &mut self,
        callback: impl Fn(bool, VarDefInfo, EvalContext) -> RhaiResultOf<bool> + SendSync + 'static,
    ) -> &mut Self {
        self.def_var_filter = Some(Box::new(callback));
        self
    }
    /// _(internals)_ Register a callback that will be invoked during parsing to remap certain tokens.
    /// Exported under the `internals` feature only.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(token: Token, pos: Position, state: &TokenizeState) -> Token`
    ///
    /// where:
    /// * [`token`][crate::types::Token]: current token parsed
    /// * [`pos`][`Position`]: location of the token
    /// * [`state`][crate::tokenizer::TokenizeState]: current state of the tokenizer
    ///
    /// ## Raising errors
    ///
    /// It is possible to raise a parsing error by returning
    /// [`Token::LexError`][crate::types::Token::LexError] as the mapped token.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Engine, Token};
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Register a token mapper.
    /// # #[allow(deprecated)]
    /// engine.on_parse_token(|token, _, _| {
    ///     match token {
    ///         // Convert all integer literals to strings
    ///         Token::IntegerConstant(n) => Token::StringConstant(Box::new(n.to_string().into())),
    ///         // Convert 'begin' .. 'end' to '{' .. '}'
    ///         Token::Identifier(s) if &*s == "begin" => Token::LeftBrace,
    ///         Token::Identifier(s) if &*s == "end" => Token::RightBrace,
    ///         // Pass through all other tokens unchanged
    ///         _ => token
    ///     }
    /// });
    ///
    /// assert_eq!(engine.eval::<String>("42")?, "42");
    /// assert_eq!(engine.eval::<bool>("true")?, true);
    /// assert_eq!(engine.eval::<String>("let x = 42; begin let x = 0; end; x")?, "42");
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[cfg(feature = "internals")]
    #[cfg(not(feature = "no_ast"))]
    #[inline(always)]
    pub fn on_parse_token(
        &mut self,
        callback: impl Fn(
                crate::types::Token,
                Position,
                &crate::tokenizer::TokenizeState,
            ) -> crate::types::Token
            + SendSync
            + 'static,
    ) -> &mut Self {
        self.token_mapper = Some(Box::new(callback));
        self
    }
    /// Register a callback for script evaluation progress.
    ///
    /// Not available under `unchecked`.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(counter: u64) -> Option<Dynamic>`
    ///
    /// ## Return value
    ///
    /// * `None`: continue running the script.
    /// * `Some(Dynamic)`: terminate the script with the specified exception value.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use std::sync::RwLock;
    /// # use std::sync::Arc;
    /// use rhai::Engine;
    ///
    /// let result = Arc::new(RwLock::new(0_u64));
    /// let logger = result.clone();
    ///
    /// let mut engine = Engine::new();
    ///
    /// engine.on_progress(move |ops| {
    ///     if ops > 1000 {
    ///         Some("Over 1,000 operations!".into())
    ///     } else if ops % 123 == 0 {
    ///         *logger.write().unwrap() = ops;
    ///         None
    ///     } else {
    ///         None
    ///     }
    /// });
    ///
    /// engine.run("for x in 0..5000 { print(x); }")
    ///       .expect_err("should error");
    ///
    /// assert_eq!(*result.read().unwrap(), 984);
    ///
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(not(feature = "unchecked"))]
    #[inline(always)]
    pub fn on_progress(
        &mut self,
        callback: impl Fn(u64) -> Option<Dynamic> + SendSync + 'static,
    ) -> &mut Self {
        self.progress = Some(Box::new(callback));
        self
    }
    /// Register an [`OperationMeter`][crate::OperationMeter] and a callback that is called only
    /// when the number of operations reaches the meter's deadline.
    ///
    /// This is a cheaper alternative to [`on_progress`][Engine::on_progress], whose callback is
    /// called on every single operation.
    ///
    /// The meter keeps the number of operations of the running evaluation, and the host can move
    /// its deadline at any time, including from inside a native function that wants its own work
    /// to count ([`OperationMeter::charge`][crate::OperationMeter::charge]).
    ///
    /// When the number of operations reaches the deadline, the callback is called with it. Return
    /// `Some(token)` to terminate the script with
    /// [`ErrorTerminated`][crate::EvalAltResult::ErrorTerminated], or `None` to continue. A
    /// callback that returns `None` should move the deadline forward, otherwise it is called again
    /// on the next operation.
    ///
    /// Not available under `unchecked`, or under `sync` on targets without 64-bit atomics.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// use rhai::{Dynamic, Engine, OperationMeter, Shared};
    ///
    /// let meter = Shared::new(OperationMeter::new());
    /// meter.set_deadline(1000);
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Called only once, when the 1000th operation is reached
    /// engine.on_operation_deadline(meter.clone(), |count| Some(Dynamic::from(count)));
    ///
    /// let err = engine.run("let s = 0; for x in 0..5000 { s += x; }").unwrap_err();
    ///
    /// assert!(matches!(*err, rhai::EvalAltResult::ErrorTerminated(..)));
    /// assert_eq!(meter.count(), 1000);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(operation_meter)]
    #[inline(always)]
    pub fn on_operation_deadline(
        &mut self,
        meter: crate::Shared<crate::OperationMeter>,
        callback: impl Fn(u64) -> Option<Dynamic> + SendSync + 'static,
    ) -> &mut Self {
        self.operation_meter = Some((meter, Box::new(callback)));
        self
    }
    /// Override default action of `print` (print to stdout using [`println!`])
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use std::sync::RwLock;
    /// # use std::sync::Arc;
    /// use rhai::Engine;
    ///
    /// let result = Arc::new(RwLock::new(String::new()));
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Override action of 'print' function
    /// let logger = result.clone();
    /// engine.on_print(move |s| logger.write().unwrap().push_str(s));
    ///
    /// engine.run("print(40 + 2);")?;
    ///
    /// assert_eq!(*result.read().unwrap(), "42");
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn on_print(&mut self, callback: impl Fn(&str) + SendSync + 'static) -> &mut Self {
        self.print = Some(Box::new(callback));
        self
    }
    /// Override default action of `debug` (print to stdout using [`println!`])
    ///
    /// # Callback Function Signature
    ///
    /// The callback function signature passed takes the following form:
    ///
    /// `Fn(text: &str, source: Option<&str>, pos: Position)`
    ///
    /// where:
    /// * `text`: the text to display
    /// * `source`: current source, if any
    /// * [`pos`][`Position`]: location of the `debug` call
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use std::sync::RwLock;
    /// # use std::sync::Arc;
    /// use rhai::Engine;
    ///
    /// let result = Arc::new(RwLock::new(String::new()));
    ///
    /// let mut engine = Engine::new();
    ///
    /// // Override action of 'print' function
    /// let logger = result.clone();
    /// engine.on_debug(move |s, src, pos| logger.write().unwrap().push_str(
    ///                     &format!("{} @ {:?} > {}", src.unwrap_or("unknown"), pos, s)
    ///                ));
    ///
    /// let mut ast = engine.compile(r#"let x = "hello"; debug(x);"#)?;
    /// ast.set_source("world");
    /// engine.run_ast(&ast)?;
    ///
    /// #[cfg(not(feature = "no_position"))]
    /// assert_eq!(*result.read().unwrap(), r#"world @ 1:18 > "hello""#);
    /// #[cfg(feature = "no_position")]
    /// assert_eq!(*result.read().unwrap(), r#"world @ none > "hello""#);
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn on_debug(
        &mut self,
        callback: impl Fn(&str, Option<&str>, Position) + SendSync + 'static,
    ) -> &mut Self {
        self.debug = Some(Box::new(callback));
        self
    }
    /// _(internals)_ Register a callback for access to [`Map`][crate::Map] properties that do not exist.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_index`.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(array: &mut Array, index: INT) -> Result<Target, Box<EvalAltResult>>`
    ///
    /// where:
    /// * `array`: mutable reference to the [`Array`][crate::Array] instance.
    /// * `index`: numeric index of the array access.
    ///
    /// ## Return value
    ///
    /// * `Ok(Target)`: [`Target`][crate::Target] of the indexing access.
    ///
    /// ## Raising errors
    ///
    /// Return `Err(...)` if there is an error, usually
    /// [`EvalAltResult::ErrorPropertyNotFound`][crate::EvalAltResult::ErrorPropertyNotFound].
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use rhai::{Engine, Dynamic, EvalAltResult, Position};
    /// # use std::convert::TryInto;
    /// let mut engine = Engine::new();
    ///
    /// engine.on_invalid_array_index(|arr, index, _| match index
    /// {
    ///     -100 => {
    ///         // The array can be modified in place
    ///         arr.push((42_i64).into());
    ///         // Return a mutable reference to an element
    ///         let value_ref = arr.last_mut().unwrap();
    ///         value_ref.try_into()
    ///     }
    ///     100 => {
    ///         let value = Dynamic::from(100_i64);
    ///         // Return a temporary value (not a reference)
    ///         Ok(value.into())
    ///     }
    ///     // Return the standard out-of-bounds error
    ///     _ => Err(EvalAltResult::ErrorArrayBounds(
    ///                 arr.len(), index, Position::NONE
    ///          ).into()),
    /// });
    ///
    /// let r = engine.eval::<i64>("
    ///             let a = [1, 2, 3];
    ///             a[-100] += 1;
    ///             a[3] + a[100]
    ///         ")?;
    ///
    /// assert_eq!(r, 143);
    /// # Ok(()) }
    /// ```
    #[cfg(not(feature = "no_index"))]
    #[cfg(feature = "internals")]
    #[inline(always)]
    pub fn on_invalid_array_index(
        &mut self,
        callback: impl for<'a> Fn(
                &'a mut crate::Array,
                crate::INT,
                EvalContext,
            ) -> RhaiResultOf<crate::Target<'a>>
            + SendSync
            + 'static,
    ) -> &mut Self {
        self.invalid_array_index = Some(Box::new(callback));
        self
    }
    /// _(internals)_ Register a callback for access to [`Map`][crate::Map] properties that do not exist.
    /// Exported under the `internals` feature only.
    ///
    /// Not available under `no_object`.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(map: &mut Map, prop: &str) -> Result<Target, Box<EvalAltResult>>`
    ///
    /// where:
    /// * `map`: mutable reference to the [`Map`][crate::Map] instance.
    /// * `prop`: name of the property that does not exist.
    ///
    /// ## Return value
    ///
    /// * `Ok(Target)`: [`Target`][crate::Target] of the property access.
    ///
    /// ## Raising errors
    ///
    /// Return `Err(...)` if there is an error, usually [`EvalAltResult::ErrorPropertyNotFound`][crate::EvalAltResult::ErrorPropertyNotFound].
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use rhai::{Engine, Dynamic, EvalAltResult, Position};
    /// # use std::convert::TryInto;
    /// let mut engine = Engine::new();
    ///
    /// engine.on_map_missing_property(|map, prop, _| match prop
    /// {
    ///     "x" => {
    ///         // The object-map can be modified in place
    ///         map.insert("y".into(), (42_i64).into());
    ///         // Return a mutable reference to an element
    ///         let value_ref = map.get_mut("y").unwrap();
    ///         value_ref.try_into()
    ///     }
    ///     "z" => {
    ///         // Return a temporary value (not a reference)
    ///         let value = Dynamic::from(100_i64);
    ///         Ok(value.into())
    ///     }
    ///     // Return the standard property-not-found error
    ///     _ => Err(EvalAltResult::ErrorPropertyNotFound(
    ///                 prop.to_string(), Position::NONE
    ///          ).into()),
    /// });
    ///
    /// let r = engine.eval::<i64>("
    ///             let obj = #{ a:1, b:2 };
    ///             obj.x += 1;
    ///             obj.y + obj.z
    ///         ")?;
    ///
    /// assert_eq!(r, 143);
    /// # Ok(()) }
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[cfg(feature = "internals")]
    #[inline(always)]
    pub fn on_map_missing_property(
        &mut self,
        callback: impl for<'a> Fn(&'a mut crate::Map, &str, EvalContext) -> RhaiResultOf<crate::Target<'a>>
            + SendSync
            + 'static,
    ) -> &mut Self {
        self.missing_map_property = Some(Box::new(callback));
        self
    }
    /// _(internals)_ Register a callback for when a function call is not found.
    /// Exported under the `internals` feature only.
    ///
    /// Not triggered for qualified function calls (e.g. `module::function()`).
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(name: &str, args: &mut [&mut Dynamic], is_method_call: bool, context: EvalContext) -> RhaiResultOf<Option<Dynamic>>`
    ///
    /// where:
    /// * `name`: name of the function being called.
    /// * `args`: mutable reference to the argument list. For method calls, the first element is the object.
    /// * `is_method_call`: `true` if this is a method-style call (`obj.method()`), `false` for a function-style call (`func()`).
    /// * `context`: the current evaluation context.
    ///
    /// ## Return value
    ///
    /// * `Ok(Some(value))`: provide a return value as the result of the function call.
    /// * `Ok(None)`: fall through to the standard [`EvalAltResult::ErrorFunctionNotFound`][crate::EvalAltResult::ErrorFunctionNotFound].
    ///
    /// ## Raising errors
    ///
    /// Return `Err(...)` if there is an error.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # use rhai::{Engine, Dynamic};
    /// let mut engine = Engine::new();
    ///
    /// #[allow(deprecated)]
    /// engine.on_missing_function(|name, args, _is_method_call, _ctx| {
    ///     if name == "greet" {
    ///         Ok(Some(Dynamic::from("hello")))
    ///     } else {
    ///         Ok(None) // fall through to standard error
    ///     }
    /// });
    ///
    /// # #[cfg(not(feature = "no_object"))]
    /// let result = engine.eval::<String>(r#"let x = 42; x.greet()"#)?;
    /// # #[cfg(feature = "no_object")]
    /// # let result = engine.eval::<String>(r#"greet(42)"#)?;
    /// assert_eq!(result, "hello");
    /// # Ok(()) }
    /// ```
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[cfg(feature = "internals")]
    #[inline(always)]
    pub fn on_missing_function(
        &mut self,
        callback: impl Fn(&str, &mut [&mut Dynamic], bool, EvalContext) -> RhaiResultOf<Option<Dynamic>>
            + SendSync
            + 'static,
    ) -> &mut Self {
        self.missing_function = Some(Box::new(callback));
        self
    }
    /// _(debugging)_ Register a callback for debugging.
    /// Exported under the `debugging` feature only.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[cfg(feature = "debugging")]
    #[cfg(not(feature = "no_ast"))]
    #[inline(always)]
    pub fn register_debugger(
        &mut self,
        init: impl Fn(&Self, crate::debugger::Debugger) -> crate::debugger::Debugger
            + SendSync
            + 'static,
        callback: impl Fn(
                EvalContext,
                crate::eval::DebuggerEvent,
                crate::ast::ASTNode,
                Option<&str>,
                Position,
            ) -> RhaiResultOf<crate::eval::DebuggerCommand>
            + SendSync
            + 'static,
    ) -> &mut Self {
        self.debugger_interface = Some((Box::new(init), Box::new(callback)));
        self
    }
}
