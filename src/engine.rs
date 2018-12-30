use std::{
    any::TypeId,
    borrow::Borrow,
    collections::HashMap,
    error::Error,
    fmt,
    sync::Arc,
    ops::{
        Add,
        BitAnd,
        BitOr,
        BitXor,
        Deref,
        Div,
        Mul,
        Neg,
        Rem,
        Shl,
        Shr,
        Sub
    }
};

use any::{
    Any,
    AnyExt
};
use call::FunArgs;
use fn_register::{
    Mut
    RegisterFn,
};
use parser::{
    Expr,
    FnDef,
    lex,
    parse,
    Stmt
};

#[derive(Debug)]
pub enum EvalAltResult {
    ErrorFunctionNotFound(String),
    ErrorFunctionArgMismatch,
    ErrorFunctionCallNotSupported,
    ErrorIndexMismatch,
    ErrorIfGuardMismatch,
    ErrorVariableNotFound(String),
    ErrorFunctionArityNotSupported,
    ErrorAssignmentToUnknownLHS,
    ErrorMismatchOutputType(String),
    ErrorCantOpenScriptFile,
    InternalErrorMalformedDotExpression,
    LoopBreak,
    Return(Box<Any>),
}

impl EvalAltResult {
    fn as_str(&self) -> Option<&str> {
        match *self {
            EvalAltResult::ErrorVariableNotFound(ref s) => Some(s.as_str()),
            EvalAltResult::ErrorFunctionNotFound(ref s) => Some(s.as_str()),
            EvalAltResult::ErrorMismatchOutputType(ref s) => Some(s.as_str()),
            _ => None
        }
    }
}

impl PartialEq for EvalAltResult {
    fn eq(&self, other: &Self) -> bool {
        use EvalAltResult::*;

        match (self, other) {
            (&ErrorFunctionNotFound(ref a), &ErrorFunctionNotFound(ref b)) => a == b,
            (&ErrorFunctionArgMismatch, &ErrorFunctionArgMismatch) => true,
            (&ErrorFunctionCallNotSupported, &ErrorFunctionCallNotSupported) => true,
            (&ErrorIndexMismatch, &ErrorIndexMismatch) => true,
            (&ErrorIfGuardMismatch, &ErrorIfGuardMismatch) => true,
            (&ErrorVariableNotFound(ref a), &ErrorVariableNotFound(ref b)) => a == b,
            (&ErrorFunctionArityNotSupported, &ErrorFunctionArityNotSupported) => true,
            (&ErrorAssignmentToUnknownLHS, &ErrorAssignmentToUnknownLHS) => true,
            (&ErrorMismatchOutputType(ref a), &ErrorMismatchOutputType(ref b)) => a == b,
            (&ErrorCantOpenScriptFile, &ErrorCantOpenScriptFile) => true,
            (&InternalErrorMalformedDotExpression, &InternalErrorMalformedDotExpression) => true,
            (&LoopBreak, &LoopBreak) => true,
            _ => false,
        }
    }
}

impl Error for EvalAltResult {
    fn description(&self) -> &str {
        match *self {
            EvalAltResult::ErrorFunctionNotFound(_) => "Function not found",
            EvalAltResult::ErrorFunctionArgMismatch => "Function argument types do not match",
            EvalAltResult::ErrorFunctionCallNotSupported => {
                "Function call with > 2 argument not supported"
            }
            EvalAltResult::ErrorIndexMismatch => "Index does not match array",
            EvalAltResult::ErrorIfGuardMismatch => "If guards expect boolean expression",
            EvalAltResult::ErrorVariableNotFound(_) => "Variable not found",
            EvalAltResult::ErrorFunctionArityNotSupported => {
                "Functions of more than 3 parameters are not yet supported"
            }
            EvalAltResult::ErrorAssignmentToUnknownLHS => {
                "Assignment to an unsupported left-hand side"
            }
            EvalAltResult::ErrorMismatchOutputType(_) => "Cast of output failed",
            EvalAltResult::ErrorCantOpenScriptFile => "Cannot open script file",
            EvalAltResult::InternalErrorMalformedDotExpression => {
                "[Internal error] Unexpected expression in dot expression"
            }
            EvalAltResult::LoopBreak => "Loop broken before completion (not an error)",
            EvalAltResult::Return(_) => "Function returned value (not an error)",
        }
    }

