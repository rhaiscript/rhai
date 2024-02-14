//! Module defining script statements.

use super::{ASTFlags, ASTNode, BinaryExpr, Expr, FnCallExpr, Ident};
use crate::engine::{KEYWORD_EVAL, OP_EQUALS};
use crate::func::StraightHashMap;
use crate::tokenizer::Token;
use crate::types::dynamic::Union;
use crate::types::Span;
use crate::{calc_fn_hash, Dynamic, FnArgsVec, Position, StaticVec, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    mem,
    num::NonZeroUsize,
    ops::{Range, RangeInclusive},
};

/// _(internals)_ An op-assignment operator.
/// Exported under the `internals` feature only.
///
/// This type may hold a straight assignment (i.e. not an op-assignment).
#[derive(Clone, PartialEq, Hash)]
pub struct OpAssignment {
    /// Hash of the op-assignment call.
    hash_op_assign: u64,
    /// Hash of the underlying operator call (for fallback).
    hash_op: u64,
    /// Op-assignment operator.
    op_assign: Token,
    /// Syntax of op-assignment operator.
    op_assign_syntax: &'static str,
    /// Underlying operator.
    op: Token,
    /// Syntax of underlying operator.
    op_syntax: &'static str,
    /// [Position] of the op-assignment operator.
    pos: Position,
}

impl OpAssignment {
    /// Create a new [`OpAssignment`] that is only a straight assignment.
    #[must_use]
    #[inline(always)]
    pub const fn new_assignment(pos: Position) -> Self {
        Self {
            hash_op_assign: 0,
            hash_op: 0,
            op_assign: Token::Equals,
            op_assign_syntax: OP_EQUALS,
            op: Token::Equals,
            op_syntax: OP_EQUALS,
            pos,
        }
    }
    /// Is this an op-assignment?
    #[must_use]
    #[inline(always)]
    pub const fn is_op_assignment(&self) -> bool {
        !matches!(self.op, Token::Equals)
    }
    /// Get information if this [`OpAssignment`] is an op-assignment.
    ///
    /// Returns `( hash_op_assign, hash_op, op_assign, op_assign_syntax, op, op_syntax )`:
    ///
    /// * `hash_op_assign`: Hash of the op-assignment call.
    /// * `hash_op`: Hash of the underlying operator call (for fallback).
    /// * `op_assign`: Op-assignment operator.
    /// * `op_assign_syntax`: Syntax of op-assignment operator.
    /// * `op`: Underlying operator.
    /// * `op_syntax`: Syntax of underlying operator.
    #[must_use]
    #[inline]
    pub const fn get_op_assignment_info(
        &self,
    ) -> Option<(u64, u64, &Token, &'static str, &Token, &'static str)> {
        if self.is_op_assignment() {
            Some((
                self.hash_op_assign,
                self.hash_op,
                &self.op_assign,
                self.op_assign_syntax,
                &self.op,
                self.op_syntax,
            ))
        } else {
            None
        }
    }
    /// Get the [position][Position] of this [`OpAssignment`].
    #[must_use]
    #[inline(always)]
    pub const fn position(&self) -> Position {
        self.pos
    }
    /// Create a new [`OpAssignment`].
    ///
    /// # Panics
    ///
    /// Panics if the name is not an op-assignment operator.
    #[must_use]
    #[inline(always)]
    pub fn new_op_assignment(name: &str, pos: Position) -> Self {
        let op = Token::lookup_symbol_from_syntax(name)
            .unwrap_or_else(|| panic!("{} is not an op-assignment operator", name));
        Self::new_op_assignment_from_token(op, pos)
    }
    /// Create a new [`OpAssignment`] from a [`Token`].
    ///
    /// # Panics
    ///
    /// Panics if the token is not an op-assignment operator.
    #[must_use]
    pub fn new_op_assignment_from_token(op_assign: Token, pos: Position) -> Self {
        let op = op_assign
            .get_base_op_from_assignment()
            .unwrap_or_else(|| panic!("{:?} is not an op-assignment operator", op_assign));

        let op_assign_syntax = op_assign.literal_syntax();
        let op_syntax = op.literal_syntax();

        Self {
            hash_op_assign: calc_fn_hash(None, op_assign_syntax, 2),
            hash_op: calc_fn_hash(None, op_syntax, 2),
            op_assign,
            op_assign_syntax,
            op,
            op_syntax,
            pos,
        }
    }
    /// Create a new [`OpAssignment`] from a base operator.
    ///
    /// # Panics
    ///
    /// Panics if the name is not an operator that can be converted into an op-operator.
    #[must_use]
    #[inline(always)]
    pub fn new_op_assignment_from_base(name: &str, pos: Position) -> Self {
        let op = Token::lookup_symbol_from_syntax(name)
            .unwrap_or_else(|| panic!("{} cannot be converted into an op-operator", name));
        Self::new_op_assignment_from_base_token(&op, pos)
    }
    /// Convert a [`Token`] into a new [`OpAssignment`].
    ///
    /// # Panics
    ///
    /// Panics if the token is cannot be converted into an op-assignment operator.
    #[inline(always)]
    #[must_use]
    pub fn new_op_assignment_from_base_token(op: &Token, pos: Position) -> Self {
        Self::new_op_assignment_from_token(
            op.convert_to_op_assignment()
                .unwrap_or_else(|| panic!("{:?} cannot be converted into an op-operator", op)),
            pos,
        )
    }
}

