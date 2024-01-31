//! Module defining script expressions.

use super::{ASTFlags, ASTNode, Ident, Namespace, Stmt, StmtBlock};
use crate::engine::KEYWORD_FN_PTR;
use crate::tokenizer::Token;
use crate::types::dynamic::Union;
use crate::{
    calc_fn_hash, Dynamic, FnArgsVec, FnPtr, Identifier, ImmutableString, Position, SmartString,
    StaticVec, ThinVec, INT,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    collections::BTreeMap,
    fmt,
    fmt::Write,
    hash::Hash,
    iter::once,
    mem,
    num::{NonZeroU8, NonZeroUsize},
};

/// _(internals)_ A binary expression.
/// Exported under the `internals` feature only.
#[derive(Debug, Clone, Hash, Default)]
pub struct BinaryExpr {
    /// LHS expression.
    pub lhs: Expr,
    /// RHS expression.
    pub rhs: Expr,
}

/// _(internals)_ A custom syntax expression.
/// Exported under the `internals` feature only.
///
/// Not available under `no_custom_syntax`.
#[cfg(not(feature = "no_custom_syntax"))]
#[derive(Debug, Clone, Hash)]
pub struct CustomExpr {
    /// List of keywords.
    pub inputs: FnArgsVec<Expr>,
    /// List of tokens actually parsed.
    pub tokens: FnArgsVec<ImmutableString>,
    /// State value.
    pub state: Dynamic,
    /// Is the current [`Scope`][crate::Scope] possibly modified by this custom statement
    /// (e.g. introducing a new variable)?
    pub scope_may_be_changed: bool,
    /// Is this custom syntax self-terminated?
    pub self_terminated: bool,
}

#[cfg(not(feature = "no_custom_syntax"))]
impl CustomExpr {
    /// Is this custom syntax self-terminated (i.e. no need for a semicolon terminator)?
    ///
    /// A self-terminated custom syntax always ends in `$block$`, `}` or `;`
    #[inline(always)]
    #[must_use]
    pub const fn is_self_terminated(&self) -> bool {
        self.self_terminated
    }
}

/// _(internals)_ A set of function call hashes. Exported under the `internals` feature only.
///
/// Two separate hashes are pre-calculated because of the following patterns:
///
/// ```rhai
/// func(a, b, c);      // Native: func(a, b, c)        - 3 parameters
///                     // Script: func(a, b, c)        - 3 parameters
///
/// a.func(b, c);       // Native: func(&mut a, b, c)   - 3 parameters
///                     // Script: func(b, c)           - 2 parameters
/// ```
///
/// For normal function calls, the native hash equals the script hash.
///
/// For method-style calls, the script hash contains one fewer parameter.
///
/// Function call hashes are used in the following manner:
///
/// * First, the script hash (if any) is tried, which contains only the called function's name plus
///   the number of parameters.
///
/// * Next, the actual types of arguments are hashed and _combined_ with the native hash, which is
///   then used to search for a native function.
///
///   In other words, a complete native function call hash always contains the called function's
///   name plus the types of the arguments.  This is due to possible function overloading for
///   different parameter types.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct FnCallHashes {
    /// Pre-calculated hash for a script-defined function ([`None`] if native functions only).
    #[cfg(not(feature = "no_function"))]
    script: Option<u64>,
    /// Pre-calculated hash for a native Rust function with no parameter types.
    native: u64,
}

impl fmt::Debug for FnCallHashes {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_function"))]
        return match self.script {
            Some(script) if script == self.native => fmt::Debug::fmt(&self.native, f),
            Some(script) => write!(f, "({script}, {})", self.native),
            None => write!(f, "{} (native only)", self.native),
        };

        #[cfg(feature = "no_function")]
        return write!(f, "{}", self.native);
    }
}

