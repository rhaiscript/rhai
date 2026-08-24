//! The `FnPtr` type.

use crate::func::FnCallArgs;
use crate::tokenizer::{is_reserved_keyword_or_symbol, is_valid_function_name, Token};
use crate::types::dynamic::Variant;
use crate::{
    expose_under_internals, Dynamic, Engine, FnArgsVec, FuncArgs, ImmutableString,
    NativeCallContext, Position, RhaiError, RhaiResult, RhaiResultOf, Shared, StaticVec, ThinVec,
    AST, ERR, PERR,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::type_name,
    convert::{TryFrom, TryInto},
    fmt, mem,
    ops::{Index, IndexMut},
};

/// Function pointer type.
#[derive(Clone, Default)]
pub enum FnPtrType {
    /// Normal function pointer.
    #[default]
    Normal,
    /// Pre-calculated hash of a script-defined function.
    #[cfg(not(feature = "no_function"))]
    Script { num_params: usize, hash: u64 },
    /// Embedded native Rust function.
    #[cfg(not(feature = "sync"))]
    Native(Shared<dyn Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult + 'static>),
    #[cfg(feature = "sync")]
    Native(
        Shared<dyn Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult + Send + Sync + 'static>,
    ),
}

impl fmt::Display for FnPtrType {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => f.write_str("Fn"),
            #[cfg(not(feature = "no_function"))]
            Self::Script { .. } => f.write_str("Fn*"),
            Self::Native(..) => f.write_str("Fn"),
        }
    }
}

impl FnPtrType {
    /// Get a shared reference to the script-defined function in the [`GlobalRuntimeState`][crate::eval::GlobalRuntimeState]
    /// if one is linked to this [`FnPtr`].
    #[cfg(not(feature = "no_function"))]
    #[inline]
    #[must_use]
    pub fn get_linked_script<'a>(
        &self,
        global: &'a crate::eval::GlobalRuntimeState,
        num_args: usize,
    ) -> Option<&'a Shared<crate::ast::ScriptFuncDef>> {
        match self {
            Self::Script { num_params, hash } if *num_params == num_args => {
                global.lib[0].get_script_fn_by_hash(*hash)
            }
            _ => None,
        }
    }
}

/// A general function pointer, which may carry additional (i.e. curried) argument values
/// to be passed onto a function during a call.
#[derive(Clone)]
pub struct FnPtr {
    /// Name of the function.
    pub(crate) name: ImmutableString,
    /// Curried arguments.
    pub(crate) curry: ThinVec<Dynamic>,
    /// Encapsulated environment.
    #[cfg(not(feature = "no_function"))]
    pub(crate) env: Option<Shared<crate::ast::EncapsulatedEnviron>>,
    /// Type of function pointer.
    pub(crate) typ: FnPtrType,
}

impl fmt::Display for FnPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fn({})", self.fn_name())
    }
}

impl fmt::Debug for FnPtr {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            #[cfg(not(feature = "no_function"))]
            _ if self.env.is_some() => format!("{}+", self.typ),
            _ => self.typ.to_string(),
        };
        let ff = &mut f.debug_tuple(&name);
        ff.field(&self.name);
        self.curry.iter().for_each(|curry| {
            ff.field(curry);
        });
        ff.finish()?;

        Ok(())
    }
}

