//! Module implementing the [`AST`] optimizer.
#![cfg(not(feature = "no_optimize"))]

use crate::ast::{
    ASTFlags, Expr, FlowControl, OpAssignment, Stmt, StmtBlock, StmtBlockContainer,
    SwitchCasesCollection,
};
use crate::engine::{
    KEYWORD_DEBUG, KEYWORD_EVAL, KEYWORD_FN_PTR, KEYWORD_FN_PTR_CURRY, KEYWORD_PRINT,
    KEYWORD_TYPE_OF, OP_NOT,
};
use crate::eval::{Caches, GlobalRuntimeState};
use crate::func::builtin::get_builtin_binary_op_fn;
use crate::func::hashing::get_hasher;
use crate::module::ModuleFlags;
use crate::tokenizer::Token;
use crate::{
    calc_fn_hash, calc_fn_hash_full, Dynamic, Engine, FnArgsVec, FnPtr, ImmutableString, Position,
    Scope, AST,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    any::TypeId,
    borrow::Cow,
    convert::TryFrom,
    hash::{Hash, Hasher},
    mem,
};

/// Level of optimization performed.
///
/// Not available under `no_optimize`.
#[derive(Debug, Eq, PartialEq, Clone, Copy, Default, Hash)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[non_exhaustive]
pub enum OptimizationLevel {
    /// No optimization performed.
    None,
    /// Only perform simple optimizations without evaluating functions.
    #[default]
    Simple,
    /// Full optimizations performed, including evaluating functions.
    /// Take care that this may cause side effects as it essentially assumes that all functions are pure.
    Full,
}

/// Mutable state throughout an optimization pass.
#[derive(Debug, Clone)]
struct OptimizerState<'a> {
    /// Has the [`AST`] been changed during this pass?
    is_dirty: bool,
    /// Stack of variables/constants for constants propagation and strict variables checking.
    variables: Vec<(ImmutableString, Option<Cow<'a, Dynamic>>)>,
    /// Activate constants propagation?
    propagate_constants: bool,
    /// [`Engine`] instance for eager function evaluation.
    engine: &'a Engine,
    /// Optional [`Scope`].
    scope: Option<&'a Scope<'a>>,
    /// The global runtime state.
    global: GlobalRuntimeState,
    /// Function resolution caches.
    caches: Caches,
    /// Optimization level.
    optimization_level: OptimizationLevel,
}

impl<'a> OptimizerState<'a> {
    /// Create a new [`OptimizerState`].
    #[inline(always)]
    pub fn new(
        engine: &'a Engine,
        lib: &'a [crate::SharedModule],
        scope: Option<&'a Scope<'a>>,
        optimization_level: OptimizationLevel,
    ) -> Self {
        let mut _global = GlobalRuntimeState::new(engine);
        let _lib = lib;

        #[cfg(not(feature = "no_function"))]
        {
            _global.lib = _lib.into();
        }

        Self {
            is_dirty: false,
            variables: Vec::new(),
            propagate_constants: true,
            engine,
            scope,
            global: _global,
            caches: Caches::new(),
            optimization_level,
        }
    }
    /// Set the [`AST`] state to be dirty (i.e. changed).
    #[inline(always)]
    pub fn set_dirty(&mut self) {
        self.is_dirty = true;
    }
    /// Set the [`AST`] state to be not dirty (i.e. unchanged).
    #[inline(always)]
    pub fn clear_dirty(&mut self) {
        self.is_dirty = false;
    }
    /// Is the [`AST`] dirty (i.e. changed)?
    #[inline(always)]
    pub const fn is_dirty(&self) -> bool {
        self.is_dirty
    }
    /// Rewind the variables stack back to a specified size.
    #[inline(always)]
    pub fn rewind_var(&mut self, len: usize) {
        self.variables.truncate(len);
    }
    /// Add a new variable to the stack.
    ///
    /// `Some(value)` if literal constant (which can be used for constants propagation), `None` otherwise.
    #[inline(always)]
    pub fn push_var<'x: 'a>(&mut self, name: ImmutableString, value: Option<Cow<'x, Dynamic>>) {
        self.variables.push((name, value));
    }
    /// Look up a literal constant from the variables stack.
    #[inline]
    pub fn find_literal_constant(&self, name: &str) -> Option<&Dynamic> {
        self.variables
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .and_then(|(_, value)| value.as_deref())
    }
    /// Call a registered function
    #[inline]
    pub fn call_fn_with_const_args(
        &mut self,
        fn_name: &str,
        op_token: Option<&Token>,
        arg_values: &mut [Dynamic],
    ) -> Option<Dynamic> {
        self.engine
            .exec_native_fn_call(
                &mut self.global,
                &mut self.caches,
                fn_name,
                op_token,
                calc_fn_hash(None, fn_name, arg_values.len()),
                &mut arg_values.iter_mut().collect::<FnArgsVec<_>>(),
                false,
                true,
                Position::NONE,
            )
            .ok()
            .map(|(v, ..)| v)
    }
}

