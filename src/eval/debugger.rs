//! Module defining the debugging interface.
#![cfg(feature = "debugging")]

use super::{Caches, EvalContext, GlobalRuntimeState};
use crate::ast::{ASTNode, Expr, Stmt};
use crate::{
    Dynamic, Engine, EvalAltResult, ImmutableString, Position, RhaiResultOf, Scope, ThinVec,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{fmt, iter::repeat, mem};

/// Callback function to initialize the debugger.
#[cfg(not(feature = "sync"))]
pub type OnDebuggingInit = dyn Fn(&Engine, Debugger) -> Debugger;
/// Callback function to initialize the debugger.
#[cfg(feature = "sync")]
pub type OnDebuggingInit = dyn Fn(&Engine, Debugger) -> Debugger + Send + Sync;

/// Callback function for debugging.
#[cfg(not(feature = "sync"))]
pub type OnDebuggerCallback = dyn Fn(
    EvalContext,
    DebuggerEvent,
    ASTNode,
    Option<&str>,
    Position,
) -> RhaiResultOf<DebuggerCommand>;
/// Callback function for debugging.
#[cfg(feature = "sync")]
pub type OnDebuggerCallback = dyn Fn(EvalContext, DebuggerEvent, ASTNode, Option<&str>, Position) -> RhaiResultOf<DebuggerCommand>
    + Send
    + Sync;

/// A command for the debugger on the next iteration.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Hash)]
#[non_exhaustive]
pub enum DebuggerCommand {
    /// Continue normal execution.
    #[default]
    Continue,
    /// Step into the next expression, diving into functions.
    StepInto,
    /// Run to the next expression or statement, stepping over functions.
    StepOver,
    /// Run to the next statement, skipping over functions.
    Next,
    /// Run to the end of the current function call.
    FunctionExit,
}

/// The debugger status.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum DebuggerStatus {
    // Script evaluation starts.
    Init,
    // Stop at the next statement or expression.
    Next(bool, bool),
    // Run to the end of the current level of function call.
    FunctionExit(usize),
    // Script evaluation ends.
    Terminate,
}

impl DebuggerStatus {
    pub const CONTINUE: Self = Self::Next(false, false);
    pub const STEP: Self = Self::Next(true, true);
    pub const NEXT: Self = Self::Next(true, false);
    pub const INTO: Self = Self::Next(false, true);
}

/// A event that triggers the debugger.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum DebuggerEvent<'a> {
    /// Script evaluation starts.
    Start,
    /// Break on next step.
    Step,
    /// Break on break-point.
    BreakPoint(usize),
    /// Return from a function with a value.
    FunctionExitWithValue(&'a Dynamic),
    /// Return from a function with a value.
    FunctionExitWithError(&'a EvalAltResult),
    /// Script evaluation ends.
    End,
}

/// A break-point for debugging.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum BreakPoint {
    /// Break at a particular position under a particular source.
    ///
    /// Not available under `no_position`.
    #[cfg(not(feature = "no_position"))]
    AtPosition {
        /// Source (empty if not available) of the break-point.
        source: Option<ImmutableString>,
        /// [Position] of the break-point.
        pos: Position,
        /// Is the break-point enabled?
        enabled: bool,
    },
    /// Break at a particular function call.
    AtFunctionName {
        /// Function name.
        name: ImmutableString,
        /// Is the break-point enabled?
        enabled: bool,
    },
    /// Break at a particular function call with a particular number of arguments.
    AtFunctionCall {
        /// Function name.
        name: ImmutableString,
        /// Number of arguments.
        args: usize,
        /// Is the break-point enabled?
        enabled: bool,
    },
    /// Break at a particular property .
    ///
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    AtProperty {
        /// Property name.
        name: ImmutableString,
        /// Is the break-point enabled?
        enabled: bool,
    },
}