impl FnCallHashes {
    /// Create a [`FnCallHashes`] from a single hash.
    #[inline]
    #[must_use]
    pub const fn from_hash(hash: u64) -> Self {
        Self {
            #[cfg(not(feature = "no_function"))]
            script: Some(hash),
            native: hash,
        }
    }
    /// Create a [`FnCallHashes`] with only the native Rust hash.
    #[inline]
    #[must_use]
    pub const fn from_native_only(hash: u64) -> Self {
        Self {
            #[cfg(not(feature = "no_function"))]
            script: None,
            native: hash,
        }
    }
    /// Create a [`FnCallHashes`] with both script function and native Rust hashes.
    ///
    /// Not available under `no_function`.
    #[cfg(not(feature = "no_function"))]
    #[inline]
    #[must_use]
    pub const fn from_script_and_native(script: u64, native: u64) -> Self {
        Self {
            script: Some(script),
            native,
        }
    }
    /// Is this [`FnCallHashes`] native-only?
    #[inline(always)]
    #[must_use]
    pub const fn is_native_only(&self) -> bool {
        #[cfg(not(feature = "no_function"))]
        return self.script.is_none();
        #[cfg(feature = "no_function")]
        return true;
    }
    /// Get the native hash.
    ///
    /// The hash returned is never zero.
    #[inline(always)]
    #[must_use]
    pub const fn native(&self) -> u64 {
        self.native
    }
    /// Get the script hash.
    ///
    /// The hash returned is never zero.
    ///
    /// # Panics
    ///
    /// Panics if this [`FnCallHashes`] is native-only.
    #[cfg(not(feature = "no_function"))]
    #[inline(always)]
    #[must_use]
    pub fn script(&self) -> u64 {
        self.script.expect("native-only hash")
    }
}

/// _(internals)_ A function call.
/// Exported under the `internals` feature only.
#[derive(Clone, Hash)]
pub struct FnCallExpr {
    /// Namespace of the function, if any.
    pub namespace: Namespace,
    /// Function name.
    pub name: ImmutableString,
    /// Pre-calculated hashes.
    pub hashes: FnCallHashes,
    /// List of function call argument expressions.
    pub args: FnArgsVec<Expr>,
    /// Does this function call capture the parent scope?
    pub capture_parent_scope: bool,
    /// Is this function call a native operator?
    pub op_token: Option<Token>,
}

impl fmt::Debug for FnCallExpr {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ff = f.debug_struct("FnCallExpr");
        if !self.namespace.is_empty() {
            ff.field("namespace", &self.namespace);
        }
        ff.field("hash", &self.hashes)
            .field("name", &self.name)
            .field("args", &self.args);
        if self.op_token.is_some() {
            ff.field("op_token", &self.op_token);
        }
        if self.capture_parent_scope {
            ff.field("capture_parent_scope", &self.capture_parent_scope);
        }
        ff.finish()
    }
}

impl FnCallExpr {
    /// Does this function call contain a qualified namespace?
    ///
    /// Always `false` under `no_module`.
    #[inline(always)]
    #[must_use]
    pub fn is_qualified(&self) -> bool {
        !self.namespace.is_empty()
    }
    /// Convert this into an [`Expr::FnCall`].
    #[inline(always)]
    #[must_use]
    pub fn into_fn_call_expr(self, pos: Position) -> Expr {
        Expr::FnCall(self.into(), pos)
    }
    /// Are all arguments constant?
    #[inline]
    #[must_use]
    pub fn constant_args(&self) -> bool {
        self.args.is_empty() || self.args.iter().all(Expr::is_constant)
    }
}