impl FnPtr {
    /// Create a new function pointer.
    ///
    /// # Errors
    ///
    /// Returns an error if the function name is not a valid identifier or is a reserved keyword.
    #[inline(always)]
    pub fn new(name: impl Into<ImmutableString>) -> RhaiResultOf<Self> {
        name.into().try_into()
    }
    /// Create a new function pointer from a native Rust function.
    ///
    /// # Errors
    ///
    /// Returns an error if the function name is not a valid identifier or is a reserved keyword.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(context: NativeCallContext, &mut [&mut Dynamic]) -> Result<Dynamic, Box<EvalAltResult>>`
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[inline(always)]
    pub fn from_fn(
        name: impl Into<ImmutableString>,
        #[cfg(not(feature = "sync"))] func: impl Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult
            + 'static,
        #[cfg(feature = "sync")] func: impl Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult
            + Send
            + Sync
            + 'static,
    ) -> RhaiResultOf<Self> {
        #[allow(deprecated)]
        Self::from_dyn_fn(name, Box::new(func))
    }
    /// Create a new function pointer from a native Rust function.
    ///
    /// # Errors
    ///
    /// Returns an error if the function name is not a valid identifier or is a reserved keyword.
    ///
    /// # WARNING - Unstable API
    ///
    /// This API is volatile and may change in the future.
    ///
    /// # Callback Function Signature
    ///
    /// `Fn(context: NativeCallContext, &mut [&mut Dynamic]) -> Result<Dynamic, Box<EvalAltResult>>`
    #[deprecated = "This API is NOT deprecated, but it is considered volatile and may change in the future."]
    #[inline]
    pub fn from_dyn_fn(
        name: impl Into<ImmutableString>,
        #[cfg(not(feature = "sync"))] func: Box<
            dyn Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult + 'static,
        >,
        #[cfg(feature = "sync")] func: Box<
            dyn Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResult + Send + Sync + 'static,
        >,
    ) -> RhaiResultOf<Self> {
        let mut fp = Self::new(name)?;
        fp.typ = FnPtrType::Native(Shared::new(func));
        Ok(fp)
    }

    /// Get the name of the function.
    #[inline(always)]
    #[must_use]
    pub fn fn_name(&self) -> &str {
        self.fn_name_raw()
    }
    /// Get the name of the function.
    #[inline(always)]
    #[must_use]
    pub(crate) const fn fn_name_raw(&self) -> &ImmutableString {
        &self.name
    }
    /// Get the curried arguments.
    #[inline(always)]
    pub fn curry(&self) -> &[Dynamic] {
        self.curry.as_ref()
    }
    /// Iterate the curried arguments.
    #[inline(always)]
    pub fn iter_curry(&self) -> impl Iterator<Item = &Dynamic> {
        self.curry.iter()
    }
    /// Mutably-iterate the curried arguments.
    #[inline(always)]
    pub fn iter_curry_mut(&mut self) -> impl Iterator<Item = &mut Dynamic> {
        self.curry.iter_mut()
    }
    /// Add a new curried argument.
    #[inline(always)]
    pub fn add_curry(&mut self, value: Dynamic) -> &mut Self {
        self.curry.push(value);
        self
    }
    /// Set curried arguments to the function pointer.
    #[inline]
    pub fn set_curry(&mut self, values: impl IntoIterator<Item = Dynamic>) -> &mut Self {
        self.curry = values.into_iter().collect();
        self
    }
    /// Is the function pointer curried?
    #[inline(always)]
    #[must_use]
    pub fn is_curried(&self) -> bool {
        !self.curry.is_empty()
    }
    /// Does the function pointer refer to an anonymous function?
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub fn is_anonymous(&self) -> bool {
        crate::func::is_anonymous_fn(&self.name)
    }
    /// Call the function pointer with curried arguments (if any).
    /// The function may be script-defined (not available under `no_function`) or native Rust.
    ///
    /// This method is intended for calling a function pointer directly, possibly on another [`Engine`].
    /// Therefore, the [`AST`] is _NOT_ evaluated before calling the function.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() -> Result<(), Box<rhai::EvalAltResult>> {
    /// # #[cfg(not(feature = "no_function"))]
    /// # {
    /// use rhai::{Engine, FnPtr};
    ///
    /// let engine = Engine::new();
    ///
    /// let ast = engine.compile("fn foo(x, y) { len(x) + y }")?;
    ///
    /// let mut fn_ptr = FnPtr::new("foo")?;
    ///
    /// // Curry values into the function pointer
    /// fn_ptr.set_curry(vec!["abc".into()]);
    ///
    /// // Values are only needed for non-curried parameters
    /// let result: i64 = fn_ptr.call(&engine, &ast, ( 39_i64, ) )?;
    ///
    /// assert_eq!(result, 42);
    /// # }
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn call<T: Variant + Clone>(
        &self,
        engine: &Engine,
        ast: &AST,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let _ast = ast;
        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        let global = &mut engine.new_global_runtime_state();

        #[cfg(not(feature = "no_function"))]
        global.lib.push(_ast.shared_lib().clone());

        let ctx = (engine, self.fn_name(), None, &*global, Position::NONE).into();

        self.call_raw(&ctx, None, arg_values).and_then(|result| {
            result.try_cast_result().map_err(|r| {
                let result_type = engine.map_type_name(r.type_name());
                let cast_type = match type_name::<T>() {
                    typ if typ.contains("::") => engine.map_type_name(typ),
                    typ => typ,
                };
                ERR::ErrorMismatchOutputType(cast_type.into(), result_type.into(), Position::NONE)
                    .into()
            })
        })
    }
    /// Call the function pointer as a _method_ call with a bound `this` pointer and curried arguments (if any).
    /// The function may be script-defined (not available under `no_function`) or native Rust.
    ///
    /// Not available under `no_object`.
    ///
    /// This method is intended for calling a function pointer directly, possibly on another [`Engine`].
    /// Therefore, the [`AST`] is _NOT_ evaluated before calling the function.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    pub fn call_as_method<T: Variant + Clone>(
        &self,
        engine: &Engine,
        ast: &AST,
        this_ptr: &mut Dynamic,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let _ast = ast;
        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        let global = &mut engine.new_global_runtime_state();

