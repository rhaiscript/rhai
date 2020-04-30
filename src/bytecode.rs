use crate::stdlib::{
    any::TypeId,
    collections::HashMap,
    mem::replace,
    num::NonZeroUsize,
    rc::Rc,
    sync::Arc,
    iter::once,
};

use crate::{
    calc_fn_hash,
    engine::{
        calc_fn_def,
        KEYWORD_DEBUG,
        KEYWORD_EVAL,
        KEYWORD_PRINT,
        KEYWORD_TYPE_OF,
        Engine,
        FunctionsLib,
        FnAny,
    },
    token::Position,
    parser::{Expr, Stmt, FnDef, AST},
    Dynamic,
    scope::{self, Scope},
    result::EvalAltResult,
};

use self::Instruction::*;

// #[derive(Debug)]
pub enum Instruction {
    /// Stack [.., v] -> [..]
    Pop,
    /// Stack [..] -> [.., ()]
    PushUnit,
    /// ScopeEnds [..] -> [.., scopes.len()]
    PushScope,
    /// ScopeEnds [.., v] -> [..]
    /// Scopes.truncate(v)
    PopScope,

    /// Stack [..] -> [.., true]
    TrueConstant,
    /// Stack [..] -> [.., false]
    FalseConstant,
    /// Stack [..] -> [.., v]
    IntegerConstant(i64),
    #[cfg(not(feature = "no_float"))]
    /// Stack [..] -> [.., f]
    FloatConstant(f64),
    /// Stack [..] -> [.., s]
    StringConstant(String),
    /// Stack [..] -> [.., c]
    CharConstant(char),

    Branch{ target: u32 },
    // TODO: At present the only use of the BranchIf bytecode is in short
    // circuiting Or statements. Consider if it is/isn't worth keeping around.
    /// Stack [.., v] -> [..], v must be a bool
    BranchIf{ target: u32 },
    /// Stack [.., v] -> [..], v must be a bool
    BranchIfNot{ target: u32 },
    /// Stack [.., arr] -> if let Some(v) = iter_fn::<arr>(arr) { [.., arr, v] } else { [..] }
    CallIterFn{ end_target: u32 },
    // TODO: Consider making functions first class?
    // TODO: The default_value option here... seems suspicious. Possibly always None?
    /// Stack [.., arg1, ..., argn] -> [.., ret]
    // Call{ fn_name: String, args_len: u8, default_value: Option<Box<Dynamic>> },
    CallBytecode{ instr: u32, params: Box<[String]> },
    /// Stack [.., arg1, ..., argn] -> [.., ret]

    // Foreign functions are dynamically dispatched based on the type of the arguments,
    CallForeign{ fn_name: String, args_len: u8 },
    Return,

    // TODO: Intern strings somewhere and use indicies instead of String.
    /// Stack [.., v] -> [..]
    /// Scope.push(name, v)
    CreateVariable{ name: String },
    /// Stack [.., v] -> [..]
    /// Scopes[name] = v
    SetVariable{ name: String },
    /// Stack [..] -> [.., Scopes[name]]
    SearchVariable{ name: String },
    /// Stack [..] -> [.., Scopes[scopes.len() - index]]
    GetVariable{ index: NonZeroUsize },

    /// Stack [..., lhs, rhs] -> [.., lhs in rhs]
    In,
}

pub struct Bytecode {
    instructions: Vec<Instruction>,
}

struct FnCall {
    name: String,
    param_len: usize,
    instr: usize,
}

struct BytecodeBuilder {
    instructions: Vec<Instruction>,
    continue_target: u32,
    break_instrs: Vec<usize>,

    fn_calls: Vec<FnCall>,

    // Map fn_def hash to FnDef and instruction offset.
    fn_lib: HashMap<u64, (FnDef, u32)>,
}

#[derive(Debug, Clone)]
pub enum BuildError {
    AssignmentToConstant(String, Position),
    AssignmentToUnknownLHS(Position),
}

impl BytecodeBuilder {
    fn get_function(&self, fn_name: &str, params: usize) -> Option<&(FnDef, u32)> {
        let hash = calc_fn_def(fn_name, params);
        self.fn_lib.get(&hash)
    }
    