/// _(internals)_ An expression sub-tree.
/// Exported under the `internals` feature only.
#[derive(Clone, Hash)]
#[non_exhaustive]
#[allow(clippy::type_complexity)]
pub enum Expr {
    /// Dynamic constant.
    ///
    /// Used to hold complex constants such as [`Array`][crate::Array] or [`Map`][crate::Map] for quick cloning.
    /// Primitive data types should use the appropriate variants to avoid an allocation.
    ///
    /// The [`Dynamic`] value is boxed in order to avoid bloating the size of [`Expr`].
    DynamicConstant(Box<Dynamic>, Position),
    /// Boolean constant.
    BoolConstant(bool, Position),
    /// Integer constant.
    IntegerConstant(INT, Position),
    /// Floating-point constant.
    #[cfg(not(feature = "no_float"))]
    FloatConstant(crate::types::FloatWrapper<crate::FLOAT>, Position),
    /// Character constant.
    CharConstant(char, Position),
    /// [String][ImmutableString] constant.
    StringConstant(ImmutableString, Position),
    /// An interpolated [string][ImmutableString].
    InterpolatedString(ThinVec<Expr>, Position),
    /// [ expr, ... ]
    Array(ThinVec<Expr>, Position),
    /// #{ name:expr, ... }
    Map(
        Box<(StaticVec<(Ident, Expr)>, BTreeMap<Identifier, Dynamic>)>,
        Position,
    ),
    /// ()
    Unit(Position),
    /// Variable access - (optional long index, namespace, namespace hash, variable name), optional short index, position
    ///
    /// The short index is [`u8`] which is used when the index is <= 255, which should be
    /// the vast majority of cases (unless there are more than 255 variables defined!).
    /// This is to avoid reading a pointer redirection during each variable access.
    Variable(
        Box<(Option<NonZeroUsize>, Namespace, u64, ImmutableString)>,
        Option<NonZeroU8>,
        Position,
    ),
    /// `this`.
    ThisPtr(Position),
    /// Property access - ((getter, hash), (setter, hash), prop)
    Property(
        Box<(
            (ImmutableString, u64),
            (ImmutableString, u64),
            ImmutableString,
        )>,
        Position,
    ),
    /// xxx `.` method `(` expr `,` ... `)`
    MethodCall(Box<FnCallExpr>, Position),
    /// { [statement][Stmt] ... }
    Stmt(Box<StmtBlock>),
    /// func `(` expr `,` ... `)`
    FnCall(Box<FnCallExpr>, Position),
    /// lhs `.` rhs | lhs `?.` rhs
    ///
    /// ### Flags
    ///
    /// * [`NEGATED`][ASTFlags::NEGATED] = `?.` (`.` if unset)
    /// * [`BREAK`][ASTFlags::BREAK] = terminate the chain (recurse into the chain if unset)
    Dot(Box<BinaryExpr>, ASTFlags, Position),
    /// lhs `[` rhs `]`
    ///
    /// ### Flags
    ///
    /// * [`NEGATED`][ASTFlags::NEGATED] = `?[` ... `]` (`[` ... `]` if unset)
    /// * [`BREAK`][ASTFlags::BREAK] = terminate the chain (recurse into the chain if unset)
    Index(Box<BinaryExpr>, ASTFlags, Position),
    /// lhs `&&` rhs
    And(Box<BinaryExpr>, Position),
    /// lhs `||` rhs
    Or(Box<BinaryExpr>, Position),
    /// lhs `??` rhs
    Coalesce(Box<BinaryExpr>, Position),
    /// Custom syntax
    #[cfg(not(feature = "no_custom_syntax"))]
    Custom(Box<CustomExpr>, Position),
}

impl Default for Expr {
    #[inline(always)]
    #[must_use]
    fn default() -> Self {
        Self::Unit(Position::NONE)
    }
}

impl fmt::Debug for Expr {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut display_pos = self.start_position();

