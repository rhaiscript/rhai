//! Main module defining the lexer and parser.

use crate::api::options::LangOptions;
use crate::ast::{
    ASTFlags, BinaryExpr, CaseBlocksList, Expr, FlowControl, FnCallExpr, FnCallHashes, Ident,
    Namespace, OpAssignment, RangeCase, ScriptFuncDef, Stmt, StmtBlock, StmtBlockContainer,
    SwitchCasesCollection,
};
use crate::engine::{Precedence, OP_CONTAINS, OP_NOT};
use crate::eval::{Caches, GlobalRuntimeState};
use crate::func::{hashing::get_hasher, StraightHashMap};
use crate::tokenizer::{
    is_reserved_keyword_or_symbol, is_valid_function_name, is_valid_identifier, Token, TokenStream,
    TokenizerControl,
};
use crate::types::{
    dynamic::{AccessMode, Union},
    StringsInterner,
};
use crate::{
    calc_fn_hash, Dynamic, Engine, EvalAltResult, EvalContext, ExclusiveRange, FnArgsVec,
    ImmutableString, InclusiveRange, LexError, OptimizationLevel, ParseError, Position, Scope,
    Shared, SmartString, StaticVec, ThinVec, VarDefInfo, AST, PERR,
};
use bitflags::bitflags;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    convert::TryFrom,
    fmt,
    hash::{Hash, Hasher},
    num::{NonZeroU8, NonZeroUsize},
};

pub type ParseResult<T> = Result<T, ParseError>;

type FnLib = StraightHashMap<Shared<ScriptFuncDef>>;

/// Invalid variable name that acts as a search barrier in a [`Scope`].
const SCOPE_SEARCH_BARRIER_MARKER: &str = "$ BARRIER $";

impl PERR {
    /// Make a [`ParseError`] using the current type and position.
    #[cold]
    #[inline(never)]
    fn into_err(self, pos: Position) -> ParseError {
        ParseError(self.into(), pos)
    }
}

/// _(internals)_ A type that encapsulates the current state of the parser.
/// Exported under the `internals` feature only.
pub struct ParseState<'e, 's> {
    /// Input stream buffer containing the next character to read.
    pub tokenizer_control: TokenizerControl,
    /// Controls whether parsing of an expression should stop given the next token.
    pub expr_filter: fn(&Token) -> bool,
    /// Strings interner.
    pub interned_strings: &'s mut StringsInterner,
    /// External [scope][Scope] with constants.
    pub external_constants: Option<&'e Scope<'e>>,
    /// Global runtime state.
    pub global: Option<Box<GlobalRuntimeState>>,
    /// Encapsulates a local stack with variable names to simulate an actual runtime scope.
    pub stack: Scope<'e>,
    /// Size of the local variables stack upon entry of the current block scope.
    pub frame_pointer: usize,
    /// Tracks a list of external variables (variables that are not explicitly declared in the scope).
    #[cfg(not(feature = "no_closure"))]
    pub external_vars: ThinVec<Ident>,
    /// An indicator that, when set to `false`, disables variable capturing into externals one
    /// single time up until the nearest consumed Identifier token.
    ///
    /// If set to `false` the next call to [`access_var`][ParseState::access_var] will not capture
    /// the variable.
    ///
    /// All consequent calls to [`access_var`][ParseState::access_var] will not be affected.
    pub allow_capture: bool,
    /// Encapsulates a local stack with imported [module][crate::Module] names.
    #[cfg(not(feature = "no_module"))]
    pub imports: ThinVec<ImmutableString>,
    /// List of globally-imported [module][crate::Module] names.
    #[cfg(not(feature = "no_module"))]
    pub global_imports: ThinVec<ImmutableString>,
}

impl fmt::Debug for ParseState<'_, '_> {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("ParseState");

        f.field("tokenizer_control", &self.tokenizer_control)
            .field("interned_strings", &self.interned_strings)
            .field("external_constants_scope", &self.external_constants)
            .field("global", &self.global)
            .field("stack", &self.stack)
            .field("frame_pointer", &self.frame_pointer);

        #[cfg(not(feature = "no_closure"))]
        f.field("external_vars", &self.external_vars)
            .field("allow_capture", &self.allow_capture);

        #[cfg(not(feature = "no_module"))]
        f.field("imports", &self.imports)
            .field("global_imports", &self.global_imports);

        f.finish()
    }
}

impl<'e, 's> ParseState<'e, 's> {
    /// Create a new [`ParseState`].
    #[inline]
    #[must_use]
    pub fn new(
        external_constants: Option<&'e Scope>,
        interned_strings: &'s mut StringsInterner,
        tokenizer_control: TokenizerControl,
    ) -> Self {
        Self {
            tokenizer_control,
            expr_filter: |_| true,
            #[cfg(not(feature = "no_closure"))]
            external_vars: ThinVec::new(),
            allow_capture: true,
            interned_strings,
            external_constants,
            global: None,
            stack: Scope::new(),
            frame_pointer: 0,
            #[cfg(not(feature = "no_module"))]
            imports: ThinVec::new(),
            #[cfg(not(feature = "no_module"))]
            global_imports: ThinVec::new(),
        }
    }

    /// Find explicitly declared variable by name in the [`ParseState`], searching in reverse order.
    ///
    /// The first return value is the offset to be deducted from `ParseState::stack::len()`,
    /// i.e. the top element of [`ParseState`]'s variables stack is offset 1.
    ///
    /// If the variable is not present in the scope, the first return value is zero.
    ///
    /// The second return value indicates whether the barrier has been hit before finding the variable.
    #[must_use]
    pub fn find_var(&self, name: &str) -> (usize, bool) {
        let mut hit_barrier = false;

        let index = self
            .stack
            .iter_rev_inner()
            .position(|(n, ..)| {
                if n == SCOPE_SEARCH_BARRIER_MARKER {
                    // Do not go beyond the barrier
                    hit_barrier = true;
                    false
                } else {
                    n == name
                }
            })
            .map_or(0, |i| i + 1);

        (index, hit_barrier)
    }

    /// Find explicitly declared variable by name in the [`ParseState`], searching in reverse order.
    ///
    /// If the variable is not present in the scope adds it to the list of external variables.
    ///
    /// The return value is the offset to be deducted from `ParseState::stack::len()`,
    /// i.e. the top element of [`ParseState`]'s variables stack is offset 1.
    ///
    /// # Return value: `(index, is_func_name)`
    ///
    /// * `index`: [`None`] when the variable name is not found in the `stack`,
    ///            otherwise the index value.
    ///
    /// * `is_func_name`: `true` if the variable is actually the name of a function
    ///                   (in which case it will be converted into a function pointer).
    #[must_use]
    pub fn access_var(
        &mut self,
        name: &str,
        lib: &FnLib,
        pos: Position,
    ) -> (Option<NonZeroUsize>, bool) {
        let _lib = lib;
        let _pos = pos;

        let (index, hit_barrier) = self.find_var(name);

        #[cfg(not(feature = "no_function"))]
        let is_func_name = _lib.values().any(|f| f.name == name);
        #[cfg(feature = "no_function")]
        let is_func_name = false;

        #[cfg(not(feature = "no_closure"))]
        if self.allow_capture {
            if !is_func_name && index == 0 && !self.external_vars.iter().any(|v| v.name == name) {
                let name = self.interned_strings.get(name);
                self.external_vars.push(Ident { name, pos: _pos });
            }
        } else {
            self.allow_capture = true;
        }

        let index = (!hit_barrier).then(|| NonZeroUsize::new(index)).flatten();

        (index, is_func_name)
    }

    /// Find a module by name in the [`ParseState`], searching in reverse.
    ///
    /// Returns the offset to be deducted from `Stack::len`,
    /// i.e. the top element of the [`ParseState`] is offset 1.
    ///
    /// Returns [`None`] when the variable name is not found in the [`ParseState`].
    ///
    /// # Panics
    ///
    /// Panics when called under `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[must_use]
    pub fn find_module(&self, name: &str) -> Option<NonZeroUsize> {
        self.imports
            .iter()
            .rev()
            .rposition(|n| n == name)
            .and_then(|i| NonZeroUsize::new(i + 1))
    }

    /// Get an interned string, creating one if it is not yet interned.
    #[inline(always)]
    #[must_use]
    pub fn get_interned_string(
        &mut self,
        text: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        self.interned_strings.get(text)
    }

    /// Get an interned property getter, creating one if it is not yet interned.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    #[must_use]
    pub fn get_interned_getter(
        &mut self,
        text: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        self.interned_strings.get_with_mapper(
            b'g',
            |s| crate::engine::make_getter(s.as_ref()).into(),
            text,
        )
    }

    /// Get an interned property setter, creating one if it is not yet interned.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    #[must_use]
    pub fn get_interned_setter(
        &mut self,
        text: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        self.interned_strings.get_with_mapper(
            b's',
            |s| crate::engine::make_setter(s.as_ref()).into(),
            text,
        )
    }
}

bitflags! {
    /// Bit-flags containing all status for [`ParseSettings`].
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct ParseSettingFlags: u8 {
        /// Is the construct being parsed located at global level?
        const GLOBAL_LEVEL = 0b0000_0001;
        /// Is the construct being parsed located inside a function definition?
        const FN_SCOPE = 0b0000_0010;
        /// Is the construct being parsed located inside a closure definition?
        const CLOSURE_SCOPE = 0b0000_0100;
        /// Is the construct being parsed located inside a breakable loop?
        const BREAKABLE = 0b0000_1000;

        /// Disallow statements in blocks?
        const DISALLOW_STATEMENTS_IN_BLOCKS = 0b0001_0000;
        /// Disallow unquoted map properties?
        const DISALLOW_UNQUOTED_MAP_PROPERTIES = 0b0010_0000;
    }
}

bitflags! {
    /// Bit-flags containing all status for parsing property/indexing/namespace chains.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    struct ChainingFlags: u8 {
        /// Is the construct being parsed a property?
        const PROPERTY = 0b0000_0001;
        /// Disallow namespaces?
        const DISALLOW_NAMESPACES = 0b0000_0010;
    }
}

/// A type that encapsulates all the settings for a particular parsing function.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ParseSettings {
    /// Flags.
    pub flags: ParseSettingFlags,
    /// Language options in effect (overrides Engine options).
    pub options: LangOptions,
    /// Current expression nesting level.
    pub level: usize,
    /// Current position.
    pub pos: Position,
    /// Maximum levels of expression nesting (0 for unlimited).
    #[cfg(not(feature = "unchecked"))]
    pub max_expr_depth: usize,
}

impl ParseSettings {
    /// Is a particular flag on?
    #[inline(always)]
    #[must_use]
    pub const fn has_flag(&self, flag: ParseSettingFlags) -> bool {
        self.flags.intersects(flag)
    }
    /// Is a particular language option on?
    #[inline(always)]
    #[must_use]
    pub const fn has_option(&self, option: LangOptions) -> bool {
        self.options.intersects(option)
    }
    /// Create a new `ParseSettings` with one higher expression level.
    #[inline]
    pub fn level_up(&self) -> ParseResult<Self> {
        #[cfg(not(feature = "unchecked"))]
        if self.max_expr_depth > 0 && self.level >= self.max_expr_depth {
            return Err(PERR::ExprTooDeep.into_err(self.pos));
        }

        Ok(Self {
            level: self.level + 1,
            ..*self
        })
    }
    /// Create a new `ParseSettings` with one higher expression level.
    #[inline]
    pub fn level_up_with_position(&self, pos: Position) -> ParseResult<Self> {
        let mut x = self.level_up()?;
        x.pos = pos;
        Ok(x)
    }
}

/// Make an anonymous function.
#[cfg(not(feature = "no_function"))]
#[inline]
#[must_use]
pub fn make_anonymous_fn(hash: u64) -> crate::Identifier {
    use std::fmt::Write;

    let mut buf = crate::Identifier::new_const();
    write!(&mut buf, "{}{hash:016x}", crate::engine::FN_ANONYMOUS).unwrap();
    buf
}

/// Is this function an anonymous function?
#[cfg(not(feature = "no_function"))]
#[inline(always)]
#[must_use]
pub fn is_anonymous_fn(fn_name: &str) -> bool {
    fn_name.starts_with(crate::engine::FN_ANONYMOUS)
}

impl Expr {
    /// Convert a [`Variable`][Expr::Variable] into a [`Property`][Expr::Property].
    /// All other variants are untouched.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    #[must_use]
    fn into_property(self, state: &mut ParseState) -> Self {
        match self {
            #[cfg(not(feature = "no_module"))]
            Self::Variable(x, ..) if !x.1.is_empty() => unreachable!("qualified property"),
            Self::Variable(x, .., pos) => {
                let ident = x.3.clone();
                let getter = state.get_interned_getter(&ident);
                let hash_get = calc_fn_hash(None, &getter, 1);
                let setter = state.get_interned_setter(&ident);
                let hash_set = calc_fn_hash(None, &setter, 2);

                Self::Property(
                    Box::new(((getter, hash_get), (setter, hash_set), ident)),
                    pos,
                )
            }
            _ => self,
        }
    }
    /// Raise an error if the expression can never yield a boolean value.
    fn ensure_bool_expr(self) -> ParseResult<Self> {
        let type_name = match self {
            Self::Unit(..) => "()",
            Self::DynamicConstant(ref v, ..) if !v.is_bool() => v.type_name(),
            Self::IntegerConstant(..) => "a number",
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(..) => "a floating-point number",
            Self::CharConstant(..) => "a character",
            Self::StringConstant(..) => "a string",
            Self::InterpolatedString(..) => "a string",
            Self::Array(..) => "an array",
            Self::Map(..) => "an object map",
            _ => return Ok(self),
        };

        Err(
            PERR::MismatchedType("a boolean expression".into(), type_name.into())
                .into_err(self.start_position()),
        )
    }
    /// Raise an error if the expression can never yield an iterable value.
    fn ensure_iterable(self) -> ParseResult<Self> {
        let type_name = match self {
            Self::Unit(..) => "()",
            Self::BoolConstant(..) => "a boolean",
            Self::IntegerConstant(..) => "a number",
            #[cfg(not(feature = "no_float"))]
            Self::FloatConstant(..) => "a floating-point number",
            Self::CharConstant(..) => "a character",
            Self::Map(..) => "an object map",
            _ => return Ok(self),
        };

        Err(
            PERR::MismatchedType("an iterable value".into(), type_name.into())
                .into_err(self.start_position()),
        )
    }
}

/// Make sure that the next expression is not a statement expression (i.e. wrapped in `{}`).
fn ensure_not_statement_expr(
    input: &mut TokenStream,
    type_name: &(impl ToString + ?Sized),
) -> ParseResult<()> {
    match input.peek().unwrap() {
        (Token::LeftBrace, pos) => Err(PERR::ExprExpected(type_name.to_string()).into_err(*pos)),
        _ => Ok(()),
    }
}

/// Make sure that the next expression is not a mis-typed assignment (i.e. `a = b` instead of `a == b`).
fn ensure_not_assignment(input: &mut TokenStream) -> ParseResult<()> {
    match input.peek().unwrap() {
        (token @ Token::Equals, pos) => Err(LexError::ImproperSymbol(
            token.literal_syntax().into(),
            "Possibly a typo of '=='?".into(),
        )
        .into_err(*pos)),
        _ => Ok(()),
    }
}

/// Consume a particular [token][Token], checking that it is the expected one.
///
/// # Panics
///
/// Panics if the next token is not the expected one, or either tokens is not a literal symbol.
fn eat_token(input: &mut TokenStream, expected_token: &Token) -> Position {
    let (t, pos) = input.next().unwrap();

    assert_eq!(
        &t,
        expected_token,
        "{} expected but gets {} at {}",
        expected_token.literal_syntax(),
        t.literal_syntax(),
        pos,
    );

    pos
}

/// Match a particular [token][Token], consuming it if matched.
fn match_token(input: &mut TokenStream, token: &Token) -> (bool, Position) {
    let (t, pos) = input.peek().unwrap();
    if t == token {
        (true, eat_token(input, token))
    } else {
        (false, *pos)
    }
}