impl fmt::Debug for OpAssignment {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_op_assignment() {
            f.debug_struct("OpAssignment")
                .field("hash_op_assign", &self.hash_op_assign)
                .field("hash_op", &self.hash_op)
                .field("op_assign", &self.op_assign)
                .field("op_assign_syntax", &self.op_assign_syntax)
                .field("op", &self.op)
                .field("op_syntax", &self.op_syntax)
                .field("pos", &self.pos)
                .finish()
        } else {
            write!(f, "{} @ {:?}", Token::Equals, self.pos)
        }
    }
}

/// _(internals)_ A type containing a range case for a `switch` statement.
/// Exported under the `internals` feature only.
#[derive(Clone, Hash)]
pub enum RangeCase {
    /// Exclusive range.
    ExclusiveInt(Range<INT>, usize),
    /// Inclusive range.
    InclusiveInt(RangeInclusive<INT>, usize),
}

impl fmt::Debug for RangeCase {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExclusiveInt(r, n) => write!(f, "{}..{} => {n}", r.start, r.end),
            Self::InclusiveInt(r, n) => write!(f, "{}..={} => {n}", *r.start(), *r.end()),
        }
    }
}

impl From<Range<INT>> for RangeCase {
    #[inline(always)]
    fn from(value: Range<INT>) -> Self {
        Self::ExclusiveInt(value, usize::MAX)
    }
}

impl From<RangeInclusive<INT>> for RangeCase {
    #[inline(always)]
    fn from(value: RangeInclusive<INT>) -> Self {
        Self::InclusiveInt(value, usize::MAX)
    }
}

impl IntoIterator for RangeCase {
    type Item = INT;
    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    #[inline]
    #[must_use]
    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::ExclusiveInt(r, ..) => Box::new(r),
            Self::InclusiveInt(r, ..) => Box::new(r),
        }
    }
}