    fn has_override(&self, engine: &Engine, name: &str) -> bool {
        let fn_hash = calc_fn_hash(name, once(TypeId::of::<String>()));
        let fn_def_hash = calc_fn_def(name, 1);

        // First check registered functions
        engine.functions.contains_key(&fn_hash)
            // Then check packages
            || engine.packages.iter().any(|p| p.functions.contains_key(&fn_hash))
            // Then check script-defined functions
            || self.fn_lib.contains_key(&fn_def_hash)
    }

    fn resolve_fn_calls(&mut self, engine: &Engine) {
        for &FnCall{ref name, param_len, instr} in &self.fn_calls {
            // Step 1: Check for eval (from Engine::eval_expr)
            if name == KEYWORD_EVAL
                && param_len == 1
                && !self.has_override(engine, KEYWORD_EVAL) 
            {
                unimplemented!("Eval not implemented in bytecode");
            }

            // Step 2: exec_fn_call
            match name.as_str() {
                KEYWORD_TYPE_OF if param_len == 1 && !self.has_override(engine, KEYWORD_TYPE_OF) => {
                    // let _type_name = engine.map_type_name(args[0].type_name());
                    unimplemented!("Need to pop value and push type_name... in one instr... probably a call");
                }
                // exec_fn_call also matches KEYWORD_EVAL here with the exact same set of
                // conditions as above. Since we handle it above it should never reach here.
                _ => { /* continue to call_fn_raw below */ }
            }

            // Step 3: call_fn_raw

            // Step 3.1 First search in script-defined functions (can override built-in)
            if let Some(&(ref fn_def, fn_instr)) = self.get_function(name, param_len) {
                self.instructions[instr] = CallBytecode{ instr: fn_instr, params: fn_def.params.clone().into_boxed_slice() };
                continue;
            }

            // Step 3.2 Search built-in's and external functions
            
            // Deferred until execution time because foreign functions are dispatched
            // based on the type of the arguments.
            self.instructions[instr] = CallForeign{ fn_name: name.clone(), args_len: param_len as u8 };
            continue;
            // TODO: Handle PRINT and DEBUG

            // NOTE: `def_val` filtered out at an earlier stage.
        }
    }
}

impl Engine {
    /// Evaluate bytecode
    /// 
    /// # Example
    /// 
    /// ```
    /// # fn main() -> Result<(), rhai::EvalAltResult> {
    /// use rhai::Engine;
    /// use rhai::bytecode::Bytecode;
    /// 
    /// let engine = Engine::new();
    /// 
    /// let ast = engine.compile("40 + 2").unwrap();
    /// let bytecode = Bytecode::from_ast(&ast).unwrap();
    /// let res = engine.eval_bytecode(&bytecode).unwrap();
    /// 
    /// assert_eq!(res.as_int().unwrap(), 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn eval_bytecode(&self, bytecode: &Bytecode) -> Result<Dynamic, Box<EvalAltResult>> {
        let mut stack: Vec<Dynamic> = vec![];
        // (fn_ptr, stack_size)
        let mut call_stack: Vec<(u32, u32)> = vec![];
        let mut scope = Scope::new();
        let mut scope_ends = vec![];
        let mut instr_ptr: u32 = 0;

        // TODO: Dummy position for now
        let pos = Position::new(1, 0);