/// Process a block comment such that it indents properly relative to the start token.
#[cfg(not(feature = "no_function"))]
#[cfg(feature = "metadata")]
#[inline]
fn unindent_block_comment(comment: String, pos: usize) -> String {
    if pos == 0 || !comment.contains('\n') {
        return comment;
    }

    // Note, use `trim_start_matches` instead of `trim` because `trim` will remove even multi-byte
    // Unicode spaces, which may cause the minimum offset to end up inside that multi-byte space
    // character. Therefore, be conservative and only remove ASCII spaces.
    let offset = comment
        .lines()
        .skip(1)
        .map(|s| s.len() - s.trim_start_matches(' ').len())
        .min()
        .unwrap_or(pos)
        .min(pos);

    if offset == 0 {
        return comment;
    }

    comment
        .lines()
        .enumerate()
        .map(|(i, s)| if i > 0 { &s[offset..] } else { s })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse a variable name.
fn parse_var_name(input: &mut TokenStream) -> ParseResult<(SmartString, Position)> {
    match input.next().unwrap() {
        // Variable name
        (Token::Identifier(s), pos) => Ok((*s, pos)),
        // Reserved keyword
        (Token::Reserved(s), pos) if is_valid_identifier(&s) => {
            Err(PERR::Reserved(s.to_string()).into_err(pos))
        }
        // Bad identifier
        (Token::LexError(err), pos) => Err(err.into_err(pos)),
        // Not a variable name
        (.., pos) => Err(PERR::VariableExpected.into_err(pos)),
    }
}

/// Optimize the structure of a chained expression where the root expression is another chained expression.
///
/// # Panics
///
/// Panics if the expression is not a combo chain.
#[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
fn optimize_combo_chain(expr: &mut Expr) {
    #[allow(clippy::type_complexity)]
    let (mut x, x_options, x_pos, mut root, mut root_options, root_pos, make_sub, make_root): (
        _,
        _,
        _,
        _,
        _,
        _,
        fn(_, _, _) -> Expr,
        fn(_, _, _) -> Expr,
    ) = match expr.take() {
        #[cfg(not(feature = "no_index"))]
        Expr::Index(mut x, opt, pos) => match x.lhs.take() {
            Expr::Index(x2, opt2, pos2) => (x, opt, pos, x2, opt2, pos2, Expr::Index, Expr::Index),
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(x2, opt2, pos2) => (x, opt, pos, x2, opt2, pos2, Expr::Index, Expr::Dot),
            _ => unreachable!("combo chain expected"),
        },
        #[cfg(not(feature = "no_object"))]
        Expr::Dot(mut x, opt, pos) => match x.lhs.take() {
            #[cfg(not(feature = "no_index"))]
            Expr::Index(x2, opt2, pos2) => (x, opt, pos, x2, opt2, pos2, Expr::Dot, Expr::Index),
            Expr::Dot(x2, opt2, pos2) => (x, opt, pos, x2, opt2, pos2, Expr::Dot, Expr::Dot),
            _ => unreachable!("combo chain expected"),
        },
        _ => unreachable!("combo chain expected"),
    };

    // Rewrite the chains like this:
    //
    // Source: ( x[y].prop_a )[z].prop_b
    //         ^             ^
    //         parentheses that generated the combo chain
    //
    // From: Index( Index( x, Dot(y, prop_a) ), Dot(z, prop_b) )
    //       ^      ^         ^
    //       x      root      tail
    //
    // To:   Index( x, Dot(y, Index(prop_a, Dot(z, prop_b) ) ) )
    //
    // Equivalent to:  x[y].prop_a[z].prop_b

    // Find the end of the root chain.
    let mut tail = root.as_mut();
    let mut tail_options = &mut root_options;

    while !tail_options.intersects(ASTFlags::BREAK) {
        match tail.rhs {
            Expr::Index(ref mut x, ref mut options2, ..) => {
                tail = x.as_mut();
                tail_options = options2;
            }
            #[cfg(not(feature = "no_object"))]
            Expr::Dot(ref mut x, ref mut options2, ..) => {
                tail = x.as_mut();
                tail_options = options2;
            }
            _ => break,
        }
    }

    // Since we attach the outer chain to the root chain, we no longer terminate at the end of the
    // root chain, so remove the ASTFlags::BREAK flag.
    tail_options.remove(ASTFlags::BREAK);

    x.lhs = tail.rhs.take(); // remove tail and insert it into head of outer chain
    tail.rhs = make_sub(x, x_options, x_pos); // attach outer chain to tail
    *expr = make_root(root, root_options, root_pos);
}

impl Engine {
    /// Parse a function call.
    fn parse_fn_call(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
        id: ImmutableString,
        no_args: bool,
        capture_parent_scope: bool,
        namespace: Namespace,
    ) -> ParseResult<Expr> {
        let (token, token_pos) = if no_args {
            &(Token::RightParen, Position::NONE)
        } else {
            input.peek().unwrap()
        };

        let mut _namespace = namespace;
        let mut args = FnArgsVec::new();

        match token {
            // id( <EOF>
            Token::EOF => {
                return Err(PERR::MissingToken(
                    Token::RightParen.into(),
                    format!("to close the arguments list of this function call '{id}'"),
                )
                .into_err(*token_pos))
            }
            // id( <error>
            Token::LexError(err) => return Err(err.clone().into_err(*token_pos)),
            // id()
            Token::RightParen => {
                if !no_args {
                    eat_token(input, &Token::RightParen);
                }

                #[cfg(not(feature = "no_module"))]
                let hash = if _namespace.is_empty() {
                    calc_fn_hash(None, &id, 0)
                } else {
                    let root = _namespace.root();
                    let index = state.find_module(root);
                    let is_global = false;

                    #[cfg(not(feature = "no_function"))]
                    #[cfg(not(feature = "no_module"))]
                    let is_global = is_global || root == crate::engine::KEYWORD_GLOBAL;

                    if settings.has_option(LangOptions::STRICT_VAR)
                        && index.is_none()
                        && !is_global
                        && !state.global_imports.iter().any(|m| m == root)
                        && !self.global_sub_modules.contains_key(root)
                    {
                        return Err(
                            PERR::ModuleUndefined(root.into()).into_err(_namespace.position())
                        );
                    }

                    _namespace.index = index;

                    calc_fn_hash(_namespace.path.iter().map(Ident::as_str), &id, 0)
                };
                #[cfg(feature = "no_module")]
                let hash = calc_fn_hash(None, &id, 0);

                let hashes = if is_valid_function_name(&id) {
                    FnCallHashes::from_hash(hash)
                } else {
                    FnCallHashes::from_native_only(hash)
                };

                args.shrink_to_fit();

                return Ok(FnCallExpr {
                    name: state.get_interned_string(id),
                    capture_parent_scope,
                    op_token: None,
                    namespace: _namespace,
                    hashes,
                    args,
                }
                .into_fn_call_expr(settings.pos));
            }
            // id...
            _ => (),
        }

        let settings = settings.level_up()?;

        loop {
            match input.peek().unwrap() {
                // id(...args, ) - handle trailing comma
                (Token::RightParen, ..) => (),
                _ => args.push(self.parse_expr(input, state, lib, settings)?),
            }

            match input.peek().unwrap() {
                // id(...args)
                (Token::RightParen, ..) => {
                    eat_token(input, &Token::RightParen);

                    #[cfg(not(feature = "no_module"))]
                    let hash = if _namespace.is_empty() {
                        calc_fn_hash(None, &id, args.len())
                    } else {
                        let root = _namespace.root();
                        let index = state.find_module(root);

                        #[cfg(not(feature = "no_function"))]
                        #[cfg(not(feature = "no_module"))]
                        let is_global = root == crate::engine::KEYWORD_GLOBAL;
                        #[cfg(any(feature = "no_function", feature = "no_module"))]
                        let is_global = false;

                        if settings.has_option(LangOptions::STRICT_VAR)
                            && index.is_none()
                            && !is_global
                            && !state.global_imports.iter().any(|m| m == root)
                            && !self.global_sub_modules.contains_key(root)
                        {
                            return Err(
                                PERR::ModuleUndefined(root.into()).into_err(_namespace.position())
                            );
                        }

                        _namespace.index = index;

                        calc_fn_hash(_namespace.path.iter().map(Ident::as_str), &id, args.len())
                    };
                    #[cfg(feature = "no_module")]
                    let hash = calc_fn_hash(None, &id, args.len());

                    let hashes = if is_valid_function_name(&id) {
                        FnCallHashes::from_hash(hash)
                    } else {
                        FnCallHashes::from_native_only(hash)
                    };

                    args.shrink_to_fit();

                    return Ok(FnCallExpr {
                        name: state.get_interned_string(id),
                        capture_parent_scope,
                        op_token: None,
                        namespace: _namespace,
                        hashes,
                        args,
                    }
                    .into_fn_call_expr(settings.pos));
                }
                // id(...args,
                (Token::Comma, ..) => {
                    eat_token(input, &Token::Comma);
                }
                // id(...args <EOF>
                (Token::EOF, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightParen.into(),
                        format!("to close the arguments list of this function call '{id}'"),
                    )
                    .into_err(*pos))
                }
                // id(...args <error>
                (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
                // id(...args ???
                (.., pos) => {
                    return Err(PERR::MissingToken(
                        Token::Comma.into(),
                        format!("to separate the arguments to function call '{id}'"),
                    )
                    .into_err(*pos))
                }
            }
        }
    }

    /// Parse an indexing chain.
    /// Indexing binds to the right, so this call parses all possible levels of indexing following in the input.
    #[cfg(not(feature = "no_index"))]
    fn parse_index_chain(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
        lhs: Expr,
        options: ASTFlags,
        check_types: bool,
    ) -> ParseResult<Expr> {
        fn check_argument_types(lhs: &Expr, idx_expr: &Expr) -> Result<(), ParseError> {
            // Check types of indexing that cannot be overridden
            // - arrays, maps, strings, bit-fields
            match *lhs {
                Expr::Map(..) => match *idx_expr {
                    // lhs[int]
                    Expr::IntegerConstant(..) => Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not a number".into(),
                    )
                    .into_err(idx_expr.start_position())),

                    // lhs[string]
                    Expr::StringConstant(..) | Expr::InterpolatedString(..) => Ok(()),

                    // lhs[float]
                    #[cfg(not(feature = "no_float"))]
                    Expr::FloatConstant(..) => Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not a float".into(),
                    )
                    .into_err(idx_expr.start_position())),
                    // lhs[char]
                    Expr::CharConstant(..) => Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not a character".into(),
                    )
                    .into_err(idx_expr.start_position())),
                    // lhs[()]
                    Expr::Unit(..) => Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not ()".into(),
                    )
                    .into_err(idx_expr.start_position())),
                    // lhs[??? && ???], lhs[??? || ???], lhs[true], lhs[false]
                    Expr::And(..) | Expr::Or(..) | Expr::BoolConstant(..) => {
                        Err(PERR::MalformedIndexExpr(
                            "Object map expects string index, not a boolean".into(),
                        )
                        .into_err(idx_expr.start_position()))
                    }
                    _ => Ok(()),
                },

                Expr::IntegerConstant(..)
                | Expr::Array(..)
                | Expr::StringConstant(..)
                | Expr::InterpolatedString(..) => match *idx_expr {
                    // lhs[int]
                    Expr::IntegerConstant(..) => Ok(()),

                    // lhs[string]
                    Expr::StringConstant(..) | Expr::InterpolatedString(..) => {
                        Err(PERR::MalformedIndexExpr(
                            "Array, string or bit-field expects numeric index, not a string".into(),
                        )
                        .into_err(idx_expr.start_position()))
                    }
                    // lhs[float]
                    #[cfg(not(feature = "no_float"))]
                    Expr::FloatConstant(..) => Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects integer index, not a float".into(),
                    )
                    .into_err(idx_expr.start_position())),
                    // lhs[char]
                    Expr::CharConstant(..) => Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects integer index, not a character".into(),
                    )
                    .into_err(idx_expr.start_position())),
                    // lhs[()]
                    Expr::Unit(..) => Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects integer index, not ()".into(),
                    )
                    .into_err(idx_expr.start_position())),
                    // lhs[??? && ???], lhs[??? || ???], lhs[true], lhs[false]
                    Expr::And(..) | Expr::Or(..) | Expr::BoolConstant(..) => {
                        Err(PERR::MalformedIndexExpr(
                            "Array, string or bit-field expects integer index, not a boolean"
                                .into(),
                        )
                        .into_err(idx_expr.start_position()))
                    }
                    _ => Ok(()),
                },
                _ => Ok(()),
            }
        }

        let idx_expr = self.parse_expr(input, state, lib, settings.level_up()?)?;

        if check_types {
            check_argument_types(&lhs, &idx_expr)?;
        }

        // Check if there is a closing bracket
        match input.peek().unwrap() {
            (Token::RightBracket, ..) => {
                eat_token(input, &Token::RightBracket);

                // Any more indexing following?
                match input.peek().unwrap() {
                    // If another indexing level, right-bind it
                    (Token::LeftBracket | Token::QuestionBracket, ..) => {
                        let (token, pos) = input.next().unwrap();
                        let prev_pos = settings.pos;
                        settings.pos = pos;
                        let settings = settings.level_up()?;
                        // Recursively parse the indexing chain, right-binding each
                        let options = match token {
                            Token::LeftBracket => ASTFlags::empty(),
                            Token::QuestionBracket => ASTFlags::NEGATED,
                            _ => unreachable!("`[` or `?[`"),
                        };
                        let idx_expr = self.parse_index_chain(
                            input, state, lib, settings, idx_expr, options, false,
                        )?;
                        // Indexing binds to right
                        Ok(Expr::Index(
                            BinaryExpr { lhs, rhs: idx_expr }.into(),
                            options,
                            prev_pos,
                        ))
                    }
                    // Otherwise terminate the indexing chain
                    _ => Ok(Expr::Index(
                        BinaryExpr { lhs, rhs: idx_expr }.into(),
                        options | ASTFlags::BREAK,
                        settings.pos,
                    )),
                }
            }
            (Token::LexError(err), pos) => Err(err.clone().into_err(*pos)),
            (.., pos) => Err(PERR::MissingToken(
                Token::RightBracket.into(),
                "for a matching [ in this index expression".into(),
            )
            .into_err(*pos)),
        }
    }

    /// Parse an array literal.
    #[cfg(not(feature = "no_index"))]
    fn parse_array_literal(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
    ) -> ParseResult<Expr> {
        // [ ...
        settings.pos = eat_token(input, &Token::LeftBracket);

        let mut array = ThinVec::new();

        loop {
            const MISSING_RBRACKET: &str = "to end this array literal";

            #[cfg(not(feature = "unchecked"))]
            if self.max_array_size() > 0 && array.len() >= self.max_array_size() {
                return Err(PERR::LiteralTooLarge(
                    "Size of array literal".into(),
                    self.max_array_size(),
                )
                .into_err(input.peek().unwrap().1));
            }

            match input.peek().unwrap() {
                (Token::RightBracket, ..) => {
                    eat_token(input, &Token::RightBracket);
                    break;
                }
                (Token::EOF, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightBracket.into(),
                        MISSING_RBRACKET.into(),
                    )
                    .into_err(*pos))
                }
                _ => array.push(self.parse_expr(input, state, lib, settings.level_up()?)?),
            }

            match input.peek().unwrap() {
                (Token::Comma, ..) => {
                    eat_token(input, &Token::Comma);
                }
                (Token::RightBracket, ..) => (),
                (Token::EOF, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightBracket.into(),
                        MISSING_RBRACKET.into(),
                    )
                    .into_err(*pos))
                }
                (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
                (.., pos) => {
                    return Err(PERR::MissingToken(
                        Token::Comma.into(),
                        "to separate the items of this array literal".into(),
                    )
                    .into_err(*pos))
                }
            };
        }

        array.shrink_to_fit();

        Ok(Expr::Array(array, settings.pos))
    }

    /// Parse a map literal.
    #[cfg(not(feature = "no_object"))]
    fn parse_map_literal(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
    ) -> ParseResult<Expr> {
        // #{ ...
        settings.pos = eat_token(input, &Token::MapStart);

        let mut map = StaticVec::<(Ident, Expr)>::new();
        let mut template = std::collections::BTreeMap::<crate::Identifier, crate::Dynamic>::new();

        loop {
            const MISSING_RBRACE: &str = "to end this object map literal";

            match input.peek().unwrap() {
                (Token::RightBrace, ..) => {
                    eat_token(input, &Token::RightBrace);
                    break;
                }
                (Token::EOF, pos) => {
                    return Err(
                        PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                            .into_err(*pos),
                    )
                }
                _ => (),
            }

            let (name, pos) = match input.next().unwrap() {
                (Token::Identifier(..), pos)
                    if settings.has_flag(ParseSettingFlags::DISALLOW_UNQUOTED_MAP_PROPERTIES) =>
                {
                    return Err(PERR::PropertyExpected.into_err(pos))
                }
                (Token::Identifier(s) | Token::StringConstant(s), pos) => {
                    if map.iter().any(|(p, ..)| p.as_str() == s.as_str()) {
                        return Err(PERR::DuplicatedProperty(s.to_string()).into_err(pos));
                    }
                    (*s, pos)
                }
                (Token::InterpolatedString(..), pos) => {
                    return Err(PERR::PropertyExpected.into_err(pos))
                }
                (Token::Reserved(s), pos) if is_valid_identifier(&s) => {
                    return Err(PERR::Reserved(s.to_string()).into_err(pos));
                }
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (Token::EOF, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightBrace.into(),
                        MISSING_RBRACE.into(),
                    )
                    .into_err(pos));
                }
                (.., pos) if map.is_empty() => {
                    return Err(PERR::MissingToken(
                        Token::RightBrace.into(),
                        MISSING_RBRACE.into(),
                    )
                    .into_err(pos));
                }
                (.., pos) => return Err(PERR::PropertyExpected.into_err(pos)),
            };

            match input.next().unwrap() {
                (Token::Colon, ..) => (),
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (.., pos) => {
                    return Err(PERR::MissingToken(
                        Token::Colon.into(),
                        format!("to follow the property '{name}' in this object map literal"),
                    )
                    .into_err(pos))
                }
            };

            #[cfg(not(feature = "unchecked"))]
            if self.max_map_size() > 0 && map.len() >= self.max_map_size() {
                return Err(PERR::LiteralTooLarge(
                    "Number of properties in object map literal".into(),
                    self.max_map_size(),
                )
                .into_err(input.peek().unwrap().1));
            }

            let expr = self.parse_expr(input, state, lib, settings.level_up()?)?;
            template.insert(name.clone(), crate::Dynamic::UNIT);

            let name = state.get_interned_string(name);
            map.push((Ident { name, pos }, expr));

            match input.peek().unwrap() {
                (Token::Comma, ..) => {
                    eat_token(input, &Token::Comma);
                }
                (Token::RightBrace, ..) => (),
                (Token::Identifier(..), pos) => {
                    return Err(PERR::MissingToken(
                        Token::Comma.into(),
                        "to separate the items of this object map literal".into(),
                    )
                    .into_err(*pos))
                }
                (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
                (.., pos) => {
                    return Err(
                        PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                            .into_err(*pos),
                    )
                }
            }
        }

        map.shrink_to_fit();

        Ok(Expr::Map((map, template).into(), settings.pos))
    }

    /// Parse a switch expression.
    fn parse_switch(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        // switch ...
        let settings = settings.level_up_with_position(eat_token(input, &Token::Switch))?;

        let item = self.parse_expr(input, state, lib, settings)?;

        match input.next().unwrap() {
            (Token::LeftBrace, ..) => (),
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (.., pos) => {
                return Err(PERR::MissingToken(
                    Token::LeftBrace.into(),
                    "to start a switch block".into(),
                )
                .into_err(pos))
            }
        }

        let mut expressions = FnArgsVec::<BinaryExpr>::new();
        let mut cases = StraightHashMap::<CaseBlocksList>::default();
        let mut ranges = StaticVec::<RangeCase>::new();
        let mut def_case = None;
        let mut def_case_pos = Position::NONE;

        loop {
            const MISSING_RBRACE: &str = "to end this switch block";

            let (case_expr_list, condition) = match input.peek().unwrap() {
                (Token::RightBrace, ..) => {
                    eat_token(input, &Token::RightBrace);
                    break;
                }
                (Token::EOF, pos) => {
                    return Err(
                        PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                            .into_err(*pos),
                    )
                }
                (Token::Underscore, pos) if def_case.is_none() => {
                    def_case_pos = *pos;
                    eat_token(input, &Token::Underscore);

                    let (if_clause, if_pos) = match_token(input, &Token::If);

                    if if_clause {
                        return Err(PERR::WrongSwitchCaseCondition.into_err(if_pos));
                    }

                    (
                        StaticVec::new_const(),
                        Expr::BoolConstant(true, Position::NONE),
                    )
                }
                _ if def_case.is_some() => {
                    return Err(PERR::WrongSwitchDefaultCase.into_err(def_case_pos))
                }

                _ => {
                    let mut case_expr_list = StaticVec::new_const();

                    loop {
                        let filter = state.expr_filter;
                        state.expr_filter = |t| t != &Token::Pipe;
                        let expr = self.parse_expr(input, state, lib, settings);
                        state.expr_filter = filter;

                        match expr {
                            Ok(expr) => case_expr_list.push(expr),
                            Err(err) => {
                                return Err(PERR::ExprExpected("literal".into()).into_err(err.1))
                            }
                        }

                        if !match_token(input, &Token::Pipe).0 {
                            break;
                        }
                    }

                    let condition = if match_token(input, &Token::If).0 {
                        ensure_not_statement_expr(input, "a boolean")?;
                        let guard = self
                            .parse_expr(input, state, lib, settings)?
                            .ensure_bool_expr()?;
                        ensure_not_assignment(input)?;
                        guard
                    } else {
                        Expr::BoolConstant(true, Position::NONE)
                    };
                    (case_expr_list, condition)
                }
            };

            match input.next().unwrap() {
                (Token::DoubleArrow, ..) => (),
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (.., pos) => {
                    return Err(PERR::MissingToken(
                        Token::DoubleArrow.into(),
                        "in this switch case".into(),
                    )
                    .into_err(pos))
                }
            };

            let (action_expr, need_comma) =
                if settings.has_flag(ParseSettingFlags::DISALLOW_STATEMENTS_IN_BLOCKS) {
                    (self.parse_expr(input, state, lib, settings)?, true)
                } else {
                    let stmt = self.parse_stmt(input, state, lib, settings)?;
                    let need_comma = !stmt.is_self_terminated();

                    let stmt_block: StmtBlock = stmt.into();
                    (Expr::Stmt(stmt_block.into()), need_comma)
                };

            expressions.push(BinaryExpr {
                lhs: condition,
                rhs: action_expr,
            });

            let index = expressions.len() - 1;

            if case_expr_list.is_empty() {
                def_case = Some(index);
            } else {
                for expr in case_expr_list {
                    let value = expr.get_literal_value().ok_or_else(|| {
                        PERR::ExprExpected("a literal".into()).into_err(expr.start_position())
                    })?;

                    let mut range_value: Option<RangeCase> = None;

                    if let Some(range) = value.read_lock::<ExclusiveRange>() {
                        range_value = Some(range.clone().into());
                    } else if let Some(range) = value.read_lock::<InclusiveRange>() {
                        range_value = Some(range.clone().into());
                    }

                    if let Some(mut r) = range_value {
                        if !r.is_empty() {
                            r.set_index(index);
                            ranges.push(r);
                        }
                    } else if !ranges.is_empty() {
                        // Check for numeric values after ranges
                        let forbidden = match value {
                            Dynamic(Union::Int(..)) => true,
                            #[cfg(not(feature = "no_float"))]
                            Dynamic(Union::Float(..)) => true,
                            #[cfg(feature = "decimal")]
                            Dynamic(Union::Decimal(..)) => true,
                            _ => false,
                        };

                        if forbidden {
                            return Err(
                                PERR::WrongSwitchIntegerCase.into_err(expr.start_position())
                            );
                        }
                    }

                    let hasher = &mut get_hasher();
                    value.hash(hasher);
                    let hash = hasher.finish();

                    cases
                        .entry(hash)
                        .or_insert(CaseBlocksList::new_const())
                        .push(index);
                }
            }

            match input.peek().unwrap() {
                (Token::Comma, ..) => {
                    eat_token(input, &Token::Comma);
                }
                (Token::RightBrace, ..) => (),
                (Token::EOF, pos) => {
                    return Err(
                        PERR::MissingToken(Token::RightParen.into(), MISSING_RBRACE.into())
                            .into_err(*pos),
                    )
                }
                (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
                (.., pos) if need_comma => {
                    return Err(PERR::MissingToken(
                        Token::Comma.into(),
                        "to separate the items in this switch block".into(),
                    )
                    .into_err(*pos))
                }
                _ => (),
            }
        }

        expressions.shrink_to_fit();
        cases.shrink_to_fit();
        ranges.shrink_to_fit();

        let cases = SwitchCasesCollection {
            expressions,
            cases,
            ranges,
            def_case,
        };

        Ok(Stmt::Switch((item, cases).into(), settings.pos))
    }

    /// Parse a primary expression.
    fn parse_primary(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
        options: ChainingFlags,
    ) -> ParseResult<Expr> {
        let (token, token_pos) = input.peek().unwrap();

        settings.pos = *token_pos;

        let root_expr = match token {
            _ if !(state.expr_filter)(token) => {
                return Err(LexError::UnexpectedInput(token.to_string()).into_err(settings.pos))
            }

            Token::EOF => return Err(PERR::UnexpectedEOF.into_err(settings.pos)),

            Token::Unit => {
                input.next();
                Expr::Unit(settings.pos)
            }

            Token::IntegerConstant(..)
            | Token::CharConstant(..)
            | Token::StringConstant(..)
            | Token::True
            | Token::False => match input.next().unwrap().0 {
                Token::IntegerConstant(x) => Expr::IntegerConstant(x, settings.pos),
                Token::CharConstant(c) => Expr::CharConstant(c, settings.pos),
                Token::StringConstant(s) => {
                    Expr::StringConstant(state.get_interned_string(*s), settings.pos)
                }
                Token::True => Expr::BoolConstant(true, settings.pos),
                Token::False => Expr::BoolConstant(false, settings.pos),
                token => unreachable!("token is {:?}", token),
            },
            #[cfg(not(feature = "no_float"))]
            Token::FloatConstant(x) => {
                let x = x.0;
                input.next();
                Expr::FloatConstant(x, settings.pos)
            }
            #[cfg(feature = "decimal")]
            Token::DecimalConstant(x) => {
                let x = x.0;
                input.next();
                Expr::DynamicConstant(Box::new(x.into()), settings.pos)
            }

            // { - block statement as expression
            Token::LeftBrace if settings.has_option(LangOptions::STMT_EXPR) => {
                match self.parse_block(input, state, lib, settings.level_up()?)? {
                    block @ Stmt::Block(..) => Expr::Stmt(Box::new(block.into())),
                    stmt => unreachable!("Stmt::Block expected but gets {:?}", stmt),
                }
            }

            // ( - grouped expression
            Token::LeftParen => {
                settings.pos = eat_token(input, &Token::LeftParen);

                let expr = self.parse_expr(input, state, lib, settings.level_up()?)?;

                match input.next().unwrap() {
                    // ( ... )
                    (Token::RightParen, ..) => expr,
                    // ( <error>
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    // ( ... ???
                    (.., pos) => {
                        return Err(PERR::MissingToken(
                            Token::RightParen.into(),
                            "for a matching ( in this expression".into(),
                        )
                        .into_err(pos))
                    }
                }
            }

            // If statement is allowed to act as expressions
            Token::If if settings.has_option(LangOptions::IF_EXPR) => Expr::Stmt(Box::new(
                self.parse_if(input, state, lib, settings.level_up()?)?
                    .into(),
            )),
            // Loops are allowed to act as expressions
            Token::While | Token::Loop
                if self.allow_looping() && settings.has_option(LangOptions::LOOP_EXPR) =>
            {
                Expr::Stmt(Box::new(
                    self.parse_while_loop(input, state, lib, settings.level_up()?)?
                        .into(),
                ))
            }
            Token::Do if self.allow_looping() && settings.has_option(LangOptions::LOOP_EXPR) => {
                Expr::Stmt(Box::new(
                    self.parse_do(input, state, lib, settings.level_up()?)?
                        .into(),
                ))
            }
            Token::For if self.allow_looping() && settings.has_option(LangOptions::LOOP_EXPR) => {
                Expr::Stmt(Box::new(
                    self.parse_for(input, state, lib, settings.level_up()?)?
                        .into(),
                ))
            }
            // Switch statement is allowed to act as expressions
            Token::Switch if settings.has_option(LangOptions::SWITCH_EXPR) => Expr::Stmt(Box::new(
                self.parse_switch(input, state, lib, settings.level_up()?)?
                    .into(),
            )),

            // | ...
            #[cfg(not(feature = "no_function"))]
            #[cfg(not(feature = "unchecked"))]
            Token::Pipe | Token::Or
                if settings.has_option(LangOptions::ANON_FN)
                    && lib.len() >= self.max_functions() =>
            {
                return Err(PERR::TooManyFunctions.into_err(settings.pos));
            }
            #[cfg(not(feature = "no_function"))]
            Token::Pipe | Token::Or if settings.has_option(LangOptions::ANON_FN) => {
                // Build new parse state
                let new_interner = &mut StringsInterner::new();
                let new_state = &mut ParseState::new(
                    state.external_constants,
                    new_interner,
                    state.tokenizer_control.clone(),
                );

                // We move the strings interner to the new parse state object by swapping it...
                std::mem::swap(state.interned_strings, new_state.interned_strings);

                #[cfg(not(feature = "no_module"))]
                {
                    // Do not allow storing an index to a globally-imported module
                    // just in case the function is separated from this `AST`.
                    //
                    // Keep them in `global_imports` instead so that strict variables
                    // mode will not complain.
                    new_state.global_imports.clone_from(&state.global_imports);
                    new_state.global_imports.extend(state.imports.clone());
                }

                // Brand new options
                #[cfg(not(feature = "no_closure"))]
                let options = self.options & !LangOptions::STRICT_VAR; // a capturing closure can access variables not defined locally, so turn off Strict Variables mode
                #[cfg(feature = "no_closure")]
                let options = self.options | (settings.options & LangOptions::STRICT_VAR);

                // Brand new flags, turn on function scope and closure scope
                let flags = ParseSettingFlags::FN_SCOPE
                    | ParseSettingFlags::CLOSURE_SCOPE
                    | (settings.flags
                        & (ParseSettingFlags::DISALLOW_UNQUOTED_MAP_PROPERTIES
                            | ParseSettingFlags::DISALLOW_STATEMENTS_IN_BLOCKS));

                let new_settings = ParseSettings {
                    flags,
                    options,
                    ..settings
                };

                let result =
                    self.parse_anon_fn(input, new_state, lib, new_settings.level_up()?, state);

                // Restore the strings interner by swapping it back
                std::mem::swap(state.interned_strings, new_state.interned_strings);

                let (expr, fn_def) = result?;

                #[cfg(not(feature = "no_closure"))]
                for Ident { name, pos } in &new_state.external_vars {
                    let (index, is_func) = state.access_var(name, lib, *pos);

                    if !is_func
                        && index.is_none()
                        && !settings.has_flag(ParseSettingFlags::CLOSURE_SCOPE)
                        && settings.has_option(LangOptions::STRICT_VAR)
                        && !state
                            .external_constants
                            .map_or(false, |scope| scope.contains(name))
                    {
                        // If the parent scope is not inside another capturing closure
                        // then we can conclude that the captured variable doesn't exist.
                        // Under Strict Variables mode, this is not allowed.
                        return Err(PERR::VariableUndefined(name.to_string()).into_err(*pos));
                    }
                }

                let hash_script = calc_fn_hash(None, &fn_def.name, fn_def.params.len());
                lib.insert(hash_script, fn_def);

                expr
            }

            // Interpolated string
            Token::InterpolatedString(..) => {
                let mut segments = ThinVec::new();
                let settings = settings.level_up()?;

                match input.next().unwrap() {
                    (Token::InterpolatedString(s), ..) if s.is_empty() => (),
                    (Token::InterpolatedString(s), pos) => {
                        segments.push(Expr::StringConstant(state.get_interned_string(*s), pos))
                    }
                    token => {
                        unreachable!("Token::InterpolatedString expected but gets {:?}", token)
                    }
                }

                loop {
                    let expr = match self.parse_block(input, state, lib, settings)? {
                        block @ Stmt::Block(..) => Expr::Stmt(Box::new(block.into())),
                        stmt => unreachable!("Stmt::Block expected but gets {:?}", stmt),
                    };
                    match expr {
                        Expr::StringConstant(s, ..) if s.is_empty() => (),
                        _ => segments.push(expr),
                    }

                    // Make sure to parse the following as text
                    state.tokenizer_control.borrow_mut().is_within_text = true;

                    match input.next().unwrap() {
                        (Token::StringConstant(s), pos) => {
                            if !s.is_empty() {
                                segments
                                    .push(Expr::StringConstant(state.get_interned_string(*s), pos));
                            }
                            // End the interpolated string if it is terminated by a back-tick.
                            break;
                        }
                        (Token::InterpolatedString(s), pos) => {
                            if !s.is_empty() {
                                segments
                                    .push(Expr::StringConstant(state.get_interned_string(*s), pos));
                            }
                        }
                        (Token::LexError(err), pos) => match *err {
                            LexError::UnterminatedString | LexError::StringTooLong(_) => {
                                return Err(err.into_err(pos))
                            }
                            _ => unreachable!("improper lex error: {:?}", err),
                        },
                        (token, ..) => unreachable!(
                            "string within an interpolated string literal expected but gets {:?}",
                            token
                        ),
                    }
                }

                if segments.is_empty() {
                    Expr::StringConstant(state.get_interned_string(""), settings.pos)
                } else {
                    segments.shrink_to_fit();
                    Expr::InterpolatedString(segments, settings.pos)
                }
            }

            // Array literal
            #[cfg(not(feature = "no_index"))]
            Token::LeftBracket => {
                self.parse_array_literal(input, state, lib, settings.level_up()?)?
            }

            // Map literal
            #[cfg(not(feature = "no_object"))]
            Token::MapStart => self.parse_map_literal(input, state, lib, settings.level_up()?)?,

            // Custom syntax.
            #[cfg(not(feature = "no_custom_syntax"))]
            Token::Custom(key) | Token::Reserved(key) | Token::Identifier(key)
                if self.custom_syntax.contains_key(&**key) =>
            {
                let (key, syntax) = self.custom_syntax.get_key_value(&**key).unwrap();
                let (.., pos) = input.next().unwrap();
                let settings = settings.level_up_with_position(pos)?;
                self.parse_custom_syntax(input, state, lib, settings, key, syntax)?
            }

            // Identifier
            Token::Identifier(..) => {
                let ns = Namespace::NONE;

                let s = match input.next().unwrap() {
                    (Token::Identifier(s), ..) => s,
                    token => unreachable!("Token::Identifier expected but gets {:?}", token),
                };

                match input.peek().unwrap() {
                    // Function call
                    (Token::LeftParen | Token::Bang | Token::Unit, _) => {
                        // Once the identifier consumed we must enable next variables capturing
                        state.allow_capture = true;

                        Expr::Variable(
                            (None, ns, 0, state.get_interned_string(*s)).into(),
                            None,
                            settings.pos,
                        )
                    }
                    // Namespace qualification
                    #[cfg(not(feature = "no_module"))]
                    (token @ Token::DoubleColon, pos) => {
                        if options.intersects(ChainingFlags::DISALLOW_NAMESPACES) {
                            return Err(LexError::ImproperSymbol(
                                token.literal_syntax().into(),
                                String::new(),
                            )
                            .into_err(*pos));
                        }

                        // Once the identifier consumed we must enable next variables capturing
                        state.allow_capture = true;

                        let name = state.get_interned_string(*s);
                        Expr::Variable((None, ns, 0, name).into(), None, settings.pos)
                    }
                    // Normal variable access
                    _ => {
                        let (index, is_func) = state.access_var(&s, lib, settings.pos);

                        if !options.intersects(ChainingFlags::PROPERTY)
                            && !is_func
                            && index.is_none()
                            && settings.has_option(LangOptions::STRICT_VAR)
                            && !state
                                .external_constants
                                .map_or(false, |scope| scope.contains(&s))
                        {
                            return Err(
                                PERR::VariableUndefined(s.to_string()).into_err(settings.pos)
                            );
                        }

                        let short_index = index
                            .and_then(|x| u8::try_from(x.get()).ok())
                            .and_then(NonZeroU8::new);
                        let name = state.get_interned_string(*s);
                        Expr::Variable((index, ns, 0, name).into(), short_index, settings.pos)
                    }
                }
            }

            // Reserved keyword or symbol
            Token::Reserved(..) => {
                let ns = Namespace::NONE;

                let s = match input.next().unwrap() {
                    (Token::Reserved(s), ..) => s,
                    token => unreachable!("Token::Reserved expected but gets {:?}", token),
                };

                match input.peek().unwrap().0 {
                    // Function call is allowed to have reserved keyword
                    Token::LeftParen | Token::Bang | Token::Unit
                        if is_reserved_keyword_or_symbol(&s).1 =>
                    {
                        Expr::Variable(
                            (None, ns, 0, state.get_interned_string(*s)).into(),
                            None,
                            settings.pos,
                        )
                    }
                    // Access to `this` as a variable
                    #[cfg(not(feature = "no_function"))]
                    _ if *s == crate::engine::KEYWORD_THIS => {
                        // OK within a function scope
                        if settings.has_flag(ParseSettingFlags::FN_SCOPE) {
                            Expr::ThisPtr(settings.pos)
                        } else {
                            // Cannot access to `this` as a variable not in a function scope
                            let msg = format!("'{s}' can only be used in functions");
                            return Err(
                                LexError::ImproperSymbol(s.to_string(), msg).into_err(settings.pos)
                            );
                        }
                    }
                    _ => return Err(PERR::Reserved(s.to_string()).into_err(settings.pos)),
                }
            }

            Token::LexError(..) => match input.next().unwrap() {
                (Token::LexError(err), ..) => return Err(err.into_err(settings.pos)),
                token => unreachable!("Token::LexError expected but gets {:?}", token),
            },

            _ => return Err(LexError::UnexpectedInput(token.to_string()).into_err(settings.pos)),
        };

        if !(state.expr_filter)(&input.peek().unwrap().0) {
            return Ok(root_expr);
        }

        self.parse_postfix(
            input,
            state,
            lib,
            settings,
            root_expr,
            ChainingFlags::empty(),
        )
    }

    /// Tail processing of all possible postfix operators of a primary expression.
    fn parse_postfix(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
        mut lhs: Expr,
        _options: ChainingFlags,
    ) -> ParseResult<Expr> {
        // Break just in case `lhs` is `Expr::Dot` or `Expr::Index`
        let mut _parent_options = ASTFlags::BREAK;

        // Tail processing all possible postfix operators
        loop {
            let (tail_token, ..) = input.peek().unwrap();

            if !lhs.is_valid_postfix(tail_token) {
                break;
            }

            let (tail_token, tail_pos) = input.next().unwrap();
            settings.pos = tail_pos;

            lhs = match (lhs, tail_token) {
                // Qualified function call with !
                #[cfg(not(feature = "no_module"))]
                (Expr::Variable(x, ..), Token::Bang) if !x.1.is_empty() => {
                    return match input.peek().unwrap() {
                        (Token::LeftParen | Token::Unit, ..) => {
                            Err(LexError::UnexpectedInput(Token::Bang.into()).into_err(tail_pos))
                        }
                        _ => Err(LexError::ImproperSymbol(
                            "!".into(),
                            "'!' cannot be used to call module functions".into(),
                        )
                        .into_err(tail_pos)),
                    };
                }
                // Function call with !
                (Expr::Variable(x, .., pos), Token::Bang) => {
                    match input.peek().unwrap() {
                        (Token::LeftParen | Token::Unit, ..) => (),
                        (_, pos) => {
                            return Err(PERR::MissingToken(
                                Token::LeftParen.into(),
                                "to start arguments list of function call".into(),
                            )
                            .into_err(*pos))
                        }
                    }

                    let no_args = input.next().unwrap().0 == Token::Unit;

                    let (.., ns, _, name) = *x;
                    settings.pos = pos;

                    self.parse_fn_call(input, state, lib, settings, name, no_args, true, ns)?
                }
                // Function call
                (Expr::Variable(x, .., pos), t @ (Token::LeftParen | Token::Unit)) => {
                    let (.., ns, _, name) = *x;
                    let no_args = t == Token::Unit;
                    settings.pos = pos;

                    self.parse_fn_call(input, state, lib, settings, name, no_args, false, ns)?
                }
                // Disallowed module separator
                #[cfg(not(feature = "no_module"))]
                (_, token @ Token::DoubleColon)
                    if _options.intersects(ChainingFlags::DISALLOW_NAMESPACES) =>
                {
                    return Err(LexError::ImproperSymbol(
                        token.literal_syntax().into(),
                        String::new(),
                    )
                    .into_err(tail_pos))
                }
                // module access
                #[cfg(not(feature = "no_module"))]
                (Expr::Variable(x, .., pos), Token::DoubleColon) => {
                    let (id2, pos2) = parse_var_name(input)?;
                    let (.., mut namespace, _, name) = *x;
                    let var_name_def = Ident { name, pos };

                    namespace.path.push(var_name_def);

                    let var_name = state.get_interned_string(id2);

                    Expr::Variable((None, namespace, 0, var_name).into(), None, pos2)
                }
                // Indexing
                #[cfg(not(feature = "no_index"))]
                (expr, token @ (Token::LeftBracket | Token::QuestionBracket)) => {
                    let opt = match token {
                        Token::LeftBracket => ASTFlags::empty(),
                        Token::QuestionBracket => ASTFlags::NEGATED,
                        _ => unreachable!("`[` or `?[`"),
                    };
                    let settings = settings.level_up()?;
                    self.parse_index_chain(input, state, lib, settings, expr, opt, true)?
                }
                // Property access
                #[cfg(not(feature = "no_object"))]
                (expr, op @ (Token::Period | Token::Elvis)) => {
                    // Expression after dot must start with an identifier
                    match input.peek().unwrap() {
                        (Token::Identifier(..), ..) => {
                            // Prevents capturing of the object properties as vars: xxx.<var>
                            state.allow_capture = false;
                        }
                        (Token::Reserved(s), ..) if is_reserved_keyword_or_symbol(s).2 => (),
                        (Token::Reserved(s), pos) => {
                            return Err(PERR::Reserved(s.to_string()).into_err(*pos))
                        }
                        (.., pos) => return Err(PERR::PropertyExpected.into_err(*pos)),
                    }

                    let op_flags = match op {
                        Token::Period => ASTFlags::empty(),
                        Token::Elvis => ASTFlags::NEGATED,
                        _ => unreachable!("`.` or `?.`"),
                    };
                    let options = ChainingFlags::PROPERTY | ChainingFlags::DISALLOW_NAMESPACES;
                    let rhs =
                        self.parse_primary(input, state, lib, settings.level_up()?, options)?;

                    Self::make_dot_expr(state, expr, rhs, _parent_options, op_flags, tail_pos)?
                }
                // Unknown postfix operator
                (expr, token) => {
                    unreachable!("unknown postfix operator '{}' for {:?}", token, expr)
                }
            };

            // The chain is now extended
            _parent_options = ASTFlags::empty();
        }

        // Optimize chain where the root expression is another chain
        #[cfg(any(not(feature = "no_index"), not(feature = "no_object")))]
        if matches!(lhs, Expr::Index(ref x, ..) | Expr::Dot(ref x, ..) if matches!(x.lhs, Expr::Index(..) | Expr::Dot(..)))
        {
            optimize_combo_chain(&mut lhs);
        }

        // Cache the hash key for namespace-qualified variables
        #[cfg(not(feature = "no_module"))]
        let namespaced_variable = match lhs {
            Expr::Variable(ref mut x, ..) if !x.1.is_empty() => Some(&mut **x),
            Expr::Index(ref mut x, ..) | Expr::Dot(ref mut x, ..) => match x.lhs {
                Expr::Variable(ref mut x, ..) if !x.1.is_empty() => Some(&mut **x),
                _ => None,
            },
            _ => None,
        };

        #[cfg(not(feature = "no_module"))]
        if let Some((.., namespace, hash, name)) = namespaced_variable {
            if !namespace.is_empty() {
                *hash = crate::calc_var_hash(namespace.path.iter().map(Ident::as_str), name);

                #[cfg(not(feature = "no_module"))]
                {
                    let root = namespace.root();
                    let index = state.find_module(root);
                    let is_global = false;

                    #[cfg(not(feature = "no_function"))]
                    #[cfg(not(feature = "no_module"))]
                    let is_global = is_global || root == crate::engine::KEYWORD_GLOBAL;

                    if settings.has_option(LangOptions::STRICT_VAR)
                        && index.is_none()
                        && !is_global
                        && !state.global_imports.iter().any(|m| m == root)
                        && !self.global_sub_modules.contains_key(root)
                    {
                        return Err(
                            PERR::ModuleUndefined(root.into()).into_err(namespace.position())
                        );
                    }

                    namespace.index = index;
                }
            }
        }

        // Make sure identifiers are valid
        Ok(lhs)
    }

    /// Parse a potential unary operator.
    fn parse_unary(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
    ) -> ParseResult<Expr> {
        let (token, token_pos) = input.peek().unwrap();

        if !(state.expr_filter)(token) {
            return Err(LexError::UnexpectedInput(token.to_string()).into_err(*token_pos));
        }

        settings.pos = *token_pos;

        match token {
            // -expr
            Token::Minus | Token::UnaryMinus => {
                let token = token.clone();
                let pos = eat_token(input, &token);

                match self.parse_unary(input, state, lib, settings.level_up()?)? {
                    // Negative integer
                    Expr::IntegerConstant(num, ..) => num
                        .checked_neg()
                        .map(|i| Expr::IntegerConstant(i, pos))
                        .or_else(|| {
                            #[cfg(not(feature = "no_float"))]
                            return Some(Expr::FloatConstant((-(num as crate::FLOAT)).into(), pos));
                            #[cfg(feature = "no_float")]
                            return None;
                        })
                        .ok_or_else(|| LexError::MalformedNumber(format!("-{num}")).into_err(pos)),

                    // Negative float
                    #[cfg(not(feature = "no_float"))]
                    Expr::FloatConstant(x, ..) => Ok(Expr::FloatConstant((-(*x)).into(), pos)),

                    // Call negative function
                    expr => Ok(FnCallExpr {
                        namespace: Namespace::NONE,
                        name: state.get_interned_string("-"),
                        hashes: FnCallHashes::from_native_only(calc_fn_hash(None, "-", 1)),
                        args: IntoIterator::into_iter([expr]).collect(),
                        op_token: Some(token),
                        capture_parent_scope: false,
                    }
                    .into_fn_call_expr(pos)),
                }
            }
            // +expr
            Token::Plus | Token::UnaryPlus => {
                let token = token.clone();
                let pos = eat_token(input, &token);

                match self.parse_unary(input, state, lib, settings.level_up()?)? {
                    expr @ Expr::IntegerConstant(..) => Ok(expr),
                    #[cfg(not(feature = "no_float"))]
                    expr @ Expr::FloatConstant(..) => Ok(expr),

                    // Call plus function
                    expr => Ok(FnCallExpr {
                        namespace: Namespace::NONE,
                        name: state.get_interned_string("+"),
                        hashes: FnCallHashes::from_native_only(calc_fn_hash(None, "+", 1)),
                        args: IntoIterator::into_iter([expr]).collect(),
                        op_token: Some(token),
                        capture_parent_scope: false,
                    }
                    .into_fn_call_expr(pos)),
                }
            }
            // !expr
            Token::Bang => {
                let token = token.clone();
                let pos = eat_token(input, &Token::Bang);

                Ok(FnCallExpr {
                    namespace: Namespace::NONE,
                    name: state.get_interned_string("!"),
                    hashes: FnCallHashes::from_native_only(calc_fn_hash(None, "!", 1)),
                    args: {
                        let expr = self.parse_unary(input, state, lib, settings.level_up()?)?;
                        IntoIterator::into_iter([expr]).collect()
                    },
                    op_token: Some(token),
                    capture_parent_scope: false,
                }
                .into_fn_call_expr(pos))
            }
            // <EOF>
            Token::EOF => Err(PERR::UnexpectedEOF.into_err(settings.pos)),
            // All other tokens
            _ => self.parse_primary(input, state, lib, settings, ChainingFlags::empty()),
        }
    }

    /// Make an assignment statement.
    fn make_assignment_stmt(
        op: Option<Token>,
        state: &mut ParseState,
        lhs: Expr,
        rhs: Expr,
        op_pos: Position,
    ) -> ParseResult<Stmt> {
        #[must_use]
        fn check_lvalue(expr: &Expr, parent_is_dot: bool) -> Option<Position> {
            match expr {
                Expr::Index(x, options, ..) | Expr::Dot(x, options, ..) if parent_is_dot => {
                    match x.lhs {
                        Expr::Property(..) if !options.intersects(ASTFlags::BREAK) => {
                            check_lvalue(&x.rhs, matches!(expr, Expr::Dot(..)))
                        }
                        Expr::Property(..) => None,
                        // Anything other than a property after dotting (e.g. a method call) is not an l-value
                        ref e => Some(e.position()),
                    }
                }
                Expr::Index(x, options, ..) | Expr::Dot(x, options, ..) => match x.lhs {
                    Expr::Property(..) => unreachable!("unexpected Expr::Property in indexing"),
                    _ if !options.intersects(ASTFlags::BREAK) => {
                        check_lvalue(&x.rhs, matches!(expr, Expr::Dot(..)))
                    }
                    _ => None,
                },
                Expr::Property(..) if parent_is_dot => None,
                Expr::Property(..) => unreachable!("unexpected Expr::Property in indexing"),
                e if parent_is_dot => Some(e.position()),
                _ => None,
            }
        }

        let op_info = op.map_or_else(
            || OpAssignment::new_assignment(op_pos),
            |op| OpAssignment::new_op_assignment_from_token(op, op_pos),
        );

        match lhs {
            // this = rhs
            Expr::ThisPtr(_) => Ok(Stmt::Assignment((op_info, BinaryExpr { lhs, rhs }).into())),
            // var (non-indexed) = rhs
            Expr::Variable(ref x, None, _) if x.0.is_none() => {
                Ok(Stmt::Assignment((op_info, BinaryExpr { lhs, rhs }).into()))
            }
            // var (indexed) = rhs
            Expr::Variable(ref x, i, var_pos) => {
                let (index, .., name) = &**x;
                let index = i.map_or_else(
                    || index.expect("long or short index must be `Some`").get(),
                    |n| n.get() as usize,
                );

                match state
                    .stack
                    .get_mut_by_index(state.stack.len() - index)
                    .access_mode()
                {
                    AccessMode::ReadWrite => {
                        Ok(Stmt::Assignment((op_info, BinaryExpr { lhs, rhs }).into()))
                    }
                    // Constant values cannot be assigned to
                    AccessMode::ReadOnly => {
                        Err(PERR::AssignmentToConstant(name.to_string()).into_err(var_pos))
                    }
                }
            }
            // xxx[???]... = rhs, xxx.prop... = rhs
            Expr::Index(ref x, options, ..) | Expr::Dot(ref x, options, ..) => {
                let valid_lvalue = if options.intersects(ASTFlags::BREAK) {
                    None
                } else {
                    check_lvalue(&x.rhs, matches!(lhs, Expr::Dot(..)))
                };

                if let Some(err_pos) = valid_lvalue {
                    Err(PERR::AssignmentToInvalidLHS(String::new()).into_err(err_pos))
                } else {
                    match x.lhs {
                        // var[???] = rhs, this[???] = rhs, var.??? = rhs, this.??? = rhs
                        Expr::Variable(..) | Expr::ThisPtr(..) => {
                            Ok(Stmt::Assignment((op_info, BinaryExpr { lhs, rhs }).into()))
                        }
                        // expr[???] = rhs, expr.??? = rhs
                        ref expr => {
                            Err(PERR::AssignmentToInvalidLHS(String::new())
                                .into_err(expr.position()))
                        }
                    }
                }
            }
            // const_expr = rhs
            ref expr if expr.is_constant() => {
                Err(PERR::AssignmentToConstant(String::new()).into_err(lhs.start_position()))
            }
            // ??? && ??? = rhs, ??? || ??? = rhs, xxx ?? xxx = rhs
            Expr::And(..) | Expr::Or(..) | Expr::Coalesce(..) if !op_info.is_op_assignment() => {
                Err(LexError::ImproperSymbol(
                    Token::Equals.literal_syntax().into(),
                    "Possibly a typo of '=='?".into(),
                )
                .into_err(op_pos))
            }
            // expr = rhs
            _ => Err(PERR::AssignmentToInvalidLHS(String::new()).into_err(lhs.position())),
        }
    }

    /// Make a dot expression.
    #[cfg(not(feature = "no_object"))]
    fn make_dot_expr(
        state: &mut ParseState,
        lhs: Expr,
        rhs: Expr,
        parent_options: ASTFlags,
        op_flags: ASTFlags,
        op_pos: Position,
    ) -> ParseResult<Expr> {
        match (lhs, rhs) {
            // lhs[...][...].rhs
            (Expr::Index(mut x, options, pos), rhs)
                if !parent_options.intersects(ASTFlags::BREAK) =>
            {
                let options = options | parent_options;
                x.rhs = Self::make_dot_expr(state, x.rhs, rhs, options, op_flags, op_pos)?;
                Ok(Expr::Index(x, ASTFlags::empty(), pos))
            }
            // lhs.module::id - syntax error
            #[cfg(not(feature = "no_module"))]
            (.., Expr::Variable(x, ..)) if !x.1.is_empty() => unreachable!("lhs.ns::id"),
            // lhs.id
            (lhs, var_expr @ Expr::Variable(..)) => {
                let rhs = var_expr.into_property(state);
                Ok(Expr::Dot(BinaryExpr { lhs, rhs }.into(), op_flags, op_pos))
            }
            // lhs.prop
            (lhs, prop @ Expr::Property(..)) => Ok(Expr::Dot(
                BinaryExpr { lhs, rhs: prop }.into(),
                op_flags,
                op_pos,
            )),
            // lhs.nnn::func(...) - syntax error
            #[cfg(not(feature = "no_module"))]
            (.., Expr::FnCall(f, ..)) if f.is_qualified() => unreachable!("lhs.ns::func()"),
            // lhs.Fn() or lhs.eval()
            (.., Expr::FnCall(f, func_pos))
                if f.args.is_empty()
                    && matches!(
                        &*f.name,
                        crate::engine::KEYWORD_FN_PTR | crate::engine::KEYWORD_EVAL
                    ) =>
            {
                let err_msg = format!(
                    "'{}' should not be called in method style. Try {}(...);",
                    f.name, f.name
                );
                Err(LexError::ImproperSymbol(f.name.to_string(), err_msg).into_err(func_pos))
            }
            // lhs.func!(...)
            (.., Expr::FnCall(f, func_pos)) if f.capture_parent_scope => {
                Err(PERR::MalformedCapture(
                    "method-call style does not support running within the caller's scope".into(),
                )
                .into_err(func_pos))
            }
            // lhs.func(...)
            (lhs, Expr::FnCall(mut f, func_pos)) => {
                // Recalculate hash
                let args_len = f.args.len() + 1;
                f.hashes = if is_valid_function_name(&f.name) {
                    #[cfg(not(feature = "no_function"))]
                    {
                        FnCallHashes::from_script_and_native(
                            calc_fn_hash(None, &f.name, args_len - 1),
                            calc_fn_hash(None, &f.name, args_len),
                        )
                    }
                    #[cfg(feature = "no_function")]
                    {
                        FnCallHashes::from_native_only(calc_fn_hash(None, &f.name, args_len))
                    }
                } else {
                    FnCallHashes::from_native_only(calc_fn_hash(None, &f.name, args_len))
                };

                let rhs = Expr::MethodCall(f, func_pos);
                Ok(Expr::Dot(BinaryExpr { lhs, rhs }.into(), op_flags, op_pos))
            }
            // lhs.dot_lhs.dot_rhs or lhs.dot_lhs[idx_rhs]
            (lhs, rhs @ (Expr::Dot(..) | Expr::Index(..))) => {
                let (x, options, pos, is_dot) = match rhs {
                    Expr::Dot(x, options, pos) => (x, options, pos, true),
                    Expr::Index(x, options, pos) => (x, options, pos, false),
                    expr => unreachable!("Expr::Dot or Expr::Index expected but gets {:?}", expr),
                };

                match x.lhs {
                    // lhs.module::id.dot_rhs or lhs.module::id[idx_rhs] - syntax error
                    #[cfg(not(feature = "no_module"))]
                    Expr::Variable(x, ..) if !x.1.is_empty() => unreachable!("lhs.ns::id..."),
                    // lhs.module::func().dot_rhs or lhs.module::func()[idx_rhs] - syntax error
                    #[cfg(not(feature = "no_module"))]
                    Expr::FnCall(f, ..) if f.is_qualified() => {
                        unreachable!("lhs.ns::func()...")
                    }
                    // lhs.id.dot_rhs or lhs.id[idx_rhs]
                    Expr::Variable(..) | Expr::Property(..) => {
                        let new_binary = BinaryExpr {
                            lhs: x.lhs.into_property(state),
                            rhs: x.rhs,
                        }
                        .into();

                        let rhs = if is_dot {
                            Expr::Dot(new_binary, options, pos)
                        } else {
                            Expr::Index(new_binary, options, pos)
                        };
                        Ok(Expr::Dot(BinaryExpr { lhs, rhs }.into(), op_flags, op_pos))
                    }
                    // lhs.func().dot_rhs or lhs.func()[idx_rhs]
                    Expr::FnCall(mut f, func_pos) => {
                        // Recalculate hash
                        let args_len = f.args.len() + 1;
                        f.hashes = if is_valid_function_name(&f.name) {
                            #[cfg(not(feature = "no_function"))]
                            {
                                FnCallHashes::from_script_and_native(
                                    calc_fn_hash(None, &f.name, args_len - 1),
                                    calc_fn_hash(None, &f.name, args_len),
                                )
                            }
                            #[cfg(feature = "no_function")]
                            {
                                FnCallHashes::from_native_only(calc_fn_hash(
                                    None, &f.name, args_len,
                                ))
                            }
                        } else {
                            FnCallHashes::from_native_only(calc_fn_hash(None, &f.name, args_len))
                        };

                        let new_lhs = BinaryExpr {
                            lhs: Expr::MethodCall(f, func_pos),
                            rhs: x.rhs,
                        }
                        .into();

                        let rhs = if is_dot {
                            Expr::Dot(new_lhs, options, pos)
                        } else {
                            Expr::Index(new_lhs, options, pos)
                        };
                        Ok(Expr::Dot(BinaryExpr { lhs, rhs }.into(), op_flags, op_pos))
                    }
                    expr => unreachable!("invalid dot expression: {:?}", expr),
                }
            }
            // lhs.rhs
            (.., rhs) => Err(PERR::PropertyExpected.into_err(rhs.start_position())),
        }
    }

    /// Parse a binary expression (if any).
    fn parse_binary_op(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
        parent_precedence: Option<Precedence>,
        lhs: Expr,
    ) -> ParseResult<Expr> {
        settings.pos = lhs.position();

        let mut root = lhs;

        loop {
            let (current_op, current_pos) = input.peek().unwrap();

            if !(state.expr_filter)(current_op) {
                return Ok(root);
            }

            let precedence = match current_op {
                #[cfg(not(feature = "no_custom_syntax"))]
                Token::Custom(c) => self
                    .custom_keywords
                    .get(&**c)
                    .copied()
                    .ok_or_else(|| PERR::Reserved(c.to_string()).into_err(*current_pos))?,
                Token::Reserved(c) if !is_valid_identifier(c) => {
                    return Err(PERR::UnknownOperator(c.to_string()).into_err(*current_pos))
                }
                _ => current_op.precedence(),
            };
            let bind_right = current_op.is_bind_right();

            // Bind left to the parent lhs expression if precedence is higher
            // If same precedence, then check if the operator binds right
            if precedence < parent_precedence || (precedence == parent_precedence && !bind_right) {
                return Ok(root);
            }

            let (op_token, pos) = input.next().unwrap();

            let rhs = self.parse_unary(input, state, lib, settings)?;

            let (next_op, next_pos) = input.peek().unwrap();
            let next_precedence = match next_op {
                #[cfg(not(feature = "no_custom_syntax"))]
                Token::Custom(c) => self
                    .custom_keywords
                    .get(&**c)
                    .copied()
                    .ok_or_else(|| PERR::Reserved(c.to_string()).into_err(*next_pos))?,
                Token::Reserved(c) if !is_valid_identifier(c) => {
                    return Err(PERR::UnknownOperator(c.to_string()).into_err(*next_pos))
                }
                _ => next_op.precedence(),
            };

            // Bind to right if the next operator has higher precedence
            // If same precedence, then check if the operator binds right
            let rhs =
                if (precedence == next_precedence && bind_right) || precedence < next_precedence {
                    self.parse_binary_op(input, state, lib, settings.level_up()?, precedence, rhs)?
                } else {
                    // Otherwise bind to left (even if next operator has the same precedence)
                    rhs
                };

            settings = settings.level_up()?;
            settings.pos = pos;

            let op = op_token.to_string();
            let hash = calc_fn_hash(None, &op, 2);
            let native_only = !is_valid_function_name(&op);

            let mut op_base = FnCallExpr {
                namespace: Namespace::NONE,
                name: state.get_interned_string(&op),
                hashes: FnCallHashes::from_native_only(hash),
                args: IntoIterator::into_iter([root, rhs]).collect(),
                op_token: native_only.then(|| op_token.clone()),
                capture_parent_scope: false,
            };

            root = match op_token {
                // '!=' defaults to true when passed invalid operands
                Token::NotEqualsTo => op_base.into_fn_call_expr(pos),

                // Comparison operators default to false when passed invalid operands
                Token::EqualsTo
                | Token::LessThan
                | Token::LessThanEqualsTo
                | Token::GreaterThan
                | Token::GreaterThanEqualsTo => {
                    let pos = op_base.args[0].start_position();
                    op_base.into_fn_call_expr(pos)
                }

                Token::Or => {
                    let rhs = op_base.args[1].take().ensure_bool_expr()?;
                    let lhs = op_base.args[0].take().ensure_bool_expr()?;
                    Expr::Or(BinaryExpr { lhs, rhs }.into(), pos)
                }
                Token::And => {
                    let rhs = op_base.args[1].take().ensure_bool_expr()?;
                    let lhs = op_base.args[0].take().ensure_bool_expr()?;
                    Expr::And(BinaryExpr { lhs, rhs }.into(), pos)
                }
                Token::DoubleQuestion => {
                    let rhs = op_base.args[1].take();
                    let lhs = op_base.args[0].take();
                    Expr::Coalesce(BinaryExpr { lhs, rhs }.into(), pos)
                }
                Token::In | Token::NotIn => {
                    // Swap the arguments
                    let (lhs, rhs) = op_base.args.split_first_mut().unwrap();
                    let pos = lhs.start_position();
                    std::mem::swap(lhs, &mut rhs[0]);

                    // Convert into a call to `contains`
                    op_base.hashes = FnCallHashes::from_hash(calc_fn_hash(None, OP_CONTAINS, 2));
                    op_base.name = state.get_interned_string(OP_CONTAINS);
                    let fn_call = op_base.into_fn_call_expr(pos);

                    if op_token == Token::In {
                        fn_call
                    } else {
                        // Put a `!` call in front
                        let not_base = FnCallExpr {
                            namespace: Namespace::NONE,
                            name: state.get_interned_string(OP_NOT),
                            hashes: FnCallHashes::from_native_only(calc_fn_hash(None, OP_NOT, 1)),
                            args: IntoIterator::into_iter([fn_call]).collect(),
                            op_token: Some(Token::Bang),
                            capture_parent_scope: false,
                        };
                        not_base.into_fn_call_expr(pos)
                    }
                }

                #[cfg(not(feature = "no_custom_syntax"))]
                Token::Custom(s) if self.custom_keywords.contains_key(&*s) => {
                    op_base.hashes = if native_only {
                        FnCallHashes::from_native_only(calc_fn_hash(None, &s, 2))
                    } else {
                        FnCallHashes::from_hash(calc_fn_hash(None, &s, 2))
                    };
                    op_base.into_fn_call_expr(pos)
                }

                _ => {
                    let pos = op_base.args[0].start_position();
                    op_base.into_fn_call_expr(pos)
                }
            };
        }
    }

    /// Parse a custom syntax.
    #[cfg(not(feature = "no_custom_syntax"))]
    fn parse_custom_syntax(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
        key: impl Into<ImmutableString>,
        syntax: &crate::api::custom_syntax::CustomSyntax,
    ) -> ParseResult<Expr> {
        #[allow(clippy::wildcard_imports)]
        use crate::api::custom_syntax::markers::*;

        const KEYWORD_SEMICOLON: &str = Token::SemiColon.literal_syntax();
        const KEYWORD_CLOSE_BRACE: &str = Token::RightBrace.literal_syntax();

        let pos = settings.pos;

        let mut inputs = FnArgsVec::new();
        let mut segments = FnArgsVec::new();
        let mut tokens = FnArgsVec::new();

        // Adjust the variables stack
        if syntax.scope_may_be_changed {
            // Add a barrier variable to the stack so earlier variables will not be matched.
            // Variable searches stop at the first barrier.
            let marker = state.get_interned_string(SCOPE_SEARCH_BARRIER_MARKER);
            state.stack.push(marker, ());
        }

        let mut user_state = Dynamic::UNIT;
        let parse_func = &*syntax.parse;
        let mut required_token: ImmutableString = key.into();

        tokens.push(required_token.clone());
        segments.push(required_token.clone());

        loop {
            let (fwd_token, fwd_pos) = input.peek().unwrap();
            settings.pos = *fwd_pos;
            let settings = settings.level_up()?;

            required_token = match parse_func(&segments, &fwd_token.to_string(), &mut user_state) {
                Ok(Some(seg))
                    if seg.starts_with(CUSTOM_SYNTAX_MARKER_SYNTAX_VARIANT)
                        && seg.len() > CUSTOM_SYNTAX_MARKER_SYNTAX_VARIANT.len() =>
                {
                    inputs.push(Expr::StringConstant(state.get_interned_string(seg), pos));
                    break;
                }
                Ok(Some(seg)) => seg,
                Ok(None) => break,
                Err(err) => return Err(err.0.into_err(settings.pos)),
            };

            match required_token.as_str() {
                CUSTOM_SYNTAX_MARKER_IDENT => {
                    let (name, pos) = parse_var_name(input)?;
                    let name = state.get_interned_string(name);

                    let ns = Namespace::NONE;

                    segments.push(name.clone());
                    tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_IDENT));
                    inputs.push(Expr::Variable((None, ns, 0, name).into(), None, pos));
                }
                CUSTOM_SYNTAX_MARKER_SYMBOL => {
                    let (symbol, pos) = match input.next().unwrap() {
                        // Standard symbol
                        (token, pos) if token.is_standard_symbol() => {
                            Ok((token.literal_syntax().into(), pos))
                        }
                        // Reserved symbol
                        (Token::Reserved(s), pos) if !is_valid_identifier(s.as_str()) => {
                            Ok((*s, pos))
                        }
                        // Bad symbol
                        (Token::LexError(err), pos) => Err(err.into_err(pos)),
                        // Not a symbol
                        (.., pos) => Err(PERR::MissingSymbol(String::new()).into_err(pos)),
                    }?;
                    let symbol = state.get_interned_string(symbol);
                    segments.push(symbol.clone());
                    tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_SYMBOL));
                    inputs.push(Expr::StringConstant(symbol, pos));
                }
                CUSTOM_SYNTAX_MARKER_EXPR => {
                    inputs.push(self.parse_expr(input, state, lib, settings)?);
                    let keyword = state.get_interned_string(CUSTOM_SYNTAX_MARKER_EXPR);
                    segments.push(keyword.clone());
                    tokens.push(keyword);
                }
                CUSTOM_SYNTAX_MARKER_BLOCK => {
                    match self.parse_block(input, state, lib, settings)? {
                        block @ Stmt::Block(..) => {
                            inputs.push(Expr::Stmt(Box::new(block.into())));
                            let keyword = state.get_interned_string(CUSTOM_SYNTAX_MARKER_BLOCK);
                            segments.push(keyword.clone());
                            tokens.push(keyword);
                        }
                        stmt => unreachable!("Stmt::Block expected but gets {:?}", stmt),
                    }
                }
                CUSTOM_SYNTAX_MARKER_BOOL => match input.next().unwrap() {
                    (b @ (Token::True | Token::False), pos) => {
                        inputs.push(Expr::BoolConstant(b == Token::True, pos));
                        segments.push(state.get_interned_string(b.literal_syntax()));
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_BOOL));
                    }
                    (.., pos) => {
                        return Err(
                            PERR::MissingSymbol("Expecting 'true' or 'false'".into()).into_err(pos)
                        )
                    }
                },
                CUSTOM_SYNTAX_MARKER_INT => match input.next().unwrap() {
                    (Token::IntegerConstant(i), pos) => {
                        inputs.push(Expr::IntegerConstant(i, pos));
                        segments.push(i.to_string().into());
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_INT));
                    }
                    (.., pos) => {
                        return Err(
                            PERR::MissingSymbol("Expecting an integer number".into()).into_err(pos)
                        )
                    }
                },
                #[cfg(not(feature = "no_float"))]
                CUSTOM_SYNTAX_MARKER_FLOAT => match input.next().unwrap() {
                    (Token::FloatConstant(f), pos) => {
                        inputs.push(Expr::FloatConstant(f.0, pos));
                        segments.push(f.1.into());
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_FLOAT));
                    }
                    (.., pos) => {
                        return Err(
                            PERR::MissingSymbol("Expecting a floating-point number".into())
                                .into_err(pos),
                        )
                    }
                },
                CUSTOM_SYNTAX_MARKER_STRING => match input.next().unwrap() {
                    (Token::StringConstant(s), pos) => {
                        let s = state.get_interned_string(*s);
                        inputs.push(Expr::StringConstant(s.clone(), pos));
                        segments.push(s);
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_STRING));
                    }
                    (.., pos) => {
                        return Err(PERR::MissingSymbol("Expecting a string".into()).into_err(pos))
                    }
                },
                s => match input.next().unwrap() {
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (Token::Identifier(t) | Token::Reserved(t) | Token::Custom(t), ..)
                        if *t == s =>
                    {
                        segments.push(required_token.clone());
                        tokens.push(required_token.clone());
                    }
                    (t, ..) if t.is_literal() && t.literal_syntax() == s => {
                        segments.push(required_token.clone());
                        tokens.push(required_token.clone());
                    }
                    (.., pos) => {
                        return Err(PERR::MissingToken(
                            s.into(),
                            format!("for '{}' expression", segments[0]),
                        )
                        .into_err(pos))
                    }
                },
            }
        }

        inputs.shrink_to_fit();
        tokens.shrink_to_fit();

        let self_terminated = matches!(
            &*required_token,
            // It is self-terminating if the last symbol is a block
            CUSTOM_SYNTAX_MARKER_BLOCK |
            // If the last symbol is `;` or `}`, it is self-terminating
            KEYWORD_SEMICOLON | KEYWORD_CLOSE_BRACE
        );

        Ok(Expr::Custom(
            crate::ast::CustomExpr {
                inputs,
                tokens,
                state: user_state,
                scope_may_be_changed: syntax.scope_may_be_changed,
                self_terminated,
            }
            .into(),
            pos,
        ))
    }

    /// Parse an expression.
    fn parse_expr(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
    ) -> ParseResult<Expr> {
        settings.pos = input.peek().unwrap().1;

        // Parse expression normally.
        let precedence = Precedence::new(1);
        let settings = settings.level_up()?;
        let lhs = self.parse_unary(input, state, lib, settings)?;
        self.parse_binary_op(input, state, lib, settings, precedence, lhs)
    }

    /// Parse an if statement.
    fn parse_if(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        // if ...
        let settings = settings.level_up_with_position(eat_token(input, &Token::If))?;

        // if guard { if_body }
        ensure_not_statement_expr(input, "a boolean")?;
        let expr = self
            .parse_expr(input, state, lib, settings)?
            .ensure_bool_expr()?;
        ensure_not_assignment(input)?;
        let body = self.parse_block(input, state, lib, settings)?.into();

        // if guard { if_body } else ...
        let branch = if match_token(input, &Token::Else).0 {
            match input.peek().unwrap() {
                // if guard { if_body } else if ...
                (Token::If, ..) => self.parse_if(input, state, lib, settings)?,
                // if guard { if_body } else { else-body }
                _ => self.parse_block(input, state, lib, settings)?,
            }
        } else {
            Stmt::Noop(Position::NONE)
        }
        .into();

        Ok(Stmt::If(
            FlowControl { expr, body, branch }.into(),
            settings.pos,
        ))
    }

    /// Parse a while loop.
    fn parse_while_loop(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        let mut settings = settings.level_up()?;

        // while|loops ...
        let (expr, token_pos) = match input.next().unwrap() {
            (Token::While, pos) => {
                ensure_not_statement_expr(input, "a boolean")?;
                let expr = self
                    .parse_expr(input, state, lib, settings)?
                    .ensure_bool_expr()?;
                ensure_not_assignment(input)?;
                (expr, pos)
            }
            (Token::Loop, pos) => (Expr::Unit(Position::NONE), pos),
            token => unreachable!("Token::While or Token::Loop expected but gets {:?}", token),
        };
        settings.pos = token_pos;
        settings.flags |= ParseSettingFlags::BREAKABLE;

        let body = self.parse_block(input, state, lib, settings)?.into();
        let branch = StmtBlock::NONE;

        Ok(Stmt::While(
            FlowControl { expr, body, branch }.into(),
            settings.pos,
        ))
    }

    /// Parse a do loop.
    fn parse_do(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        // do ...
        let mut settings = settings.level_up_with_position(eat_token(input, &Token::Do))?;
        let orig_breakable = settings.has_flag(ParseSettingFlags::BREAKABLE);
        settings.flags |= ParseSettingFlags::BREAKABLE;

        // do { body } [while|until] guard

        let body = self.parse_block(input, state, lib, settings)?.into();

        let negated = match input.next().unwrap() {
            (Token::While, ..) => ASTFlags::empty(),
            (Token::Until, ..) => ASTFlags::NEGATED,
            (.., pos) => {
                return Err(
                    PERR::MissingToken(Token::While.into(), "for the do statement".into())
                        .into_err(pos),
                )
            }
        };

        if !orig_breakable {
            settings.flags.remove(ParseSettingFlags::BREAKABLE);
        }

        ensure_not_statement_expr(input, "a boolean")?;
        let expr = self
            .parse_expr(input, state, lib, settings)?
            .ensure_bool_expr()?;
        ensure_not_assignment(input)?;

        let branch = StmtBlock::NONE;

        Ok(Stmt::Do(
            FlowControl { expr, body, branch }.into(),
            negated,
            settings.pos,
        ))
    }

    /// Parse a for loop.
    fn parse_for(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        // for ...
        let mut settings = settings.level_up_with_position(eat_token(input, &Token::For))?;

        // for name ...
        let (name, name_pos, counter_name, counter_pos) = if match_token(input, &Token::LeftParen).0
        {
            // ( name, counter )
            let (name, name_pos) = parse_var_name(input)?;
            let (has_comma, pos) = match_token(input, &Token::Comma);
            if !has_comma {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "after the iteration variable name".into(),
                )
                .into_err(pos));
            }
            let (counter_name, counter_pos) = parse_var_name(input)?;

            if counter_name == name {
                return Err(PERR::DuplicatedVariable(counter_name.into()).into_err(counter_pos));
            }

            let (has_close_paren, pos) = match_token(input, &Token::RightParen);
            if !has_close_paren {
                return Err(PERR::MissingToken(
                    Token::RightParen.into(),
                    "to close the iteration variable".into(),
                )
                .into_err(pos));
            }
            (name, name_pos, Some(counter_name), counter_pos)
        } else {
            // name
            let (name, name_pos) = parse_var_name(input)?;
            (name, name_pos, None, Position::NONE)
        };

        // for name in ...
        match input.next().unwrap() {
            (Token::In, ..) => (),
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (.., pos) => {
                return Err(PERR::MissingToken(
                    Token::In.into(),
                    "after the iteration variable".into(),
                )
                .into_err(pos))
            }
        }

        // for name in expr { body }
        ensure_not_statement_expr(input, "a boolean")?;
        let expr = self
            .parse_expr(input, state, lib, settings)?
            .ensure_iterable()?;

        let counter_var = counter_name.map(|counter_name| Ident {
            name: state.get_interned_string(counter_name),
            pos: counter_pos,
        });

        let loop_var = Ident {
            name: state.get_interned_string(name),
            pos: name_pos,
        };

        let prev_stack_len = {
            let prev_stack_len = state.stack.len();

            if let Some(ref counter_var) = counter_var {
                state.stack.push(counter_var.name.clone(), ());
            }
            state.stack.push(&loop_var.name, ());

            prev_stack_len
        };

        settings.flags |= ParseSettingFlags::BREAKABLE;
        let body = self.parse_block(input, state, lib, settings)?.into();

        state.stack.rewind(prev_stack_len);

        let branch = StmtBlock::NONE;

        Ok(Stmt::For(
            Box::new((loop_var, counter_var, FlowControl { expr, body, branch })),
            settings.pos,
        ))
    }

    /// Parse a variable definition statement.
    fn parse_let(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
        access: AccessMode,
        is_export: bool,
    ) -> ParseResult<Stmt> {
        // let/const... (specified in `var_type`)
        settings.pos = input.next().unwrap().1;

        // let name ...
        let (name, pos) = parse_var_name(input)?;

        if !self.allow_shadowing() && state.stack.get(&name).is_some() {
            return Err(PERR::VariableExists(name.into()).into_err(pos));
        }

        if let Some(ref filter) = self.def_var_filter {
            let will_shadow = state.stack.get(&name).is_some();

            let global = state
                .global
                .get_or_insert_with(|| GlobalRuntimeState::new(self).into());

            global.level = settings.level;
            let is_const = access == AccessMode::ReadOnly;
            let info = VarDefInfo::new(&name, is_const, settings.level, will_shadow);
            let caches = &mut Caches::new();
            let context = EvalContext::new(self, global, caches, &mut state.stack, None);

            match filter(false, info, context) {
                Ok(true) => (),
                Ok(false) => return Err(PERR::ForbiddenVariable(name.into()).into_err(pos)),
                Err(err) => {
                    return Err(match *err {
                        EvalAltResult::ErrorParsing(e, pos) => e.into_err(pos),
                        _ => PERR::ForbiddenVariable(name.into()).into_err(pos),
                    })
                }
            }
        }

        let name = state.get_interned_string(name);

        // let name = ...
        let expr = if match_token(input, &Token::Equals).0 {
            // let name = expr
            self.parse_expr(input, state, lib, settings.level_up()?)?
        } else {
            Expr::Unit(Position::NONE)
        };

        let export = if is_export {
            ASTFlags::EXPORTED
        } else {
            ASTFlags::empty()
        };

        let (existing, hit_barrier) = state.find_var(&name);

        let existing = if !hit_barrier && existing > 0 {
            match state.stack.len() - existing {
                // Variable has been aliased
                #[cfg(not(feature = "no_module"))]
                offset if !state.stack.get_entry_by_index(offset).2.is_empty() => None,
                // Defined in parent block
                offset if offset < state.frame_pointer => None,
                offset => Some(offset),
            }
        } else {
            None
        };

        let idx = if let Some(n) = existing {
            state.stack.get_mut_by_index(n).set_access_mode(access);
            Some(NonZeroUsize::new(state.stack.len() - n).unwrap())
        } else {
            state.stack.push_entry(name.clone(), access, Dynamic::UNIT);
            None
        };

        #[cfg(not(feature = "no_module"))]
        if is_export {
            state
                .stack
                .add_alias_by_index(state.stack.len() - 1, name.clone());
        }

        let var_def = (Ident { name, pos }, expr, idx).into();

        Ok(match access {
            // let name = expr
            AccessMode::ReadWrite => Stmt::Var(var_def, export, settings.pos),
            // const name = { expr:constant }
            AccessMode::ReadOnly => Stmt::Var(var_def, ASTFlags::CONSTANT | export, settings.pos),
        })
    }

    /// Parse an import statement.
    #[cfg(not(feature = "no_module"))]
    fn parse_import(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        // import ...
        let settings = settings.level_up_with_position(eat_token(input, &Token::Import))?;

        // import expr ...
        let expr = self.parse_expr(input, state, lib, settings)?;

        let export = if match_token(input, &Token::As).0 {
            // import expr as name ...
            let (name, pos) = parse_var_name(input)?;
            Ident {
                name: state.get_interned_string(name),
                pos,
            }
        } else {
            // import expr;
            Ident {
                name: state.get_interned_string(""),
                pos: Position::NONE,
            }
        };

        state.imports.push(export.name.clone());

        Ok(Stmt::Import((expr, export).into(), settings.pos))
    }

    /// Parse an export statement.
    #[cfg(not(feature = "no_module"))]
    fn parse_export(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        settings.pos = eat_token(input, &Token::Export);

        match input.peek().unwrap() {
            (Token::Let, pos) => {
                let pos = *pos;
                let settings = settings.level_up()?;
                let mut stmt =
                    self.parse_let(input, state, lib, settings, AccessMode::ReadWrite, true)?;
                stmt.set_position(pos);
                return Ok(stmt);
            }
            (Token::Const, pos) => {
                let pos = *pos;
                let settings = settings.level_up()?;
                let mut stmt =
                    self.parse_let(input, state, lib, settings, AccessMode::ReadOnly, true)?;
                stmt.set_position(pos);
                return Ok(stmt);
            }
            _ => (),
        }

        let (id, id_pos) = parse_var_name(input)?;

        let (alias, alias_pos) = if match_token(input, &Token::As).0 {
            parse_var_name(input).map(|(name, pos)| (state.get_interned_string(name), pos))?
        } else {
            (state.get_interned_string(""), Position::NONE)
        };

        let (existing, hit_barrier) = state.find_var(&id);

        if !hit_barrier && existing > 0 {
            state
                .stack
                .add_alias_by_index(state.stack.len() - existing, alias.clone());
        }

        let export = (
            Ident {
                name: state.get_interned_string(id),
                pos: id_pos,
            },
            Ident {
                name: alias,
                pos: alias_pos,
            },
        );

        Ok(Stmt::Export(export.into(), settings.pos))
    }

    /// Parse a statement block.
    fn parse_block(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        // Must start with {
        let brace_start_pos = match input.next().unwrap() {
            (Token::LeftBrace, pos) => pos,
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (.., pos) => {
                return Err(PERR::MissingToken(
                    Token::LeftBrace.into(),
                    "to start a statement block".into(),
                )
                .into_err(pos))
            }
        };
        let mut settings = settings.level_up_with_position(brace_start_pos)?;

        let mut block = StmtBlock::empty(settings.pos);

        if settings.has_flag(ParseSettingFlags::DISALLOW_STATEMENTS_IN_BLOCKS) {
            let stmt = self.parse_expr_stmt(input, state, lib, settings)?;
            block.statements_mut().push(stmt);

            // Must end with }
            return match input.next().unwrap() {
                (Token::RightBrace, pos) => {
                    Ok(Stmt::Block(StmtBlock::new(block, settings.pos, pos).into()))
                }
                (Token::LexError(err), pos) => Err(err.into_err(pos)),
                (.., pos) => Err(PERR::MissingToken(
                    Token::LeftBrace.into(),
                    "to start a statement block".into(),
                )
                .into_err(pos)),
            };
        }

        let prev_frame_pointer = state.frame_pointer;
        state.frame_pointer = state.stack.len();

        #[cfg(not(feature = "no_module"))]
        let orig_imports_len = state.imports.len();

        let end_pos = loop {
            // Terminated?
            match input.peek().unwrap() {
                (Token::RightBrace, ..) => break eat_token(input, &Token::RightBrace),
                (Token::EOF, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightBrace.into(),
                        "to terminate this block".into(),
                    )
                    .into_err(*pos));
                }
                _ => (),
            }

            // Parse statements inside the block
            settings.flags.remove(ParseSettingFlags::GLOBAL_LEVEL);

            let stmt = self.parse_stmt(input, state, lib, settings)?;

            if stmt.is_noop() {
                continue;
            }

            // See if it needs a terminating semicolon
            let need_semicolon = !stmt.is_self_terminated();

            block.statements_mut().push(stmt);

            match input.peek().unwrap() {
                // { ... stmt }
                (Token::RightBrace, ..) => break eat_token(input, &Token::RightBrace),
                // { ... stmt;
                (Token::SemiColon, ..) if need_semicolon => {
                    eat_token(input, &Token::SemiColon);
                }
                // { ... { stmt } ;
                (Token::SemiColon, ..) if !need_semicolon => {
                    eat_token(input, &Token::SemiColon);
                }
                // { ... { stmt } ???
                _ if !need_semicolon => (),
                // { ... stmt <error>
                (Token::LexError(err), err_pos) => return Err(err.clone().into_err(*err_pos)),
                // { ... stmt ???
                (.., pos) => {
                    // Semicolons are not optional between statements
                    return Err(PERR::MissingToken(
                        Token::SemiColon.into(),
                        "to terminate this statement".into(),
                    )
                    .into_err(*pos));
                }
            }
        };

        state.stack.rewind(state.frame_pointer);
        state.frame_pointer = prev_frame_pointer;

        #[cfg(not(feature = "no_module"))]
        state.imports.truncate(orig_imports_len);

        Ok(Stmt::Block(
            StmtBlock::new(block, settings.pos, end_pos).into(),
        ))
    }

    /// Parse an expression as a statement.
    fn parse_expr_stmt(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        settings.pos = input.peek().unwrap().1;

        let expr = self.parse_expr(input, state, lib, settings)?;

        let (op, pos) = match input.peek().unwrap() {
            // var = ...
            (Token::Equals, ..) => (None, eat_token(input, &Token::Equals)),
            // var op= ...
            (token, ..) if token.is_op_assignment() => {
                input.next().map(|(op, pos)| (Some(op), pos)).unwrap()
            }
            // Not op-assignment
            _ => return Ok(Stmt::Expr(expr.into())),
        };

        settings.pos = pos;

        let rhs = self.parse_expr(input, state, lib, settings)?;

        Self::make_assignment_stmt(op, state, expr, rhs, pos)
    }

    /// Parse a single statement.
    fn parse_stmt(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        use AccessMode::{ReadOnly, ReadWrite};

        #[cfg(not(feature = "no_function"))]
        #[cfg(feature = "metadata")]
        let comments = {
            let mut comments = StaticVec::<SmartString>::new_const();
            let mut comments_pos = Position::NONE;
            let mut buf = SmartString::new_const();

            // Handle doc-comments.
            while let (Token::Comment(ref comment), pos) = input.peek().unwrap() {
                if comments_pos.is_none() {
                    comments_pos = *pos;
                }

                debug_assert!(
                    crate::tokenizer::is_doc_comment(comment),
                    "doc-comment expected but gets {:?}",
                    comment
                );

                if !settings.has_flag(ParseSettingFlags::GLOBAL_LEVEL) {
                    return Err(PERR::WrongDocComment.into_err(comments_pos));
                }

                match input.next().unwrap() {
                    (Token::Comment(comment), pos) => {
                        if comment.contains('\n') {
                            // Assume block comment
                            if !buf.is_empty() {
                                comments.push(buf.clone());
                                buf.clear();
                            }
                            let c =
                                unindent_block_comment(*comment, pos.position().unwrap_or(1) - 1);
                            comments.push(c.into());
                        } else {
                            if !buf.is_empty() {
                                buf.push('\n');
                            }
                            buf.push_str(&comment);
                        }

                        match input.peek().unwrap() {
                            (Token::Fn | Token::Private, ..) => break,
                            (Token::Comment(..), ..) => (),
                            _ => return Err(PERR::WrongDocComment.into_err(comments_pos)),
                        }
                    }
                    (token, ..) => unreachable!("Token::Comment expected but gets {:?}", token),
                }
            }

            if !buf.is_empty() {
                comments.push(buf);
            }

            comments
        };

        let (token, token_pos) = match input.peek().unwrap() {
            (Token::EOF, pos) => return Ok(Stmt::Noop(*pos)),
            (x, pos) => (x, *pos),
        };

        settings.pos = token_pos;

        match token {
            // ; - empty statement
            Token::SemiColon => {
                eat_token(input, &Token::SemiColon);
                Ok(Stmt::Noop(token_pos))
            }

            // { - statements block
            Token::LeftBrace => Ok(self.parse_block(input, state, lib, settings.level_up()?)?),

            // fn ...
            #[cfg(not(feature = "no_function"))]
            Token::Fn if !settings.has_flag(ParseSettingFlags::GLOBAL_LEVEL) => {
                Err(PERR::WrongFnDefinition.into_err(token_pos))
            }

            #[cfg(not(feature = "no_function"))]
            Token::Fn | Token::Private => {
                let access = if matches!(token, Token::Private) {
                    eat_token(input, &Token::Private);
                    crate::FnAccess::Private
                } else {
                    crate::FnAccess::Public
                };

                match input.next().unwrap() {
                    #[cfg(not(feature = "unchecked"))]
                    (Token::Fn, pos) if lib.len() >= self.max_functions() => {
                        Err(PERR::TooManyFunctions.into_err(pos))
                    }
                    (Token::Fn, pos) => {
                        // Build new parse state
                        let new_state = &mut ParseState::new(
                            state.external_constants,
                            state.interned_strings,
                            state.tokenizer_control.clone(),
                        );

                        #[cfg(not(feature = "no_module"))]
                        {
                            // Do not allow storing an index to a globally-imported module
                            // just in case the function is separated from this `AST`.
                            //
                            // Keep them in `global_imports` instead so that strict variables
                            // mode will not complain.
                            new_state.global_imports.clone_from(&state.global_imports);
                            new_state.global_imports.extend(state.imports.clone());
                        }

                        // Brand new options
                        let options = self.options | (settings.options & LangOptions::STRICT_VAR);

                        // Brand new flags, turn on function scope
                        let flags = ParseSettingFlags::FN_SCOPE
                            | (settings.flags
                                & ParseSettingFlags::DISALLOW_UNQUOTED_MAP_PROPERTIES);

                        let new_settings = ParseSettings {
                            flags,
                            level: 0,
                            options,
                            pos,
                            #[cfg(not(feature = "unchecked"))]
                            max_expr_depth: self.max_function_expr_depth(),
                        };

                        let f = self.parse_fn(
                            input,
                            new_state,
                            lib,
                            new_settings,
                            access,
                            #[cfg(feature = "metadata")]
                            comments,
                        )?;

                        let hash = calc_fn_hash(None, &f.name, f.params.len());

                        #[cfg(not(feature = "no_object"))]
                        let hash = f
                            .this_type
                            .as_ref()
                            .map_or(hash, |typ| crate::calc_typed_method_hash(hash, typ));

                        if !lib.is_empty() && lib.contains_key(&hash) {
                            return Err(PERR::FnDuplicatedDefinition(
                                f.name.to_string(),
                                f.params.len(),
                            )
                            .into_err(pos));
                        }

                        lib.insert(hash, f.into());

                        Ok(Stmt::Noop(pos))
                    }

                    (.., pos) => Err(PERR::MissingToken(
                        Token::Fn.into(),
                        format!("following '{}'", Token::Private),
                    )
                    .into_err(pos)),
                }
            }

            Token::If => self.parse_if(input, state, lib, settings.level_up()?),
            Token::Switch => self.parse_switch(input, state, lib, settings.level_up()?),
            Token::While | Token::Loop if self.allow_looping() => {
                self.parse_while_loop(input, state, lib, settings.level_up()?)
            }
            Token::Do if self.allow_looping() => {
                self.parse_do(input, state, lib, settings.level_up()?)
            }
            Token::For if self.allow_looping() => {
                self.parse_for(input, state, lib, settings.level_up()?)
            }

            Token::Continue
                if self.allow_looping() && settings.has_flag(ParseSettingFlags::BREAKABLE) =>
            {
                let pos = eat_token(input, &Token::Continue);
                Ok(Stmt::BreakLoop(None, ASTFlags::empty(), pos))
            }
            Token::Break
                if self.allow_looping() && settings.has_flag(ParseSettingFlags::BREAKABLE) =>
            {
                let pos = eat_token(input, &Token::Break);

                let expr = match input.peek().unwrap() {
                    // `break` at <EOF>
                    (Token::EOF, ..) => None,
                    // `break` at end of block
                    (Token::RightBrace, ..) => None,
                    // `break;`
                    (Token::SemiColon, ..) => None,
                    // `break`  with expression
                    _ => Some(
                        self.parse_expr(input, state, lib, settings.level_up()?)?
                            .into(),
                    ),
                };

                Ok(Stmt::BreakLoop(expr, ASTFlags::BREAK, pos))
            }
            Token::Continue | Token::Break if self.allow_looping() => {
                Err(PERR::LoopBreak.into_err(token_pos))
            }

            Token::Return | Token::Throw => {
                let (return_type, token_pos) = input
                    .next()
                    .map(|(token, pos)| {
                        let flags = match token {
                            Token::Return => ASTFlags::empty(),
                            Token::Throw => ASTFlags::BREAK,
                            token => unreachable!(
                                "Token::Return or Token::Throw expected but gets {:?}",
                                token
                            ),
                        };
                        (flags, pos)
                    })
                    .unwrap();

                match input.peek().unwrap() {
                    // `return`/`throw` at <EOF>
                    (Token::EOF, ..) => Ok(Stmt::Return(None, return_type, token_pos)),
                    // `return`/`throw` at end of block
                    (Token::RightBrace, ..)
                        if !settings.has_flag(ParseSettingFlags::GLOBAL_LEVEL) =>
                    {
                        Ok(Stmt::Return(None, return_type, token_pos))
                    }
                    // `return;` or `throw;`
                    (Token::SemiColon, ..) => Ok(Stmt::Return(None, return_type, token_pos)),
                    // `return` or `throw` with expression
                    _ => {
                        let expr = self.parse_expr(input, state, lib, settings.level_up()?)?;
                        Ok(Stmt::Return(Some(expr.into()), return_type, token_pos))
                    }
                }
            }

            Token::Try => self.parse_try_catch(input, state, lib, settings.level_up()?),

            Token::Let => self.parse_let(input, state, lib, settings.level_up()?, ReadWrite, false),
            Token::Const => {
                self.parse_let(input, state, lib, settings.level_up()?, ReadOnly, false)
            }

            #[cfg(not(feature = "no_module"))]
            Token::Import => self.parse_import(input, state, lib, settings.level_up()?),

            #[cfg(not(feature = "no_module"))]
            Token::Export if !settings.has_flag(ParseSettingFlags::GLOBAL_LEVEL) => {
                Err(PERR::WrongExport.into_err(token_pos))
            }

            #[cfg(not(feature = "no_module"))]
            Token::Export => self.parse_export(input, state, lib, settings.level_up()?),

            _ => self.parse_expr_stmt(input, state, lib, settings.level_up()?),
        }
    }

    /// Parse a try/catch statement.
    fn parse_try_catch(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        // try ...
        let settings = settings.level_up_with_position(eat_token(input, &Token::Try))?;

        // try { try_block }
        let body = self.parse_block(input, state, lib, settings)?.into();

        // try { try_block } catch
        let (matched, catch_pos) = match_token(input, &Token::Catch);

        if !matched {
            return Err(
                PERR::MissingToken(Token::Catch.into(), "for the 'try' statement".into())
                    .into_err(catch_pos),
            );
        }

        // try { try_block } catch (
        let catch_var = if match_token(input, &Token::LeftParen).0 {
            let (name, pos) = parse_var_name(input)?;
            let (matched, err_pos) = match_token(input, &Token::RightParen);

            if !matched {
                return Err(PERR::MissingToken(
                    Token::RightParen.into(),
                    "to enclose the catch variable".into(),
                )
                .into_err(err_pos));
            }

            let name = state.get_interned_string(name);
            state.stack.push(name.clone(), ());
            Ident { name, pos }
        } else {
            Ident {
                name: state.get_interned_string(""),
                pos: Position::NONE,
            }
        };

        // try { try_block } catch ( var ) { catch_block }
        let branch = self.parse_block(input, state, lib, settings)?.into();

        let expr = if catch_var.is_empty() {
            Expr::Unit(catch_var.pos)
        } else {
            // Remove the error variable from the stack
            state.stack.pop();

            Expr::Variable(
                (None, <_>::default(), 0, catch_var.name).into(),
                None,
                catch_var.pos,
            )
        };

        Ok(Stmt::TryCatch(
            FlowControl { expr, body, branch }.into(),
            settings.pos,
        ))
    }

    /// Parse a function definition.
    #[cfg(not(feature = "no_function"))]
    fn parse_fn(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
        access: crate::FnAccess,
        #[cfg(feature = "metadata")] comments: impl IntoIterator<Item = crate::Identifier>,
    ) -> ParseResult<ScriptFuncDef> {
        let settings = settings.level_up()?;

        let (token, pos) = input.next().unwrap();

        // Parse type for `this` pointer
        #[cfg(not(feature = "no_object"))]
        let ((token, pos), this_type) = {
            let (next_token, next_pos) = input.peek().unwrap();

            match token {
                Token::StringConstant(s) if next_token == &Token::Period => {
                    eat_token(input, &Token::Period);
                    let s = match s.as_str() {
                        "int" => state.get_interned_string(std::any::type_name::<crate::INT>()),
                        #[cfg(not(feature = "no_float"))]
                        "float" => state.get_interned_string(std::any::type_name::<crate::FLOAT>()),
                        _ => state.get_interned_string(*s),
                    };
                    (input.next().unwrap(), Some(s))
                }
                Token::StringConstant(..) => {
                    return Err(PERR::MissingToken(
                        Token::Period.into(),
                        "after the type name for 'this'".into(),
                    )
                    .into_err(*next_pos))
                }
                Token::Identifier(s) if next_token == &Token::Period => {
                    eat_token(input, &Token::Period);
                    let s = match s.as_str() {
                        "int" => state.get_interned_string(std::any::type_name::<crate::INT>()),
                        #[cfg(not(feature = "no_float"))]
                        "float" => state.get_interned_string(std::any::type_name::<crate::FLOAT>()),
                        _ => state.get_interned_string(*s),
                    };
                    (input.next().unwrap(), Some(s))
                }
                _ => ((token, pos), None),
            }
        };

        let name = match token {
            #[cfg(not(feature = "no_custom_syntax"))]
            Token::Custom(s) if is_valid_function_name(&s) => *s,
            Token::Identifier(s) if is_valid_function_name(&s) => *s,
            Token::Reserved(s) => return Err(PERR::Reserved(s.to_string()).into_err(pos)),
            _ => return Err(PERR::FnMissingName.into_err(pos)),
        };

        let no_params = match input.peek().unwrap() {
            (Token::LeftParen, ..) => {
                eat_token(input, &Token::LeftParen);
                match_token(input, &Token::RightParen).0
            }
            (Token::Unit, ..) => {
                eat_token(input, &Token::Unit);
                true
            }
            (.., pos) => return Err(PERR::FnMissingParams(name.into()).into_err(*pos)),
        };

        let mut params = StaticVec::<(ImmutableString, _)>::new_const();

        if !no_params {
            let sep_err = format!("to separate the parameters of function '{name}'");

            loop {
                match input.next().unwrap() {
                    (Token::RightParen, ..) => break,
                    (Token::Identifier(s), pos) => {
                        if params.iter().any(|(p, _)| p == &*s) {
                            return Err(
                                PERR::FnDuplicatedParam(name.into(), s.to_string()).into_err(pos)
                            );
                        }

                        let s = state.get_interned_string(*s);
                        state.stack.push(s.clone(), ());
                        params.push((s, pos));
                    }
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (.., pos) => {
                        return Err(PERR::MissingToken(
                            Token::RightParen.into(),
                            format!("to close the parameters list of function '{name}'"),
                        )
                        .into_err(pos))
                    }
                }

                match input.next().unwrap() {
                    (Token::RightParen, ..) => break,
                    (Token::Comma, ..) => (),
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (.., pos) => {
                        return Err(PERR::MissingToken(Token::Comma.into(), sep_err).into_err(pos))
                    }
                }
            }
        }

        // Parse function body
        let body = match input.peek().unwrap() {
            (Token::LeftBrace, ..) => self.parse_block(input, state, lib, settings)?,
            (.., pos) => return Err(PERR::FnMissingBody(name.into()).into_err(*pos)),
        }
        .into();

        let mut params: FnArgsVec<_> = params.into_iter().map(|(p, ..)| p).collect();
        params.shrink_to_fit();

        Ok(ScriptFuncDef {
            name: state.get_interned_string(name),
            access,
            #[cfg(not(feature = "no_object"))]
            this_type,
            params,
            body,
            #[cfg(feature = "metadata")]
            comments: comments.into_iter().collect(),
        })
    }

    /// Creates a curried expression from a list of external variables
    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_closure"))]
    fn make_curry_from_externals(
        state: &mut ParseState,
        parent: &mut ParseState,
        lib: &FnLib,
        fn_expr: Expr,
        externals: impl AsRef<[Ident]> + IntoIterator<Item = Ident>,
        pos: Position,
    ) -> Expr {
        // If there are no captured variables, no need to curry
        if externals.as_ref().is_empty() {
            return fn_expr;
        }

        let num_externals = externals.as_ref().len();
        let mut args = FnArgsVec::with_capacity(externals.as_ref().len() + 1);

        args.push(fn_expr);

        args.extend(
            externals
                .as_ref()
                .iter()
                .cloned()
                .map(|Ident { name, pos }| {
                    let (index, is_func) = parent.access_var(&name, lib, pos);
                    let idx = match index {
                        Some(n) if !is_func => u8::try_from(n.get()).ok().and_then(NonZeroU8::new),
                        _ => None,
                    };
                    Expr::Variable((index, <_>::default(), 0, name).into(), idx, pos)
                }),
        );

        let expr = FnCallExpr {
            namespace: Namespace::NONE,
            name: state.get_interned_string(crate::engine::KEYWORD_FN_PTR_CURRY),
            hashes: FnCallHashes::from_native_only(calc_fn_hash(
                None,
                crate::engine::KEYWORD_FN_PTR_CURRY,
                num_externals + 1,
            )),
            args,
            op_token: None,
            capture_parent_scope: false,
        }
        .into_fn_call_expr(pos);

        // Convert the entire expression into a statement block, then insert the relevant
        // [`Share`][Stmt::Share] statements.
        let mut statements = StaticVec::with_capacity(2);
        statements.push(Stmt::Share(
            externals
                .into_iter()
                .map(|var| {
                    let (index, _) = parent.access_var(&var.name, lib, var.pos);
                    (var, index)
                })
                .collect::<FnArgsVec<_>>()
                .into(),
        ));
        statements.push(Stmt::Expr(expr.into()));
        Expr::Stmt(StmtBlock::new(statements, pos, Position::NONE).into())
    }

    /// Parse an anonymous function definition.
    #[cfg(not(feature = "no_function"))]
    fn parse_anon_fn(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
        _parent: &mut ParseState,
    ) -> ParseResult<(Expr, Shared<ScriptFuncDef>)> {
        use core::iter::FromIterator;

        let settings = settings.level_up()?;
        let mut params_list = StaticVec::<ImmutableString>::new_const();

        if input.next().unwrap().0 != Token::Or && !match_token(input, &Token::Pipe).0 {
            loop {
                match input.next().unwrap() {
                    (Token::Pipe, ..) => break,
                    (Token::Identifier(s), pos) => {
                        if params_list.iter().any(|p| p == &*s) {
                            return Err(
                                PERR::FnDuplicatedParam(String::new(), s.to_string()).into_err(pos)
                            );
                        }

                        let s = state.get_interned_string(*s);
                        state.stack.push(s.clone(), ());
                        params_list.push(s);
                    }
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (.., pos) => {
                        return Err(PERR::MissingToken(
                            Token::Pipe.into(),
                            "to close the parameters list of anonymous function or closure".into(),
                        )
                        .into_err(pos))
                    }
                }

                match input.next().unwrap() {
                    (Token::Pipe, ..) => break,
                    (Token::Comma, ..) => (),
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (.., pos) => {
                        return Err(PERR::MissingToken(
                            Token::Comma.into(),
                            "to separate the parameters of anonymous function".into(),
                        )
                        .into_err(pos))
                    }
                }
            }
        }

        // Parse function body
        let body = self.parse_stmt(input, state, lib, settings)?;

        // External variables may need to be processed in a consistent order,
        // so extract them into a list.
        #[cfg(not(feature = "no_closure"))]
        let (mut params, externals) = if state.external_vars.is_empty() {
            (
                FnArgsVec::with_capacity(params_list.len()),
                FnArgsVec::new_const(),
            )
        } else {
            let externals = FnArgsVec::from_iter(state.external_vars.iter().cloned());

            let mut params = FnArgsVec::with_capacity(params_list.len() + externals.len());
            params.extend(externals.iter().map(|Ident { name, .. }| name.clone()));

            (params, externals)
        };
        #[cfg(feature = "no_closure")]
        let mut params = FnArgsVec::with_capacity(params_list.len());

        params.append(&mut params_list);

        // Create unique function name by hashing the script body plus the parameters.
        let hasher = &mut get_hasher();
        params.iter().for_each(|p| p.hash(hasher));
        body.hash(hasher);
        let hash = hasher.finish();
        let fn_name = state.get_interned_string(make_anonymous_fn(hash));

        // Define the function
        let script = Shared::new(ScriptFuncDef {
            name: fn_name.clone(),
            access: crate::FnAccess::Public,
            #[cfg(not(feature = "no_object"))]
            this_type: None,
            params,
            body: body.into(),
            #[cfg(not(feature = "no_function"))]
            #[cfg(feature = "metadata")]
            comments: <_>::default(),
        });

        let fn_ptr = crate::FnPtr {
            name: fn_name,
            curry: ThinVec::new(),
            environ: None,
            #[cfg(not(feature = "no_function"))]
            fn_def: Some(script.clone()),
        };
        let expr = Expr::DynamicConstant(Box::new(fn_ptr.into()), settings.pos);

        #[cfg(not(feature = "no_closure"))]
        let expr =
            Self::make_curry_from_externals(state, _parent, lib, expr, externals, settings.pos);

        Ok((expr, script))
    }

    /// Parse a global level expression.
    pub(crate) fn parse_global_expr(
        &self,
        mut input: TokenStream,
        state: &mut ParseState,
        process_settings: impl FnOnce(&mut ParseSettings),
        _optimization_level: OptimizationLevel,
    ) -> ParseResult<AST> {
        let mut functions = <_>::default();

        let options = self.options & !LangOptions::STMT_EXPR & !LangOptions::LOOP_EXPR;

        let mut settings = ParseSettings {
            level: 0,
            flags: ParseSettingFlags::GLOBAL_LEVEL
                | ParseSettingFlags::DISALLOW_STATEMENTS_IN_BLOCKS,
            options,
            pos: Position::START,
            #[cfg(not(feature = "unchecked"))]
            max_expr_depth: self.max_expr_depth(),
        };
        process_settings(&mut settings);

        let expr = self.parse_expr(&mut input, state, &mut functions, settings)?;

        #[cfg(feature = "no_function")]
        debug_assert!(functions.is_empty());

        match input.peek().unwrap() {
            (Token::EOF, ..) => (),
            // Return error if the expression doesn't end
            (token, pos) => return Err(LexError::UnexpectedInput(token.to_string()).into_err(*pos)),
        }

        let mut statements = StmtBlockContainer::new_const();
        statements.push(Stmt::Expr(expr.into()));

        #[cfg(not(feature = "no_optimize"))]
        return Ok(crate::optimizer::optimize_into_ast(
            self,
            state.external_constants,
            statements,
            #[cfg(not(feature = "no_function"))]
            functions.into_iter().map(|(.., v)| v).collect(),
            _optimization_level,
        ));

        #[cfg(feature = "no_optimize")]
        return Ok(AST::new(
            statements,
            #[cfg(not(feature = "no_function"))]
            crate::Module::from(functions.into_iter().map(|(.., v)| v)),
        ));
    }

    /// Parse the global level statements.
    fn parse_global_level(
        &self,
        mut input: TokenStream,
        state: &mut ParseState,
        process_settings: impl FnOnce(&mut ParseSettings),
    ) -> ParseResult<(StmtBlockContainer, Vec<Shared<ScriptFuncDef>>)> {
        let mut statements = StmtBlockContainer::new_const();
        let mut functions = <_>::default();

        let mut settings = ParseSettings {
            level: 0,
            flags: ParseSettingFlags::GLOBAL_LEVEL,
            options: self.options,
            pos: Position::START,
            #[cfg(not(feature = "unchecked"))]
            max_expr_depth: self.max_expr_depth(),
        };
        process_settings(&mut settings);

        while input.peek().unwrap().0 != Token::EOF {
            let stmt = self.parse_stmt(&mut input, state, &mut functions, settings)?;

            if stmt.is_noop() {
                continue;
            }

            let need_semicolon = !stmt.is_self_terminated();

            statements.push(stmt);

            match input.peek().unwrap() {
                // EOF
                (Token::EOF, ..) => break,
                // stmt ;
                (Token::SemiColon, ..) if need_semicolon => {
                    eat_token(&mut input, &Token::SemiColon);
                }
                // stmt ;
                (Token::SemiColon, ..) if !need_semicolon => (),
                // { stmt } ???
                _ if !need_semicolon => (),
                // stmt <error>
                (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
                // stmt ???
                (.., pos) => {
                    // Semicolons are not optional between statements
                    return Err(PERR::MissingToken(
                        Token::SemiColon.into(),
                        "to terminate this statement".into(),
                    )
                    .into_err(*pos));
                }
            }
        }

        Ok((statements, functions.into_iter().map(|(.., v)| v).collect()))
    }

    /// Run the parser on an input stream, returning an AST.
    #[inline]
    pub(crate) fn parse(
        &self,
        input: TokenStream,
        state: &mut ParseState,
        _optimization_level: OptimizationLevel,
    ) -> ParseResult<AST> {
        let (statements, _lib) = self.parse_global_level(input, state, |_| {})?;

        #[cfg(not(feature = "no_optimize"))]
        return Ok(crate::optimizer::optimize_into_ast(
            self,
            state.external_constants,
            statements,
            #[cfg(not(feature = "no_function"))]
            _lib,
            _optimization_level,
        ));

        #[cfg(feature = "no_optimize")]
        #[cfg(not(feature = "no_function"))]
        {
            let mut m = crate::Module::new();

            _lib.into_iter().for_each(|fn_def| {
                m.set_script_fn(fn_def);
            });

            return Ok(AST::new(statements, m));
        }

        #[cfg(feature = "no_optimize")]
        #[cfg(feature = "no_function")]
        return Ok(AST::new(
            statements,
            #[cfg(not(feature = "no_function"))]
            crate::Module::new(),
        ));
    }
}