impl RangeCase {
    /// Returns `true` if the range contains no items.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::ExclusiveInt(r, ..) => r.is_empty(),
            Self::InclusiveInt(r, ..) => r.is_empty(),
        }
    }
    /// Size of the range.
    #[inline]
    #[must_use]
    pub fn len(&self) -> INT {
        match self {
            Self::ExclusiveInt(r, ..) if r.is_empty() => 0,
            Self::ExclusiveInt(r, ..) => r.end - r.start,
            Self::InclusiveInt(r, ..) if r.is_empty() => 0,
            Self::InclusiveInt(r, ..) => *r.end() - *r.start() + 1,
        }
    }
    /// Is the specified value within this range?
    #[inline]
    #[must_use]
    pub fn contains(&self, value: &Dynamic) -> bool {
        match value {
            Dynamic(Union::Int(v, ..)) => self.contains_int(*v),
            #[cfg(not(feature = "no_float"))]
            Dynamic(Union::Float(v, ..)) => self.contains_float(**v),
            #[cfg(feature = "decimal")]
            Dynamic(Union::Decimal(v, ..)) => self.contains_decimal(**v),
            _ => false,
        }
    }
    /// Is the specified number within this range?
    #[inline]
    #[must_use]
    pub fn contains_int(&self, n: INT) -> bool {
        match self {
            Self::ExclusiveInt(r, ..) => r.contains(&n),
            Self::InclusiveInt(r, ..) => r.contains(&n),
        }
    }
    /// Is the specified floating-point number within this range?
    #[cfg(not(feature = "no_float"))]
    #[inline]
    #[must_use]
    pub fn contains_float(&self, n: crate::FLOAT) -> bool {
        use crate::FLOAT;

        match self {
            Self::ExclusiveInt(r, ..) => ((r.start as FLOAT)..(r.end as FLOAT)).contains(&n),
            Self::InclusiveInt(r, ..) => ((*r.start() as FLOAT)..=(*r.end() as FLOAT)).contains(&n),
        }
    }
    /// Is the specified decimal number within this range?
    #[cfg(feature = "decimal")]
    #[inline]
    #[must_use]
    pub fn contains_decimal(&self, n: rust_decimal::Decimal) -> bool {
        use rust_decimal::Decimal;

        match self {
            Self::ExclusiveInt(r, ..) => {
                (Into::<Decimal>::into(r.start)..Into::<Decimal>::into(r.end)).contains(&n)
            }
            Self::InclusiveInt(r, ..) => {
                (Into::<Decimal>::into(*r.start())..=Into::<Decimal>::into(*r.end())).contains(&n)
            }
        }
    }
    /// Is the specified range inclusive?
    #[inline(always)]
    #[must_use]
    pub const fn is_inclusive(&self) -> bool {
        match self {
            Self::ExclusiveInt(..) => false,
            Self::InclusiveInt(..) => true,
        }
    }
    /// Get the index to the list of expressions.
    #[inline(always)]
    #[must_use]
    pub const fn index(&self) -> usize {
        match self {
            Self::ExclusiveInt(.., n) | Self::InclusiveInt(.., n) => *n,
        }
    }
    /// Set the index to the list of expressions.
    #[inline(always)]
    pub fn set_index(&mut self, index: usize) {
        match self {
            Self::ExclusiveInt(.., n) | Self::InclusiveInt(.., n) => *n = index,
        }
    }
}

pub type CaseBlocksList = smallvec::SmallVec<[usize; 2]>;

/// _(internals)_ A type containing all cases for a `switch` statement.
/// Exported under the `internals` feature only.
#[derive(Debug, Clone)]
pub struct SwitchCasesCollection {
    /// List of conditional expressions: LHS = condition, RHS = expression.
    pub expressions: FnArgsVec<BinaryExpr>,
    /// Dictionary mapping value hashes to [`CaseBlocksList`]'s.
    pub cases: StraightHashMap<CaseBlocksList>,
    /// List of range cases.
    pub ranges: StaticVec<RangeCase>,
    /// Statements block for the default case (there can be no condition for the default case).
    pub def_case: Option<usize>,
}

impl Hash for SwitchCasesCollection {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expressions.hash(state);

        self.cases.len().hash(state);
        self.cases.iter().for_each(|kv| kv.hash(state));

        self.ranges.hash(state);
        self.def_case.hash(state);
    }
}

/// Number of items to keep inline for [`StmtBlockContainer`].
#[cfg(not(feature = "no_std"))]
const STMT_BLOCK_INLINE_SIZE: usize = 8;

/// _(internals)_ The underlying container type for [`StmtBlock`].
/// Exported under the `internals` feature only.
///
/// A [`SmallVec`](https://crates.io/crates/smallvec) containing up to 8 items inline is used to
/// hold a statements block, with the assumption that most program blocks would container fewer than
/// 8 statements, and those that do have a lot more statements.
#[cfg(not(feature = "no_std"))]
pub type StmtBlockContainer = smallvec::SmallVec<[Stmt; STMT_BLOCK_INLINE_SIZE]>;

/// _(internals)_ The underlying container type for [`StmtBlock`].
/// Exported under the `internals` feature only.
#[cfg(feature = "no_std")]
pub type StmtBlockContainer = crate::StaticVec<Stmt>;

/// _(internals)_ A scoped block of statements.
/// Exported under the `internals` feature only.
#[derive(Clone, Hash, Default)]
pub struct StmtBlock {
    /// List of [statements][Stmt].
    block: StmtBlockContainer,
    /// [Position] of the statements block.
    span: Span,
}