/// Optimize a block of [statements][Stmt].
fn optimize_stmt_block(
    mut statements: StmtBlockContainer,
    state: &mut OptimizerState,
    preserve_result: bool,
    is_internal: bool,
    reduce_return: bool,
) -> StmtBlockContainer {
    if statements.is_empty() {
        return statements;
    }

    let mut is_dirty = state.is_dirty();

    let is_pure = if is_internal {
        Stmt::is_internally_pure
    } else {
        Stmt::is_pure
    };

    // Flatten blocks
    while let Some(n) = statements.iter().position(
        |s| matches!(s, Stmt::Block(block, ..) if !block.iter().any(Stmt::is_block_dependent)),
    ) {
        let (first, second) = statements.split_at_mut(n);
        let mut stmt = second[0].take();
        let stmts = match stmt {
            Stmt::Block(ref mut block, ..) => block.statements_mut(),
            _ => unreachable!("Stmt::Block expected but gets {:?}", stmt),
        };
        statements = first
            .iter_mut()
            .map(mem::take)
            .chain(stmts.iter_mut().map(mem::take))
            .chain(second.iter_mut().skip(1).map(mem::take))
            .collect();

        is_dirty = true;
    }

    // Optimize
    loop {
        state.clear_dirty();

        let orig_constants_len = state.variables.len(); // Original number of constants in the state, for restore later
        let orig_propagate_constants = state.propagate_constants;

        // Remove everything following control flow breaking statements
        let mut dead_code = false;

        statements.retain(|stmt| {
            if dead_code {
                state.set_dirty();
                false
            } else if stmt.is_control_flow_break() {
                dead_code = true;
                true
            } else {
                true
            }
        });

        // Optimize each statement in the block
        statements.iter_mut().for_each(|stmt| {
            match stmt {
                Stmt::Var(x, options, ..) => {
                    optimize_expr(&mut x.1, state, false);

                    let value = if options.intersects(ASTFlags::CONSTANT) && x.1.is_constant() {
                        // constant literal
                        Some(Cow::Owned(x.1.get_literal_value().unwrap()))
                    } else {
                        // variable
                        None
                    };
                    state.push_var(x.0.name.clone(), value);
                }
                // Optimize the statement
                _ => optimize_stmt(stmt, state, preserve_result),
            }
        });

        // Remove all pure statements except the last one
        let mut index = 0;
        let mut first_non_constant = statements
            .iter()
            .rev()
            .position(|stmt| match stmt {
                stmt if !is_pure(stmt) => true,

                Stmt::Var(x, ..) if x.1.is_constant() => true,
                Stmt::Expr(e) if !e.is_constant() => true,

                #[cfg(not(feature = "no_module"))]
                Stmt::Import(x, ..) if !x.0.is_constant() => true,

                _ => false,
            })
            .map_or(0, |n| statements.len() - n - 1);

        while index < statements.len() {
            if preserve_result && index >= statements.len() - 1 {
                break;
            }
            match statements[index] {
                ref stmt if is_pure(stmt) && index >= first_non_constant => {
                    state.set_dirty();
                    statements.remove(index);
                }
                ref stmt if stmt.is_pure() => {
                    state.set_dirty();
                    if index < first_non_constant {
                        first_non_constant -= 1;
                    }
                    statements.remove(index);
                }
                _ => index += 1,
            }
        }

        // Remove all pure statements that do not return values at the end of a block.
        // We cannot remove anything for non-pure statements due to potential side-effects.
        if preserve_result {
            loop {
                match statements[..] {
                    // { return; } -> {}
                    [Stmt::Return(None, options, ..)]
                        if reduce_return && !options.intersects(ASTFlags::BREAK) =>
                    {
                        state.set_dirty();
                        statements.clear();
                    }
                    [ref stmt] if !stmt.returns_value() && is_pure(stmt) => {
                        state.set_dirty();
                        statements.clear();
                    }
                    // { ...; return; } -> { ... }
                    [.., ref last_stmt, Stmt::Return(None, options, ..)]
                        if reduce_return
                            && !options.intersects(ASTFlags::BREAK)
                            && !last_stmt.returns_value() =>
                    {
                        state.set_dirty();
                        statements.pop().unwrap();
                    }
                    // { ...; return val; } -> { ...; val }
                    [.., Stmt::Return(ref mut expr, options, pos)]
                        if reduce_return && !options.intersects(ASTFlags::BREAK) =>
                    {
                        state.set_dirty();
                        *statements.last_mut().unwrap() = expr
                            .as_mut()
                            .map_or_else(|| Stmt::Noop(pos), |e| Stmt::Expr(mem::take(e)));
                    }
                    // { ...; stmt; noop } -> done
                    [.., ref second_last_stmt, Stmt::Noop(..)]
                        if second_last_stmt.returns_value() =>
                    {
                        break
                    }
                    // { ...; stmt_that_returns; pure_non_value_stmt } -> { ...; stmt_that_returns; noop }
                    // { ...; stmt; pure_non_value_stmt } -> { ...; stmt }
                    [.., ref second_last_stmt, ref last_stmt]
                        if !last_stmt.returns_value() && is_pure(last_stmt) =>
                    {
                        state.set_dirty();
                        if second_last_stmt.returns_value() {
                            *statements.last_mut().unwrap() = Stmt::Noop(last_stmt.position());
                        } else {
                            statements.pop().unwrap();
                        }
                    }
                    _ => break,
                }
            }
        } else {
            loop {
                match statements[..] {
                    [ref stmt] if is_pure(stmt) => {
                        state.set_dirty();
                        statements.clear();
                    }
                    // { ...; return; } -> { ... }
                    [.., Stmt::Return(None, options, ..)]
                        if reduce_return && !options.intersects(ASTFlags::BREAK) =>
                    {
                        state.set_dirty();
                        statements.pop().unwrap();
                    }
                    // { ...; return pure_val; } -> { ... }
                    [.., Stmt::Return(Some(ref expr), options, ..)]
                        if reduce_return
                            && !options.intersects(ASTFlags::BREAK)
                            && expr.is_pure() =>
                    {
                        state.set_dirty();
                        statements.pop().unwrap();
                    }
                    [.., ref last_stmt] if is_pure(last_stmt) => {
                        state.set_dirty();
                        statements.pop().unwrap();
                    }
                    _ => break,
                }
            }
        }

        // Pop the stack and remove all the local constants
        state.rewind_var(orig_constants_len);
        state.propagate_constants = orig_propagate_constants;

        if !state.is_dirty() {
            break;
        }

        is_dirty = true;
    }

    if is_dirty {
        state.set_dirty();
    }

    statements.shrink_to_fit();
    statements
}

impl StmtBlock {
    #[inline(always)]
    #[must_use]
    fn take_statements(&mut self) -> StmtBlockContainer {
        mem::take(self.statements_mut())
    }
}

/// Is this [`Expr`] a constant that is hashable?
#[inline(always)]
fn is_hashable_constant(expr: &Expr) -> bool {
    match expr {
        _ if !expr.is_constant() => false,
        Expr::DynamicConstant(v, ..) => v.is_hashable(),
        _ => false,
    }
}