        match self {
            Self::DynamicConstant(value, ..) => write!(f, "{value:?}"),
            Self::BoolConstant(value, ..) => write!(f, "{value:?}"),
            Self::IntegerConstant(value, ..) => write!(f, "{value:?}"),
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(value, ..) => write!(f, "{value:?}"),
            Self::CharConstant(value, ..) => write!(f, "{value:?}"),
            Self::StringConstant(value, ..) => write!(f, "{value:?}"),
            Self::Unit(..) => f.write_str("()"),

            Self::InterpolatedString(x, ..) => {
                f.write_str("InterpolatedString")?;
                return f.debug_list().entries(x.iter()).finish();
            }
            Self::Array(x, ..) => {
                f.write_str("Array")?;
                f.debug_list().entries(x.iter()).finish()
            }
            Self::Map(x, ..) => {
                f.write_str("Map")?;
                f.debug_map()
                    .entries(x.0.iter().map(|(k, v)| (k, v)))
                    .finish()
            }
            Self::ThisPtr(..) => f.debug_struct("ThisPtr").finish(),
            Self::Variable(x, i, ..) => {
                f.write_str("Variable(")?;

                #[cfg(not(feature = "no_module"))]
                if !x.1.is_empty() {
                    write!(f, "{}{}", x.1, crate::engine::NAMESPACE_SEPARATOR)?;
                    let pos = x.1.position();
                    if !pos.is_none() {
                        display_pos = pos;
                    }
                }
                f.write_str(&x.3)?;
                #[cfg(not(feature = "no_module"))]
                if let Some(n) = x.1.index {
                    write!(f, " #{n}")?;
                }
                if let Some(n) = i.map_or_else(|| x.0, |n| NonZeroUsize::new(n.get() as usize)) {
                    write!(f, " #{n}")?;
                }
                f.write_str(")")
            }
            Self::Property(x, ..) => write!(f, "Property({})", x.2),
            Self::MethodCall(x, ..) => f.debug_tuple("MethodCall").field(x).finish(),
            Self::Stmt(x) => {
                let pos = x.span();
                if !pos.is_none() {
                    display_pos = pos.start();
                }
                f.write_str("ExprStmtBlock")?;
                f.debug_list().entries(x.iter()).finish()
            }
            Self::FnCall(x, ..) => fmt::Debug::fmt(x, f),
            Self::Index(x, options, pos) => {
                if !pos.is_none() {
                    display_pos = *pos;
                }

                let mut f = f.debug_struct("Index");

                f.field("lhs", &x.lhs).field("rhs", &x.rhs);
                if !options.is_empty() {
                    f.field("options", options);
                }
                f.finish()
            }
            Self::Dot(x, options, pos) => {
                if !pos.is_none() {
                    display_pos = *pos;
                }

                let mut f = f.debug_struct("Dot");

                f.field("lhs", &x.lhs).field("rhs", &x.rhs);
                if !options.is_empty() {
                    f.field("options", options);
                }
                f.finish()
            }
            Self::And(x, pos) | Self::Or(x, pos) | Self::Coalesce(x, pos) => {
                let op_name = match self {
                    Self::And(..) => "And",
                    Self::Or(..) => "Or",
                    Self::Coalesce(..) => "Coalesce",
                    expr => unreachable!("`And`, `Or` or `Coalesce` expected but gets {:?}", expr),
                };

                if !pos.is_none() {
                    display_pos = *pos;
                }

                f.debug_struct(op_name)
                    .field("lhs", &x.lhs)
                    .field("rhs", &x.rhs)
                    .finish()
            }
            #[cfg(not(feature = "no_custom_syntax"))]
            Self::Custom(x, ..) => f.debug_tuple("Custom").field(x).finish(),
        }?;

        write!(f, " @ {display_pos:?}")
    }
}