impl StmtBlock {
    /// A [`StmtBlock`] that does not exist.
    pub const NONE: Self = Self::empty(Position::NONE);

    /// Create a new [`StmtBlock`].
    #[inline(always)]
    #[must_use]
    pub fn new(
        statements: impl IntoIterator<Item = Stmt>,
        start_pos: Position,
        end_pos: Position,
    ) -> Self {
        Self::new_with_span(statements, Span::new(start_pos, end_pos))
    }
    /// Create a new [`StmtBlock`].
    #[must_use]
    pub fn new_with_span(statements: impl IntoIterator<Item = Stmt>, span: Span) -> Self {
        let mut statements: smallvec::SmallVec<_> = statements.into_iter().collect();
        statements.shrink_to_fit();
        Self {
            block: statements,
            span,
        }
    }
    /// Create an empty [`StmtBlock`].
    #[inline(always)]
    #[must_use]
    pub const fn empty(pos: Position) -> Self {
        Self {
            block: StmtBlockContainer::new_const(),
            span: Span::new(pos, pos),
        }
    }
    /// Returns `true` if this statements block contains no statements.
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.block.is_empty()
    }
    /// Number of statements in this statements block.
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.block.len()
    }
    /// Get the statements of this statements block.
    #[inline(always)]
    #[must_use]
    pub fn statements(&self) -> &[Stmt] {
        &self.block
    }
    /// Get the statements of this statements block.
    #[inline(always)]
    #[must_use]
    pub fn statements_mut(&mut self) -> &mut StmtBlockContainer {
        &mut self.block
    }
    /// Get an iterator over the statements of this statements block.
    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &Stmt> {
        self.block.iter()
    }
    /// Get the start position (location of the beginning `{`) of this statements block.
    #[inline(always)]
    #[must_use]
    pub const fn position(&self) -> Position {
        (self.span).start()
    }
    /// Get the end position (location of the ending `}`) of this statements block.
    #[inline(always)]
    #[must_use]
    pub const fn end_position(&self) -> Position {
        (self.span).end()
    }
    /// Get the positions (locations of the beginning `{` and ending `}`) of this statements block.
    #[inline(always)]
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
    /// Get the positions (locations of the beginning `{` and ending `}`) of this statements block
    /// or a default.
    #[inline(always)]
    #[must_use]
    pub const fn span_or_else(&self, def_start_pos: Position, def_end_pos: Position) -> Span {
        Span::new(
            (self.span).start().or_else(def_start_pos),
            (self.span).end().or_else(def_end_pos),
        )
    }
    /// Set the positions of this statements block.
    #[inline(always)]
    pub fn set_position(&mut self, start_pos: Position, end_pos: Position) {
        self.span = Span::new(start_pos, end_pos);
    }
}

impl Borrow<[Stmt]> for StmtBlock {
    #[inline(always)]
    #[must_use]
    fn borrow(&self) -> &[Stmt] {
        &self.block
    }
}

impl AsRef<[Stmt]> for StmtBlock {
    #[inline(always)]
    #[must_use]
    fn as_ref(&self) -> &[Stmt] {
        &self.block
    }
}

impl AsMut<[Stmt]> for StmtBlock {
    #[inline(always)]
    #[must_use]
    fn as_mut(&mut self) -> &mut [Stmt] {
        &mut self.block
    }
}

impl fmt::Debug for StmtBlock {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Block")?;
        fmt::Debug::fmt(&self.block, f)?;
        if !self.span.is_none() {
            write!(f, " @ {:?}", self.span())?;
        }
        Ok(())
    }
}

impl From<Stmt> for StmtBlock {
    #[inline]
    fn from(stmt: Stmt) -> Self {
        match stmt {
            Stmt::Block(block) => *block,
            Stmt::Noop(pos) => Self {
                block: StmtBlockContainer::new_const(),
                span: Span::new(pos, pos),
            },
            _ => {
                let pos = stmt.position();
                Self {
                    block: vec![stmt].into(),
                    span: Span::new(pos, Position::NONE),
                }
            }
        }
    }
}