        #[cfg(not(feature = "no_function"))]
        global.lib.push(_ast.shared_lib().clone());

        let ctx = (engine, self.fn_name(), None, &*global, Position::NONE).into();

        self.call_raw(&ctx, Some(this_ptr), arg_values)
            .and_then(|result| {
                result.try_cast_result().map_err(|r| {
                    let result_type = engine.map_type_name(r.type_name());
                    let cast_type = match type_name::<T>() {
                        typ if typ.contains("::") => engine.map_type_name(typ),
                        typ => typ,
                    };
                    ERR::ErrorMismatchOutputType(
                        cast_type.into(),
                        result_type.into(),
                        Position::NONE,
                    )
                    .into()
                })
            })
    }
    /// Call the function pointer with curried arguments (if any).
    /// The function may be script-defined (not available under `no_function`) or native Rust.
    ///
    /// This method is intended for calling a function pointer that is passed into a native Rust
    /// function as an argument.  Therefore, the [`AST`] is _NOT_ evaluated before calling the
    /// function.
    #[inline]
    pub fn call_within_context<T: Variant + Clone>(
        &self,
        context: &NativeCallContext,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        self.call_raw(context, None, arg_values).and_then(|result| {
            result.try_cast_result().map_err(|r| {
                let result_type = context.engine().map_type_name(r.type_name());
                let cast_type = match type_name::<T>() {
                    typ if typ.contains("::") => context.engine().map_type_name(typ),
                    typ => typ,
                };
                ERR::ErrorMismatchOutputType(cast_type.into(), result_type.into(), Position::NONE)
                    .into()
            })
        })
    }
    /// Call the function pointer as a _method_ call with a bound `this` pointer and curried arguments (if any).
    /// The function may be script-defined (not available under `no_function`) or native Rust.
    ///
    /// Not available under `no_object`.
    ///
    /// This method is intended for calling a function pointer that is passed into a native Rust
    /// function as an argument.  Therefore, the [`AST`] is _NOT_ evaluated before calling the
    /// function.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    pub fn call_as_method_within_context<T: Variant + Clone>(
        &self,
        context: &NativeCallContext,
        this_ptr: &mut Dynamic,
        args: impl FuncArgs,
    ) -> RhaiResultOf<T> {
        let mut arg_values = StaticVec::new_const();
        args.parse(&mut arg_values);

        self.call_raw(context, Some(this_ptr), arg_values)
            .and_then(|result| {
                result.try_cast_result().map_err(|r| {
                    let result_type = context.engine().map_type_name(r.type_name());
                    let cast_type = match type_name::<T>() {
                        typ if typ.contains("::") => context.engine().map_type_name(typ),
                        typ => typ,
                    };
                    ERR::ErrorMismatchOutputType(
                        cast_type.into(),
                        result_type.into(),
                        Position::NONE,
                    )
                    .into()
                })
            })
    }
    /// Call the function pointer with curried arguments (if any).
    /// The function may be script-defined (not available under `no_function`) or native Rust.
    ///
    /// This method is intended for calling a function pointer that is passed into a native Rust
    /// function as an argument.  Therefore, the [`AST`] is _NOT_ evaluated before calling the
    /// function.
    ///
    /// # WARNING - Low Level API
    ///
    /// This function is very low level.
    ///
    /// # Arguments
    ///
    /// All the arguments are _consumed_, meaning that they're replaced by `()`.
    /// This is to avoid unnecessarily cloning the arguments.
    ///
    /// Do not use the arguments after this call. If they are needed afterwards,
    /// clone them _before_ calling this function.
    #[inline]
    pub fn call_raw(
        &self,
        context: &NativeCallContext,
        this_ptr: Option<&mut Dynamic>,
        arg_values: impl AsMut<[Dynamic]>,
    ) -> RhaiResult {
        let global = context.global_runtime_state();
        let mut this_ptr = this_ptr;
        let mut arg_values = arg_values;
        let mut arg_values = arg_values.as_mut();
        let mut args_data;

        if self.is_curried() {
            args_data = FnArgsVec::with_capacity(self.curry().len() + arg_values.len());
            args_data.extend(self.curry().iter().cloned());
            args_data.extend(arg_values.iter_mut().map(mem::take));
            arg_values = &mut *args_data;
        }

        // Linked to scripted function?
        #[cfg(not(feature = "no_function"))]
        if let Some(fn_def) = self.typ.get_linked_script(global, arg_values.len()) {
            let args = &mut StaticVec::with_capacity(arg_values.len() + 1);
            args.extend(arg_values.iter_mut());

            let global = &mut global.clone();
            global.level += 1;

            return context.engine().call_script_fn(
                global,
                &mut crate::eval::Caches::new(),
                &mut crate::Scope::new(),
                this_ptr,
                #[cfg(not(feature = "no_function"))]
                self.env.as_deref(),
                #[cfg(feature = "no_function")]
                None,
                fn_def,
                args,
                true,
                context.call_position(),
            );
        }

        // Embedded native Rust function?
        match self.typ {
            FnPtrType::Native(ref func) => {
                let mut cloned_this_ptr;
                let args = &mut StaticVec::with_capacity(arg_values.len() + 1);
                args.extend(arg_values.iter_mut());

                let global = &mut global.clone();
                global.level += 1;
                let engine = context.engine();
                let pos = context.call_position();
                if let Some(this_ptr) = this_ptr {
                    if self.is_curried() {
                        cloned_this_ptr = this_ptr.clone();
                        // Arguments order is: curry + this_ptr (cloned) + args
                        args.insert(self.curry().len(), &mut cloned_this_ptr);
                    } else {
                        // Arguments order is: this_ptr (&mut) + args
                        args.insert(0, this_ptr);
                    }
                }

                let context = (engine, self.fn_name(), None, &*global, pos).into();

                return func(context, args)
                    .and_then(|r| engine.check_data_size(r, pos))
                    .map_err(|err| err.fill_position(pos));
            }
            _ => (),
        }

        // Curried method call must check where the `this_ptr` is inserted into the arguments.
        // For script-defined functions, `this_ptr` is always the first argument,
        // but for native Rust functions, it goes after the curried arguments.
        if let Some(this_ptr) = this_ptr.as_deref_mut() {
            let curry_len = self.curry().len();
            if curry_len > 0 {
                // Check if a script-defined function exists with the name and number of parameters.
                // If not, then it'll hit a native Rust function.
                #[cfg(not(feature = "no_function"))]
                let is_native = {
                    let hash_script = crate::calc_fn_hash(None, self.fn_name(), arg_values.len());
                    let mut caches = crate::eval::Caches::new();
                    !context
                        .engine()
                        .has_script_fn(global, &mut caches, hash_script)
                };
                // No script-defined functions under `no_function`.
                #[cfg(feature = "no_function")]
                let is_native = true;

                // Native Rust function, insert `this_ptr` after the curried arguments.
                // Arguments order is: curry + this_ptr (cloned) + args
                if is_native {
                    let mut args_data = FnArgsVec::with_capacity(arg_values.len() + 1);
                    args_data.extend(arg_values[..curry_len].iter_mut().map(mem::take));
                    args_data.push(this_ptr.clone());
                    args_data.extend(arg_values[curry_len..].iter_mut().map(mem::take));
                    let args = &mut args_data.iter_mut().collect::<FnArgsVec<_>>();
                    return context.call_native_fn_raw(self.fn_name(), false, args);
                }
            }
        }

        // Go through normal dispatch otherwise.
        let args = &mut StaticVec::with_capacity(arg_values.len() + 1);
        args.extend(arg_values.iter_mut());

        let is_method = this_ptr.is_some();

        if let Some(this_ptr) = this_ptr {
            args.insert(0, this_ptr);
        }

        context.call_fn_raw(self.fn_name(), is_method, is_method, args)
    }

    /// _(internals)_ Make a call to a function pointer with either a specified number of arguments,
    /// or with extra arguments attached.
    /// Exported under the `internals` feature only.
    ///
    /// If `this_ptr` is provided, it is first provided to script-defined functions bound to `this`.
    ///
    /// When an appropriate function is not found and `move_this_ptr_to_args` is `Some`, `this_ptr`
    /// is removed and inserted as the appropriate parameter number.
    ///
    /// This is useful for calling predicate closures within an iteration loop where the extra
    /// argument is the current element's index.
    ///
    /// If the function pointer is linked to a scripted function definition, use the appropriate
    /// number of arguments to call it directly (one version attaches extra arguments).
    #[expose_under_internals]
    #[inline(always)]
    fn call_raw_with_extra_args<const N: usize, const E: usize>(
        &self,
        caller_fn: &str,
        ctx: &NativeCallContext,
        this_ptr: Option<&mut Dynamic>,
        args: [Dynamic; N],
        extras: [Dynamic; E],
        move_this_ptr_to_args: Option<usize>,
    ) -> RhaiResult {
        match move_this_ptr_to_args {
            Some(m) => {
                self._call_with_extra_args::<true, N, E>(caller_fn, ctx, this_ptr, args, extras, m)
            }
            None => {
                self._call_with_extra_args::<false, N, E>(caller_fn, ctx, this_ptr, args, extras, 0)
            }
        }
    }
    /// Make a call to a function pointer with either a specified number of arguments, or with extra
    /// arguments attached.
    fn _call_with_extra_args<const MOVE_PTR: bool, const N: usize, const E: usize>(
        &self,
        caller_fn: &str,
        ctx: &NativeCallContext,
        mut this_ptr: Option<&mut Dynamic>,
        args: [Dynamic; N],
        extras: [Dynamic; E],
        move_this_ptr_to_args: usize,
    ) -> RhaiResult {
        // Wrap the error in an `ErrorInFunctionCall`
        fn wrap_err(result: RhaiResult, ctx: &NativeCallContext, caller_fn: &str) -> RhaiResult {
            result.map_err(|err| {
                Box::new(ERR::ErrorInFunctionCall(
                    caller_fn.to_string(),
                    ctx.call_source().unwrap_or("").to_string(),
                    err,
                    Position::NONE,
                ))
            })
        }

        match self.typ {
            #[cfg(not(feature = "no_function"))]
            FnPtrType::Script { num_params, .. } => {
                if num_params == N + self.curry().len() {
                    let result = self.call_raw(ctx, this_ptr, args);
                    return wrap_err(result, ctx, caller_fn);
                }
                if MOVE_PTR {
                    if let Some(this_ptr) = this_ptr.as_deref() {
                        if num_params == N + 1 + self.curry().len() {
                            let mut args2 = FnArgsVec::with_capacity(args.len() + 1);
                            if move_this_ptr_to_args == 0 {
                                args2.push(this_ptr.clone());
                                args2.extend(args);
                            } else {
                                args2.extend(args);
                                args2.insert(move_this_ptr_to_args, this_ptr.clone());
                            }
                            let result = self.call_raw(ctx, None, args2);
                            return wrap_err(result, ctx, caller_fn);
                        }
                        if num_params == N + E + 1 + self.curry().len() {
                            let mut args2 = FnArgsVec::with_capacity(args.len() + extras.len() + 1);
                            if move_this_ptr_to_args == 0 {
                                args2.push(this_ptr.clone());
                                args2.extend(args);
                                args2.extend(extras);
                            } else {
                                args2.extend(args);
                                args2.extend(extras);
                                args2.insert(move_this_ptr_to_args, this_ptr.clone());
                            }
                            let result = self.call_raw(ctx, None, args2);
                            return wrap_err(result, ctx, caller_fn);
                        }
                    }
                }
                if num_params == N + E + self.curry().len() {
                    let mut args2 = FnArgsVec::with_capacity(args.len() + extras.len());
                    args2.extend(args);
                    args2.extend(extras);
                    let result = self.call_raw(ctx, this_ptr, args2);
                    return wrap_err(result, ctx, caller_fn);
                }
            }
            _ => (),
        }

        self.call_raw(ctx, this_ptr.as_deref_mut(), args.clone())
            .or_else(|err| match *err {
                ERR::ErrorFunctionNotFound(sig, ..)
                    if MOVE_PTR && this_ptr.is_some() && sig.starts_with(self.fn_name()) =>
                {
                    let mut args2 = FnArgsVec::with_capacity(args.len() + 1);
                    if move_this_ptr_to_args == 0 {
                        args2.push(this_ptr.as_mut().unwrap().clone());
                        args2.extend(args.clone());
                    } else {
                        args2.extend(args.clone());
                        args2.insert(move_this_ptr_to_args, this_ptr.as_mut().unwrap().clone());
                    }
                    self.call_raw(ctx, None, args2)
                }
                _ => Err(err),
            })
            .or_else(|err| match *err {
                ERR::ErrorFunctionNotFound(sig, ..) if sig.starts_with(self.fn_name()) => {
                    if MOVE_PTR {
                        if let Some(this_ptr) = this_ptr.as_deref_mut() {
                            let mut args2 = FnArgsVec::with_capacity(args.len() + extras.len() + 1);
                            if move_this_ptr_to_args == 0 {
                                args2.push(this_ptr.clone());
                                args2.extend(args);
                                args2.extend(extras);
                            } else {
                                args2.extend(args);
                                args2.extend(extras);
                                args2.insert(move_this_ptr_to_args, this_ptr.clone());
                            }
                            return self.call_raw(ctx, None, args2);
                        }
                    }

                    let mut args2 = FnArgsVec::with_capacity(args.len() + extras.len());
                    args2.extend(args);
                    args2.extend(extras);

                    self.call_raw(ctx, this_ptr, args2)
                }
                _ => Err(err),
            })
            .or_else(|err| wrap_err(Err(err), ctx, caller_fn))
    }
}