impl Expr {
    /// Get the [`Dynamic`] value of a literal constant expression.
    ///
    /// Returns [`None`] if the expression is not a literal constant.
    #[inline]
    #[must_use]
    pub fn get_literal_value(&self) -> Option<Dynamic> {
        Some(match self {
            Self::DynamicConstant(x, ..) => x.as_ref().clone(),
            Self::IntegerConstant(x, ..) => (*x).into(),
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(x, ..) => (*x).into(),
            Self::CharConstant(x, ..) => (*x).into(),
            Self::StringConstant(x, ..) => x.clone().into(),
            Self::BoolConstant(x, ..) => (*x).into(),
            Self::Unit(..) => Dynamic::UNIT,

            #[cfg(not(feature = "no_index"))]
            Self::Array(x, ..) if self.is_constant() => {
                let mut arr = crate::Array::with_capacity(x.len());
                arr.extend(x.iter().map(|v| v.get_literal_value().unwrap()));
                Dynamic::from_array(arr)
            }

            #[cfg(not(feature = "no_object"))]
            Self::Map(x, ..) if self.is_constant() => {
                let mut map = x.1.clone();

                for (k, v) in &x.0 {
                    *map.get_mut(k.as_str()).unwrap() = v.get_literal_value().unwrap();
                }

                Dynamic::from_map(map)
            }

            // Interpolated string
            Self::InterpolatedString(x, ..) if self.is_constant() => {
                let mut s = SmartString::new_const();
                for segment in x.iter() {
                    let v = segment.get_literal_value().unwrap();
                    write!(&mut s, "{v}").unwrap();
                }
                s.into()
            }

            // Fn
            Self::FnCall(ref x, ..)
                if !x.is_qualified() && x.args.len() == 1 && x.name == KEYWORD_FN_PTR =>
            {
                match x.args[0] {
                    Self::StringConstant(ref s, ..) => FnPtr::new(s.clone()).ok()?.into(),
                    _ => return None,
                }
            }

            // Binary operators
            Self::FnCall(x, ..) if !x.is_qualified() && x.args.len() == 2 => {
                pub const OP_EXCLUSIVE_RANGE: &str = Token::ExclusiveRange.literal_syntax();
                pub const OP_INCLUSIVE_RANGE: &str = Token::InclusiveRange.literal_syntax();

                match x.name.as_str() {
                    // x..y
                    OP_EXCLUSIVE_RANGE => match (&x.args[0], &x.args[1]) {
                        (
                            Self::IntegerConstant(ref start, ..),
                            Self::IntegerConstant(ref end, ..),
                        ) => (*start..*end).into(),
                        _ => return None,
                    },
                    // x..=y
                    OP_INCLUSIVE_RANGE => match (&x.args[0], &x.args[1]) {
                        (
                            Self::IntegerConstant(ref start, ..),
                            Self::IntegerConstant(ref end, ..),
                        ) => (*start..=*end).into(),
                        _ => return None,
                    },
                    _ => return None,
                }
            }

            _ => return None,
        })
    }
    /// Create an [`Expr`] from a [`Dynamic`] value.
    #[inline]
    #[must_use]
    pub fn from_dynamic(value: Dynamic, pos: Position) -> Self {
        match value.0 {
            Union::Unit(..) => Self::Unit(pos),
            Union::Bool(b, ..) => Self::BoolConstant(b, pos),
            Union::Str(s, ..) => Self::StringConstant(s, pos),
            Union::Char(c, ..) => Self::CharConstant(c, pos),
            Union::Int(i, ..) => Self::IntegerConstant(i, pos),

            #[cfg(feature = "decimal")]
            Union::Decimal(value, ..) => Self::DynamicConstant(Box::new((*value).into()), pos),

            #[cfg(not(feature = "no_float"))]
            Union::Float(f, ..) => Self::FloatConstant(f, pos),

            #[cfg(not(feature = "no_index"))]
            Union::Array(a, ..) => Self::DynamicConstant(Box::new((*a).into()), pos),

            #[cfg(not(feature = "no_object"))]
            Union::Map(m, ..) => Self::DynamicConstant(Box::new((*m).into()), pos),

            Union::FnPtr(f, ..) if !f.is_curried() => Self::FnCall(
                FnCallExpr {
                    namespace: Namespace::NONE,
                    name: KEYWORD_FN_PTR.into(),
                    hashes: FnCallHashes::from_hash(calc_fn_hash(None, f.fn_name(), 1)),
                    args: once(Self::StringConstant(f.fn_name().into(), pos)).collect(),
                    capture_parent_scope: false,
                    op_token: None,
                }
                .into(),
                pos,
            ),

            _ => Self::DynamicConstant(value.into(), pos),
        }
    }
    /// Return the variable name if the expression a simple variable access.
    ///
    /// `non_qualified` is ignored under `no_module`.
    #[inline]
    #[must_use]
    pub(crate) fn get_variable_name(&self, _non_qualified: bool) -> Option<&str> {
        match self {
            #[cfg(not(feature = "no_module"))]
            Self::Variable(x, ..) if _non_qualified && !x.1.is_empty() => None,
            Self::Variable(x, ..) => Some(&x.3),
            _ => None,
        }
    }
    /// Get the [options][ASTFlags] of the expression.
    #[inline]
    #[must_use]
    pub const fn options(&self) -> ASTFlags {
        match self {
            Self::Index(_, options, _) | Self::Dot(_, options, _) => *options,

            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(..) => ASTFlags::empty(),

            Self::DynamicConstant(..)
            | Self::BoolConstant(..)
            | Self::IntegerConstant(..)
            | Self::CharConstant(..)
            | Self::Unit(..)
            | Self::StringConstant(..)
            | Self::Array(..)
            | Self::Map(..)
            | Self::Variable(..)
            | Self::ThisPtr(..)
            | Self::And(..)
            | Self::Or(..)
            | Self::Coalesce(..)
            | Self::FnCall(..)
            | Self::MethodCall(..)
            | Self::InterpolatedString(..)
            | Self::Property(..)
            | Self::Stmt(..) => ASTFlags::empty(),

            #[cfg(not(feature = "no_custom_syntax"))]
            Self::Custom(..) => ASTFlags::empty(),
        }
    }
    /// Get the [position][Position] of the expression.
    #[inline]
    #[must_use]
    pub const fn position(&self) -> Position {
        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(.., pos) => *pos,

            Self::DynamicConstant(.., pos)
            | Self::BoolConstant(.., pos)
            | Self::IntegerConstant(.., pos)
            | Self::CharConstant(.., pos)
            | Self::Unit(pos)
            | Self::StringConstant(.., pos)
            | Self::Array(.., pos)
            | Self::Map(.., pos)
            | Self::Variable(.., pos)
            | Self::ThisPtr(pos)
            | Self::And(.., pos)
            | Self::Or(.., pos)
            | Self::Coalesce(.., pos)
            | Self::FnCall(.., pos)
            | Self::MethodCall(.., pos)
            | Self::Index(.., pos)
            | Self::Dot(.., pos)
            | Self::InterpolatedString(.., pos)
            | Self::Property(.., pos) => *pos,

            #[cfg(not(feature = "no_custom_syntax"))]
            Self::Custom(.., pos) => *pos,

            Self::Stmt(x) => x.position(),
        }
    }
    /// Get the starting [position][Position] of the expression.
    /// For a binary expression, this will be the left-most LHS instead of the operator.
    #[inline]
    #[must_use]
    pub fn start_position(&self) -> Position {
        match self {
            #[cfg(not(feature = "no_module"))]
            Self::Variable(x, ..) => {
                if x.1.is_empty() {
                    self.position()
                } else {
                    x.1.position()
                }
            }

            Self::And(x, ..)
            | Self::Or(x, ..)
            | Self::Coalesce(x, ..)
            | Self::Index(x, ..)
            | Self::Dot(x, ..) => x.lhs.start_position(),

            Self::FnCall(.., pos) => *pos,

            _ => self.position(),
        }
    }
    /// Override the [position][Position] of the expression.
    #[inline]
    pub fn set_position(&mut self, new_pos: Position) -> &mut Self {
        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(.., pos) => *pos = new_pos,

            Self::DynamicConstant(.., pos)
            | Self::BoolConstant(.., pos)
            | Self::IntegerConstant(.., pos)
            | Self::CharConstant(.., pos)
            | Self::Unit(pos)
            | Self::StringConstant(.., pos)
            | Self::Array(.., pos)
            | Self::Map(.., pos)
            | Self::And(.., pos)
            | Self::Or(.., pos)
            | Self::Coalesce(.., pos)
            | Self::Dot(.., pos)
            | Self::Index(.., pos)
            | Self::Variable(.., pos)
            | Self::ThisPtr(pos)
            | Self::FnCall(.., pos)
            | Self::MethodCall(.., pos)
            | Self::InterpolatedString(.., pos)
            | Self::Property(.., pos) => *pos = new_pos,

            #[cfg(not(feature = "no_custom_syntax"))]
            Self::Custom(.., pos) => *pos = new_pos,

            Self::Stmt(x) => x.set_position(new_pos, Position::NONE),
        }

        self
    }
    /// Is the expression pure?
    ///
    /// A pure expression has no side effects.
    #[inline]
    #[must_use]
    pub fn is_pure(&self) -> bool {
        match self {
            Self::InterpolatedString(x, ..) | Self::Array(x, ..) => x.iter().all(Self::is_pure),

            Self::Map(x, ..) => x.0.iter().map(|(.., v)| v).all(Self::is_pure),

            Self::And(x, ..) | Self::Or(x, ..) | Self::Coalesce(x, ..) => {
                x.lhs.is_pure() && x.rhs.is_pure()
            }

            Self::Stmt(x) => x.iter().all(Stmt::is_pure),

            Self::Variable(..) => true,

            _ => self.is_constant(),
        }
    }
    /// Is the expression the unit `()` literal?
    #[inline(always)]
    #[must_use]
    pub const fn is_unit(&self) -> bool {
        matches!(self, Self::Unit(..))
    }
    /// Is the expression a constant?
    #[inline]
    #[must_use]
    pub fn is_constant(&self) -> bool {
        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(..) => true,

            Self::DynamicConstant(..)
            | Self::BoolConstant(..)
            | Self::IntegerConstant(..)
            | Self::CharConstant(..)
            | Self::StringConstant(..)
            | Self::Unit(..) => true,

            Self::InterpolatedString(x, ..) | Self::Array(x, ..) => x.iter().all(Self::is_constant),

            Self::Map(x, ..) => x.0.iter().map(|(.., expr)| expr).all(Self::is_constant),

            _ => false,
        }
    }
    /// Is a particular [token][Token] allowed as a postfix operator to this expression?
    #[inline]
    #[must_use]
    pub const fn is_valid_postfix(&self, token: &Token) -> bool {
        match token {
            #[cfg(not(feature = "no_object"))]
            Token::Period | Token::Elvis => return true,
            #[cfg(not(feature = "no_index"))]
            Token::LeftBracket | Token::QuestionBracket => return true,
            _ => (),
        }

        match self {
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(..) => false,

            Self::DynamicConstant(..)
            | Self::BoolConstant(..)
            | Self::CharConstant(..)
            | Self::And(..)
            | Self::Or(..)
            | Self::Coalesce(..)
            | Self::Unit(..) => false,

            Self::IntegerConstant(..)
            | Self::StringConstant(..)
            | Self::InterpolatedString(..)
            | Self::FnCall(..)
            | Self::ThisPtr(..)
            | Self::MethodCall(..)
            | Self::Stmt(..)
            | Self::Dot(..)
            | Self::Index(..)
            | Self::Array(..)
            | Self::Map(..) => false,

            #[cfg(not(feature = "no_custom_syntax"))]
            Self::Custom(..) => false,

            Self::Variable(..) => matches!(
                token,
                Token::LeftParen | Token::Unit | Token::Bang | Token::DoubleColon
            ),

            Self::Property(..) => matches!(token, Token::LeftParen),
        }
    }
    /// Return this [`Expr`], replacing it with [`Expr::Unit`].
    #[inline(always)]
    #[must_use]
    pub fn take(&mut self) -> Self {
        mem::take(self)
    }
    /// Recursively walk this expression.
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
            Self::Stmt(x) => {
                for s in &**x {
                    if !s.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::InterpolatedString(x, ..) | Self::Array(x, ..) => {
                for e in &**x {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Map(x, ..) => {
                for (.., e) in &x.0 {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            Self::Index(x, ..)
            | Self::Dot(x, ..)
            | Self::And(x, ..)
            | Self::Or(x, ..)
            | Self::Coalesce(x, ..) => {
                if !x.lhs.walk(path, on_node) {
                    return false;
                }
                if !x.rhs.walk(path, on_node) {
                    return false;
                }
            }
            Self::FnCall(x, ..) => {
                for e in &*x.args {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            #[cfg(not(feature = "no_custom_syntax"))]
            Self::Custom(x, ..) => {
                for e in &*x.inputs {
                    if !e.walk(path, on_node) {
                        return false;
                    }
                }
            }
            _ => (),
        }

        path.pop().unwrap();

        true
    }
}