impl IntoIterator for StmtBlock {
    type Item = Stmt;
    #[cfg(not(feature = "no_std"))]
    type IntoIter = smallvec::IntoIter<[Stmt; STMT_BLOCK_INLINE_SIZE]>;
    #[cfg(feature = "no_std")]
    type IntoIter = smallvec::IntoIter<[Stmt; crate::STATIC_VEC_INLINE_SIZE]>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.block.into_iter()
    }
}

impl<'a> IntoIterator for &'a StmtBlock {
    type Item = &'a Stmt;
    type IntoIter = std::slice::Iter<'a, Stmt>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.block.iter()
    }
}

impl Extend<Stmt> for StmtBlock {
    #[inline(always)]
    fn extend<T: IntoIterator<Item = Stmt>>(&mut self, iter: T) {
        self.block.extend(iter);
    }
}

/// _(internals)_ A flow control block containing:
/// * an expression,
/// * a statements body
/// * an alternate statements body
///
/// Exported under the `internals` feature only.
#[derive(Debug, Clone, Hash)]
pub struct FlowControl {
    /// Flow control expression.
    pub expr: Expr,
    /// Main body.
    pub body: StmtBlock,
    /// Branch body.
    pub branch: StmtBlock,
}

/// _(internals)_ A statement.
/// Exported under the `internals` feature only.
#[derive(Debug, Clone, Hash)]
#[non_exhaustive]
#[allow(clippy::type_complexity)]
pub enum Stmt {
    /// No-op.
    Noop(Position),
    /// `if` expr `{` stmt `}` `else` `{` stmt `}`
    If(Box<FlowControl>, Position),
    /// `switch` expr `{` literal or range or _ `if` condition `=>` stmt `,` ... `}`
    ///
    /// ### Data Structure
    ///
    /// 0) Hash table for (condition, block)
    /// 1) Default block
    /// 2) List of ranges: (start, end, inclusive, condition, statement)
    Switch(Box<(Expr, SwitchCasesCollection)>, Position),
    /// `while` expr `{` stmt `}` | `loop` `{` stmt `}`
    ///
    /// If the guard expression is [`UNIT`][Expr::Unit], then it is a `loop` statement.
    While(Box<FlowControl>, Position),
    /// `do` `{` stmt `}` `while`|`until` expr
    ///
    /// ### Flags
    ///
    /// * [`NONE`][ASTFlags::NONE] = `while`  
    /// * [`NEGATED`][ASTFlags::NEGATED] = `until`
    Do(Box<FlowControl>, ASTFlags, Position),
    /// `for` `(` id `,` counter `)` `in` expr `{` stmt `}`
    For(Box<(Ident, Option<Ident>, FlowControl)>, Position),
    /// \[`export`\] `let`|`const` id `=` expr
    ///
    /// ### Flags
    ///
    /// * [`EXPORTED`][ASTFlags::EXPORTED] = `export`  
    /// * [`CONSTANT`][ASTFlags::CONSTANT] = `const`
    Var(Box<(Ident, Expr, Option<NonZeroUsize>)>, ASTFlags, Position),
    /// expr op`=` expr
    Assignment(Box<(OpAssignment, BinaryExpr)>),
    /// func `(` expr `,` ... `)`
    ///
    /// This is a duplicate of [`Expr::FnCall`] to cover the very common pattern of a single
    /// function call forming one statement.
    FnCall(Box<FnCallExpr>, Position),
    /// `{` stmt`;` ... `}`
    Block(Box<StmtBlock>),
    /// `try` `{` stmt; ... `}` `catch` `(` var `)` `{` stmt; ... `}`
    TryCatch(Box<FlowControl>, Position),
    /// [expression][Expr]
    Expr(Box<Expr>),
    /// `continue`/`break` expr
    ///
    /// ### Flags
    ///
    /// * [`NONE`][ASTFlags::NONE] = `continue`
    /// * [`BREAK`][ASTFlags::BREAK] = `break`
    BreakLoop(Option<Box<Expr>>, ASTFlags, Position),
    /// `return`/`throw` expr
    ///
    /// ### Flags
    ///
    /// * [`NONE`][ASTFlags::NONE] = `return`
    /// * [`BREAK`][ASTFlags::BREAK] = `throw`
    Return(Option<Box<Expr>>, ASTFlags, Position),
    /// `import` expr `as` alias
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    Import(Box<(Expr, Ident)>, Position),
    /// `export` var `as` alias
    ///
    /// Not available under `no_module`.
    #[cfg(not(feature = "no_module"))]
    Export(Box<(Ident, Ident)>, Position),
    /// Convert a list of variables to shared.
    ///
    /// Not available under `no_closure`.
    ///
    /// # Notes
    ///
    /// This variant does not map to any language structure.  It is currently only used only to
    /// convert normal variables into shared variables when they are _captured_ by a closure.
    #[cfg(not(feature = "no_closure"))]
    Share(Box<crate::FnArgsVec<(Ident, Option<NonZeroUsize>)>>),
}