        loop {
            match bytecode.instructions[instr_ptr as usize] {
                Instruction::Pop => { 
                    let p = stack.pop();
                    debug_assert!(p.is_some());
                },
                Instruction::PushUnit => stack.push(().into()),
                Instruction::PushScope => scope_ends.push(scope.len()),
                Instruction::PopScope => scope.rewind(scope_ends.pop().unwrap()),

                Instruction::TrueConstant => stack.push(true.into()),
                Instruction::FalseConstant => stack.push(false.into()),
                Instruction::IntegerConstant(i) => stack.push(i.into()),
                #[cfg(not(feature = "no_float"))]
                Instruction::FloatConstant(f) => stack.push(f.into()),
                Instruction::StringConstant(ref s) => stack.push(s.clone().into()),
                Instruction::CharConstant(c) => stack.push(c.into()),

                Instruction::Branch{ target } => {
                    instr_ptr = target;
                    continue
                }
                Instruction::BranchIf{ target } => {
                    let guard = stack.pop().unwrap().as_bool();

                    match guard {
                        Ok(true) => {
                            instr_ptr = target;
                            continue
                        }
                        Ok(false) => {}
                        Err(_) => return Err(Box::new(EvalAltResult::ErrorLogicGuard(pos)))
                    }
                }
                Instruction::BranchIfNot{ target } => {
                    let guard = stack.pop().unwrap().as_bool();

                    match guard {
                        Ok(false) => {
                            instr_ptr = target;
                            continue
                        }
                        Ok(true) => {}
                        Err(_) => return Err(Box::new(EvalAltResult::ErrorLogicGuard(pos)))
                    }
                }
                Instruction::CallIterFn{ end_target } => {

                    /*
                    let arr = stack.pop().unwrap();
                    let tid = arr.type_id();

                    // TODO: Really need to find this before now, or at least cache it somehow.
                    // Apart from being slow, this is pushing the bounds of correctness... if somehow
                    // the function this is referring to changed inside the iterator, the subsequent
                    // iterations would change which function they where using as the iter function.
                    if let Some(iter_fn) = self.type_iterators.get(&tid).or_else(|| {
                        self.packages
                            .iter()
                            .find(|pkg| pkg.type_iterators.contains_key(&tid))
                            .and_then(|pkg| pkg.type_iterators.get(&tid))
                    }) {
                        stack.push(iter_fn(arr))
                    }
                    */

                    unimplemented!()
                }

                Instruction::CallBytecode{ instr, ref params } => {
                    call_stack.push((instr_ptr, stack.len() as u32));
                    instr_ptr = instr;
                    scope.extend(
                        params.iter().rev().map(|name| 
                            (name.clone(), scope::EntryType::Normal, stack.pop().unwrap()))
                    );
                }

                Instruction::CallForeign{ ref fn_name, args_len } => {
                    let mut args = stack.split_off(stack.len() - args_len as usize);
                    // TODO: Remove once I confirm this is how split_off works
                    assert_eq!(args.len(), args_len as usize);

                    // TODO: This seems to be an entirely pointless allocation, but it's needed to fit the API.
                    // The AST execution implementation does the same thing. Despite passing mut pointers
                    // instead of just the value nothing but the Drop fn ever looks at the value again.
                    let mut args: Vec<_> = args.iter_mut().collect();
                    if let Some(func) = self.get_foreign_function(fn_name, args.iter().map(|a| a.type_id())) {
                        // Run external function
                        let result = func(&mut args, pos)?;
            
                        // See if the function match print/debug (which requires special processing)
                        return Ok(match fn_name.as_str() {
                            KEYWORD_PRINT => (self.print)(result.as_str().map_err(|type_name| {
                                Box::new(EvalAltResult::ErrorMismatchOutputType(
                                    type_name.into(),
                                    pos,
                                ))
                            })?)
                            .into(),
                            KEYWORD_DEBUG => (self.debug)(result.as_str().map_err(|type_name| {
                                Box::new(EvalAltResult::ErrorMismatchOutputType(
                                    type_name.into(),
                                    pos,
                                ))
                            })?)
                            .into(),
                            _ => result,
                        });
                    }
                }

                Instruction::Return => {

                }

                CreateVariable{ ref name } => {
                    let v = stack.pop().unwrap();
                    scope.push_dynamic(name.clone(), v);
                }

                SetVariable{ ref name } => {
                    let v = stack.pop().unwrap();
                    match scope.get(name) {
                        None => return Err(Box::new(EvalAltResult::ErrorVariableNotFound(name.clone(), pos))),
                        Some((index, scope::EntryType::Normal)) => {
                            *scope.get_mut(index).0 = v;
                        }
                        Some((_, scope::EntryType::Constant)) => {
                            return Err(Box::new(EvalAltResult::ErrorAssignmentToConstant(name.clone(), pos)));
                        }
                    }
                }

                SearchVariable{ref name } => {
                    match scope.get_dynamic(name) {
                        Some(value) => stack.push(value),
                        None => return Err(Box::new(EvalAltResult::ErrorVariableNotFound(name.clone(), pos))),
                    }
                }

                GetVariable{index} => {
                    stack.push(scope.get_mut(scope.len() - index.get()).0.clone())
                }

                Instruction::In => {
                    // Deal with in calling == properly
                    unimplemented!();
                    // let rhs = stack.pop().unwrap();
                    // let lhs = stack.pop().unwrap();
                    // let v = self.eval_in_expr(
                    //     &bytecode.fn_lib,
                    //     lhs,
                    //     pos,
                    //     rhs,
                    //     pos,
                    //     level
                    // ).map_err(|err| *err)?;
                    // stack.push(v);
                }
            }

            instr_ptr += 1;
        }
    }
}

