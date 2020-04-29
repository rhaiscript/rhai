use std::mem::replace;
use std::num::NonZeroUsize;

use crate::engine::KEYWORD_EVAL;
use crate::token::Position;
use crate::parser::{Expr, Stmt, AST};
use crate::Dynamic;

use self::Instruction::*;

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
    /// Stack [.., f] -> if let Some(v) = f() { [.., f, v] } else { [..] }
    CallIterFn{ end_target: u32 },

    // TODO: Intern strings somewhere and use indicies instead of String.
    /// Stack [.., v] -> [..]
    /// Scope.push(name, v)
    CreateVariable{ name: String },
    /// Stack [.., v] -> [..]
    /// Scopes[name] = v
    SetVariable{ name: String },
    /// Stack [..] -> [.., Scopes[name]]
    SearchVariable{ name: String, begin: Position },
    /// Stack [..] -> [.., Scopes[scopes.len() - index]]
    GetVariable{ index: NonZeroUsize },

    // TODO: Consider making functions first class?
    // TODO: The default_value option here... seems suspicious. Possibly always None?
    /// Stack [.., arg1, ..., argn] -> [.., ret]
    Call{ fn_name: String, default_value: Option<Box<Dynamic>> },

    /// Stack [..., lhs, rhs] -> [.., lhs in rhs]
    In,
    And,
    Or,
}

pub struct BytecodeExecution {
    stack: Vec<Dynamic>,
}

pub struct Bytecode {
    instructions: Vec<Instruction>,
}

struct BytecodeBuilder {
    instructions: Vec<Instruction>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StorageIdx {
    idx: u32
}

pub enum BuildError {
    AssignmentToConstant(String, Position),
    AssignmentToUnknownLHS(Position),
}

enum BuildAltResult {
    Err(BuildError),
    Return(StorageIdx),
    // Yield(StorageIdx),
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
                    instructions: replace(&mut self.instructions, vec![])
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
                self.build_expr(guard)?;
                
                let end_branch_instr = self.instructions.len();
                self.instructions.push(BranchIfNot{ target: !0 });

                // TODO: Deal with break...
                self.build_stmt(body)?;
                self.instructions.push(Pop);

                self.instructions.push(Branch{ target: cond_target });

                let end_target = self.instructions.len() as u32;
                self.instructions[end_branch_instr].set_target_branch(end_target);
            }

            Stmt::Loop(body) => {
                // TODO: Deal with break...
                let start_target = self.instructions.len() as u32;
                self.build_stmt(body)?;
                self.instructions.push(Branch{ target: start_target });
            }

            Stmt::For(name, expr, body) => {
                self.build_expr(expr)?;
                
                self.instructions.push(PushUnit);
                self.instructions.push(CreateVariable{ name: name.clone() });
                
                let call_iter_instr = self.instructions.len(); 
                let cond_target =  call_iter_instr as u32;
                self.instructions.push(CallIterFn{ end_target: !0 });
                self.instructions.push(SetVariable{ name: name.clone() });
                
                // TODO: Deal with break...
                self.build_stmt(stmt)?;
                self.instructions.push(Branch{ target: cond_target });

                let end_target = self.instructions.len() as u32;
                self.instructions[call_iter_instr].set_end_target_call_iter_fn(end_target);
            }

            Stmt::Continue(..) => unimplemented!(),
            Stmt::Break(..) => unimplemented!(),
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
            Expr::Variable(ref id, _, pos) =>
                self.instructions.push(SearchVariable{ name: id.clone(), begin: pos }),
            Expr::Property(_, _) => panic!("unexpected property."),

            Expr::Stmt(ref stmt, _) => self.build_stmt(stmt)?,

            Expr::Assignment(ref lhs, ref rhs, op_pos) => {
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
            
            Expr::FunctionCall(ref fn_name, ref arg_exprs, ref def_val, pos) => {
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

                self.instructions.push(Call{ fn_name: fn_name.to_string(), default_value: def_val.clone()});
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