impl fmt::Display for BreakPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(not(feature = "no_position"))]
            Self::AtPosition {
                source,
                pos,
                enabled,
            } => {
                if let Some(ref source) = source {
                    write!(f, "{source} ")?;
                }
                write!(f, "@ {pos:?}")?;
                if !*enabled {
                    f.write_str(" (disabled)")?;
                }
                Ok(())
            }
            Self::AtFunctionName { name, enabled } => {
                write!(f, "{name} (...)")?;
                if !*enabled {
                    f.write_str(" (disabled)")?;
                }
                Ok(())
            }
            Self::AtFunctionCall {
                name,
                args,
                enabled,
            } => {
                write!(
                    f,
                    "{name} ({})",
                    repeat("_").take(*args).collect::<Vec<_>>().join(", ")
                )?;
                if !*enabled {
                    f.write_str(" (disabled)")?;
                }
                Ok(())
            }
            #[cfg(not(feature = "no_object"))]
            Self::AtProperty { name, enabled } => {
                write!(f, ".{name}")?;
                if !*enabled {
                    f.write_str(" (disabled)")?;
                }
                Ok(())
            }
        }
    }
}

impl BreakPoint {
    /// Is this [`BreakPoint`] enabled?
    #[inline(always)]
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        match self {
            #[cfg(not(feature = "no_position"))]
            Self::AtPosition { enabled, .. } => *enabled,
            Self::AtFunctionName { enabled, .. } | Self::AtFunctionCall { enabled, .. } => *enabled,
            #[cfg(not(feature = "no_object"))]
            Self::AtProperty { enabled, .. } => *enabled,
        }
    }
    /// Enable/disable this [`BreakPoint`].
    #[inline(always)]
    pub fn enable(&mut self, value: bool) {
        match self {
            #[cfg(not(feature = "no_position"))]
            Self::AtPosition { enabled, .. } => *enabled = value,
            Self::AtFunctionName { enabled, .. } | Self::AtFunctionCall { enabled, .. } => {
                *enabled = value
            }
            #[cfg(not(feature = "no_object"))]
            Self::AtProperty { enabled, .. } => *enabled = value,
        }
    }
}

/// A function call.
#[derive(Debug, Clone, Hash)]
pub struct CallStackFrame {
    /// Function name.
    pub fn_name: ImmutableString,
    /// Copies of function call arguments, if any.
    pub args: ThinVec<Dynamic>,
    /// Source of the function.
    pub source: Option<ImmutableString>,
    /// [Position][`Position`] of the function call.
    pub pos: Position,
}

impl fmt::Display for CallStackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fp = f.debug_tuple(&self.fn_name);

        for arg in &self.args {
            fp.field(arg);
        }

        fp.finish()?;

        if !self.pos.is_none() {
            if let Some(ref source) = self.source {
                write!(f, ": {source}")?;
            }
            write!(f, " @ {:?}", self.pos)?;
        }

        Ok(())
    }
}

/// A type providing debugging facilities.
#[derive(Debug, Clone, Hash)]
pub struct Debugger {
    /// The current status command.
    pub(crate) status: DebuggerStatus,
    /// The current set of break-points.
    break_points: Vec<BreakPoint>,
    /// The current function call stack.
    call_stack: Vec<CallStackFrame>,
    /// The current state.
    state: Dynamic,
}