impl Instruction {
    fn set_target_branch(&mut self, new_target: u32) {
        match self {
            &mut Branch{ ref mut target } => {
                debug_assert_eq!(*target, !0);
                *target = new_target;
            }
            _ => unreachable!()
        }
    }

    fn set_target_branch_if_not(&mut self, new_target: u32) {
        match self {
            &mut BranchIfNot{ ref mut target } => {
                debug_assert_eq!(*target, !0);
                *target = new_target;
            }
            _ => unreachable!()
        }
    }

    fn set_end_target_call_iter_fn(&mut self, new_target: u32) {
        match self {
            &mut CallIterFn{ ref mut end_target } => {
                debug_assert_eq!(*end_target, !0);
                *end_target = new_target;
            }
            _ => unreachable!()
        }
    }
}

impl Bytecode {
    pub fn from_ast(engine: &Engine, ast: &AST) -> Result<Self, BuildError> {
        let mut builder = BytecodeBuilder::new();
        builder.build_ast(engine, ast)
    }
}

impl BytecodeBuilder {
    fn new() -> BytecodeBuilder {
        BytecodeBuilder {
            instructions: vec![],
            continue_target: !0,
            break_instrs: vec![],
            fn_lib: HashMap::new(),
            fn_calls: vec![],
        }
    }

    fn save_continue_break(&mut self, new_continue_target: u32) -> (u32, Vec<usize>) {
        let old_continue_target = self.continue_target;
        let old_break_instrs = replace(&mut self.break_instrs, vec![]);
        self.continue_target = new_continue_target;
        (old_continue_target, old_break_instrs)
    }

    fn restore_continue_break(&mut self, (old_continue, old_break): (u32, Vec<usize>), end_target: u32) {
        self.continue_target = old_continue;
        let break_instrs = replace(&mut self.break_instrs, old_break);
        for instr in break_instrs {
            self.instructions[instr].set_target_branch(end_target);
        }
    }

    fn build_ast(&mut self, engine: &Engine, ast: &AST) -> Result<Bytecode, BuildError>{
        ast.0
            .iter()
            .try_for_each(|stmt| self.build_stmt(stmt))?;
        self.instructions.push(Return);
        
        for (&hash, fn_def) in ast.1.iter() {
            let fn_entry = self.instructions.len() as u32;
            self.build_stmt(&fn_def.body)?;
            self.instructions.push(Return);
            // TODO: Only store the parts of fn_def I need... or take advantage of the Rc/Arc.
            self.fn_lib.insert(hash, ((**fn_def).clone(), fn_entry));
        }

        self.resolve_fn_calls(engine);

        Ok(Bytecode {
            instructions: replace(&mut self.instructions, vec![]),
            // fn_lib: ast.1.clone(),
        })
    }