/// Optimize a [statement][Stmt].
fn optimize_stmt(stmt: &mut Stmt, state: &mut OptimizerState, preserve_result: bool) {
    #[inline(always)]
    #[must_use]
    fn is_variable_access(expr: &Expr, _non_qualified: bool) -> bool {
        match expr {
            #[cfg(not(feature = "no_module"))]
            Expr::Variable(x, ..) if _non_qualified && !x.1.is_empty() => false,
            Expr::Variable(..) => true,
            _ => false,
        }
    }

    match stmt {
        // var = var op expr => var op= expr
        Stmt::Assignment(x, ..)
            if !x.0.is_op_assignment()
                && is_variable_access(&x.1.lhs, true)
                && matches!(&x.1.rhs, Expr::FnCall(x2, ..)
                        if Token::lookup_symbol_from_syntax(&x2.name).map_or(false, |t| t.has_op_assignment())
                        && x2.args.len() == 2
                        && x2.args[0].get_variable_name(true) == x.1.lhs.get_variable_name(true)
                ) =>
        {
            match x.1.rhs {
                Expr::FnCall(ref mut x2, pos) => {
                    state.set_dirty();
                    x.0 = OpAssignment::new_op_assignment_from_base(&x2.name, pos);
                    x.1.rhs = x2.args[1].take();
                }
                ref expr => unreachable!("Expr::FnCall expected but gets {:?}", expr),
            }
        }

        // expr op= expr
        Stmt::Assignment(x, ..) => {
            if !is_variable_access(&x.1.lhs, false) {
                optimize_expr(&mut x.1.lhs, state, false);
            }
            optimize_expr(&mut x.1.rhs, state, false);
        }

        // if expr {}
        Stmt::If(x, ..) if x.body.is_empty() && x.branch.is_empty() => {
            let condition = &mut x.expr;
            state.set_dirty();

            let pos = condition.start_position();
            let mut expr = condition.take();
            optimize_expr(&mut expr, state, false);

            *stmt = if preserve_result {
                // -> { expr, Noop }
                Stmt::Block(
                    StmtBlock::new(
                        [Stmt::Expr(expr.into()), Stmt::Noop(pos)],
                        pos,
                        Position::NONE,
                    )
                    .into(),
                )
            } else {
                // -> expr
                Stmt::Expr(expr.into())
            };
        }
        // if false { if_block } -> Noop
        Stmt::If(x, ..)
            if matches!(x.expr, Expr::BoolConstant(false, ..)) && x.branch.is_empty() =>
        {
            match x.expr {
                Expr::BoolConstant(false, pos) => {
                    state.set_dirty();
                    *stmt = Stmt::Noop(pos);
                }
                _ => unreachable!("`Expr::BoolConstant`"),
            }
        }
        // if false { if_block } else { else_block } -> else_block
        Stmt::If(x, ..) if matches!(x.expr, Expr::BoolConstant(false, ..)) => {
            state.set_dirty();
            let body = x.branch.take_statements();
            *stmt = match optimize_stmt_block(body, state, preserve_result, true, false) {
                statements if statements.is_empty() => Stmt::Noop(x.branch.position()),
                statements => {
                    Stmt::Block(StmtBlock::new_with_span(statements, x.branch.span()).into())
                }
            }
        }
        // if true { if_block } else { else_block } -> if_block
        Stmt::If(x, ..) if matches!(x.expr, Expr::BoolConstant(true, ..)) => {
            state.set_dirty();
            let body = x.body.take_statements();
            *stmt = match optimize_stmt_block(body, state, preserve_result, true, false) {
                statements if statements.is_empty() => Stmt::Noop(x.body.position()),
                statements => {
                    Stmt::Block(StmtBlock::new_with_span(statements, x.body.span()).into())
                }
            }
        }
        // if expr { if_block } else { else_block }
        Stmt::If(x, ..) => {
            let FlowControl { expr, body, branch } = &mut **x;
            optimize_expr(expr, state, false);
            *body.statements_mut() =
                optimize_stmt_block(body.take_statements(), state, preserve_result, true, false);
            *branch.statements_mut() = optimize_stmt_block(
                branch.take_statements(),
                state,
                preserve_result,
                true,
                false,
            );
        }

        // switch const { ... }
        Stmt::Switch(x, pos) if is_hashable_constant(&x.0) => {
            let (
                match_expr,
                SwitchCasesCollection {
                    expressions,
                    cases,
                    ranges,
                    def_case,
                },
            ) = &mut **x;

            let value = match_expr.get_literal_value().unwrap();
            let hasher = &mut get_hasher();
            value.hash(hasher);
            let hash = hasher.finish();

            // First check hashes
            if let Some(case_blocks_list) = cases.get(&hash) {
                match &case_blocks_list[..] {
                    [] => (),
                    [index] => {
                        let mut b = mem::take(&mut expressions[*index]);
                        cases.clear();

                        if matches!(b.lhs, Expr::BoolConstant(true, ..)) {
                            // Promote the matched case
                            let mut statements = Stmt::Expr(b.rhs.take().into());
                            optimize_stmt(&mut statements, state, true);
                            *stmt = statements;
                        } else {
                            // switch const { case if condition => stmt, _ => def } => if condition { stmt } else { def }
                            optimize_expr(&mut b.lhs, state, false);

                            let branch = def_case.map_or(StmtBlock::NONE, |index| {
                                let mut def_stmt = Stmt::Expr(expressions[index].rhs.take().into());
                                optimize_stmt(&mut def_stmt, state, true);
                                def_stmt.into()
                            });
                            let body = Stmt::Expr(b.rhs.take().into()).into();
                            let expr = b.lhs.take();

                            *stmt = Stmt::If(
                                FlowControl { expr, body, branch }.into(),
                                match_expr.start_position(),
                            );
                        }

                        state.set_dirty();
                        return;
                    }
                    _ => {
                        for &index in case_blocks_list {
                            let mut b = mem::take(&mut expressions[index]);

                            if matches!(b.lhs, Expr::BoolConstant(true, ..)) {
                                // Promote the matched case
                                let mut statements = Stmt::Expr(b.rhs.take().into());
                                optimize_stmt(&mut statements, state, true);
                                *stmt = statements;
                                state.set_dirty();
                                return;
                            }
                        }
                    }
                }
            }

            // Then check ranges
            if !ranges.is_empty() {
                // Only one range or all ranges without conditions
                if ranges.len() == 1
                    || ranges
                        .iter()
                        .all(|r| matches!(expressions[r.index()].lhs, Expr::BoolConstant(true, ..)))
                {
                    if let Some(r) = ranges.iter().find(|r| r.contains(&value)) {
                        let range_block = &mut expressions[r.index()];

                        if matches!(range_block.lhs, Expr::BoolConstant(true, ..)) {
                            // Promote the matched case
                            let block = &mut expressions[r.index()];
                            let mut statements = Stmt::Expr(block.rhs.take().into());
                            optimize_stmt(&mut statements, state, true);
                            *stmt = statements;
                        } else {
                            let mut expr = range_block.lhs.take();

                            // switch const { range if condition => stmt, _ => def } => if condition { stmt } else { def }
                            optimize_expr(&mut expr, state, false);

                            let branch = def_case.map_or(StmtBlock::NONE, |index| {
                                let mut def_stmt = Stmt::Expr(expressions[index].rhs.take().into());
                                optimize_stmt(&mut def_stmt, state, true);
                                def_stmt.into()
                            });

                            let body = Stmt::Expr(expressions[r.index()].rhs.take().into()).into();

                            *stmt = Stmt::If(
                                FlowControl { expr, body, branch }.into(),
                                match_expr.start_position(),
                            );
                        }

                        state.set_dirty();
                        return;
                    }
                } else {
                    // Multiple ranges - clear the table and just keep the right ranges
                    if !cases.is_empty() {
                        state.set_dirty();
                        cases.clear();
                    }

                    let old_ranges_len = ranges.len();

                    ranges.retain(|r| r.contains(&value));

                    if ranges.len() != old_ranges_len {
                        state.set_dirty();
                    }

                    for r in ranges {
                        let b = &mut expressions[r.index()];
                        optimize_expr(&mut b.lhs, state, false);
                        optimize_expr(&mut b.rhs, state, false);
                    }
                    return;
                }
            }

            // Promote the default case
            state.set_dirty();

            match def_case {
                Some(index) => {
                    let mut def_stmt = Stmt::Expr(expressions[*index].rhs.take().into());
                    optimize_stmt(&mut def_stmt, state, true);
                    *stmt = def_stmt;
                }
                _ => *stmt = Stmt::Block(StmtBlock::empty(*pos).into()),
            }
        }
        // switch
        Stmt::Switch(x, ..) => {
            let (
                match_expr,
                SwitchCasesCollection {
                    expressions,
                    cases,
                    ranges,
                    def_case,
                    ..
                },
            ) = &mut **x;

            optimize_expr(match_expr, state, false);

            // Optimize blocks
            for b in &mut *expressions {
                optimize_expr(&mut b.lhs, state, false);
                optimize_expr(&mut b.rhs, state, false);

                if matches!(b.lhs, Expr::BoolConstant(false, ..)) && !b.rhs.is_unit() {
                    b.rhs = Expr::Unit(b.rhs.position());
                    state.set_dirty();
                }
            }

            // Remove false cases
            cases.retain(|_, list| {
                // Remove all entries that have false conditions
                list.retain(|index| {
                    if matches!(expressions[*index].lhs, Expr::BoolConstant(false, ..)) {
                        state.set_dirty();
                        false
                    } else {
                        true
                    }
                });
                // Remove all entries after a `true` condition
                if let Some(n) = list.iter().position(|&index| {
                    matches!(expressions[index].lhs, Expr::BoolConstant(true, ..))
                }) {
                    if n + 1 < list.len() {
                        state.set_dirty();
                        list.truncate(n + 1);
                    }
                }
                // Remove if no entry left
                if list.is_empty() {
                    state.set_dirty();
                    false
                } else {
                    true
                }
            });

            // Remove false ranges
            ranges.retain(|r| {
                if matches!(expressions[r.index()].lhs, Expr::BoolConstant(false, ..)) {
                    state.set_dirty();
                    false
                } else {
                    true
                }
            });

            if let Some(index) = def_case {
                optimize_expr(&mut expressions[*index].rhs, state, false);
            }

            // Remove unused block statements
            expressions.iter_mut().enumerate().for_each(|(index, b)| {
                if *def_case != Some(index)
                    && cases.values().flat_map(|c| c.iter()).all(|&n| n != index)
                    && ranges.iter().all(|r| r.index() != index)
                    && !b.rhs.is_unit()
                {
                    b.rhs = Expr::Unit(b.rhs.position());
                    state.set_dirty();
                }
            });
        }

        // while false { block } -> Noop
        Stmt::While(x, ..) if matches!(x.expr, Expr::BoolConstant(false, ..)) => match x.expr {
            Expr::BoolConstant(false, pos) => {
                state.set_dirty();
                *stmt = Stmt::Noop(pos);
            }
            _ => unreachable!("`Expr::BoolConstant"),
        },
        // while expr { block }
        Stmt::While(x, ..) => {
            let FlowControl { expr, body, .. } = &mut **x;
            optimize_expr(expr, state, false);
            if let Expr::BoolConstant(true, pos) = expr {
                *expr = Expr::Unit(*pos);
            }
            *body.statements_mut() =
                optimize_stmt_block(body.take_statements(), state, false, true, false);
        }
        // do { block } while|until expr
        Stmt::Do(x, ..) => {
            optimize_expr(&mut x.expr, state, false);
            *x.body.statements_mut() =
                optimize_stmt_block(x.body.take_statements(), state, false, true, false);
        }
        // for id in expr { block }
        Stmt::For(x, ..) => {
            optimize_expr(&mut x.2.expr, state, false);
            *x.2.body.statements_mut() =
                optimize_stmt_block(x.2.body.take_statements(), state, false, true, false);
        }
        // let id = expr;
        Stmt::Var(x, options, ..) if !options.intersects(ASTFlags::CONSTANT) => {
            optimize_expr(&mut x.1, state, false);
        }
        // import expr as var;
        #[cfg(not(feature = "no_module"))]
        Stmt::Import(x, ..) => optimize_expr(&mut x.0, state, false),
        // { block }
        Stmt::Block(block) => {
            let mut stmts =
                optimize_stmt_block(block.take_statements(), state, preserve_result, true, false);

            match stmts.as_mut_slice() {
                [] => {
                    state.set_dirty();
                    *stmt = Stmt::Noop(block.span().start());
                }
                // Only one statement which is not block-dependent - promote
                [s] if !s.is_block_dependent() => {
                    state.set_dirty();
                    *stmt = s.take();
                }
                _ => *block.statements_mut() = stmts,
            }
        }
        // try { pure try_block } catch ( var ) { catch_block } -> try_block
        Stmt::TryCatch(x, ..) if x.body.iter().all(Stmt::is_pure) => {
            // If try block is pure, there will never be any exceptions
            state.set_dirty();
            *x.body.statements_mut() =
                optimize_stmt_block(x.body.take_statements(), state, false, true, false);
        }
        // try { try_block } catch ( var ) { catch_block }
        Stmt::TryCatch(x, ..) => {
            *x.body.statements_mut() =
                optimize_stmt_block(x.body.take_statements(), state, false, true, false);
            *x.branch.statements_mut() =
                optimize_stmt_block(x.branch.take_statements(), state, false, true, false);
        }

        // expr(stmt)
        Stmt::Expr(expr) if matches!(**expr, Expr::Stmt(..)) => {
            state.set_dirty();
            match expr.as_mut() {
                Expr::Stmt(block) if !block.is_empty() => {
                    let mut stmts_blk = mem::take(block.as_mut());
                    *stmts_blk.statements_mut() =
                        optimize_stmt_block(stmts_blk.take_statements(), state, true, true, false);
                    *stmt = Stmt::Block(stmts_blk.into());
                }
                Expr::Stmt(..) => *stmt = Stmt::Noop(expr.position()),
                _ => unreachable!("`Expr::Stmt`"),
            }
        }

        // expr(func())
        Stmt::Expr(expr) if matches!(**expr, Expr::FnCall(..)) => {
            state.set_dirty();
            match expr.take() {
                Expr::FnCall(x, pos) => *stmt = Stmt::FnCall(x, pos),
                _ => unreachable!(),
            }
        }

        Stmt::Expr(expr) => optimize_expr(expr, state, false),

        // func(...)
        Stmt::FnCall(..) => match stmt.take() {
            Stmt::FnCall(x, pos) => {
                let mut expr = Expr::FnCall(x, pos);
                optimize_expr(&mut expr, state, false);
                *stmt = match expr {
                    Expr::FnCall(x, pos) => Stmt::FnCall(x, pos),
                    _ => Stmt::Expr(expr.into()),
                }
            }
            _ => unreachable!(),
        },

        // break expr;
        Stmt::BreakLoop(Some(ref mut expr), ..) => optimize_expr(expr, state, false),

        // return expr;
        Stmt::Return(Some(ref mut expr), ..) => optimize_expr(expr, state, false),

        // Share nothing
        #[cfg(not(feature = "no_closure"))]
        Stmt::Share(x) if x.is_empty() => {
            state.set_dirty();
            *stmt = Stmt::Noop(Position::NONE);
        }
        // Share constants
        #[cfg(not(feature = "no_closure"))]
        Stmt::Share(x) => {
            let orig_len = x.len();

            if state.propagate_constants {
                x.retain(|(v, _)| state.find_literal_constant(v.as_str()).is_none());

                if x.len() != orig_len {
                    state.set_dirty();
                }
            }
        }

        // All other statements - skip
        _ => (),
    }
}