impl Debugger {
    /// Create a new [`Debugger`].
    #[inline(always)]
    #[must_use]
    pub fn new(status: DebuggerStatus) -> Self {
        Self {
            status,
            break_points: Vec::new(),
            call_stack: Vec::new(),
            state: Dynamic::UNIT,
        }
    }
    /// Get the current call stack.
    #[inline(always)]
    #[must_use]
    pub fn call_stack(&self) -> &[CallStackFrame] {
        &self.call_stack
    }
    /// Rewind the function call stack to a particular depth.
    #[inline(always)]
    pub(crate) fn rewind_call_stack(&mut self, len: usize) {
        self.call_stack.truncate(len);
    }
    /// Add a new frame to the function call stack.
    #[inline(always)]
    pub(crate) fn push_call_stack_frame(
        &mut self,
        fn_name: ImmutableString,
        args: impl IntoIterator<Item = Dynamic>,
        source: Option<ImmutableString>,
        pos: Position,
    ) {
        self.call_stack.push(CallStackFrame {
            fn_name,
            args: args.into_iter().collect(),
            source,
            pos,
        });
    }
    /// Change the current status to [`CONTINUE`][DebuggerStatus::CONTINUE] and return the previous status.
    pub(crate) fn clear_status_if(
        &mut self,
        filter: impl FnOnce(&DebuggerStatus) -> bool,
    ) -> Option<DebuggerStatus> {
        if filter(&self.status) {
            Some(mem::replace(&mut self.status, DebuggerStatus::CONTINUE))
        } else {
            None
        }
    }
    /// Override the status of this [`Debugger`] if the current status is
    /// [`CONTINUE`][DebuggerStatus::CONTINUE].
    #[inline(always)]
    pub(crate) fn reset_status(&mut self, status: DebuggerStatus) {
        if self.status == DebuggerStatus::CONTINUE {
            self.status = status;
        }
    }
    /// Returns the first break-point triggered by a particular [`AST` Node][ASTNode].
    #[must_use]
    pub fn is_break_point(&self, src: Option<&str>, node: ASTNode) -> Option<usize> {
        let _src = src;

        self.break_points()
            .iter()
            .enumerate()
            .filter(|(.., bp)| bp.is_enabled())
            .find(|(.., bp)| match bp {
                #[cfg(not(feature = "no_position"))]
                BreakPoint::AtPosition { pos, .. } if pos.is_none() => false,
                #[cfg(not(feature = "no_position"))]
                BreakPoint::AtPosition { source, pos, .. } if pos.is_beginning_of_line() => {
                    node.position().line().unwrap_or(0) == pos.line().unwrap()
                        && _src == source.as_deref()
                }
                #[cfg(not(feature = "no_position"))]
                BreakPoint::AtPosition { source, pos, .. } => {
                    node.position() == *pos && _src == source.as_deref()
                }
                BreakPoint::AtFunctionName { name, .. } => match node {
                    ASTNode::Expr(Expr::FnCall(x, ..)) | ASTNode::Stmt(Stmt::FnCall(x, ..)) => {
                        x.name == *name
                    }
                    ASTNode::Stmt(Stmt::Expr(e)) => match &**e {
                        Expr::FnCall(x, ..) => x.name == *name,
                        _ => false,
                    },
                    _ => false,
                },
                BreakPoint::AtFunctionCall { name, args, .. } => match node {
                    ASTNode::Expr(Expr::FnCall(x, ..)) | ASTNode::Stmt(Stmt::FnCall(x, ..)) => {
                        x.args.len() == *args && x.name == *name
                    }
                    ASTNode::Stmt(Stmt::Expr(e)) => match &**e {
                        Expr::FnCall(x, ..) => x.args.len() == *args && x.name == *name,
                        _ => false,
                    },
                    _ => false,
                },
                #[cfg(not(feature = "no_object"))]
                BreakPoint::AtProperty { name, .. } => match node {
                    ASTNode::Expr(Expr::Property(x, ..)) => x.2 == *name,
                    _ => false,
                },
            })
            .map(|(i, ..)| i)
    }
    /// Get a slice of all [`BreakPoint`]'s.
    #[inline(always)]
    #[must_use]
    pub fn break_points(&self) -> &[BreakPoint] {
        &self.break_points
    }
    /// Get the underlying [`Vec`] holding all [`BreakPoint`]'s.
    #[inline(always)]
    #[must_use]
    pub fn break_points_mut(&mut self) -> &mut Vec<BreakPoint> {
        &mut self.break_points
    }
    /// Get the custom state.
    #[inline(always)]
    pub const fn state(&self) -> &Dynamic {
        &self.state
    }
    /// Get a mutable reference to the custom state.
    #[inline(always)]
    pub fn state_mut(&mut self) -> &mut Dynamic {
        &mut self.state
    }
    /// Set the custom state.
    #[inline(always)]
    pub fn set_state(&mut self, state: impl Into<Dynamic>) {
        self.state = state.into();
    }
}