    fn build_stmt(&mut self, stmt: &Stmt) -> Result<(), BuildError> {
        match stmt {
            Stmt::Noop(_) => {
                self.instructions.push(PushUnit);
            },

            Stmt::Expr(expr) => {
                self.build_expr(expr)?;
                
                if let Expr::Assignment(_, _, _) = *expr.as_ref() {
                    // If it is an assignment, erase the result at the root
                    self.instructions.push(Pop);
                    self.instructions.push(PushUnit);
                }
            }

            Stmt::Block(block, _) => {
                self.instructions.push(PushScope);
                block.iter().try_for_each(|stmt| self.build_stmt(stmt))?;
                self.instructions.push(PopScope);
                // TODO: state.always_search = false;
            }

            Stmt::IfThenElse(guard, if_body, else_body) => {
                // NOTE: Error handling might differ here
                self.build_expr(guard)?;

                let else_branch_instr = self.instructions.len();
                self.instructions.push(BranchIfNot{ target: !0 });
                
                self.build_stmt(if_body)?;
                
                let end_branch_instr = self.instructions.len();
                self.instructions.push(Branch{ target: !0 });
                
                let else_target = self.instructions.len() as u32;
                self.instructions[else_branch_instr].set_target_branch_if_not(else_target);
                
                if let Some(else_body) = else_body {
                    self.build_stmt(else_body)?;
                }
                else {
                    self.instructions.push(PushUnit);
                }

                let end_target = self.instructions.len() as u32;
                self.instructions[end_branch_instr].set_target_branch(end_target);
            }

            Stmt::While(guard, body) => {
                // NOTE: Error handling might differ here.
                let cond_target = self.instructions.len() as u32;

                // NOTE: I can't seem to put break/continue statements in the condition right now, but
                // if that ever becomes possible, I need to be careful that they are handled the same here
                // as in the ast evaluation.
                let old_cont_break = self.save_continue_break(cond_target);

                self.build_expr(guard)?;
                
                let end_branch_instr = self.instructions.len();
                self.instructions.push(BranchIfNot{ target: !0 });

                self.build_stmt(body)?;
                self.instructions.push(Pop);

                self.instructions.push(Branch{ target: cond_target });

                let end_target = self.instructions.len() as u32;
                self.instructions[end_branch_instr].set_target_branch(end_target);
                self.restore_continue_break(old_cont_break, end_target)
            }

            Stmt::Loop(body) => {
                // TODO: Deal with break...
                let start_target = self.instructions.len() as u32;
                let old_cont_break = self.save_continue_break(start_target);
                self.build_stmt(body)?;
                self.instructions.push(Branch{ target: start_target });
                let end_target = self.instructions.len() as u32;
                self.restore_continue_break(old_cont_break, end_target);
            }

            Stmt::For(name, expr, body) => {
                self.build_expr(expr)?;
                
                self.instructions.push(PushScope);
                self.instructions.push(PushUnit);
                self.instructions.push(CreateVariable{ name: name.clone() });
                
                let call_iter_instr = self.instructions.len(); 
                let cond_target =  call_iter_instr as u32;
                let old_cont_break = self.save_continue_break(cond_target);

                self.instructions.push(CallIterFn{ end_target: !0 });
                self.instructions.push(SetVariable{ name: name.clone() });
                
                // TODO: Deal with break...
                self.build_stmt(body)?;
                self.instructions.push(Branch{ target: cond_target });

                let end_target = self.instructions.len() as u32;
                self.instructions[call_iter_instr].set_end_target_call_iter_fn(end_target);
                self.restore_continue_break(old_cont_break, end_target);
                self.instructions.push(PopScope);
            }

            Stmt::Continue(_) => {
                self.instructions.push(Branch{ target: self.continue_target });
            },
            Stmt::Break(_) => {
                self.break_instrs.push(self.instructions.len());
                self.instructions.push(Branch{ target: !0 });
            }
            Stmt::ReturnWithVal(..) => unimplemented!(),

            Stmt::Let(name, Some(expr), _) => {
                self.build_expr(expr)?;
                self.instructions.push(CreateVariable{ name: name.clone() });
            }

            Stmt::Let(name, None, _) => {
                self.instructions.push(PushUnit);
                self.instructions.push(CreateVariable{ name: name.clone() });
            }

            Stmt::Const(name, expr, _) if expr.is_constant() => {
                self.build_expr(expr)?;
                self.instructions.push(CreateVariable{ name: name.clone() });
            }

            // TODO: Either this should be returning an error not panicking, or it
            // should just be a debug_assert!. Same applies to the version of this
            // in engine.rs::eval_stmnt
            Stmt::Const(_, _, _) => panic!("constant expression not constant!"),
        };

        Ok(())
    }