/// Optimize an [expression][Expr].
fn optimize_expr(expr: &mut Expr, state: &mut OptimizerState, _chaining: bool) {
    // These keywords are handled specially
    const DONT_EVAL_KEYWORDS: &[&str] = &[
        KEYWORD_PRINT, // side effects
        KEYWORD_DEBUG, // side effects
        KEYWORD_EVAL,  // arbitrary scripts
    ];

    match expr {
        // {}
        Expr::Stmt(x) if x.is_empty() => { state.set_dirty(); *expr = Expr::Unit(x.position()) }
        Expr::Stmt(x) if x.len() == 1 && matches!(x.statements()[0], Stmt::Expr(..)) => {
            state.set_dirty();
            match x.take_statements().remove(0) {
                Stmt::Expr(mut e) => {
                    optimize_expr(&mut e, state, false);
                    *expr = *e;
                }
                _ => unreachable!("`Expr::Stmt`")
            }
        }
        // { stmt; ... } - do not count promotion as dirty because it gets turned back into an array
        Expr::Stmt(x) => {
            *x.statements_mut() = optimize_stmt_block(x.take_statements(), state, true, true, false);

            // { Stmt(Expr) } - promote
            if let [ Stmt::Expr(e) ] = x.statements_mut().as_mut() { state.set_dirty(); *expr = e.take(); }
        }
        // ()?.rhs
        #[cfg(not(feature = "no_object"))]
        Expr::Dot(x, options, ..) if options.intersects(ASTFlags::NEGATED) && matches!(x.lhs, Expr::Unit(..)) => {
            state.set_dirty();
            *expr = x.lhs.take();
        }
        // lhs.rhs
        #[cfg(not(feature = "no_object"))]
        Expr::Dot(x, ..) if !_chaining => match (&mut x.lhs, &mut x.rhs) {
            // map.string
            (Expr::Map(m, pos), Expr::Property(p, ..)) if m.0.iter().all(|(.., x)| x.is_pure()) => {
                // Map literal where everything is pure - promote the indexed item.
                // All other items can be thrown away.
                state.set_dirty();
                *expr = mem::take(&mut m.0).into_iter().find(|(x, ..)| x.name == p.2)
                            .map_or_else(|| Expr::Unit(*pos), |(.., mut expr)| { expr.set_position(*pos); expr });
            }
            // var.rhs or this.rhs
            (Expr::Variable(..) | Expr::ThisPtr(..), rhs) => optimize_expr(rhs, state, true),
            // const.type_of()
            (lhs, Expr::MethodCall(x, pos)) if lhs.is_constant() && x.name == KEYWORD_TYPE_OF && x.args.is_empty() => {
                if let Some(value) = lhs.get_literal_value() {
                    state.set_dirty();
                    let typ = state.engine.map_type_name(value.type_name()).into();
                    *expr = Expr::from_dynamic(typ, *pos);
                }
            }
            // const.is_shared()
            #[cfg(not(feature = "no_closure"))]
            (lhs, Expr::MethodCall(x, pos)) if lhs.is_constant() && x.name == crate::engine::KEYWORD_IS_SHARED && x.args.is_empty() => {
                if lhs.get_literal_value().is_some() {
                    state.set_dirty();
                    *expr = Expr::from_dynamic(Dynamic::FALSE, *pos);
                }
            }
            // lhs.rhs
            (lhs, rhs) => { optimize_expr(lhs, state, false); optimize_expr(rhs, state, true); }
        }
        // ....lhs.rhs
        #[cfg(not(feature = "no_object"))]
        Expr::Dot(x,..) => { optimize_expr(&mut x.lhs, state, false); optimize_expr(&mut x.rhs, state, _chaining); }

        // ()?[rhs]
        #[cfg(not(feature = "no_index"))]
        Expr::Index(x, options, ..) if options.intersects(ASTFlags::NEGATED) && matches!(x.lhs, Expr::Unit(..)) => {
            state.set_dirty();
            *expr = x.lhs.take();
        }
        // lhs[rhs]
        #[cfg(not(feature = "no_index"))]
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Expr::Index(x, ..) if !_chaining => match (&mut x.lhs, &mut x.rhs) {
            // array[int]
            (Expr::Array(a, pos), Expr::IntegerConstant(i, ..)) if *i >= 0 && *i <= crate::MAX_USIZE_INT && (*i as usize) < a.len() && a.iter().all(Expr::is_pure) => {
                // Array literal where everything is pure - promote the indexed item.
                // All other items can be thrown away.
                state.set_dirty();
                let mut result = a[*i as usize].take();
                result.set_position(*pos);
                *expr = result;
            }
            // array[-int]
            #[allow(clippy::unnecessary_cast)]
            (Expr::Array(a, pos), Expr::IntegerConstant(i, ..)) if *i < 0 && i.unsigned_abs() as u64 <= a.len() as u64 && a.iter().all(Expr::is_pure) => {
                // Array literal where everything is pure - promote the indexed item.
                // All other items can be thrown away.
                state.set_dirty();
                let index = a.len() - i.unsigned_abs() as usize;
                let mut result = a[index].take();
                result.set_position(*pos);
                *expr = result;
            }
            // map[string]
            (Expr::Map(m, pos), Expr::StringConstant(s, ..)) if m.0.iter().all(|(.., x)| x.is_pure()) => {
                // Map literal where everything is pure - promote the indexed item.
                // All other items can be thrown away.
                state.set_dirty();
                *expr = mem::take(&mut m.0).into_iter().find(|(x, ..)| x.name == s)
                            .map_or_else(|| Expr::Unit(*pos), |(.., mut expr)| { expr.set_position(*pos); expr });
            }
            // int[int]
            (Expr::IntegerConstant(n, pos), Expr::IntegerConstant(i, ..)) if *i >= 0 && *i <= crate::MAX_USIZE_INT && (*i as usize) < crate::INT_BITS => {
                // Bit-field literal indexing - get the bit
                state.set_dirty();
                *expr = Expr::BoolConstant((*n & (1 << (*i as usize))) != 0, *pos);
            }
            // int[-int]
            #[allow(clippy::unnecessary_cast)]
            (Expr::IntegerConstant(n, pos), Expr::IntegerConstant(i, ..)) if *i < 0 && i.unsigned_abs() as u64 <= crate::INT_BITS as u64 => {
                // Bit-field literal indexing - get the bit
                state.set_dirty();
                *expr = Expr::BoolConstant((*n & (1 << (crate::INT_BITS - i.unsigned_abs() as usize))) != 0, *pos);
            }
            // string[int]
            #[allow(clippy::unnecessary_cast)]
            (Expr::StringConstant(s, pos), Expr::IntegerConstant(i, ..)) if *i >= 0 && *i <= crate::MAX_USIZE_INT && (*i as usize) < s.chars().count() => {
                // String literal indexing - get the character
                state.set_dirty();
                *expr = Expr::CharConstant(s.chars().nth(*i as usize).unwrap(), *pos);
            }
            // string[-int]
            #[allow(clippy::unnecessary_cast)]
            (Expr::StringConstant(s, pos), Expr::IntegerConstant(i, ..)) if *i < 0 && i.unsigned_abs() as u64 <= s.chars().count() as u64 => {
                // String literal indexing - get the character
                state.set_dirty();
                *expr = Expr::CharConstant(s.chars().rev().nth(i.unsigned_abs() as usize - 1).unwrap(), *pos);
            }
            // var[rhs] or this[rhs]
            (Expr::Variable(..) | Expr::ThisPtr(..), rhs) => optimize_expr(rhs, state, true),
            // lhs[rhs]
            (lhs, rhs) => { optimize_expr(lhs, state, false); optimize_expr(rhs, state, true); }
        },
        // ...[lhs][rhs]
        #[cfg(not(feature = "no_index"))]
        Expr::Index(x, ..) => { optimize_expr(&mut x.lhs, state, false); optimize_expr(&mut x.rhs, state, _chaining); }
        // ``
        Expr::InterpolatedString(x, pos) if x.is_empty() => {
            state.set_dirty();
            *expr = Expr::StringConstant(state.engine.const_empty_string(), *pos);
        }
        // `... ${const} ...`
        Expr::InterpolatedString(..) if expr.is_constant() => {
            state.set_dirty();
            *expr = Expr::StringConstant(expr.get_literal_value().unwrap().cast::<ImmutableString>(), expr.position());
        }
        // `... ${ ... } ...`
        Expr::InterpolatedString(x, ..) => {
            x.iter_mut().for_each(|expr| optimize_expr(expr, state, false));

            let mut n = 0;

            // Merge consecutive strings
            while n < x.len() - 1 {
                match (x[n].take(),x[n+1].take()) {
                    (Expr::StringConstant(mut s1, pos), Expr::StringConstant(s2, ..)) => { s1 += s2; x[n] = Expr::StringConstant(s1, pos); x.remove(n+1); state.set_dirty(); }
                    (expr1, Expr::Unit(..)) => { x[n] = expr1; x.remove(n+1); state.set_dirty(); }
                    (Expr::Unit(..), expr2) => { x[n+1] = expr2; x.remove(n); state.set_dirty(); }
                    (expr1, Expr::StringConstant(s, ..)) if s.is_empty() => { x[n] = expr1; x.remove(n+1); state.set_dirty(); }
                    (Expr::StringConstant(s, ..), expr2) if s.is_empty()=> { x[n+1] = expr2; x.remove(n); state.set_dirty(); }
                    (expr1, expr2) => { x[n] = expr1; x[n+1] = expr2; n += 1; }
                }
            }

            x.shrink_to_fit();
        }
        // [ constant .. ]
        #[cfg(not(feature = "no_index"))]
        Expr::Array(..) if expr.is_constant() => {
            state.set_dirty();
            *expr = Expr::DynamicConstant(expr.get_literal_value().unwrap().into(), expr.position());
        }
        // [ items .. ]
        #[cfg(not(feature = "no_index"))]
        Expr::Array(x, ..) => x.iter_mut().for_each(|expr| optimize_expr(expr, state, false)),
        // #{ key:constant, .. }
        #[cfg(not(feature = "no_object"))]
        Expr::Map(..) if expr.is_constant() => {
            state.set_dirty();
            *expr = Expr::DynamicConstant(expr.get_literal_value().unwrap().into(), expr.position());
        }
        // #{ key:value, .. }
        #[cfg(not(feature = "no_object"))]
        Expr::Map(x, ..) => x.0.iter_mut().for_each(|(.., expr)| optimize_expr(expr, state, false)),
        // lhs && rhs
        Expr::And(x, ..) => match (&mut x.lhs, &mut x.rhs) {
            // true && rhs -> rhs
            (Expr::BoolConstant(true, ..), rhs) => { state.set_dirty(); optimize_expr(rhs, state, false); *expr = rhs.take(); }
            // false && rhs -> false
            (Expr::BoolConstant(false, pos), ..) => { state.set_dirty(); *expr = Expr::BoolConstant(false, *pos); }
            // lhs && true -> lhs
            (lhs, Expr::BoolConstant(true, ..)) => { state.set_dirty(); optimize_expr(lhs, state, false); *expr = lhs.take(); }
            // lhs && rhs
            (lhs, rhs) => { optimize_expr(lhs, state, false); optimize_expr(rhs, state, false); }
        },
        // lhs || rhs
        Expr::Or(ref mut x, ..) => match (&mut x.lhs, &mut x.rhs) {
            // false || rhs -> rhs
            (Expr::BoolConstant(false, ..), rhs) => { state.set_dirty(); optimize_expr(rhs, state, false); *expr = rhs.take(); }
            // true || rhs -> true
            (Expr::BoolConstant(true, pos), ..) => { state.set_dirty(); *expr = Expr::BoolConstant(true, *pos); }
            // lhs || false
            (lhs, Expr::BoolConstant(false, ..)) => { state.set_dirty(); optimize_expr(lhs, state, false); *expr = lhs.take(); }
            // lhs || rhs
            (lhs, rhs) => { optimize_expr(lhs, state, false); optimize_expr(rhs, state, false); }
        },
        // () ?? rhs -> rhs
        Expr::Coalesce(x, ..) if matches!(x.lhs, Expr::Unit(..)) => {
            state.set_dirty();
            *expr = x.rhs.take();
        },
        // lhs:constant ?? rhs -> lhs
        Expr::Coalesce(x, ..) if x.lhs.is_constant() => {
            state.set_dirty();
            *expr = x.lhs.take();
        },

        // !true or !false
        Expr::FnCall(x,..)
            if x.name == OP_NOT
            && x.args.len() == 1
            && matches!(x.args[0], Expr::BoolConstant(..))
        => {
            state.set_dirty();
            match x.args[0] {
                Expr::BoolConstant(b, pos) => *expr = Expr::BoolConstant(!b, pos),
                _ => unreachable!(),
            }
        }

        // eval!
        Expr::FnCall(x, ..) if x.name == KEYWORD_EVAL => {
            state.propagate_constants = false;
        }
        // Fn
        Expr::FnCall(x, pos)
            if !x.is_qualified() // Non-qualified
            && x.args.len() == 1
            && x.name == KEYWORD_FN_PTR
            && x.constant_args()
        => {
            let fn_name = match x.args[0] {
                Expr::StringConstant(ref s, ..) => s.clone().into(),
                _ => Dynamic::UNIT
            };

            if let Ok(fn_ptr) = fn_name.into_immutable_string().map_err(Into::into).and_then(FnPtr::try_from) {
                state.set_dirty();
                *expr = Expr::DynamicConstant(Box::new(fn_ptr.into()), *pos);
            } else {
                optimize_expr(&mut x.args[0], state, false);
            }
        }
        // curry(FnPtr, constants...)
        Expr::FnCall(x, pos)
            if !x.is_qualified() // Non-qualified
            && x.args.len() >= 2
            && x.name == KEYWORD_FN_PTR_CURRY
            && matches!(x.args[0], Expr::DynamicConstant(ref v, ..) if v.is_fnptr())
            && x.constant_args()
        => {
            let mut fn_ptr = x.args[0].get_literal_value().unwrap().cast::<FnPtr>();
            fn_ptr.extend(x.args.iter().skip(1).map(|arg_expr| arg_expr.get_literal_value().unwrap()));
            state.set_dirty();
            *expr = Expr::DynamicConstant(Box::new(fn_ptr.into()), *pos);
        }

        // Do not call some special keywords that may have side effects
        Expr::FnCall(x, ..) if DONT_EVAL_KEYWORDS.contains(&x.name.as_str()) => {
            x.args.iter_mut().for_each(|arg_expr| optimize_expr(arg_expr, state, false));
        }

        // Call built-in operators
        Expr::FnCall(x, pos)
                if !x.is_qualified() // Non-qualified
                && state.optimization_level == OptimizationLevel::Simple // simple optimizations
                && x.constant_args() // all arguments are constants
        => {
            let arg_values = &mut x.args.iter().map(|arg_expr| arg_expr.get_literal_value().unwrap()).collect::<FnArgsVec<_>>();
            let arg_types = arg_values.iter().map(Dynamic::type_id).collect::<FnArgsVec<_>>();

            match x.name.as_str() {
                KEYWORD_TYPE_OF if arg_values.len() == 1 => {
                    state.set_dirty();
                    let typ = state.engine.map_type_name(arg_values[0].type_name()).into();
                    *expr = Expr::from_dynamic(typ, *pos);
                    return;
                }
                #[cfg(not(feature = "no_closure"))]
                crate::engine::KEYWORD_IS_SHARED if arg_values.len() == 1 => {
                    state.set_dirty();
                    *expr = Expr::from_dynamic(Dynamic::FALSE, *pos);
                    return;
                }
                // Overloaded operators can override built-in.
                _ if x.args.len() == 2 && x.op_token.is_some() && (state.engine.fast_operators() || !state.engine.has_native_fn_override(x.hashes.native(), &arg_types)) => {
                    if let Some((f, ctx)) = get_builtin_binary_op_fn(x.op_token.as_ref().unwrap(), &arg_values[0], &arg_values[1]) {
                        let context = ctx.then(|| (state.engine, x.name.as_str(), None, &state.global, *pos).into());
                        let (first, second) = arg_values.split_first_mut().unwrap();

                        if let Ok(result) = f(context, &mut [ first, &mut second[0] ]) {
                            state.set_dirty();
                            *expr = Expr::from_dynamic(result, *pos);
                            return;
                        }
                    }
                }
                _ => ()
            }

            x.args.iter_mut().for_each(|arg_expr| optimize_expr(arg_expr, state, false));

            // Move constant arguments
            x.args.iter_mut().for_each(|arg_expr| match arg_expr {
                    Expr::DynamicConstant(..) | Expr::Unit(..)
                    | Expr::StringConstant(..) | Expr::CharConstant(..)
                    | Expr::BoolConstant(..) | Expr::IntegerConstant(..) => (),

                    #[cfg(not(feature = "no_float"))]
                    Expr:: FloatConstant(..) => (),

                    _ => if let Some(value) = arg_expr.get_literal_value() {
                        state.set_dirty();
                        *arg_expr = Expr::DynamicConstant(value.into(), arg_expr.start_position());
                    },
                });
        }

        // Eagerly call functions
        Expr::FnCall(x, pos)
                if !x.is_qualified() // non-qualified
                && state.optimization_level == OptimizationLevel::Full // full optimizations
                && x.constant_args() // all arguments are constants
        => {
            // First search for script-defined functions (can override built-in)
            let _has_script_fn = false;
            #[cfg(not(feature = "no_function"))]
            let _has_script_fn = !x.hashes.is_native_only() && state.global.lib.iter().find_map(|m| m.get_script_fn(&x.name, x.args.len())).is_some();

            if !_has_script_fn {
                let arg_values = &mut x.args.iter().map(Expr::get_literal_value).collect::<Option<FnArgsVec<_>>>().unwrap();

                let result = match x.name.as_str() {
                    KEYWORD_TYPE_OF if arg_values.len() == 1 => Some(state.engine.map_type_name(arg_values[0].type_name()).into()),
                    #[cfg(not(feature = "no_closure"))]
                    crate::engine::KEYWORD_IS_SHARED if arg_values.len() == 1 => Some(Dynamic::FALSE),
                    _ => state.call_fn_with_const_args(&x.name, x.op_token.as_ref(), arg_values)
                };

                if let Some(r) = result {
                    state.set_dirty();
                    *expr = Expr::from_dynamic(r, *pos);
                    return;
                }
            }

            x.args.iter_mut().for_each(|a| optimize_expr(a, state, false));
        }

        // id(args ..) or xxx.id(args ..) -> optimize function call arguments
        Expr::FnCall(x, ..) | Expr::MethodCall(x, ..) => x.args.iter_mut().for_each(|arg_expr| {
            optimize_expr(arg_expr, state, false);

            // Move constant arguments
            match arg_expr {
                Expr::DynamicConstant(..) | Expr::Unit(..)
                | Expr::StringConstant(..) | Expr::CharConstant(..)
                | Expr::BoolConstant(..) | Expr::IntegerConstant(..) => (),

                #[cfg(not(feature = "no_float"))]
                Expr:: FloatConstant(..) => (),

                _ => if let Some(value) = arg_expr.get_literal_value() {
                    state.set_dirty();
                    *arg_expr = Expr::DynamicConstant(value.into(), arg_expr.start_position());
                },
            }
        }),

        // constant-name
        #[cfg(not(feature = "no_module"))]
        Expr::Variable(x, ..) if !x.1.is_empty() => (),
        Expr::Variable(x, .., pos) if state.propagate_constants && state.find_literal_constant(&x.3).is_some() => {
            // Replace constant with value
            *expr = Expr::from_dynamic(state.find_literal_constant(&x.3).unwrap().clone(), *pos);
            state.set_dirty();
        }

        // Custom syntax
        #[cfg(not(feature = "no_custom_syntax"))]
        Expr::Custom(x, ..) => {
            if x.scope_may_be_changed {
                state.propagate_constants = false;
            }
            // Do not optimize custom syntax expressions as you won't know how they would be called
        }

        // All other expressions - skip
        _ => (),
    }
}