impl Default for Stmt {
    #[inline(always)]
    #[must_use]
    fn default() -> Self {
        Self::Noop(Position::NONE)
    }
}

impl Stmt {
    /// Is this statement [`Noop`][Stmt::Noop]?
    #[inline(always)]
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        matches!(self, Self::Noop(..))
    }
    /// Get the [options][ASTFlags] of this statement.
    #[inline]
    #[must_use]
    pub const fn options(&self) -> ASTFlags {
        match self {
            Self::Do(_, options, _)
            | Self::Var(_, options, _)
            | Self::BreakLoop(_, options, _)
            | Self::Return(_, options, _) => *options,

            Self::Noop(..)
            | Self::If(..)
            | Self::Switch(..)
            | Self::Block(..)
            | Self::Expr(..)
            | Self::FnCall(..)
            | Self::While(..)
            | Self::For(..)
            | Self::TryCatch(..)
            | Self::Assignment(..) => ASTFlags::empty(),

            #[cfg(not(feature = "no_module"))]
            Self::Import(..) | Self::Export(..) => ASTFlags::empty(),

            #[cfg(not(feature = "no_closure"))]
            Self::Share(..) => ASTFlags::empty(),
        }
    }
    /// Get the [position][Position] of this statement.
    #[must_use]
    pub fn position(&self) -> Position {
        match self {
            Self::Noop(pos)
            | Self::BreakLoop(.., pos)
            | Self::FnCall(.., pos)
            | Self::If(.., pos)
            | Self::Switch(.., pos)
            | Self::While(.., pos)
            | Self::Do(.., pos)
            | Self::For(.., pos)
            | Self::Return(.., pos)
            | Self::Var(.., pos)
            | Self::TryCatch(.., pos) => *pos,

            Self::Assignment(x) => x.0.pos,

            Self::Block(x) => x.position(),

            Self::Expr(x) => x.start_position(),

            #[cfg(not(feature = "no_module"))]
            Self::Import(.., pos) => *pos,
            #[cfg(not(feature = "no_module"))]
            Self::Export(.., pos) => *pos,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(x) => x[0].0.pos,
        }
    }
    /// Override the [position][Position] of this statement.
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            Self::Noop(pos)
            | Self::BreakLoop(.., pos)
            | Self::FnCall(.., pos)
            | Self::If(.., pos)
            | Self::Switch(.., pos)
            | Self::While(.., pos)
            | Self::Do(.., pos)
            | Self::For(.., pos)
            | Self::Return(.., pos)
            | Self::Var(.., pos)
            | Self::TryCatch(.., pos) => *pos = new_pos,

            Self::Assignment(x) => x.0.pos = new_pos,

            Self::Block(x) => x.set_position(new_pos, x.end_position()),

            Self::Expr(x) => {
                x.set_position(new_pos);
            }

            #[cfg(not(feature = "no_module"))]
            Self::Import(.., pos) => *pos = new_pos,
            #[cfg(not(feature = "no_module"))]
            Self::Export(.., pos) => *pos = new_pos,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(x) => x.iter_mut().for_each(|(x, _)| x.pos = new_pos),
        }

        self
    }
    /// Does this statement return a value?
    #[must_use]
    pub const fn returns_value(&self) -> bool {
        match self {
            Self::If(..)
            | Self::Switch(..)
            | Self::Block(..)
            | Self::Expr(..)
            | Self::FnCall(..) => true,

            Self::Noop(..)
            | Self::While(..)
            | Self::Do(..)
            | Self::For(..)
            | Self::TryCatch(..) => false,

            Self::Var(..) | Self::Assignment(..) | Self::BreakLoop(..) | Self::Return(..) => false,

            #[cfg(not(feature = "no_module"))]
            Self::Import(..) | Self::Export(..) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(..) => false,
        }
    }
    /// Is this statement self-terminated (i.e. no need for a semicolon terminator)?
    #[must_use]
    pub const fn is_self_terminated(&self) -> bool {
        match self {
            Self::If(..)
            | Self::Switch(..)
            | Self::While(..)
            | Self::For(..)
            | Self::Block(..)
            | Self::TryCatch(..) => true,

            // A No-op requires a semicolon in order to know it is an empty statement!
            Self::Noop(..) => false,

            Self::Expr(e) => match &**e {
                #[cfg(not(feature = "no_custom_syntax"))]
                Expr::Custom(x, ..) if x.is_self_terminated() => true,
                _ => false,
            },

            Self::Var(..)
            | Self::Assignment(..)
            | Self::FnCall(..)
            | Self::Do(..)
            | Self::BreakLoop(..)
            | Self::Return(..) => false,

            #[cfg(not(feature = "no_module"))]
            Self::Import(..) | Self::Export(..) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(..) => false,
        }
    }
    /// Is this statement _pure_?
    ///
    /// A pure statement has no side effects.
    #[must_use]
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Noop(..) => true,
            Self::Expr(expr) => expr.is_pure(),
            Self::If(x, ..) => {
                x.expr.is_pure()
                    && x.body.iter().all(Self::is_pure)
                    && x.branch.iter().all(Self::is_pure)
            }
            Self::Switch(x, ..) => {
                let (expr, sw) = &**x;
                expr.is_pure()
                    && sw.cases.values().flat_map(|cases| cases.iter()).all(|&c| {
                        let block = &sw.expressions[c];
                        block.lhs.is_pure() && block.rhs.is_pure()
                    })
                    && sw.ranges.iter().all(|r| {
                        let block = &sw.expressions[r.index()];
                        block.lhs.is_pure() && block.rhs.is_pure()
                    })
                    && sw.def_case.is_some()
                    && sw.expressions[sw.def_case.unwrap()].rhs.is_pure()
            }

            // Loops that exit can be pure because it can never be infinite.
            Self::While(x, ..) if matches!(x.expr, Expr::BoolConstant(false, ..)) => true,
            Self::Do(x, options, ..) if matches!(x.expr, Expr::BoolConstant(..)) => match x.expr {
                Expr::BoolConstant(cond, ..) if cond == options.intersects(ASTFlags::NEGATED) => {
                    x.body.iter().all(Self::is_pure)
                }
                _ => false,
            },

            // Loops are never pure since they can be infinite - and that's a side effect.
            Self::While(..) | Self::Do(..) => false,

            // For loops can be pure because if the iterable is pure, it is finite,
            // so infinite loops can never occur.
            Self::For(x, ..) => x.2.expr.is_pure() && x.2.body.iter().all(Self::is_pure),

            Self::Var(..) | Self::Assignment(..) | Self::FnCall(..) => false,
            Self::Block(block, ..) => block.iter().all(Self::is_pure),
            Self::BreakLoop(..) | Self::Return(..) => false,
            Self::TryCatch(x, ..) => {
                x.expr.is_pure()
                    && x.body.iter().all(Self::is_pure)
                    && x.branch.iter().all(Self::is_pure)
            }

            #[cfg(not(feature = "no_module"))]
            Self::Import(..) => false,
            #[cfg(not(feature = "no_module"))]
            Self::Export(..) => false,

            #[cfg(not(feature = "no_closure"))]
            Self::Share(..) => false,
        }
    }
    /// Does this statement's behavior depend on its containing block?
    ///
    /// A statement that depends on its containing block behaves differently when promoted to an
    /// upper block.
    ///
    /// Currently only variable definitions (i.e. `let` and `const`), `import`/`export` statements,
    /// and `eval` calls (which may in turn define variables) fall under this category.
    #[inline]
    #[must_use]
    pub fn is_block_dependent(&self) -> bool {
        match self {
            Self::Var(..) => true,

            Self::Expr(e) => match &**e {
                Expr::Stmt(s) => s.iter().all(Self::is_block_dependent),
                Expr::FnCall(x, ..) => !x.is_qualified() && x.name == KEYWORD_EVAL,
                _ => false,
            },

            Self::FnCall(x, ..) => !x.is_qualified() && x.name == KEYWORD_EVAL,

            #[cfg(not(feature = "no_module"))]
            Self::Import(..) | Self::Export(..) => true,

            _ => false,
        }
    }
    /// Is this statement _pure_ within the containing block?
    ///
    /// An internally pure statement only has side effects that disappear outside the block.
    ///
    /// Currently only variable definitions (i.e. `let` and `const`) and `import`/`export`
    /// statements are internally pure, other than pure expressions.
    #[inline]
    #[must_use]
    pub fn is_internally_pure(&self) -> bool {
        match self {
            Self::Var(x, ..) => x.1.is_pure(),

            Self::Expr(e) => match &**e {
                Expr::Stmt(s) => s.iter().all(Self::is_internally_pure),
                _ => self.is_pure(),
            },

            #[cfg(not(feature = "no_module"))]
            Self::Import(x, ..) => x.0.is_pure(),
            #[cfg(not(feature = "no_module"))]
            Self::Export(..) => true,

            _ => self.is_pure(),
        }
    }
    /// Does this statement break the current control flow through the containing block?
    ///
    /// Currently this is only true for `return`, `throw`, `break` and `continue`.
    ///
    /// All statements following this statement will essentially be dead code.
    #[inline]
    #[must_use]
    pub const fn is_control_flow_break(&self) -> bool {
        matches!(self, Self::Return(..) | Self::BreakLoop(..))
    }
    /// Return this [`Stmt`], replacing it with [`Stmt::Noop`].
    #[inline(always)]
    #[must_use]
    pub fn take(&mut self) -> Self {
        mem::take(self)
    }
    /// Recursively walk this statement.
    /// Return `false` from the callback to terminate the walk.
    pub fn walk<'a>(
        &'a self,
        path: &mut Vec<ASTNode<'a>>,
        on_node: &mut (impl FnMut(&[ASTNode]) -> bool + ?Sized),
    ) -> bool {
        // Push the current node onto the path
        path.push(self.into());

        if !on_node(path) {
            return false;
        }

        match self {
            Self::Var(x, ..) => {
                if !x.1.walk(path, on_node) {
                    return false;
                }
            }
            Self::If(x, ..) => {
                if !x.expr.walk(path, on_node) {
                    return false;
                }
                for s in &x.body {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
                for s in &x.branch {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Switch(x, ..) => {
                let (expr, sw) = &**x;

                if !expr.walk(path, on_node) {
                    return false;
                }
                for (.., blocks) in &sw.cases {
                    for &b in blocks {
                        let block = &sw.expressions[b];

                        if !block.lhs.walk(path, on_node) {
                            return false;
                        }
                        if !block.rhs.walk(path, on_node) {
                            return false;
                        }
                    }
                }
                for r in &sw.ranges {
                    let block = &sw.expressions[r.index()];

                    if !block.lhs.walk(path, on_node) {
                        return false;
                    }
                    if !block.rhs.walk(path, on_node) {
                        return false;
                    }
                }
                if let Some(index) = sw.def_case {
                    if !sw.expressions[index].lhs.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::While(x, ..) | Self::Do(x, ..) => {
                if !x.expr.walk(path, on_node) {
                    return false;
                }
                for s in x.body.statements() {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::For(x, ..) => {
                if !x.2.expr.walk(path, on_node) {
                    return false;
                }
                for s in &x.2.body {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Assignment(x, ..) => {
                if !x.1.lhs.walk(path, on_node) {
                    return false;
                }
                if !x.1.rhs.walk(path, on_node) {
                    return false;
                }
            }
            Self::FnCall(x, ..) => {
                for s in &*x.args {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Block(x, ..) => {
                for s in x.statements() {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::TryCatch(x, ..) => {
                for s in &x.body {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
                for s in &x.branch {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Expr(e) => {
                if !e.walk(path, on_node) {
                    return false;
                }
            }
            Self::Return(Some(e), ..) => {
                if !e.walk(path, on_node) {
                    return false;
                }
            }
            #[cfg(not(feature = "no_module"))]
            Self::Import(x, ..) => {
                if !x.0.walk(path, on_node) {
                    return false;
                }
            }
            _ => (),
        }

        path.pop().unwrap();

        true
    }
}