    fn build_expr(&mut self, expr: &Expr) -> Result<(), BuildError> {
        match *expr {
            Expr::True(_) => self.instructions.push(TrueConstant),
            Expr::False(_) => self.instructions.push(FalseConstant),
            Expr::Unit(_) => self.instructions.push(PushUnit),
            Expr::IntegerConstant(i, _ ) => 
                self.instructions.push(IntegerConstant(i)),
            Expr::FloatConstant(f, _) =>
                self.instructions.push(FloatConstant(f)),
            Expr::StringConstant(ref s, _) =>
                self.instructions.push(StringConstant(s.clone())),
            Expr::CharConstant(c, _) =>
                self.instructions.push(CharConstant(c)),

            // TODO: Removed if !state.always_search
            Expr::Variable(_, Some(index), _) =>
                self.instructions.push(GetVariable{ index }),
            // TODO: Is this ever used without eval?
            Expr::Variable(ref id, _, _) =>
                self.instructions.push(SearchVariable{ name: id.clone() }),
            Expr::Property(_, _) => panic!("unexpected property."),

            Expr::Stmt(ref stmt, _) => self.build_stmt(stmt)?,

            Expr::Assignment(ref lhs, ref rhs, _) => {
                self.build_expr(rhs)?;

                match lhs.as_ref() {
                    // name = rhs
                    Expr::Variable(name, _, _) =>
                        // TODO: Losing out on the error reporting here.    
                        self.instructions.push(SetVariable{ name: name.clone() }),

                    // idx_lhs[idx_expr] = rhs
                    #[cfg(not(feature = "no_index"))]
                    Expr::Index(idx_lhs, idx_expr, op_pos) => {
                        unimplemented!()
                    }

                    // idx_lhs.dot_rhs = rhs
                    #[cfg(not(feature = "no_object"))]
                    Expr::Dot(dot_lhs, dot_rhs, _) => {
                        unimplemented!()
                    }

                    expr if expr.is_constant() => {
                        return Err(BuildError::AssignmentToConstant(
                            expr.get_constant_str(),
                            lhs.position()
                        ))
                    }

                    _ => return Err(BuildError::AssignmentToUnknownLHS(lhs.position()))
                }
            }

            #[cfg(not(feature = "no_index"))]
            Expr::Index(..) =>
                unimplemented!(),
            
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(..) => 
                unimplemented!(),

            #[cfg(not(feature = "no_index"))]
            Expr::Array(..) =>
                unimplemented!(),

            #[cfg(not(feature = "no_object"))]
            Expr::Map(..) =>
                unimplemented!(),
            
            Expr::FunctionCall(ref fn_name, ref arg_exprs, ref def_val, _) => {
                assert!(
                    def_val.is_none(), 
                    "The only time default value is (currently) used is in `In` expressions, which shouldn't go through this code path"
                );
                
                for arg in arg_exprs.iter() {
                    self.build_expr(arg)?;
                }

                let instr = self.instructions.len();
                self.fn_calls.push(FnCall {
                    instr,
                    name: fn_name.to_string(),
                    param_len: arg_exprs.len(),
                });
                // Dummy instruction, to be replaced
                self.instructions.push(Branch{ target: !0 });
            }

            Expr::In(ref lhs, ref rhs, _) => {
                self.build_expr(lhs)?;
                self.build_expr(rhs)?;
                self.instructions.push(In);
            }

            Expr::And(ref lhs, ref rhs, _) => {
                self.build_expr(lhs)?;
                let short_circuit_instr = self.instructions.len();

                self.instructions.push(BranchIfNot{ target: !0 });
                self.build_expr(rhs)?;
                
                let skip_true_instr = self.instructions.len();
                let short_circuit_target = self.instructions.len() as u32;
                self.instructions[short_circuit_instr].set_target_branch_if_not(short_circuit_target);

                self.instructions.push(FalseConstant);

                let skip_true_target = self.instructions.len() as u32;
                self.instructions[skip_true_instr].set_target_branch(skip_true_target);
            }

            Expr::Or(ref lhs, ref rhs, _) => {
                self.build_expr(lhs)?;
                let short_circuit_instr = self.instructions.len();

                self.instructions.push(BranchIf{ target: !0 });
                self.build_expr(rhs)?;
                
                let skip_true_instr = self.instructions.len();
                let short_circuit_target = self.instructions.len() as u32;
                self.instructions[short_circuit_instr].set_target_branch_if_not(short_circuit_target);

                self.instructions.push(TrueConstant);

                let skip_true_target = self.instructions.len() as u32;
                self.instructions[skip_true_instr].set_target_branch(skip_true_target);
            }
        }

        Ok(())
    }
}