impl Engine {
    /// Has a system function a Rust-native override?
    fn has_native_fn_override(&self, hash_script: u64, arg_types: impl AsRef<[TypeId]>) -> bool {
        let hash = calc_fn_hash_full(hash_script, arg_types.as_ref().iter().copied());

        // First check the global namespace and packages, but skip modules that are standard because
        // they should never conflict with system functions.
        if self
            .global_modules
            .iter()
            .filter(|m| !m.flags.intersects(ModuleFlags::STANDARD_LIB))
            .any(|m| m.contains_fn(hash))
        {
            return true;
        }

        // Then check sub-modules
        #[cfg(not(feature = "no_module"))]
        if self
            .global_sub_modules
            .values()
            .any(|m| m.contains_qualified_fn(hash))
        {
            return true;
        }

        false
    }

    /// Optimize a block of [statements][Stmt] at top level.
    ///
    /// Constants and variables from the scope are added.
    fn optimize_top_level(
        &self,
        statements: StmtBlockContainer,
        scope: Option<&Scope>,
        lib: &[crate::SharedModule],
        optimization_level: OptimizationLevel,
    ) -> StmtBlockContainer {
        let mut statements = statements;

        // If optimization level is None then skip optimizing
        if optimization_level == OptimizationLevel::None {
            statements.shrink_to_fit();
            return statements;
        }

        // Set up the state
        let mut state = OptimizerState::new(self, lib, scope, optimization_level);

        // Add constants from global modules
        self.global_modules
            .iter()
            .rev()
            .flat_map(|m| m.iter_var())
            .for_each(|(name, value)| state.push_var(name.into(), Some(Cow::Borrowed(value))));

        // Add constants and variables from the scope
        state
            .scope
            .into_iter()
            .flat_map(Scope::iter_inner)
            .for_each(|(name, constant, value)| {
                state.push_var(
                    name.into(),
                    if constant {
                        Some(Cow::Borrowed(value))
                    } else {
                        None
                    },
                );
            });

        optimize_stmt_block(statements, &mut state, true, false, true)
    }
}

