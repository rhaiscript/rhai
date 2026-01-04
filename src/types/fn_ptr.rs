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
    /// Linked to a script-defined function.
    #[cfg(not(feature = "no_function"))]
    Script(Shared<crate::ast::ScriptFuncDef>),
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
            Self::Script(..) => f.write_str("Fn*"),
            Self::Native(..) => f.write_str("Fn"),
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
        let mut arg_values = arg_values;
        let mut arg_values = arg_values.as_mut();
        let mut args_data;

        if self.is_curried() {
            args_data = FnArgsVec::with_capacity(self.curry().len() + arg_values.len());
            args_data.extend(self.curry().iter().cloned());
            args_data.extend(arg_values.iter_mut().map(mem::take));
            arg_values = &mut *args_data;
        }

        let args = &mut StaticVec::with_capacity(arg_values.len() + 1);
        args.extend(arg_values.iter_mut());

        match self.typ {
            // Linked to scripted function?
            #[cfg(not(feature = "no_function"))]
            FnPtrType::Script(ref fn_def) if fn_def.params.len() == args.len() => {
                let global = &mut context.global_runtime_state().clone();
                global.level += 1;

                let caches = &mut crate::eval::Caches::new();

                return context.engine().call_script_fn(
                    global,
                    caches,
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
            _ => (),
        }

        let is_method = this_ptr.is_some();

        if let Some(obj) = this_ptr {
            args.insert(0, obj);
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
        match self.typ {
            #[cfg(not(feature = "no_function"))]
            FnPtrType::Script(ref fn_def) => {
                let arity = fn_def.params.len();

                if arity == N + self.curry().len() {
                    return self.call_raw(ctx, this_ptr, args);
                }
                if MOVE_PTR {
                    if let Some(this_ptr) = this_ptr.as_deref() {
                        if arity == N + 1 + self.curry().len() {
                            let mut args2 = FnArgsVec::with_capacity(args.len() + 1);
                            if move_this_ptr_to_args == 0 {
                                args2.push(this_ptr.clone());
                                args2.extend(args);
                            } else {
                                args2.extend(args);
                                args2.insert(move_this_ptr_to_args, this_ptr.clone());
                            }
                            return self.call_raw(ctx, None, args2);
                        }
                        if arity == N + E + 1 + self.curry().len() {
                            let mut args2 = FnArgsVec::with_capacity(args.len() + extras.len() + 1);
                            if move_this_ptr_to_args == 0 {
                                args2.push(this_ptr.clone());
                                args2.extend(args);
                            } else {
                                args2.extend(args);
                                args2.insert(move_this_ptr_to_args, this_ptr.clone());
                            }
                            args2.extend(extras);
                            return self.call_raw(ctx, None, args2);
                        }
                    }
                }
                if arity == N + E + self.curry().len() {
                    let mut args2 = FnArgsVec::with_capacity(args.len() + extras.len());
                    args2.extend(args);
                    args2.extend(extras);
                    return self.call_raw(ctx, this_ptr, args2);
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
                        if let Some(ref mut this_ptr) = this_ptr {
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
            .map_err(|err| {
                Box::new(ERR::ErrorInFunctionCall(
                    caller_fn.to_string(),
                    ctx.call_source().unwrap_or("").to_string(),
                    err,
                    Position::NONE,
                ))
            })
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
            typ: FnPtrType::Script(fn_def),
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
