use crate::stdlib::{
    mem::replace,
    num::NonZeroUsize,
    rc::Rc,
    sync::Arc,
};

use crate::{
    engine::{
        KEYWORD_EVAL,
        KEYWORD_TYPE_OF,
        Engine,
        FunctionsLib,
    },
    token::Position,
    parser::{Expr, Stmt, AST},
    Dynamic,
    scope::{self, Scope},
    result::EvalAltResult,
};

use self::Instruction::*;

#[derive(Debug)]
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

    // TODO: Consider making functions first class?
    // TODO: The default_value option here... seems suspicious. Possibly always None?
    /// Stack [.., arg1, ..., argn] -> [.., ret]
    Call{ fn_name: String, args_len: u8, default_value: Option<Box<Dynamic>> },

    /// Stack [..., lhs, rhs] -> [.., lhs in rhs]
    In,
}

pub struct Bytecode {
    instructions: Vec<Instruction>,
    #[cfg(feature = "sync")] pub(crate) fn_lib: Arc<FunctionsLib>,
    #[cfg(not(feature = "sync"))] pub(crate) fn_lib: Rc<FunctionsLib>,
}

struct BytecodeBuilder {
    instructions: Vec<Instruction>,
    continue_target: u32,
    break_instrs: Vec<usize>,
}

#[derive(Debug, Clone)]
pub enum BuildError {
    AssignmentToConstant(String, Position),
    AssignmentToUnknownLHS(Position),
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
    pub fn eval_bytecode(&self, bytecode: &Bytecode) -> Result<Dynamic, EvalAltResult> {
        let mut stack: Vec<Dynamic> = vec![];
        let mut scope = Scope::new();
        let mut scope_ends = vec![];
        let mut instr_ptr: u32 = 0;

        // Dummy position and level for now
        let pos = Position::new(1, 0);
        let level = 0;

        println!("{:?}", bytecode.instructions);

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
                        Err(_) => return Err(EvalAltResult::ErrorLogicGuard(pos))
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
                        Err(_) => return Err(EvalAltResult::ErrorLogicGuard(pos))
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

                CreateVariable{ ref name } => {
                    let v = stack.pop().unwrap();
                    scope.push_dynamic(name.clone(), v);
                }

                SetVariable{ ref name } => {
                    let v = stack.pop().unwrap();
                    match scope.get(name) {
                        None => return Err(EvalAltResult::ErrorVariableNotFound(name.clone(), pos)),
                        Some((index, scope::EntryType::Normal)) => {
                            *scope.get_mut(index).0 = v;
                        }
                        Some((_, scope::EntryType::Constant)) => {
                            return Err(EvalAltResult::ErrorAssignmentToConstant(name.clone(), pos));
                        }
                    }
                }

                SearchVariable{ref name } => {
                    match scope.get_dynamic(name) {
                        Some(value) => stack.push(value),
                        None => return Err(EvalAltResult::ErrorVariableNotFound(name.clone(), pos)),
                    }
                }

                GetVariable{index} => {
                    stack.push(scope.get_mut(scope.len() - index.get()).0.clone())
                }

                Call{ ref fn_name, args_len, ref default_value } => {
                    let mut args = stack.split_off(stack.len() - args_len as usize);
                    // TODO: Remove before commit
                    assert_eq!(args.len(), args_len as usize);
                    
                    // TODO: This seems to be an entirely pointless allocation, but it's needed to fit the API.
                    // The AST execution implementation does the same thing. Despite passing mut pointers
                    // instead of just the value nothing but the Drop fn ever looks at the value again.
                    let mut args: Vec<_> = args.iter_mut().collect();

                    // TODO:
                    // - There is way too much computation done in this fn call for my taste
                    // - I'm not qutie sure what happens, but I don't think this ends up calling more bytecode
                    let v = self.exec_fn_call (
                        &bytecode.fn_lib,
                        fn_name,
                        &mut args,
                        default_value.as_ref().map(|val| &**val),
                        pos,
                        level
                    );

                    match v {
                        Ok(v) => stack.push(v),
                        Err(err) => return Err(*err)
                    }
                }

                Instruction::In => {
                    let rhs = stack.pop().unwrap();
                    let lhs = stack.pop().unwrap();
                    let v = self.eval_in_expr(
                        &bytecode.fn_lib,
                        lhs,
                        pos,
                        rhs,
                        pos,
                        level
                    ).map_err(|err| *err)?;
                    stack.push(v);
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
    pub fn from_ast(ast: &AST) -> Result<Self, BuildError> {
        let mut builder = BytecodeBuilder::new();
        builder.build_ast(ast)
    }
}

impl BytecodeBuilder {
    fn new() -> BytecodeBuilder {
        BytecodeBuilder {
            instructions: vec![],
            continue_target: !0,
            break_instrs: vec![],
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

    fn build_ast(&mut self, ast: &AST) -> Result<Bytecode, BuildError>{
        ast.0
            .iter()
            .try_for_each(|stmt| {
                self.build_stmt(stmt)
            })
            .map(|_success| {
                Bytecode {
                    instructions: replace(&mut self.instructions, vec![]),
                    fn_lib: ast.1.clone(),
                }
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
                for arg in arg_exprs.iter() {
                    self.build_expr(arg)?;
                }

                if fn_name == KEYWORD_EVAL 
                    && arg_exprs.len() == 1
                    // TODO: Overrides
                    // && !self.has_override(fn_lib, KEYWORD_EVAL)
                {
                    unimplemented!()
                }

                self.instructions.push(Call{ 
                    fn_name: fn_name.to_string(), 
                    args_len: arg_exprs.len() as u8,
                    default_value: def_val.clone()
                });
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