//! Implement function-calling mechanism for [`Engine`].

#[cfg(not(feature = "no_module"))]
use super::RhaiFunc;
use super::{get_builtin_binary_op_fn, FnCallHashes};
use crate::ast::{Expr, FnCallExpr};
use crate::engine::{
    KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CALL, KEYWORD_FN_PTR_CURRY, KEYWORD_IS_DEF_VAR,
};
use crate::eval::{Caches, GlobalRuntimeState};
use crate::types::{dynamic::Union, fn_ptr::FnPtrType, token::is_valid_identifier, Token};
use crate::{
    calc_fn_hash, Dynamic, Engine, FnArgsVec, FnPtr, ImmutableString, Position, RhaiResult,
    RhaiResultOf, Scope, ERR,
};
use std::convert::TryFrom;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_float"))]
#[cfg(feature = "no_std")]
use num_traits::Float;

impl Engine {
    /// Evaluate an argument.
    #[inline]
    pub(crate) fn get_arg_value(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        this_ptr: Option<&mut Dynamic>,
        arg_expr: &Expr,
    ) -> RhaiResultOf<(Dynamic, Position)> {
        // Literal values
        if let Some(value) = arg_expr.get_literal_value(Some(global)) {
            self.track_operation(global, arg_expr.start_position())?;

            #[cfg(feature = "debugging")]
            self.dbg(global, caches, scope, this_ptr, arg_expr)?;

            return Ok((value, arg_expr.start_position()));
        }

        // Do not match function exit for arguments
        #[cfg(feature = "debugging")]
        let reset = global.debugger.as_deref_mut().and_then(|dbg| {
            dbg.clear_status_if(|status| {
                matches!(status, crate::eval::DebuggerStatus::FunctionExit(..))
            })
        });
        #[cfg(feature = "debugging")]
        defer! { global if Some(reset) => move |g| g.debugger_mut().reset_status(reset) }

        self.eval_expr(global, caches, scope, this_ptr, arg_expr)
            .map(|r| (r, arg_expr.start_position()))
    }