    fn cause(&self) -> Option<&Error> {
        None
    }
}

impl fmt::Display for EvalAltResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(s) = self.as_str() {
            write!(f, "{}: {}", self.description(), s)
        } else {
            write!(f, "{}", self.description())
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct FnSpec {
    ident: String,
    args: Option<Vec<TypeId>>,
}

/// Rhai's engine type. This is what you use to run Rhai scripts
///
/// ```rust
/// extern crate rhai;
/// use rhai::Engine;
///
/// fn main() {
///     let mut engine = Engine::new();
///
///     if let Ok(result) = engine.eval::<i64>("40 + 2") {
///         println!("Answer: {}", result);  // prints 42
///     }
/// }
/// ```
#[derive(Clone)]
pub struct Engine {
    /// A hashmap containing all functions known to the engine
    pub fns: HashMap<FnSpec, Arc<FnIntExt>>,
    pub type_names: HashMap<TypeId,String>,
}

pub enum FnIntExt {
    Ext(Box<FnAny>),
    Int(FnDef),
}

pub type FnAny = Fn(Vec<&mut Any>) -> Result<Box<Any>, EvalAltResult>;

/// A type containing information about current scope.
/// Useful for keeping state between `Engine` runs
///
/// ```rust
/// use rhai::{Engine, Scope};
///
/// let mut engine = Engine::new();
/// let mut my_scope = Scope::new();
///
/// assert!(engine.eval_with_scope::<()>(&mut my_scope, "let x = 5;").is_ok());
/// assert_eq!(engine.eval_with_scope::<i64>(&mut my_scope, "x + 1").unwrap(), 6);
/// ```
///
/// Between runs, `Engine` only remembers functions when not using own `Scope`.
pub type Scope = Vec<(String, Box<Any>)>;

impl Engine {
    pub fn call_fn<'a, I, A, T>(&self, ident: I, args: A) -> Result<T, EvalAltResult>
    where
        I: Into<String>,
        A: FunArgs<'a>,
        T: Any + Clone,
    {
        self.call_fn_raw(ident.into(), args.into_vec())
            .and_then(|b| {
                b.downcast()
                    .map(|b| *b)
                    .map_err(|a| EvalAltResult::ErrorMismatchOutputType(self.nice_type_name(a)))
            })
    }

    /// Universal method for calling functions, that are either
    /// registered with the `Engine` or written in Rhai
    pub fn call_fn_raw(
        &self,
        ident: String,
        args: Vec<&mut Any>,
    ) -> Result<Box<Any>, EvalAltResult> {
        debug_println!(
            "Trying to call function {:?} with args {:?}",
            ident,
            args.iter().map(|x| (&**x).type_id()).collect::<Vec<_>>()
        );

        let spec = FnSpec {
            ident: ident.clone(),
            args: Some(args.iter().map(|a| <Any as Any>::type_id(&**a)).collect()),
        };

        self.fns
            .get(&spec)
            .or_else(|| {
                let spec1 = FnSpec { ident: ident.clone(), args: None };
                self.fns.get(&spec1)
            })
            .ok_or_else(|| {
                let typenames = args.iter().map(|x| self.nice_type_name((&**x).box_clone())).collect::<Vec<_>>();
                EvalAltResult::ErrorFunctionNotFound(format!("{} ({})", ident, typenames.join(",")))
            })
            .and_then(move |f| match **f {
                FnIntExt::Ext(ref f) => f(args),
                FnIntExt::Int(ref f) => {
                    let mut scope = Scope::new();
                    scope.extend(
                        f.params
                            .iter()
                            .cloned()
                            .zip(args.iter().map(|x| (&**x).box_clone())),
                    );

                    match self.eval_stmt(&mut scope, &*f.body) {
                        Err(EvalAltResult::Return(x)) => Ok(x),
                        other => other,
                    }
                }
            })
    }

    pub fn register_fn_raw(&mut self, ident: String, args: Option<Vec<TypeId>>, f: Box<FnAny>) {
        debug_println!("Register; {:?} with args {:?}", ident, args);

        let spec = FnSpec { ident, args };

        self.fns.insert(spec, Arc::new(FnIntExt::Ext(f)));
    }

    /// Register a type for use with Engine. Keep in mind that
    /// your type must implement Clone.
    pub fn register_type<T: Any>(&mut self) {
        // currently a no-op, exists for future extensibility
    }

    /// Register a type, providing a name for nice error messages.
    pub fn register_type_name<T: Any>(&mut self, name: &str) {
        self.register_type::<T>();
        debug_println!("register type {}: {:?}", name, TypeId::of::<T>());
        self.type_names.insert(TypeId::of::<T>(), name.into());
    }

    /// Register a get function for a member of a registered type
    pub fn register_get<T: Clone + Any, U: Clone + Any, F>(&mut self, name: &str, get_fn: F)
    where
        F: 'static + Fn(&mut T) -> U,
    {
        let get_name = "get$".to_string() + name;
        self.register_fn(&get_name, get_fn);
    }

    /// Register a set function for a member of a registered type
    pub fn register_set<T: Clone + Any, U: Clone + Any, F>(&mut self, name: &str, set_fn: F)
    where
        F: 'static + Fn(&mut T, U) -> (),
    {
        let set_name = "set$".to_string() + name;
        self.register_fn(&set_name, set_fn);
    }

    /// Shorthand for registering both getters and setters
    pub fn register_get_set<T: Clone + Any, U: Clone + Any, F, G>(
        &mut self,
        name: &str,
        get_fn: F,
        set_fn: G,
    ) where
        F: 'static + Fn(&mut T) -> U,
        G: 'static + Fn(&mut T, U) -> (),
    {
        self.register_get(name, get_fn);
        self.register_set(name, set_fn);
    }

    fn get_dot_val_helper(
        &self,
        scope: &mut Scope,
        this_ptr: &mut Any,
        dot_rhs: &Expr,
    ) -> Result<Box<Any>, EvalAltResult> {
        use std::iter::once;

        match *dot_rhs {
            Expr::FnCall(ref fn_name, ref args) => {
                let mut args: Vec<Box<Any>> = args.iter()
                    .map(|arg| self.eval_expr(scope, arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let args = once(this_ptr)
                    .chain(args.iter_mut().map(|b| b.as_mut()))
                    .collect();

                self.call_fn_raw(fn_name.to_owned(), args)
            }
            Expr::Identifier(ref id) => {
                let get_fn_name = "get$".to_string() + id;

                self.call_fn_raw(get_fn_name, vec![this_ptr])
            }
            Expr::Index(ref id, ref idx_raw) => {
                let idx = self.eval_expr(scope, idx_raw)?;
                let get_fn_name = "get$".to_string() + id;

                let mut val = self.call_fn_raw(get_fn_name, vec![this_ptr])?;

                ((*val).downcast_mut() as Option<&mut Vec<Box<Any>>>)
                    .and_then(|arr| idx.downcast_ref::<i64>().map(|idx| (arr, *idx as usize)))
                    .map(|(arr, idx)| arr[idx].clone())
                    .ok_or(EvalAltResult::ErrorIndexMismatch)
            }
            Expr::Dot(ref inner_lhs, ref inner_rhs) => match **inner_lhs {
                Expr::Identifier(ref id) => {
                    let get_fn_name = "get$".to_string() + id;
                    self.call_fn_raw(get_fn_name, vec![this_ptr])
                        .and_then(|mut v| self.get_dot_val_helper(scope, v.as_mut(), inner_rhs))
                }
                _ => Err(EvalAltResult::InternalErrorMalformedDotExpression),
            },
            _ => Err(EvalAltResult::InternalErrorMalformedDotExpression),
        }
    }

    fn search_scope<'a, F, T>(
        scope: &'a mut Scope,
        id: &str,
        map: F,
    ) -> Result<(usize, T), EvalAltResult>
    where
        F: FnOnce(&'a mut Any) -> Result<T, EvalAltResult>,
    {
        scope
            .iter_mut()
            .enumerate()
            .rev()
            .find(|&(_, &mut (ref name, _))| *id == *name)
            .ok_or_else(|| EvalAltResult::ErrorVariableNotFound(id.to_owned()))
            .and_then(move |(idx, &mut (_, ref mut val))| map(val.as_mut()).map(|val| (idx, val)))
    }

    fn array_value(
        &self,
        scope: &mut Scope,
        id: &str,
        idx: &Expr,
    ) -> Result<(usize, usize, Box<Any>), EvalAltResult> {
        let idx_boxed = self.eval_expr(scope, idx)?
            .downcast::<i64>()
            .map_err(|_| EvalAltResult::ErrorIndexMismatch)?;
        let idx = *idx_boxed as usize;
        let (idx_sc, val) = Self::search_scope(scope, id, |val| {
            ((*val).downcast_mut() as Option<&mut Vec<Box<Any>>>)
                .map(|arr| arr[idx].clone())
                .ok_or(EvalAltResult::ErrorIndexMismatch)
        })?;

        Ok((idx_sc, idx, val))
    }

    fn get_dot_val(
        &self,
        scope: &mut Scope,
        dot_lhs: &Expr,
        dot_rhs: &Expr,
    ) -> Result<Box<Any>, EvalAltResult> {
        match *dot_lhs {
            Expr::Identifier(ref id) => {
                let (sc_idx, mut target) = Self::search_scope(scope, id, |x| Ok(x.box_clone()))?;
                let value = self.get_dot_val_helper(scope, target.as_mut(), dot_rhs);

                // In case the expression mutated `target`, we need to reassign it because
                // of the above `clone`.
                scope[sc_idx].1 = target;

                value
            }
            Expr::Index(ref id, ref idx_raw) => {
                let (sc_idx, idx, mut target) = self.array_value(scope, id, idx_raw)?;
                let value = self.get_dot_val_helper(scope, target.as_mut(), dot_rhs);

                // In case the expression mutated `target`, we need to reassign it because
                // of the above `clone`.
                scope[sc_idx].1.downcast_mut::<Vec<Box<Any>>>().unwrap()[idx] = target;

                value
            }
            _ => Err(EvalAltResult::InternalErrorMalformedDotExpression),
        }
    }

    fn set_dot_val_helper(
        &self,
        this_ptr: &mut Any,
        dot_rhs: &Expr,
        mut source_val: Box<Any>,
    ) -> Result<Box<Any>, EvalAltResult> {
        match *dot_rhs {
            Expr::Identifier(ref id) => {
                let set_fn_name = "set$".to_string() + id;
                self.call_fn_raw(set_fn_name, vec![this_ptr, source_val.as_mut()])
            }
            Expr::Dot(ref inner_lhs, ref inner_rhs) => match **inner_lhs {
                Expr::Identifier(ref id) => {
                    let get_fn_name = "get$".to_string() + id;
                    self.call_fn_raw(get_fn_name, vec![this_ptr])
                        .and_then(|mut v| {
                            self.set_dot_val_helper(v.as_mut(), inner_rhs, source_val)
                                .map(|_| v) // Discard Ok return value
                        })
                        .and_then(|mut v| {
                            let set_fn_name = "set$".to_string() + id;

                            self.call_fn_raw(set_fn_name, vec![this_ptr, v.as_mut()])
                        })
                }
                _ => Err(EvalAltResult::InternalErrorMalformedDotExpression),
            },
            _ => Err(EvalAltResult::InternalErrorMalformedDotExpression),
        }
    }

    fn set_dot_val(
        &self,
        scope: &mut Scope,
        dot_lhs: &Expr,
        dot_rhs: &Expr,
        source_val: Box<Any>,
    ) -> Result<Box<Any>, EvalAltResult> {
        match *dot_lhs {
            Expr::Identifier(ref id) => {
                let (sc_idx, mut target) = Self::search_scope(scope, id, |x| Ok(x.box_clone()))?;
                let value = self.set_dot_val_helper(target.as_mut(), dot_rhs, source_val);

                // In case the expression mutated `target`, we need to reassign it because
                // of the above `clone`.
                scope[sc_idx].1 = target;

                value
            }
            Expr::Index(ref id, ref idx_raw) => {
                let (sc_idx, idx, mut target) = self.array_value(scope, id, idx_raw)?;
                let value = self.set_dot_val_helper(target.as_mut(), dot_rhs, source_val);

                // In case the expression mutated `target`, we need to reassign it because
                // of the above `clone`.
                scope[sc_idx].1.downcast_mut::<Vec<Box<Any>>>().unwrap()[idx] = target;

                value
            }
            _ => Err(EvalAltResult::InternalErrorMalformedDotExpression),
        }
    }

    fn eval_expr(&self, scope: &mut Scope, expr: &Expr) -> Result<Box<Any>, EvalAltResult> {
        match *expr {
            Expr::IntConst(i) => Ok(Box::new(i)),
            Expr::FloatConst(i) => Ok(Box::new(i)),
            Expr::StringConst(ref s) => Ok(Box::new(s.clone())),
            Expr::CharConst(ref c) => Ok(Box::new(*c)),
            Expr::Identifier(ref id) => {
                for &mut (ref name, ref mut val) in &mut scope.iter_mut().rev() {
                    if *id == *name {
                        return Ok(val.clone());
                    }
                }
                Err(EvalAltResult::ErrorVariableNotFound(id.clone()))
            }
            Expr::Index(ref id, ref idx_raw) => {
                self.array_value(scope, id, idx_raw).map(|(_, _, x)| x)
            }
            Expr::Assignment(ref id, ref rhs) => {
                let rhs_val = self.eval_expr(scope, rhs)?;

                match **id {
                    Expr::Identifier(ref n) => {
                        for &mut (ref name, ref mut val) in &mut scope.iter_mut().rev() {
                            if *n == *name {
                                *val = rhs_val;

                                return Ok(Box::new(()));
                            }
                        }
                        Err(EvalAltResult::ErrorVariableNotFound(n.clone()))
                    }
                    Expr::Index(ref id, ref idx_raw) => {
                        let idx = self.eval_expr(scope, idx_raw)?;

                        for &mut (ref name, ref mut val) in &mut scope.iter_mut().rev() {
                            if *id == *name {
                                if let Some(i) = idx.downcast_ref::<i64>() {
                                    if let Some(arr_typed) =
                                        (*val).downcast_mut() as Option<&mut Vec<Box<Any>>>
                                    {
                                        arr_typed[*i as usize] = rhs_val;
                                        return Ok(Box::new(()));
                                    } else {
                                        return Err(EvalAltResult::ErrorIndexMismatch);
                                    }
                                } else {
                                    return Err(EvalAltResult::ErrorIndexMismatch);
                                }
                            }
                        }

                        Err(EvalAltResult::ErrorVariableNotFound(id.clone()))
                    }
                    Expr::Dot(ref dot_lhs, ref dot_rhs) => {
                        self.set_dot_val(scope, dot_lhs, dot_rhs, rhs_val)
                    }
                    _ => Err(EvalAltResult::ErrorAssignmentToUnknownLHS),
                }
            }
            Expr::Dot(ref lhs, ref rhs) => self.get_dot_val(scope, lhs, rhs),
            Expr::Array(ref contents) => {
                let mut arr = Vec::new();

                for item in &(*contents) {
                    let arg = self.eval_expr(scope, item)?;
                    arr.push(arg);
                }

                Ok(Box::new(arr))
            }
            Expr::FnCall(ref fn_name, ref args) => self.call_fn_raw(
                fn_name.to_owned(),
                args.iter()
                    .map(|ex| self.eval_expr(scope, ex))
                    .collect::<Result<Vec<Box<Any>>, _>>()?
                    .iter_mut()
                    .map(|b| b.as_mut())
                    .collect(),
            ),
            Expr::True => Ok(Box::new(true)),
            Expr::False => Ok(Box::new(false)),
            Expr::Unit => Ok(Box::new(())),
        }
    }

    fn eval_stmt(&self, scope: &mut Scope, stmt: &Stmt) -> Result<Box<Any>, EvalAltResult> {
        match *stmt {
            Stmt::Expr(ref e) => self.eval_expr(scope, e),
            Stmt::Block(ref b) => {
                let prev_len = scope.len();
                let mut last_result: Result<Box<Any>, EvalAltResult> = Ok(Box::new(()));

                for s in b.iter() {
                    last_result = self.eval_stmt(scope, s);
                    if let Err(x) = last_result {
                        last_result = Err(x);
                        break;
                    }
                }

                while scope.len() > prev_len {
                    scope.pop();
                }

                last_result
            }
            Stmt::If(ref guard, ref body) => {
                let guard_result = self.eval_expr(scope, guard)?;
                match guard_result.downcast::<bool>() {
                    Ok(g) => {
                        if *g {
                            self.eval_stmt(scope, body)
                        } else {
                            Ok(Box::new(()))
                        }
                    }
                    Err(_) => Err(EvalAltResult::ErrorIfGuardMismatch),
                }
            }
            Stmt::IfElse(ref guard, ref body, ref else_body) => {
                let guard_result = self.eval_expr(scope, guard)?;
                match guard_result.downcast::<bool>() {
                    Ok(g) => {
                        if *g {
                            self.eval_stmt(scope, body)
                        } else {
                            self.eval_stmt(scope, else_body)
                        }
                    }
                    Err(_) => Err(EvalAltResult::ErrorIfGuardMismatch),
                }
            }
            Stmt::While(ref guard, ref body) => loop {
                let guard_result = self.eval_expr(scope, guard)?;
                match guard_result.downcast::<bool>() {
                    Ok(g) => {
                        if *g {
                            match self.eval_stmt(scope, body) {
                                Err(EvalAltResult::LoopBreak) => return Ok(Box::new(())),
                                Err(x) => return Err(x),
                                _ => (),
                            }
                        } else {
                            return Ok(Box::new(()));
                        }
                    }
                    Err(_) => return Err(EvalAltResult::ErrorIfGuardMismatch),
                }
            },
            Stmt::Loop(ref body) => loop {
                match self.eval_stmt(scope, body) {
                    Err(EvalAltResult::LoopBreak) => return Ok(Box::new(())),
                    Err(x) => return Err(x),
                    _ => (),
                }
            },
            Stmt::Break => Err(EvalAltResult::LoopBreak),
            Stmt::Return => Err(EvalAltResult::Return(Box::new(()))),
            Stmt::ReturnWithVal(ref a) => {
                let result = self.eval_expr(scope, a)?;
                Err(EvalAltResult::Return(result))
            }
            Stmt::Var(ref name, ref init) => {
                match *init {
                    Some(ref v) => {
                        let i = self.eval_expr(scope, v)?;
                        scope.push((name.clone(), i));
                    }
                    None => scope.push((name.clone(), Box::new(()))),
                };
                Ok(Box::new(()))
            }
        }
    }

    fn nice_type_name(&self, b: Box<Any>) -> String {
        let tid = (&*b).type_id();
        if let Some(name) = self.type_names.get(&tid) {
            name.to_string()
        } else {
            format!("<unknown> {:?}", b.type_id())
        }
    }

    /// Evaluate a file
    pub fn eval_file<T: Any + Clone>(&mut self, fname: &str) -> Result<T, EvalAltResult> {
        use std::fs::File;
        use std::io::prelude::*;

        if let Ok(mut f) = File::open(fname) {
            let mut contents = String::new();

            if f.read_to_string(&mut contents).is_ok() {
                self.eval::<T>(&contents)
            } else {
                Err(EvalAltResult::ErrorCantOpenScriptFile)
            }
        } else {
            Err(EvalAltResult::ErrorCantOpenScriptFile)
        }
    }

    /// Evaluate a string
    pub fn eval<T: Any + Clone>(&mut self, input: &str) -> Result<T, EvalAltResult> {
        let mut scope: Scope = Vec::new();

        self.eval_with_scope(&mut scope, input)
    }

    /// Evaluate with own scope
    pub fn eval_with_scope<T: Any + Clone>(
        &mut self,
        scope: &mut Scope,
        input: &str,
    ) -> Result<T, EvalAltResult> {
        let tokens = lex(input);

        let mut peekables = tokens.peekable();
        let tree = parse(&mut peekables);

        match tree {
            Ok((ref os, ref fns)) => {
                let mut x: Result<Box<Any>, EvalAltResult> = Ok(Box::new(()));

                for f in fns {
                    let name = f.name.clone();
                    let local_f = f.clone();

                    let spec = FnSpec {
                        ident: name,
                        args: None,
                    };

                    self.fns.insert(spec, Arc::new(FnIntExt::Int(local_f)));
                }

                for o in os {
                    x = match self.eval_stmt(scope, o) {
                        Ok(v) => Ok(v),
                        Err(e) => return Err(e),
                    }
                }

                let x = x?;

                match x.downcast::<T>() {
                    Ok(out) => Ok(*out),
                    Err(a) => Err(EvalAltResult::ErrorMismatchOutputType(self.nice_type_name(a))),
                }
            }
            Err(_) => Err(EvalAltResult::ErrorFunctionArgMismatch),
        }
    }

    /// Evaluate a file, but only return errors, if there are any.
    /// Useful for when you don't need the result, but still need
    /// to keep track of possible errors
    pub fn consume_file(&mut self, fname: &str) -> Result<(), EvalAltResult> {
        use std::fs::File;
        use std::io::prelude::*;

        if let Ok(mut f) = File::open(fname) {
            let mut contents = String::new();

            if f.read_to_string(&mut contents).is_ok() {
                if let e @ Err(_) = self.consume(&contents) {
                    e
                } else {
                    Ok(())
                }
            } else {
                Err(EvalAltResult::ErrorCantOpenScriptFile)
            }
        } else {
            Err(EvalAltResult::ErrorCantOpenScriptFile)
        }
    }

    /// Evaluate a string, but only return errors, if there are any.
    /// Useful for when you don't need the result, but still need
    /// to keep track of possible errors
    pub fn consume(&mut self, input: &str) -> Result<(), EvalAltResult> {
        self.consume_with_scope(&mut Scope::new(), input)
    }

    /// Evaluate a string with own scoppe, but only return errors, if there are any.
    /// Useful for when you don't need the result, but still need
    /// to keep track of possible errors
    pub fn consume_with_scope(
        &mut self,
        scope: &mut Scope,
        input: &str,
    ) -> Result<(), EvalAltResult> {
        let tokens = lex(input);

        let mut peekables = tokens.peekable();
        let tree = parse(&mut peekables);

        match tree {
            Ok((ref os, ref fns)) => {
                for f in fns {
                    if f.params.len() > 6 {
                        return Ok(());
                    }
                    let name = f.name.clone();
                    let local_f = f.clone();

                    let spec = FnSpec {
                        ident: name,
                        args: None,
                    };

                    self.fns.insert(spec, Arc::new(FnIntExt::Int(local_f)));
                }

                for o in os {
                    if let Err(e) = self.eval_stmt(scope, o) {
                        return Err(e);
                    }
                }

                Ok(())
            }
            Err(_) => Err(EvalAltResult::ErrorFunctionArgMismatch),
        }
    }

    /// Register the default library. That means, numberic types, char, bool
    /// String, arithmetics and string concatenations.
    pub fn register_default_lib(engine: &mut Engine) {
        engine.register_type_name::<i32>("i32");
        engine.register_type_name::<u32>("u32");
        engine.register_type_name::<i64>("integer");
        engine.register_type_name::<u64>("u64");
        engine.register_type_name::<u64>("usize");
        engine.register_type_name::<f32>("f64");
        engine.register_type_name::<f64>("float");
        engine.register_type_name::<String>("string");
        engine.register_type_name::<char>("char");
        engine.register_type_name::<bool>("boolean");
        engine.register_type_name::<Vec<Box<Any>>>("array");

        macro_rules! reg_op {
            ($engine:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
                $(
                    $engine.register_fn($x, ($op as fn(x: $y, y: $y)->$y));
                )*
            )
        }

        macro_rules! reg_un {
            ($engine:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
                $(
                    $engine.register_fn($x, ($op as fn(x: $y)->$y));
                )*
            )
        }

        macro_rules! reg_cmp {
            ($engine:expr, $x:expr, $op:expr, $( $y:ty ),*) => (
                $(
                    $engine.register_fn($x, ($op as fn(x: $y, y: $y)->bool));
                )*
            )
        }

        fn add<T: Add>(x: T, y: T) -> <T as Add>::Output { x + y }
        fn sub<T: Sub>(x: T, y: T) -> <T as Sub>::Output { x - y }
        fn mul<T: Mul>(x: T, y: T) -> <T as Mul>::Output { x * y }
        fn div<T: Div>(x: T, y: T) -> <T as Div>::Output { x / y }
        fn neg<T: Neg>(x: T) -> <T as Neg>::Output       { -x }
        fn lt<T: PartialOrd>(x: T, y: T) -> bool  { x < y  }
        fn lte<T: PartialOrd>(x: T, y: T) -> bool { x <= y }
        fn gt<T: PartialOrd>(x: T, y: T) -> bool  { x > y  }
        fn gte<T: PartialOrd>(x: T, y: T) -> bool { x >= y }
        fn eq<T: PartialEq>(x: T, y: T) -> bool   { x == y }
        fn ne<T: PartialEq>(x: T, y: T) -> bool   { x != y }
        fn and(x: bool, y: bool) -> bool { x && y }
        fn or(x: bool, y: bool) -> bool  { x || y }
        fn not(x: bool) -> bool { !x }
        fn concat(x: String, y: String) -> String { x + &y }
        fn binary_and<T: BitAnd>(x: T, y: T) -> <T as BitAnd>::Output  { x & y }
        fn binary_or<T: BitOr>(x: T, y: T) -> <T as BitOr>::Output     { x | y }
        fn binary_xor<T: BitXor>(x: T, y: T) -> <T as BitXor>::Output  { x ^ y }
        fn left_shift<T: Shl<T>>(x: T, y: T) -> <T as Shl<T>>::Output  { x.shl(y) }
        fn right_shift<T: Shr<T>>(x: T, y: T) -> <T as Shr<T>>::Output { x.shr(y) }
        fn modulo<T: Rem<T>>(x: T, y: T) -> <T as Rem<T>>::Output { x % y }
        fn pow_i64_i64(x: i64, y: i64) -> i64 { x.pow(y as u32) }
        fn pow_f64_f64(x: f64, y: f64) -> f64 { x.powf(y) }
        fn pow_f64_i64(x: f64, y: i64) -> f64 { x.powi(y as i32) }
        fn unit_eq(a: (), b: ()) -> bool { true }

        reg_op!(engine, "+", add, i32, i64, u32, u64, f32, f64);
        reg_op!(engine, "-", sub, i32, i64, u32, u64, f32, f64);
        reg_op!(engine, "*", mul, i32, i64, u32, u64, f32, f64);
        reg_op!(engine, "/", div, i32, i64, u32, u64, f32, f64);

        reg_cmp!(engine, "<", lt, i32, i64, u32, u64, String, f64);
        reg_cmp!(engine, "<=", lte, i32, i64, u32, u64, String, f64);
        reg_cmp!(engine, ">", gt, i32, i64, u32, u64, String, f64);
        reg_cmp!(engine, ">=", gte, i32, i64, u32, u64, String, f64);
        reg_cmp!(engine, "==", eq, i32, i64, u32, u64, bool, String, f64);
        reg_cmp!(engine, "!=", ne, i32, i64, u32, u64, bool, String, f64);

        reg_op!(engine, "||", or, bool);
        reg_op!(engine, "&&", and, bool);
        reg_op!(engine, "|", binary_or, i32, i64, u32, u64);
        reg_op!(engine, "|", or, bool);
        reg_op!(engine, "&", binary_and, i32, i64, u32, u64);
        reg_op!(engine, "&", and, bool);
        reg_op!(engine, "^", binary_xor, i32, i64, u32, u64);
        reg_op!(engine, "<<", left_shift, i32, i64, u32, u64);
        reg_op!(engine, ">>", right_shift, i32, i64, u32, u64);
        reg_op!(engine, "%", modulo, i32, i64, u32, u64);
        engine.register_fn("~", pow_i64_i64);
        engine.register_fn("~", pow_f64_f64);
        engine.register_fn("~", pow_f64_i64);

        reg_un!(engine, "-", neg, i32, i64, f32, f64);
        reg_un!(engine, "!", not, bool);

        engine.register_fn("+", concat);
        engine.register_fn("==", unit_eq);

        // engine.register_fn("[]", idx);
        // FIXME?  Registering array lookups are a special case because we want to return boxes
        // directly let ent = engine.fns.entry("[]".to_string()).or_insert_with(Vec::new);
        // (*ent).push(FnType::ExternalFn2(Box::new(idx)));

    }

    /// Make a new engine
    pub fn new() -> Engine {
        let mut engine = Engine {
            fns: HashMap::new(),
            type_names: HashMap::new(),
        };

        Engine::register_default_lib(&mut engine);

        engine
    }
}