impl Engine {
    /// Run the debugger callback if there is a debugging interface registered.
    #[inline(always)]
    pub(crate) fn run_debugger<'a>(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        this_ptr: Option<&mut Dynamic>,
        node: impl Into<ASTNode<'a>>,
    ) -> RhaiResultOf<()> {
        if self.is_debugger_registered() {
            if let Some(cmd) =
                self.run_debugger_with_reset_raw(global, caches, scope, this_ptr, node)?
            {
                global.debugger_mut().status = cmd;
            }
        }

        Ok(())
    }
    /// Run the debugger callback if there is a debugging interface registered.
    ///
    /// Returns [`Some`] if the debugger needs to be reactivated at the end of the block, statement or
    /// function call.
    ///
    /// It is up to the [`Engine`] to reactivate the debugger.
    #[inline(always)]
    pub(crate) fn run_debugger_with_reset<'a>(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        this_ptr: Option<&mut Dynamic>,
        node: impl Into<ASTNode<'a>>,
    ) -> RhaiResultOf<Option<DebuggerStatus>> {
        if self.is_debugger_registered() {
            self.run_debugger_with_reset_raw(global, caches, scope, this_ptr, node)
        } else {
            Ok(None)
        }
    }
    /// Run the debugger callback.
    ///
    /// Returns [`Some`] if the debugger needs to be reactivated at the end of the block, statement or
    /// function call.
    ///
    /// It is up to the [`Engine`] to reactivate the debugger.
    #[inline]
    pub(crate) fn run_debugger_with_reset_raw<'a>(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        this_ptr: Option<&mut Dynamic>,
        node: impl Into<ASTNode<'a>>,
    ) -> RhaiResultOf<Option<DebuggerStatus>> {
        let node = node.into();

        // Skip transitive nodes
        match node {
            ASTNode::Expr(Expr::Stmt(..)) | ASTNode::Stmt(Stmt::Expr(..)) => return Ok(None),
            _ => (),
        }

        match global.debugger {
            Some(ref dbg) => {
                let event = match dbg.status {
                    DebuggerStatus::Init => Some(DebuggerEvent::Start),
                    DebuggerStatus::NEXT if node.is_stmt() => Some(DebuggerEvent::Step),
                    DebuggerStatus::INTO if node.is_expr() => Some(DebuggerEvent::Step),
                    DebuggerStatus::STEP => Some(DebuggerEvent::Step),
                    DebuggerStatus::Terminate => Some(DebuggerEvent::End),
                    _ => None,
                };

                let event = match event {
                    Some(e) => e,
                    None => match dbg.is_break_point(global.source(), node) {
                        Some(bp) => DebuggerEvent::BreakPoint(bp),
                        None => return Ok(None),
                    },
                };

                self.run_debugger_raw(global, caches, scope, this_ptr, node, event)
            }
            None => Ok(None),
        }
    }
    /// Run the debugger callback unconditionally.
    ///
    /// Returns [`Some`] if the debugger needs to be reactivated at the end of the block, statement or
    /// function call.
    ///
    /// It is up to the [`Engine`] to reactivate the debugger.
    #[inline]
    pub(crate) fn run_debugger_raw(
        &self,
        global: &mut GlobalRuntimeState,
        caches: &mut Caches,
        scope: &mut Scope,
        this_ptr: Option<&mut Dynamic>,
        node: ASTNode,
        event: DebuggerEvent,
    ) -> Result<Option<DebuggerStatus>, Box<crate::EvalAltResult>> {
        match self.debugger_interface {
            Some(ref x) => {
                let orig_scope_len = scope.len();

                let src = global.source_raw().cloned();
                let context = EvalContext::new(self, global, caches, scope, this_ptr);
                let (.., ref on_debugger) = *x;

                let command = on_debugger(context, event, node, src.as_deref(), node.position());

                if orig_scope_len != scope.len() {
                    // The scope is changed, always search from now on
                    global.always_search_scope = true;
                }

                match command? {
                    DebuggerCommand::Continue => {
                        global.debugger_mut().status = DebuggerStatus::CONTINUE;
                        Ok(None)
                    }
                    DebuggerCommand::Next => {
                        global.debugger_mut().status = DebuggerStatus::CONTINUE;
                        Ok(Some(DebuggerStatus::NEXT))
                    }
                    DebuggerCommand::StepOver => {
                        global.debugger_mut().status = DebuggerStatus::CONTINUE;
                        Ok(Some(DebuggerStatus::STEP))
                    }
                    DebuggerCommand::StepInto => {
                        global.debugger_mut().status = DebuggerStatus::STEP;
                        Ok(None)
                    }
                    DebuggerCommand::FunctionExit => {
                        // Bump a level if it is a function call
                        let level = match node {
                            ASTNode::Expr(Expr::FnCall(..)) | ASTNode::Stmt(Stmt::FnCall(..)) => {
                                global.level + 1
                            }
                            ASTNode::Stmt(Stmt::Expr(e)) if matches!(**e, Expr::FnCall(..)) => {
                                global.level + 1
                            }
                            _ => global.level,
                        };
                        global.debugger_mut().status = DebuggerStatus::FunctionExit(level);
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }
}