    /// Call a dot method.
    #[cfg(not(feature = "no_object"))]
    pub(crate) fn make_method_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        fn_name: &str,
        mut hash: FnCallHashes,
        target: &mut crate::eval::Target,
        call_args: &mut [Dynamic],
        first_arg_pos: Position,
        pos: Position,
    ) -> RhaiResultOf<(Dynamic, bool)> {
        let (result, updated) = match fn_name {
            // Handle fn_ptr.call(...)
            KEYWORD_FN_PTR_CALL if target.as_ref().is_fnptr() => {
                let fn_ptr = target.as_ref().read_lock::<FnPtr>().unwrap();
                let mut curry = fn_ptr.curry().iter().cloned().collect::<FnArgsVec<_>>();

                // Arguments are passed as-is, adding the curried arguments
                let mut args = curry
                    .iter_mut()
                    .chain(call_args.iter_mut())
                    .collect::<FnArgsVec<_>>();

                #[cfg(not(feature = "no_function"))]
                let linked_script = fn_ptr
                    .typ
                    .get_linked_script(global, args.len())
                    .map(|(fn_def, env)| (fn_def.clone(), env.cloned()));

                match fn_ptr.typ {
                    // Linked to scripted function - short-circuit
                    #[cfg(not(feature = "no_function"))]
                    _ if linked_script.is_some() => {
                        let Some((fn_def, env)) = linked_script else {
                            unreachable!()
                        };
                        let env = env.as_deref();
                        let scope = &mut Scope::new();

                        defer! { let orig_level = global.level; global.level += 1 }

                        self.call_script_fn(
                            global, caches, scope, None, env, &fn_def, &mut args, true, pos,
                        )
                        .map(|v| (v, false))
                    }
                    // Native function - short-circuit
                    FnPtrType::Native(ref func) => {
                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        func(context, &mut args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map(|v| (v, false))
                            .map_err(|err| err.fill_position(pos))
                    }
                    // Normal call
                    _ => {
                        let fn_name = fn_ptr.fn_name();

                        let _is_anon = false;
                        #[cfg(not(feature = "no_function"))]
                        let _is_anon = fn_ptr.is_anonymous();

                        // Recalculate hashes
                        let new_hash = if !_is_anon && !is_valid_identifier(fn_name) {
                            FnCallHashes::from_native_only(calc_fn_hash(None, fn_name, args.len()))
                        } else {
                            FnCallHashes::from_hash(calc_fn_hash(None, fn_name, args.len()))
                        };

                        // Map it to name(args) in function-call style
                        self.exec_fn_call(
                            global, caches, None, fn_name, None, new_hash, &mut args, false, false,
                            pos,
                        )
                    }
                }
            }

            // Handle obj.call(fn_ptr, ...)
            KEYWORD_FN_PTR_CALL => {
                if call_args.is_empty() {
                    return Err(self.make_type_mismatch_err::<FnPtr>(
                        self.map_type_name(target.as_ref().type_name()),
                        pos,
                    ));
                }

                // FnPtr call on object
                let fn_ptr = call_args[0]
                    .take()
                    .try_cast_result::<FnPtr>()
                    .map_err(|v| {
                        self.make_type_mismatch_err::<FnPtr>(
                            self.map_type_name(v.type_name()),
                            first_arg_pos,
                        )
                    })?;

                let _is_anon = false;
                #[cfg(not(feature = "no_function"))]
                let _is_anon = fn_ptr.is_anonymous();

                let FnPtr { name, curry, typ } = fn_ptr;

                // Adding the curried arguments and the remaining arguments
                let mut curry = curry.into_iter().collect::<FnArgsVec<_>>();
                let args = &mut FnArgsVec::with_capacity(curry.len() + call_args.len());
                args.extend(curry.iter_mut());
                args.extend(call_args.iter_mut().skip(1));

                #[cfg(not(feature = "no_function"))]
                let linked_script = typ
                    .get_linked_script(global, args.len())
                    .map(|(fn_def, env)| (fn_def.clone(), env.cloned()));

                match typ {
                    // Linked to scripted function - short-circuit
                    #[cfg(not(feature = "no_function"))]
                    _ if linked_script.is_some() => {
                        let Some((fn_def, env)) = linked_script else {
                            unreachable!()
                        };

                        // Check for data race.
                        #[cfg(not(feature = "no_closure"))]
                        #[cfg(not(feature = "unchecked"))]
                        super::ensure_no_data_race(&fn_def.name, args, false)?;

                        let env = env.as_deref();
                        let scope = &mut Scope::new();
                        let this_ptr = Some(target.as_mut());

                        defer! { let orig_level = global.level; global.level += 1 }

                        self.call_script_fn(
                            global, caches, scope, this_ptr, env, &fn_def, args, true, pos,
                        )
                        .map(|v| (v, false))
                    }
                    // Native function - short-circuit
                    FnPtrType::Native(ref func) => {
                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        // Add the first argument with the object pointer
                        args.insert(0, target.as_mut());

                        func(context, args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map(|v| (v, false))
                            .map_err(|err| err.fill_position(pos))
                    }
                    // Normal call
                    _ => {
                        let is_ref_mut = target.is_ref();

                        // Add the first argument with the object pointer
                        args.insert(0, target.as_mut());

                        // Recalculate hash
                        let num_args = args.len();

                        let new_hash = if !_is_anon && !is_valid_identifier(&name) {
                            FnCallHashes::from_native_only(calc_fn_hash(None, &name, num_args))
                        } else {
                            #[cfg(not(feature = "no_function"))]
                            {
                                FnCallHashes::from_script_and_native(
                                    calc_fn_hash(None, &name, num_args - 1),
                                    calc_fn_hash(None, &name, num_args),
                                )
                            }
                            #[cfg(feature = "no_function")]
                            {
                                FnCallHashes::from_native_only(calc_fn_hash(None, &name, num_args))
                            }
                        };

                        // Map it to name(args) in function-call style
                        self.exec_fn_call(
                            global, caches, None, &name, None, new_hash, args, is_ref_mut, true,
                            pos,
                        )
                    }
                }
            }

            // Handle fn_ptr.curry(...)
            KEYWORD_FN_PTR_CURRY => {
                let typ = target.as_ref().type_name();
                let mut fn_ptr = target
                    .as_ref()
                    .read_lock::<FnPtr>()
                    .ok_or_else(|| {
                        self.make_type_mismatch_err::<FnPtr>(self.map_type_name(typ), pos)
                    })?
                    .clone();

                // Append the new curried arguments to the existing list.
                fn_ptr.extend(call_args.iter_mut().map(std::mem::take));

                Ok((fn_ptr.into(), false))
            }

            // Handle var.is_shared()
            #[cfg(not(feature = "no_closure"))]
            crate::engine::KEYWORD_IS_SHARED if call_args.is_empty() => {
                return Ok((target.is_shared().into(), false));
            }

            _ => {
                let mut fn_name = fn_name;
                let _redirected;
                let mut _linked = None;
                let mut _arg_values;
                let mut call_args = call_args;

                // Check if it is a map method call in OOP style
                #[cfg(not(feature = "no_object"))]
                if let Ok(map) = target.as_ref().as_map_ref() {
                    if let Some(val) = map.get(fn_name) {
                        if let Some(fn_ptr) = val.read_lock::<FnPtr>() {
                            // Remap the function name
                            _redirected = fn_ptr.fn_name_raw().clone();
                            fn_name = &_redirected;
                            // Add curried arguments
                            if fn_ptr.is_curried() {
                                _arg_values = fn_ptr
                                    .curry()
                                    .iter()
                                    .cloned()
                                    .chain(call_args.iter_mut().map(std::mem::take))
                                    .collect::<FnArgsVec<_>>();
                                call_args = &mut _arg_values;
                            }

                            // Get scripted function if linked
                            #[cfg(not(feature = "no_function"))]
                            let linked_script =
                                fn_ptr.typ.get_linked_script(global, call_args.len());

                            match fn_ptr.typ {
                                // Linked to scripted function
                                #[cfg(not(feature = "no_function"))]
                                _ if linked_script.is_some() => {
                                    let Some((fn_def, env)) = linked_script else {
                                        unreachable!()
                                    };
                                    _linked = Some((Some(fn_def.clone()), None, env.clone()))
                                }
                                FnPtrType::Native(ref func) => {
                                    _linked = Some((
                                        #[cfg(not(feature = "no_function"))]
                                        None,
                                        #[cfg(feature = "no_function")]
                                        Option::<()>::None,
                                        Some(func.clone()),
                                        #[cfg(not(feature = "no_function"))]
                                        None,
                                        #[cfg(feature = "no_function")]
                                        Option::<()>::None,
                                    ))
                                }
                                _ => {
                                    let _is_anon = false;
                                    #[cfg(not(feature = "no_function"))]
                                    let _is_anon = fn_ptr.is_anonymous();

                                    // Recalculate the hash based on the new function name and new arguments
                                    let num_args = call_args.len() + 1;

                                    hash = if !_is_anon && !is_valid_identifier(fn_name) {
                                        FnCallHashes::from_native_only(calc_fn_hash(
                                            None, fn_name, num_args,
                                        ))
                                    } else {
                                        #[cfg(not(feature = "no_function"))]
                                        {
                                            FnCallHashes::from_script_and_native(
                                                calc_fn_hash(None, fn_name, num_args - 1),
                                                calc_fn_hash(None, fn_name, num_args),
                                            )
                                        }
                                        #[cfg(feature = "no_function")]
                                        {
                                            FnCallHashes::from_native_only(calc_fn_hash(
                                                None, fn_name, num_args,
                                            ))
                                        }
                                    };
                                }
                            }
                        }
                    }
                }

                match _linked {
                    // Linked to scripted function - short-circuit
                    #[cfg(not(feature = "no_function"))]
                    Some((Some(fn_def), None, env)) => {
                        let scope = &mut Scope::new();
                        let env = env.cloned();
                        let env = env.as_deref();
                        let this_ptr = Some(target.as_mut());
                        let args = &mut call_args.iter_mut().collect::<FnArgsVec<_>>();

                        defer! { let orig_level = global.level; global.level += 1 }

                        self.call_script_fn(
                            global, caches, scope, this_ptr, env, &fn_def, args, true, pos,
                        )
                        .map(|v| (v, false))
                    }
                    // Native function - short-circuit
                    Some((None, Some(func), ..)) => {
                        // Attached object pointer in front of the arguments
                        let args = &mut std::iter::once(target.as_mut())
                            .chain(call_args.iter_mut())
                            .collect::<FnArgsVec<_>>();

                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        func(context, args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map(|v| (v, false))
                            .map_err(|err| err.fill_position(pos))
                    }
                    // Normal call
                    None => {
                        let is_ref_mut = target.is_ref();

                        // Attached object pointer in front of the arguments
                        let args = &mut std::iter::once(target.as_mut())
                            .chain(call_args.iter_mut())
                            .collect::<FnArgsVec<_>>();

                        self.exec_fn_call(
                            global, caches, None, fn_name, None, hash, args, is_ref_mut, true, pos,
                        )
                    }
                    _ => unreachable!(),
                }
            }
        }?;

        // Propagate the changed value back to the source if necessary
        if updated {
            target.propagate_changed_value(pos)?;
        }

        Ok((result, updated))
    }

    /// Call a function in normal function-call style.
    pub(crate) fn make_function_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        fn_name: &str,
        op_token: Option<&Token>,
        first_arg: Option<&Expr>,
        args_expr: &[Expr],
        hashes: FnCallHashes,
        capture_parent_scope: bool,
        pos: Position,
    ) -> RhaiResult {
        let mut first_arg = first_arg;
        let mut args_expr = args_expr;
        let mut num_args = usize::from(first_arg.is_some()) + args_expr.len();
        let mut curry = FnArgsVec::new_const();
        let mut fn_name = fn_name;
        let mut hashes = hashes;
        let redirected; // Handle call() - Redirect function call

        match fn_name {
            _ if op_token.is_some() => (),

            // Handle call(fn_ptr, ...)
            KEYWORD_FN_PTR_CALL if num_args >= 1 => {
                let arg = first_arg.unwrap();
                let (first_arg_value, first_arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), arg)?;

                let fn_ptr = first_arg_value.try_cast_result::<FnPtr>().map_err(|v| {
                    self.make_type_mismatch_err::<FnPtr>(
                        self.map_type_name(v.type_name()),
                        first_arg_pos,
                    )
                })?;

                let _is_anon = false;
                #[cfg(not(feature = "no_function"))]
                let _is_anon = fn_ptr.is_anonymous();

                let FnPtr {
                    name,
                    curry: extra_curry,
                    typ,
                } = fn_ptr;

                curry.extend(extra_curry);

                // Get scripted function if linked
                #[cfg(not(feature = "no_function"))]
                let linked_script = typ
                    .get_linked_script(global, curry.len() + args_expr.len())
                    .map(|(f, e)| (f.clone(), e.cloned()));

                match typ {
                    // Linked to scripted function - short-circuit
                    #[cfg(not(feature = "no_function"))]
                    _ if linked_script.is_some() => {
                        let Some((fn_def, env)) = linked_script else {
                            unreachable!()
                        };

                        // Evaluate arguments
                        let mut arg_values =
                            FnArgsVec::with_capacity(curry.len() + args_expr.len());
                        arg_values.extend(curry);
                        for expr in args_expr {
                            let this_ptr = this_ptr.as_deref_mut();
                            let (value, _) =
                                self.get_arg_value(global, caches, scope, this_ptr, expr)?;
                            arg_values.push(value);
                        }
                        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();
                        let scope = &mut Scope::new();
                        let env = env.as_deref();

                        defer! { let orig_level = global.level; global.level += 1 }

                        return self.call_script_fn(
                            global, caches, scope, None, env, &fn_def, args, true, pos,
                        );
                    }
                    // Native function - short-circuit
                    FnPtrType::Native(ref func) => {
                        // Evaluate arguments
                        let mut arg_values =
                            FnArgsVec::with_capacity(curry.len() + args_expr.len());
                        arg_values.extend(curry);
                        for expr in args_expr {
                            let this_ptr = this_ptr.as_deref_mut();
                            let (value, _) =
                                self.get_arg_value(global, caches, scope, this_ptr, expr)?;
                            arg_values.push(value);
                        }
                        let args = &mut arg_values.iter_mut().collect::<FnArgsVec<_>>();

                        defer! { let orig_level = global.level; global.level += 1 }

                        let context = (self, fn_name, None, &*global, pos).into();

                        return func(context, args)
                            .and_then(|r| self.check_data_size(r, pos))
                            .map_err(|err| err.fill_position(pos));
                    }
                    // Normal function call
                    _ => (),
                }

                // Redirect function name
                redirected = name;
                fn_name = &redirected;

                // Shift the arguments
                first_arg = args_expr.first();
                if !args_expr.is_empty() {
                    args_expr = &args_expr[1..];
                }
                num_args -= 1;

                // Recalculate hash
                let args_len = num_args + curry.len();

                hashes = if !_is_anon && !is_valid_identifier(fn_name) {
                    FnCallHashes::from_native_only(calc_fn_hash(None, fn_name, args_len))
                } else {
                    FnCallHashes::from_hash(calc_fn_hash(None, fn_name, args_len))
                };
            }

            // Handle Fn(fn_name)
            KEYWORD_FN_PTR if num_args == 1 => {
                let arg = first_arg.unwrap();
                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr, arg)?;

                // Fn - only in function call style
                return arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, arg_pos))
                    .and_then(FnPtr::try_from)
                    .map(Into::into)
                    .map_err(|err| err.fill_position(arg_pos));
            }

            // Handle curry(x, ...)
            KEYWORD_FN_PTR_CURRY if num_args > 1 => {
                let first = first_arg.unwrap();
                let (first_arg_value, first_arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                let mut fn_ptr = first_arg_value.try_cast_result::<FnPtr>().map_err(|v| {
                    self.make_type_mismatch_err::<FnPtr>(
                        self.map_type_name(v.type_name()),
                        first_arg_pos,
                    )
                })?;

                // Append the new curried arguments to the existing list.
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    fn_ptr.add_curry(value);
                }

                return Ok(fn_ptr.into());
            }

            // Handle is_shared(var)
            #[cfg(not(feature = "no_closure"))]
            crate::engine::KEYWORD_IS_SHARED if num_args == 1 => {
                let arg = first_arg.unwrap();
                let (arg_value, ..) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), arg)?;
                return Ok(arg_value.is_shared().into());
            }

            // Handle is_def_var(fn_name)
            KEYWORD_IS_DEF_VAR if num_args == 1 => {
                let arg = first_arg.unwrap();
                let (arg_value, arg_pos) =
                    self.get_arg_value(global, caches, scope, this_ptr, arg)?;
                let var_name = arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, arg_pos))?;
                return Ok(scope.contains(&var_name).into());
            }

            // Handle eval(script)
            KEYWORD_EVAL if num_args == 1 => {
                // eval - only in function call style
                let orig_scope_len = scope.len();
                #[cfg(not(feature = "no_module"))]
                let orig_imports_len = global.num_imports();
                let arg = first_arg.unwrap();
                let (arg_value, pos) = self.get_arg_value(global, caches, scope, this_ptr, arg)?;
                let s = &arg_value
                    .into_immutable_string()
                    .map_err(|typ| self.make_type_mismatch_err::<ImmutableString>(typ, pos))?;

                let orig_level = global.level;
                global.level += 1;

                let result = self.eval_script_expr_in_place(global, caches, scope, s, pos);

                // IMPORTANT! If the eval defines new variables in the current scope,
                //            all variable offsets from this point on will be mis-aligned.
                //            The same is true for imports.
                let scope_changed = scope.len() != orig_scope_len;
                #[cfg(not(feature = "no_module"))]
                let scope_changed = scope_changed || global.num_imports() != orig_imports_len;

                if scope_changed {
                    global.always_search_scope = true;
                }
                global.level = orig_level;

                return result.map_err(|err| match *err {
                    ERR::Exit(..) => err,
                    _ => ERR::ErrorInFunctionCall(
                        KEYWORD_EVAL.to_string(),
                        global.source().unwrap_or("").to_string(),
                        err,
                        pos,
                    )
                    .into(),
                });
            }

            _ => (),
        }

        // Normal function call - except for Fn, curry, call and eval (handled above)
        let mut arg_values = FnArgsVec::with_capacity(num_args);
        let mut args = FnArgsVec::with_capacity(num_args + curry.len());
        let mut is_ref_mut = false;

        // Capture parent scope?
        //
        // If so, do it separately because we cannot convert the first argument (if it is a simple
        // variable access) to &mut because `scope` is needed.
        if capture_parent_scope && !scope.is_empty() {
            for expr in first_arg.iter().copied().chain(args_expr.iter()) {
                let (value, ..) =
                    self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                arg_values.push(value.flatten());
            }
            args.extend(curry.iter_mut());
            args.extend(arg_values.iter_mut());

            // Use parent scope
            let scope = Some(scope);

            return self
                .exec_fn_call(
                    global, caches, scope, fn_name, op_token, hashes, &mut args, is_ref_mut, false,
                    pos,
                )
                .map(|(v, ..)| v);
        }

        // Call with blank scope
        #[cfg(not(feature = "no_closure"))]
        let has_non_shared_this_ptr = this_ptr.as_ref().map_or(false, |v| !v.is_shared());
        #[cfg(feature = "no_closure")]
        let has_non_shared_this_ptr = this_ptr.is_some();

        // If the first argument is a variable, and there are no curried arguments,
        // convert to method-call style in order to leverage potential &mut first argument
        // and avoid cloning the value.
        match first_arg {
            Some(_first @ Expr::ThisPtr(pos)) if curry.is_empty() && has_non_shared_this_ptr => {
                // Turn it into a method call only if the object is not shared
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), _first)?;

                // func(x, ...) -> x.func(...)
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                is_ref_mut = true;
                args.push(this_ptr.unwrap());
            }
            Some(first @ Expr::Variable(.., pos)) if curry.is_empty() => {
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                // func(x, ...) -> x.func(...)
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                let mut target = self.search_namespace(global, caches, scope, this_ptr, first)?;

                if target.as_ref().is_read_only() {
                    target = target.into_owned();
                }

                if target.is_shared() || target.is_temp_value() {
                    arg_values.insert(0, target.take_or_clone().flatten());
                } else {
                    // Turn it into a method call only if the object is not shared and not a simple value
                    is_ref_mut = true;
                    let obj_ref = target.take_ref().unwrap();
                    args.push(obj_ref);
                }
            }
            _ => {
                // func(..., ...)
                for expr in first_arg.into_iter().chain(args_expr.iter()) {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }
                args.extend(curry.iter_mut());
            }
        }

        args.extend(arg_values.iter_mut());

        self.exec_fn_call(
            global, caches, None, fn_name, op_token, hashes, &mut args, is_ref_mut, false, pos,
        )
        .map(|(v, ..)| v)
    }

    /// Call a namespace-qualified function in normal function-call style.
    #[cfg(not(feature = "no_module"))]
    pub(crate) fn make_qualified_function_call(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        namespace: &crate::module::Namespace,
        fn_name: &str,
        args_expr: &[Expr],
        hash: u64,
        pos: Position,
    ) -> RhaiResult {
        let mut arg_values = FnArgsVec::with_capacity(args_expr.len());
        let args = &mut FnArgsVec::with_capacity(args_expr.len());
        let mut first_arg_value = None;

        #[cfg(not(feature = "no_closure"))]
        let has_non_shared_this_ptr = this_ptr.as_ref().map_or(false, |v| !v.is_shared());
        #[cfg(feature = "no_closure")]
        let has_non_shared_this_ptr = this_ptr.is_some();

        // See if the first argument is a variable.
        // If so, convert to method-call style in order to leverage potential
        // &mut first argument and avoid cloning the value.
        match args_expr.first() {
            Some(_first @ Expr::ThisPtr(pos)) if has_non_shared_this_ptr => {
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), _first)?;

                // The first value is a placeholder (for later if it needs to be cloned)
                arg_values.push(Dynamic::UNIT);

                for expr in args_expr.iter().skip(1) {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                // func(x, ...) -> x.func(...)
                let (first, rest) = arg_values.split_first_mut().unwrap();
                first_arg_value = Some(first);
                args.push(this_ptr.unwrap());
                args.extend(rest.iter_mut());
            }
            Some(first @ Expr::Variable(.., pos)) => {
                self.track_operation(global, *pos)?;

                #[cfg(feature = "debugging")]
                self.dbg(global, caches, scope, this_ptr.as_deref_mut(), first)?;

                // The first value is a placeholder (for later if it needs to be cloned)
                arg_values.push(Dynamic::UNIT);

                for expr in args_expr.iter().skip(1) {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }

                let target = self.search_namespace(global, caches, scope, this_ptr, first)?;

                if target.is_shared() || target.is_temp_value() {
                    arg_values[0] = target.take_or_clone().flatten();
                    args.extend(arg_values.iter_mut());
                } else {
                    // Turn it into a method call only if the object is not shared and not a simple value
                    // func(x, ...) -> x.func(...)
                    let (first, rest) = arg_values.split_first_mut().unwrap();
                    first_arg_value = Some(first);
                    let obj_ref = target.take_ref().unwrap();
                    args.push(obj_ref);
                    args.extend(rest.iter_mut());
                }
            }
            Some(_) => {
                // func(..., ...) or func(mod::x, ...)
                for expr in args_expr {
                    let (value, ..) =
                        self.get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), expr)?;
                    arg_values.push(value.flatten());
                }
                args.extend(arg_values.iter_mut());
            }
            None => (),
        }

        // Search for the root namespace
        let module = self
            .search_imports(global, namespace)
            .ok_or_else(|| ERR::ErrorModuleNotFound(namespace.to_string(), namespace.position()))?;

        // First search script-defined functions in namespace (can override built-in)
        let mut func = module.get_qualified_fn(hash).or_else(|| {
            // Then search native Rust functions
            let hash_qualified_fn =
                super::calc_fn_hash_full(hash, args.iter().map(|a| a.type_id()));
            module.get_qualified_fn(hash_qualified_fn)
        });

        // Check for `Dynamic` parameters.
        //
        // Note - This is done during every function call mismatch without cache,
        //        so hopefully the number of arguments should not be too many
        //        (expected because closures cannot be qualified).
        if func.is_none() && !args.is_empty() {
            let num_args = args.len();
            let max_dynamic_count =
                usize::min(num_args, crate::api::default_limits::MAX_DYNAMIC_PARAMETERS);
            let max_bitmask = 1usize << max_dynamic_count;
            let mut bitmask = 1usize; // Bitmask of which parameter to replace with `Dynamic`

            // Try all permutations with `Dynamic` wildcards
            while bitmask < max_bitmask {
                let hash_qualified_fn = super::calc_fn_hash_full(
                    hash,
                    args.iter().enumerate().map(|(i, a)| {
                        if i < max_dynamic_count
                            && bitmask & (1usize << (max_dynamic_count - i - 1)) != 0
                        {
                            // Replace with `Dynamic`
                            std::any::TypeId::of::<Dynamic>()
                        } else {
                            a.type_id()
                        }
                    }),
                );

                if let Some(f) = module.get_qualified_fn(hash_qualified_fn) {
                    func = Some(f);
                    break;
                }

                bitmask += 1;
            }
        }

        // Clone first argument if the function is not a method after-all
        if !func.map_or(true, RhaiFunc::is_method) {
            if let Some(first) = first_arg_value {
                *first = args[0].clone();
                args[0] = first;
            }
        }

        defer! { let orig_level = global.level; global.level += 1 }

        match func {
            #[cfg(not(feature = "no_function"))]
            Some(RhaiFunc::Script { fn_def, env }) => {
                let env = env.as_deref();
                let scope = &mut Scope::new();

                let orig_source = std::mem::replace(&mut global.source, module.id_raw().cloned());
                defer! { global => move |g| g.source = orig_source }

                self.call_script_fn(global, caches, scope, None, env, fn_def, args, true, pos)
            }

            Some(f) if !f.is_pure() && args[0].is_read_only() => {
                // If function is not pure, there must be at least one argument
                Err(ERR::ErrorNonPureMethodCallOnConstant(fn_name.to_string(), pos).into())
            }

            Some(RhaiFunc::Plugin { func }) => {
                let context = func
                    .has_context()
                    .then(|| (self, fn_name, module.id(), &*global, pos).into());
                func.call(context, args)
                    .and_then(|r| self.check_data_size(r, pos))
            }

            Some(
                RhaiFunc::Pure {
                    func, has_context, ..
                }
                | RhaiFunc::Method {
                    func, has_context, ..
                },
            ) => {
                let context =
                    has_context.then(|| (self, fn_name, module.id(), &*global, pos).into());
                func(context, args).and_then(|r| self.check_data_size(r, pos))
            }

            Some(RhaiFunc::Iterator { .. }) => {
                unreachable!("iterator functions should not occur here")
            }

            None => Err(ERR::ErrorFunctionNotFound(
                if namespace.is_empty() {
                    self.gen_fn_call_signature(fn_name, args)
                } else {
                    format!(
                        "{namespace}{}{}",
                        crate::engine::NAMESPACE_SEPARATOR,
                        self.gen_fn_call_signature(fn_name, args)
                    )
                },
                pos,
            )
            .into()),
        }
    }

    /// Evaluate a text script in place - used primarily for 'eval'.
    pub(crate) fn eval_script_expr_in_place(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        script: &str,
        _pos: Position,
    ) -> RhaiResult {
        self.track_operation(global, _pos)?;

        let script = script.trim();

        if script.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        // Compile the script text
        // No optimizations because we only run it once
        let ast = self.compile_scripts_with_scope_raw(
            None,
            [script],
            #[cfg(not(feature = "no_optimize"))]
            crate::OptimizationLevel::None,
        )?;

        // If new functions are defined within the eval string, it is an error
        #[cfg(not(feature = "no_function"))]
        if ast.has_functions() {
            return Err(crate::PERR::WrongFnDefinition.into());
        }

        let statements = ast.statements();
        if statements.is_empty() {
            return Ok(Dynamic::UNIT);
        }

        // Evaluate the AST
        self.eval_global_statements(global, caches, scope, statements, false)
    }

    /// # Main Entry-Point (`FnCallExpr`)
    ///
    /// Evaluate a function call expression.
    ///
    /// This method tries to short-circuit function resolution under Fast Operators mode if the
    /// function call is an operator.
    pub(crate) fn eval_fn_call_expr(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        mut this_ptr: Option<&mut Dynamic>,
        expr: &FnCallExpr,
        pos: Position,
    ) -> RhaiResult {
        let FnCallExpr {
            #[cfg(not(feature = "no_module"))]
            namespace,
            name,
            hashes,
            args,
            op_token,
            capture_parent_scope: capture,
            ..
        } = expr;

        let op_token = op_token.as_ref();

        // Short-circuit native unary operator call if under Fast Operators mode
        if self.fast_operators() && args.len() == 1 && op_token == Some(&Token::Bang) {
            let mut value = self
                .get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), &args[0])?
                .0
                .flatten();

            return if let Union::Bool(b, ..) = value.0 {
                Ok((!b).into())
            } else {
                let operand = &mut [&mut value];
                self.exec_fn_call(
                    global, caches, None, name, op_token, *hashes, operand, false, false, pos,
                )
                .map(|(v, ..)| v)
            };
        }

        // Short-circuit native binary operator call if under Fast Operators mode
        if self.fast_operators() && args.len() == 2 && op_token.is_some() {
            let mut lhs = self
                .get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), &args[0])?
                .0
                .flatten();

            let mut rhs = self
                .get_arg_value(global, caches, scope, this_ptr.as_deref_mut(), &args[1])?
                .0
                .flatten();

            if lhs.is_variant() || rhs.is_variant() {
                // For custom types, give registered functions a chance to run first before considering the built-in fallback
            } else {
                // For other types, try to get a built-in
                if let Some((func, need_context)) =
                    get_builtin_binary_op_fn(op_token.unwrap(), &lhs, &rhs)
                {
                    // We may not need to bump the level because built-in's do not need it.
                    //defer! { let orig_level = global.level; global.level += 1 }

                    let context =
                        need_context.then(|| (self, name.as_str(), None, &*global, pos).into());
                    return func(context, &mut [&mut lhs, &mut rhs]);
                }
            }

            let operands = &mut [&mut lhs, &mut rhs];

            return self
                .exec_fn_call(
                    global, caches, None, name, op_token, *hashes, operands, false, false, pos,
                )
                .map(|(v, ..)| v);
        }

        #[cfg(not(feature = "no_module"))]
        if !namespace.is_empty() {
            // Qualified function call
            let hash = hashes.native();

            return self.make_qualified_function_call(
                global, caches, scope, this_ptr, namespace, name, args, hash, pos,
            );
        }

        // Normal function call
        let (first_arg, rest_args) = args.split_first().map_or_else(
            || (None, args.as_ref()),
            |(first, rest)| (Some(first), rest),
        );

        self.make_function_call(
            global, caches, scope, this_ptr, name, op_token, first_arg, rest_args, *hashes,
            *capture, pos,
        )
    }
}