/// Optimize a collection of statements and functions into an [`AST`].
pub fn optimize_into_ast(
    engine: &Engine,
    scope: Option<&Scope>,
    statements: StmtBlockContainer,
    #[cfg(not(feature = "no_function"))] functions: Vec<crate::Shared<crate::ast::ScriptFuncDef>>,
    optimization_level: OptimizationLevel,
) -> AST {
    let mut statements = statements;

    #[cfg(not(feature = "no_function"))]
    let lib: crate::Shared<_> = {
        let mut module = crate::Module::new();

        if optimization_level == OptimizationLevel::None {
            for fn_def in functions {
                module.set_script_fn(fn_def);
            }
        } else {
            // We only need the script library's signatures for optimization purposes
            let mut lib2 = crate::Module::new();

            functions
                .iter()
                .map(|fn_def| crate::ast::ScriptFuncDef {
                    name: fn_def.name.clone(),
                    access: fn_def.access,
                    body: crate::ast::StmtBlock::NONE,
                    #[cfg(not(feature = "no_object"))]
                    this_type: fn_def.this_type.clone(),
                    params: fn_def.params.clone(),
                    #[cfg(feature = "metadata")]
                    comments: <_>::default(),
                })
                .for_each(|script_def| {
                    lib2.set_script_fn(script_def);
                });

            let lib2 = &[lib2.into()];

            for fn_def in functions {
                let mut fn_def = crate::func::shared_take_or_clone(fn_def);

                // Optimize the function body
                *fn_def.body.statements_mut() = engine.optimize_top_level(
                    fn_def.body.take_statements(),
                    scope,
                    lib2,
                    optimization_level,
                );

                module.set_script_fn(fn_def);
            }
        }

        module.into()
    };
    #[cfg(feature = "no_function")]
    let lib: crate::Shared<_> = crate::Module::new().into();

    statements.shrink_to_fit();

    AST::new(
        match optimization_level {
            OptimizationLevel::None => statements,
            OptimizationLevel::Simple | OptimizationLevel::Full => {
                engine.optimize_top_level(statements, scope, &[lib.clone()], optimization_level)
            }
        },
        #[cfg(not(feature = "no_function"))]
        lib,
    )
}