impl TryFrom<ImmutableString> for FnPtr {
    type Error = RhaiError;

    #[inline(always)]
    fn try_from(value: ImmutableString) -> RhaiResultOf<Self> {
        if is_valid_function_name(&value) {
            Ok(Self {
                name: value,
                curry: ThinVec::new(),
                #[cfg(not(feature = "no_function"))]
                env: None,
                typ: FnPtrType::Normal,
            })
        } else if is_reserved_keyword_or_symbol(&value).0
            || Token::lookup_symbol_from_syntax(&value).is_some()
        {
            Err(ERR::ErrorParsing(PERR::Reserved(value.to_string()), Position::NONE).into())
        } else {
            Err(ERR::ErrorFunctionNotFound(value.to_string(), Position::NONE).into())
        }
    }
}

#[cfg(not(feature = "no_function"))]
impl<T: Into<Shared<crate::ast::ScriptFuncDef>>> From<T> for FnPtr {
    #[inline(always)]
    fn from(value: T) -> Self {
        let fn_def = value.into();

        Self {
            name: fn_def.name.clone(),
            curry: ThinVec::new(),
            #[cfg(not(feature = "no_function"))]
            env: None,
            typ: FnPtrType::Script {
                num_params: fn_def.params.len(),
                hash: crate::calc_fn_hash(None, &fn_def.name, fn_def.params.len()),
            },
        }
    }
}

impl Index<usize> for FnPtr {
    type Output = Dynamic;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        self.curry.index(index)
    }
}

impl IndexMut<usize> for FnPtr {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.curry.index_mut(index)
    }
}

impl Extend<Dynamic> for FnPtr {
    #[inline(always)]
    fn extend<T: IntoIterator<Item = Dynamic>>(&mut self, iter: T) {
        self.curry.extend(iter);
    }
}
