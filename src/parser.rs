//! Main module defining the lexer and parser.

use crate::api::events::VarDefInfo;
use crate::api::options::LangOptions;
use crate::ast::{
    ASTFlags, BinaryExpr, CaseBlocksList, ConditionalExpr, Expr, FnCallExpr, FnCallHashes, Ident,
    OpAssignment, RangeCase, ScriptFnDef, Stmt, StmtBlock, StmtBlockContainer,
    SwitchCasesCollection, TryCatchBlock,
};
use crate::engine::{Precedence, KEYWORD_THIS, OP_CONTAINS};
use crate::eval::GlobalRuntimeState;
use crate::func::hashing::get_hasher;
use crate::tokenizer::{
    is_keyword_function, is_valid_function_name, is_valid_identifier, Token, TokenStream,
    TokenizerControl,
};
use crate::types::dynamic::AccessMode;
use crate::types::StringsInterner;
use crate::{
    calc_fn_hash, Dynamic, Engine, EvalAltResult, EvalContext, ExclusiveRange, Identifier,
    ImmutableString, InclusiveRange, LexError, OptimizationLevel, ParseError, Position, Scope,
    Shared, SmartString, StaticVec, AST, INT, PERR,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    collections::BTreeMap,
    fmt,
    hash::{Hash, Hasher},
    num::{NonZeroU8, NonZeroUsize},
};

pub type ParseResult<T> = Result<T, ParseError>;

type FnLib = BTreeMap<u64, Shared<ScriptFnDef>>;

/// Invalid variable name that acts as a search barrier in a [`Scope`].
const SCOPE_SEARCH_BARRIER_MARKER: &str = "$ BARRIER $";

/// The message: `TokenStream` never ends
const NEVER_ENDS: &str = "`Token`";

/// Unroll `switch` ranges no larger than this.
const SMALL_SWITCH_RANGE: usize = 16;

const NUM_INTERNERS: usize = if cfg!(feature = "no_object") { 1 } else { 3 };

/// _(internals)_ A type that encapsulates the current state of the parser.
/// Exported under the `internals` feature only.
pub struct ParseState<'e> {
    /// Input stream buffer containing the next character to read.
    pub tokenizer_control: TokenizerControl,
    /// Controls whether parsing of an expression should stop given the next token.
    pub expr_filter: fn(&Token) -> bool,
    /// String interners.
    interned_strings: [StringsInterner<'e>; NUM_INTERNERS],
    /// External [scope][Scope] with constants.
    pub scope: &'e Scope<'e>,
    /// Global runtime state.
    pub global: GlobalRuntimeState<'e>,
    /// Encapsulates a local stack with variable names to simulate an actual runtime scope.
    pub stack: Scope<'e>,
    /// Size of the local variables stack upon entry of the current block scope.
    pub block_stack_len: usize,
    /// Tracks a list of external variables (variables that are not explicitly declared in the scope).
    #[cfg(not(feature = "no_closure"))]
    pub external_vars: Vec<crate::ast::Ident>,
    /// An indicator that disables variable capturing into externals one single time
    /// up until the nearest consumed Identifier token.
    /// If set to false the next call to [`access_var`][ParseState::access_var] will not capture the variable.
    /// All consequent calls to [`access_var`][ParseState::access_var] will not be affected.
    #[cfg(not(feature = "no_closure"))]
    pub allow_capture: bool,
    /// Encapsulates a local stack with imported [module][crate::Module] names.
    #[cfg(not(feature = "no_module"))]
    pub imports: StaticVec<ImmutableString>,
    /// List of globally-imported [module][crate::Module] names.
    #[cfg(not(feature = "no_module"))]
    pub global_imports: StaticVec<ImmutableString>,
    /// Maximum levels of expression nesting (0 for unlimited).
    #[cfg(not(feature = "unchecked"))]
    pub max_expr_depth: usize,
}

impl fmt::Debug for ParseState<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("ParseState");

        f.field("tokenizer_control", &self.tokenizer_control)
            .field("interned_strings", &self.interned_strings)
            .field("scope", &self.scope)
            .field("global", &self.global)
            .field("stack", &self.stack)
            .field("block_stack_len", &self.block_stack_len);
        #[cfg(not(feature = "no_closure"))]
        f.field("external_vars", &self.external_vars)
            .field("allow_capture", &self.allow_capture);
        #[cfg(not(feature = "no_module"))]
        f.field("imports", &self.imports)
            .field("global_imports", &self.global_imports);
        #[cfg(not(feature = "unchecked"))]
        f.field("max_expr_depth", &self.max_expr_depth);
        f.finish()
    }
}

impl<'e> ParseState<'e> {
    /// Create a new [`ParseState`].
    #[inline(always)]
    #[must_use]
    pub fn new(
        engine: &Engine,
        scope: &'e Scope,
        interned_strings: [StringsInterner<'e>; NUM_INTERNERS],
        tokenizer_control: TokenizerControl,
    ) -> Self {
        Self {
            tokenizer_control,
            expr_filter: |_| true,
            #[cfg(not(feature = "no_closure"))]
            external_vars: Vec::new(),
            #[cfg(not(feature = "no_closure"))]
            allow_capture: true,
            interned_strings,
            scope,
            global: GlobalRuntimeState::new(engine),
            stack: Scope::new(),
            block_stack_len: 0,
            #[cfg(not(feature = "no_module"))]
            imports: StaticVec::new_const(),
            #[cfg(not(feature = "no_module"))]
            global_imports: StaticVec::new_const(),
            #[cfg(not(feature = "unchecked"))]
            max_expr_depth: engine.max_expr_depth(),
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

        (
            self.stack
                .iter_rev_raw()
                .enumerate()
                .find(|&(.., (n, ..))| {
                    if n == SCOPE_SEARCH_BARRIER_MARKER {
                        // Do not go beyond the barrier
                        hit_barrier = true;
                        false
                    } else {
                        n == name
                    }
                })
                .map_or(0, |(i, ..)| i + 1),
            hit_barrier,
        )
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
    /// * `index`: `None` when the variable name is not found in the `stack`,
    ///            otherwise the index value.
    ///
    /// * `is_func_name`: `true` if the variable is actually the name of a function
    ///                   (in which case it will be converted into a function pointer).
    #[inline]
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
            if !is_func_name && index == 0 && !self.external_vars.iter().any(|v| v.as_str() == name)
            {
                self.external_vars.push(crate::ast::Ident {
                    name: name.into(),
                    pos: _pos,
                });
            }
        } else {
            self.allow_capture = true;
        }

        let index = if hit_barrier {
            None
        } else {
            NonZeroUsize::new(index)
        };

        (index, is_func_name)
    }

    /// Find a module by name in the [`ParseState`], searching in reverse.
    ///
    /// Returns the offset to be deducted from `Stack::len`,
    /// i.e. the top element of the [`ParseState`] is offset 1.
    ///
    /// Returns `None` when the variable name is not found in the [`ParseState`].
    ///
    /// # Panics
    ///
    /// Panics when called under `no_module`.
    #[cfg(not(feature = "no_module"))]
    #[inline]
    #[must_use]
    pub fn find_module(&self, name: &str) -> Option<NonZeroUsize> {
        self.imports
            .iter()
            .rev()
            .enumerate()
            .find(|(.., n)| n.as_str() == name)
            .and_then(|(i, ..)| NonZeroUsize::new(i + 1))
    }

    /// Get an interned string, creating one if it is not yet interned.
    #[inline(always)]
    #[must_use]
    pub fn get_interned_string(
        &mut self,
        text: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        self.interned_strings[0].get(text)
    }

    /// Get an interned property getter, creating one if it is not yet interned.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    #[must_use]
    pub fn get_interned_getter(
        &mut self,
        text: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        self.interned_strings[1]
            .get_with_mapper(|s| crate::engine::make_getter(s.as_ref()).into(), text)
    }

    /// Get an interned property setter, creating one if it is not yet interned.
    #[cfg(not(feature = "no_object"))]
    #[inline(always)]
    #[must_use]
    pub fn get_interned_setter(
        &mut self,
        text: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        self.interned_strings[2]
            .get_with_mapper(|s| crate::engine::make_setter(s.as_ref()).into(), text)
    }
}

/// A type that encapsulates all the settings for a particular parsing function.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct ParseSettings {
    /// Is the construct being parsed located at global level?
    at_global_level: bool,
    /// Is the construct being parsed located inside a function definition?
    #[cfg(not(feature = "no_function"))]
    in_fn_scope: bool,
    /// Is the construct being parsed located inside a closure definition?
    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_closure"))]
    in_closure: bool,
    /// Is the construct being parsed located inside a breakable loop?
    is_breakable: bool,
    /// Language options in effect (overrides Engine options).
    options: LangOptions,
    /// Current expression nesting level.
    level: usize,
    /// Current position.
    pos: Position,
}

impl ParseSettings {
    /// Create a new `ParseSettings` with one higher expression level.
    #[inline(always)]
    #[must_use]
    pub const fn level_up(&self) -> Self {
        Self {
            level: self.level + 1,
            ..*self
        }
    }
    /// Make sure that the current level of expression nesting is within the maximum limit.
    ///
    /// If `limit` is zero, then checking is disabled.
    #[cfg(not(feature = "unchecked"))]
    #[inline]
    pub fn ensure_level_within_max_limit(&self, limit: usize) -> ParseResult<()> {
        if limit > 0 && self.level > limit {
            return Err(PERR::ExprTooDeep.into_err(self.pos));
        }
        Ok(())
    }
}

/// Make an anonymous function.
#[cfg(not(feature = "no_function"))]
#[inline]
#[must_use]
pub fn make_anonymous_fn(hash: u64) -> String {
    format!("{}{:016x}", crate::engine::FN_ANONYMOUS, hash)
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
                let getter = state.get_interned_getter(ident.as_str());
                let hash_get = calc_fn_hash(&getter, 1);
                let setter = state.get_interned_setter(ident.as_str());
                let hash_set = calc_fn_hash(&setter, 2);

                Self::Property(
                    Box::new(((getter, hash_get), (setter, hash_set), ident)),
                    pos,
                )
            }
            _ => self,
        }
    }
    /// Raise an error if the expression can never yield a boolean value.
    fn ensure_bool_expr(self) -> ParseResult<Expr> {
        let type_name = match self {
            Expr::Unit(..) => "()",
            Expr::DynamicConstant(ref v, ..) if !v.is::<bool>() => v.type_name(),
            Expr::IntegerConstant(..) => "a number",
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(..) => "a floating-point number",
            Expr::CharConstant(..) => "a character",
            Expr::StringConstant(..) => "a string",
            Expr::InterpolatedString(..) => "a string",
            Expr::Array(..) => "an array",
            Expr::Map(..) => "an object map",
            _ => return Ok(self),
        };

        Err(
            PERR::MismatchedType("a boolean expression".to_string(), type_name.to_string())
                .into_err(self.start_position()),
        )
    }
    /// Raise an error if the expression can never yield an iterable value.
    fn ensure_iterable(self) -> ParseResult<Expr> {
        let type_name = match self {
            Expr::Unit(..) => "()",
            Expr::BoolConstant(..) => "a boolean",
            Expr::IntegerConstant(..) => "a number",
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(..) => "a floating-point number",
            Expr::CharConstant(..) => "a character",
            Expr::Map(..) => "an object map",
            _ => return Ok(self),
        };

        Err(
            PERR::MismatchedType("an iterable value".to_string(), type_name.to_string())
                .into_err(self.start_position()),
        )
    }
}

/// Make sure that the next expression is not a statement expression (i.e. wrapped in `{}`).
#[inline]
fn ensure_not_statement_expr(
    input: &mut TokenStream,
    type_name: &(impl ToString + ?Sized),
) -> ParseResult<()> {
    match input.peek().expect(NEVER_ENDS) {
        (Token::LeftBrace, pos) => Err(PERR::ExprExpected(type_name.to_string()).into_err(*pos)),
        _ => Ok(()),
    }
}

/// Make sure that the next expression is not a mis-typed assignment (i.e. `a = b` instead of `a == b`).
#[inline]
fn ensure_not_assignment(input: &mut TokenStream) -> ParseResult<()> {
    match input.peek().expect(NEVER_ENDS) {
        (Token::Equals, pos) => Err(LexError::ImproperSymbol(
            "=".to_string(),
            "Possibly a typo of '=='?".to_string(),
        )
        .into_err(*pos)),
        _ => Ok(()),
    }
}

/// Consume a particular [token][Token], checking that it is the expected one.
///
/// # Panics
///
/// Panics if the next token is not the expected one.
#[inline]
fn eat_token(input: &mut TokenStream, expected_token: Token) -> Position {
    let (t, pos) = input.next().expect(NEVER_ENDS);

    if t != expected_token {
        unreachable!(
            "{} expected but gets {} at {}",
            expected_token.syntax(),
            t.syntax(),
            pos
        );
    }
    pos
}

/// Match a particular [token][Token], consuming it if matched.
#[inline]
fn match_token(input: &mut TokenStream, token: Token) -> (bool, Position) {
    let (t, pos) = input.peek().expect(NEVER_ENDS);
    if *t == token {
        (true, eat_token(input, token))
    } else {
        (false, *pos)
    }
}

/// Parse a variable name.
#[inline]
fn parse_var_name(input: &mut TokenStream) -> ParseResult<(SmartString, Position)> {
    match input.next().expect(NEVER_ENDS) {
        // Variable name
        (Token::Identifier(s), pos) => Ok((s, pos)),
        // Reserved keyword
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            Err(PERR::Reserved(s.to_string()).into_err(pos))
        }
        // Bad identifier
        (Token::LexError(err), pos) => Err(err.into_err(pos)),
        // Not a variable name
        (.., pos) => Err(PERR::VariableExpected.into_err(pos)),
    }
}

/// Parse a symbol.
#[cfg(not(feature = "no_custom_syntax"))]
#[inline]
fn parse_symbol(input: &mut TokenStream) -> ParseResult<(SmartString, Position)> {
    match input.next().expect(NEVER_ENDS) {
        // Symbol
        (token, pos) if token.is_standard_symbol() => Ok((token.literal_syntax().into(), pos)),
        // Reserved symbol
        (Token::Reserved(s), pos) if !is_valid_identifier(s.chars()) => Ok((s, pos)),
        // Bad symbol
        (Token::LexError(err), pos) => Err(err.into_err(pos)),
        // Not a symbol
        (.., pos) => Err(PERR::MissingSymbol(String::new()).into_err(pos)),
    }
}

impl Engine {
    /// Parse `(` expr `)`
    fn parse_paren_expr(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // ( ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::LeftParen);

        let expr = self.parse_expr(input, state, lib, settings.level_up())?;

        match input.next().expect(NEVER_ENDS) {
            // ( ... )
            (Token::RightParen, ..) => Ok(expr),
            // ( <error>
            (Token::LexError(err), pos) => Err(err.into_err(pos)),
            // ( ... ???
            (.., pos) => Err(PERR::MissingToken(
                Token::RightParen.into(),
                "for a matching ( in this expression".into(),
            )
            .into_err(pos)),
        }
    }

    /// Parse a function call.
    fn parse_fn_call(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        id: ImmutableString,
        no_args: bool,
        capture_parent_scope: bool,
        #[cfg(not(feature = "no_module"))] namespace: crate::ast::Namespace,
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let (token, token_pos) = if no_args {
            &(Token::RightParen, Position::NONE)
        } else {
            input.peek().expect(NEVER_ENDS)
        };

        #[cfg(not(feature = "no_module"))]
        let mut namespace = namespace;
        let mut args = StaticVec::new_const();

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
                    eat_token(input, Token::RightParen);
                }

                #[cfg(not(feature = "no_module"))]
                let hash = if namespace.is_empty() {
                    calc_fn_hash(&id, 0)
                } else {
                    let root = namespace.root();
                    let index = state.find_module(root);

                    #[cfg(not(feature = "no_function"))]
                    #[cfg(not(feature = "no_module"))]
                    let is_global = root == crate::engine::KEYWORD_GLOBAL;
                    #[cfg(any(feature = "no_function", feature = "no_module"))]
                    let is_global = false;

                    if settings.options.contains(LangOptions::STRICT_VAR)
                        && index.is_none()
                        && !is_global
                        && !state.global_imports.iter().any(|m| m.as_str() == root)
                        && !self.global_sub_modules.contains_key(root)
                    {
                        return Err(
                            PERR::ModuleUndefined(root.to_string()).into_err(namespace.position())
                        );
                    }

                    namespace.set_index(index);

                    crate::calc_qualified_fn_hash(namespace.iter().map(Ident::as_str), &id, 0)
                };
                #[cfg(feature = "no_module")]
                let hash = calc_fn_hash(&id, 0);

                let hashes = if is_valid_function_name(&id) {
                    hash.into()
                } else {
                    FnCallHashes::from_native(hash)
                };

                args.shrink_to_fit();

                return Ok(FnCallExpr {
                    name: id,
                    capture_parent_scope,
                    #[cfg(not(feature = "no_module"))]
                    namespace,
                    hashes,
                    args,
                    pos: settings.pos,
                }
                .into_fn_call_expr(settings.pos));
            }
            // id...
            _ => (),
        }

        let settings = settings.level_up();

        loop {
            match input.peek().expect(NEVER_ENDS) {
                // id(...args, ) - handle trailing comma
                (Token::RightParen, ..) => (),
                _ => args.push(self.parse_expr(input, state, lib, settings)?),
            }

            match input.peek().expect(NEVER_ENDS) {
                // id(...args)
                (Token::RightParen, ..) => {
                    eat_token(input, Token::RightParen);

                    #[cfg(not(feature = "no_module"))]
                    let hash = if namespace.is_empty() {
                        calc_fn_hash(&id, args.len())
                    } else {
                        let root = namespace.root();
                        let index = state.find_module(root);

                        #[cfg(not(feature = "no_function"))]
                        #[cfg(not(feature = "no_module"))]
                        let is_global = root == crate::engine::KEYWORD_GLOBAL;
                        #[cfg(any(feature = "no_function", feature = "no_module"))]
                        let is_global = false;

                        if settings.options.contains(LangOptions::STRICT_VAR)
                            && index.is_none()
                            && !is_global
                            && !state.global_imports.iter().any(|m| m.as_str() == root)
                            && !self.global_sub_modules.contains_key(root)
                        {
                            return Err(PERR::ModuleUndefined(root.to_string())
                                .into_err(namespace.position()));
                        }

                        namespace.set_index(index);

                        crate::calc_qualified_fn_hash(
                            namespace.iter().map(Ident::as_str),
                            &id,
                            args.len(),
                        )
                    };
                    #[cfg(feature = "no_module")]
                    let hash = calc_fn_hash(&id, args.len());

                    let hashes = if is_valid_function_name(&id) {
                        hash.into()
                    } else {
                        FnCallHashes::from_native(hash)
                    };

                    args.shrink_to_fit();

                    return Ok(FnCallExpr {
                        name: state.get_interned_string(id),
                        capture_parent_scope,
                        #[cfg(not(feature = "no_module"))]
                        namespace,
                        hashes,
                        args,
                        pos: settings.pos,
                    }
                    .into_fn_call_expr(settings.pos));
                }
                // id(...args,
                (Token::Comma, ..) => {
                    eat_token(input, Token::Comma);
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
        lhs: Expr,
        options: ASTFlags,
        check_index_type: bool,
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;

        let idx_expr = self.parse_expr(input, state, lib, settings.level_up())?;

        // Check types of indexing that cannot be overridden
        // - arrays, maps, strings, bit-fields
        match lhs {
            _ if !check_index_type => (),

            Expr::Map(..) => match idx_expr {
                // lhs[int]
                Expr::IntegerConstant(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not a number".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }

                // lhs[string]
                Expr::StringConstant(..) | Expr::InterpolatedString(..) => (),

                // lhs[float]
                #[cfg(not(feature = "no_float"))]
                Expr::FloatConstant(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not a float".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                // lhs[char]
                Expr::CharConstant(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not a character".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                // lhs[()]
                Expr::Unit(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not ()".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                // lhs[??? && ???], lhs[??? || ???], lhs[true], lhs[false]
                Expr::And(..) | Expr::Or(..) | Expr::BoolConstant(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Object map expects string index, not a boolean".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                _ => (),
            },

            Expr::IntegerConstant(..)
            | Expr::Array(..)
            | Expr::StringConstant(..)
            | Expr::InterpolatedString(..) => match idx_expr {
                // lhs[int]
                Expr::IntegerConstant(..) => (),

                // lhs[string]
                Expr::StringConstant(..) | Expr::InterpolatedString(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects numeric index, not a string".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                // lhs[float]
                #[cfg(not(feature = "no_float"))]
                Expr::FloatConstant(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects integer index, not a float".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                // lhs[char]
                Expr::CharConstant(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects integer index, not a character".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                // lhs[()]
                Expr::Unit(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects integer index, not ()".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                // lhs[??? && ???], lhs[??? || ???], lhs[true], lhs[false]
                Expr::And(..) | Expr::Or(..) | Expr::BoolConstant(..) => {
                    return Err(PERR::MalformedIndexExpr(
                        "Array, string or bit-field expects integer index, not a boolean".into(),
                    )
                    .into_err(idx_expr.start_position()))
                }
                _ => (),
            },
            _ => (),
        }

        // Check if there is a closing bracket
        match input.peek().expect(NEVER_ENDS) {
            (Token::RightBracket, ..) => {
                eat_token(input, Token::RightBracket);

                // Any more indexing following?
                match input.peek().expect(NEVER_ENDS) {
                    // If another indexing level, right-bind it
                    (Token::LeftBracket | Token::QuestionBracket, ..) => {
                        let (token, pos) = input.next().expect(NEVER_ENDS);
                        let prev_pos = settings.pos;
                        settings.pos = pos;
                        // Recursively parse the indexing chain, right-binding each
                        let idx_expr = self.parse_index_chain(
                            input,
                            state,
                            lib,
                            idx_expr,
                            match token {
                                Token::LeftBracket => ASTFlags::NONE,
                                Token::QuestionBracket => ASTFlags::NEGATED,
                                _ => unreachable!("`[` or `?[`"),
                            },
                            false,
                            settings.level_up(),
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
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // [ ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::LeftBracket);

        let mut arr = StaticVec::new_const();

        loop {
            const MISSING_RBRACKET: &str = "to end this array literal";

            #[cfg(not(feature = "unchecked"))]
            if self.max_array_size() > 0 && arr.len() >= self.max_array_size() {
                return Err(PERR::LiteralTooLarge(
                    "Size of array literal".to_string(),
                    self.max_array_size(),
                )
                .into_err(input.peek().expect(NEVER_ENDS).1));
            }

            match input.peek().expect(NEVER_ENDS) {
                (Token::RightBracket, ..) => {
                    eat_token(input, Token::RightBracket);
                    break;
                }
                (Token::EOF, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightBracket.into(),
                        MISSING_RBRACKET.into(),
                    )
                    .into_err(*pos))
                }
                _ => {
                    let expr = self.parse_expr(input, state, lib, settings.level_up())?;
                    arr.push(expr);
                }
            }

            match input.peek().expect(NEVER_ENDS) {
                (Token::Comma, ..) => {
                    eat_token(input, Token::Comma);
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

        arr.shrink_to_fit();

        Ok(Expr::Array(arr.into(), settings.pos))
    }

    /// Parse a map literal.
    #[cfg(not(feature = "no_object"))]
    fn parse_map_literal(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // #{ ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::MapStart);

        let mut map = StaticVec::<(Ident, Expr)>::new();
        let mut template = BTreeMap::<Identifier, crate::Dynamic>::new();

        loop {
            const MISSING_RBRACE: &str = "to end this object map literal";

            match input.peek().expect(NEVER_ENDS) {
                (Token::RightBrace, ..) => {
                    eat_token(input, Token::RightBrace);
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

            let (name, pos) = match input.next().expect(NEVER_ENDS) {
                (Token::Identifier(s) | Token::StringConstant(s), pos) => {
                    if map.iter().any(|(p, ..)| **p == s) {
                        return Err(PERR::DuplicatedProperty(s.to_string()).into_err(pos));
                    }
                    (s, pos)
                }
                (Token::InterpolatedString(..), pos) => {
                    return Err(PERR::PropertyExpected.into_err(pos))
                }
                (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
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

            match input.next().expect(NEVER_ENDS) {
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
                    "Number of properties in object map literal".to_string(),
                    self.max_map_size(),
                )
                .into_err(input.peek().expect(NEVER_ENDS).1));
            }

            let expr = self.parse_expr(input, state, lib, settings.level_up())?;
            template.insert(name.clone(), crate::Dynamic::UNIT);

            let name = state.get_interned_string(name);
            map.push((Ident { name, pos }, expr));

            match input.peek().expect(NEVER_ENDS) {
                (Token::Comma, ..) => {
                    eat_token(input, Token::Comma);
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
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // switch ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::Switch);

        let item = self.parse_expr(input, state, lib, settings.level_up())?;

        match input.next().expect(NEVER_ENDS) {
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

        let mut expressions = StaticVec::<ConditionalExpr>::new();
        let mut cases = BTreeMap::<u64, CaseBlocksList>::new();
        let mut ranges = StaticVec::<RangeCase>::new();
        let mut def_case = None;
        let mut def_case_pos = Position::NONE;

        loop {
            const MISSING_RBRACE: &str = "to end this switch block";

            let (case_expr_list, condition) = match input.peek().expect(NEVER_ENDS) {
                (Token::RightBrace, ..) => {
                    eat_token(input, Token::RightBrace);
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
                    eat_token(input, Token::Underscore);

                    let (if_clause, if_pos) = match_token(input, Token::If);

                    if if_clause {
                        return Err(PERR::WrongSwitchCaseCondition.into_err(if_pos));
                    }

                    (
                        StaticVec::default(),
                        Expr::BoolConstant(true, Position::NONE),
                    )
                }
                _ if def_case.is_some() => {
                    return Err(PERR::WrongSwitchDefaultCase.into_err(def_case_pos))
                }

                _ => {
                    let mut case_expr_list = StaticVec::new();

                    loop {
                        let filter = state.expr_filter;
                        state.expr_filter = |t| t != &Token::Pipe;
                        let expr = self.parse_expr(input, state, lib, settings.level_up());
                        state.expr_filter = filter;

                        match expr {
                            Ok(expr) => case_expr_list.push(expr),
                            Err(err) => {
                                return Err(PERR::ExprExpected("literal".into()).into_err(err.1))
                            }
                        }

                        if !match_token(input, Token::Pipe).0 {
                            break;
                        }
                    }

                    let condition = if match_token(input, Token::If).0 {
                        ensure_not_statement_expr(input, "a boolean")?;
                        let guard = self
                            .parse_expr(input, state, lib, settings.level_up())?
                            .ensure_bool_expr()?;
                        ensure_not_assignment(input)?;
                        guard
                    } else {
                        Expr::BoolConstant(true, Position::NONE)
                    };
                    (case_expr_list, condition)
                }
            };

            match input.next().expect(NEVER_ENDS) {
                (Token::DoubleArrow, ..) => (),
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (.., pos) => {
                    return Err(PERR::MissingToken(
                        Token::DoubleArrow.into(),
                        "in this switch case".to_string(),
                    )
                    .into_err(pos))
                }
            };

            let stmt = self.parse_stmt(input, state, lib, settings.level_up())?;
            let need_comma = !stmt.is_self_terminated();
            let has_condition = !matches!(condition, Expr::BoolConstant(true, ..));

            let stmt_block: StmtBlock = stmt.into();
            expressions.push((condition, Expr::Stmt(stmt_block.into())).into());
            let index = expressions.len() - 1;

            if case_expr_list.is_empty() {
                def_case = Some(index);
            } else {
                for expr in case_expr_list {
                    let value = expr.get_literal_value().ok_or_else(|| {
                        PERR::ExprExpected("a literal".to_string()).into_err(expr.start_position())
                    })?;

                    let mut range_value: Option<RangeCase> = None;

                    let guard = value.read_lock::<ExclusiveRange>();
                    if let Some(range) = guard {
                        range_value = Some(range.clone().into());
                    } else if let Some(range) = value.read_lock::<InclusiveRange>() {
                        range_value = Some(range.clone().into());
                    }

                    if let Some(mut r) = range_value {
                        if !r.is_empty() {
                            // Do not unroll ranges if there are previous non-unrolled ranges
                            if !has_condition && ranges.is_empty() && r.len() <= SMALL_SWITCH_RANGE
                            {
                                // Unroll small range
                                for n in r {
                                    let hasher = &mut get_hasher();
                                    Dynamic::from_int(n).hash(hasher);
                                    cases
                                        .entry(hasher.finish())
                                        .and_modify(|cases| cases.push(index))
                                        .or_insert_with(|| [index].into());
                                }
                            } else {
                                // Other range
                                r.set_index(index);
                                ranges.push(r);
                            }
                        }
                        continue;
                    }

                    if value.is::<INT>() && !ranges.is_empty() {
                        return Err(PERR::WrongSwitchIntegerCase.into_err(expr.start_position()));
                    }

                    let hasher = &mut get_hasher();
                    value.hash(hasher);
                    let hash = hasher.finish();

                    cases
                        .entry(hash)
                        .and_modify(|cases| cases.push(index))
                        .or_insert_with(|| [index].into());
                }
            }

            match input.peek().expect(NEVER_ENDS) {
                (Token::Comma, ..) => {
                    eat_token(input, Token::Comma);
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

        let cases = SwitchCasesCollection {
            expressions,
            cases,
            def_case,
            ranges,
        };

        Ok(Stmt::Switch((item, cases).into(), settings.pos))
    }

    /// Parse a primary expression.
    fn parse_primary(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        is_property: bool,
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let (token, token_pos) = input.peek().expect(NEVER_ENDS);

        let mut settings = settings;
        settings.pos = *token_pos;

        let root_expr = match token {
            _ if !(state.expr_filter)(token) => {
                return Err(
                    LexError::UnexpectedInput(token.syntax().to_string()).into_err(settings.pos)
                )
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
            | Token::False => match input.next().expect(NEVER_ENDS).0 {
                Token::IntegerConstant(x) => Expr::IntegerConstant(x, settings.pos),
                Token::CharConstant(c) => Expr::CharConstant(c, settings.pos),
                Token::StringConstant(s) => {
                    Expr::StringConstant(state.get_interned_string(s), settings.pos)
                }
                Token::True => Expr::BoolConstant(true, settings.pos),
                Token::False => Expr::BoolConstant(false, settings.pos),
                token => unreachable!("token is {:?}", token),
            },
            #[cfg(not(feature = "no_float"))]
            Token::FloatConstant(x) => {
                let x = *x;
                input.next();
                Expr::FloatConstant(x, settings.pos)
            }
            #[cfg(feature = "decimal")]
            Token::DecimalConstant(x) => {
                let x = (*x).into();
                input.next();
                Expr::DynamicConstant(Box::new(x), settings.pos)
            }

            // { - block statement as expression
            Token::LeftBrace if settings.options.contains(LangOptions::STMT_EXPR) => {
                match self.parse_block(input, state, lib, settings.level_up())? {
                    block @ Stmt::Block(..) => Expr::Stmt(Box::new(block.into())),
                    stmt => unreachable!("Stmt::Block expected but gets {:?}", stmt),
                }
            }

            // ( - grouped expression
            Token::LeftParen => self.parse_paren_expr(input, state, lib, settings.level_up())?,

            // If statement is allowed to act as expressions
            Token::If if settings.options.contains(LangOptions::IF_EXPR) => Expr::Stmt(Box::new(
                self.parse_if(input, state, lib, settings.level_up())?
                    .into(),
            )),
            // Switch statement is allowed to act as expressions
            Token::Switch if settings.options.contains(LangOptions::SWITCH_EXPR) => {
                Expr::Stmt(Box::new(
                    self.parse_switch(input, state, lib, settings.level_up())?
                        .into(),
                ))
            }

            // | ...
            #[cfg(not(feature = "no_function"))]
            Token::Pipe | Token::Or if settings.options.contains(LangOptions::ANON_FN) => {
                // Build new parse state
                let interned_strings = std::mem::take(&mut state.interned_strings);

                let mut new_state = ParseState::new(
                    self,
                    state.scope,
                    interned_strings,
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
                    new_state
                        .global_imports
                        .extend(state.imports.iter().cloned());
                }

                #[cfg(not(feature = "unchecked"))]
                {
                    new_state.max_expr_depth = self.max_function_expr_depth();
                }

                let mut options = self.options;
                options.set(
                    LangOptions::STRICT_VAR,
                    if cfg!(feature = "no_closure") {
                        settings.options.contains(LangOptions::STRICT_VAR)
                    } else {
                        // A capturing closure can access variables not defined locally
                        false
                    },
                );

                let new_settings = ParseSettings {
                    at_global_level: false,
                    in_fn_scope: true,
                    #[cfg(not(feature = "no_closure"))]
                    in_closure: true,
                    is_breakable: false,
                    level: 0,
                    options,
                    ..settings
                };

                let result = self.parse_anon_fn(input, &mut new_state, lib, new_settings);

                // Restore parse state
                state.interned_strings = new_state.interned_strings;

                let (expr, func) = result?;

                #[cfg(not(feature = "no_closure"))]
                new_state.external_vars.iter().try_for_each(
                    |crate::ast::Ident { name, pos }| {
                        let (index, is_func) = state.access_var(name, lib, *pos);

                        if !is_func
                            && index.is_none()
                            && !settings.in_closure
                            && settings.options.contains(LangOptions::STRICT_VAR)
                            && !state.scope.contains(name)
                        {
                            // If the parent scope is not inside another capturing closure
                            // then we can conclude that the captured variable doesn't exist.
                            // Under Strict Variables mode, this is not allowed.
                            Err(PERR::VariableUndefined(name.to_string()).into_err(*pos))
                        } else {
                            Ok::<_, ParseError>(())
                        }
                    },
                )?;

                let hash_script = calc_fn_hash(&func.name, func.params.len());
                lib.insert(hash_script, func.into());

                expr
            }

            // Interpolated string
            Token::InterpolatedString(..) => {
                let mut segments = StaticVec::<Expr>::new();

                match input.next().expect(NEVER_ENDS) {
                    (Token::InterpolatedString(s), ..) if s.is_empty() => (),
                    (Token::InterpolatedString(s), pos) => {
                        segments.push(Expr::StringConstant(s.into(), pos))
                    }
                    token => {
                        unreachable!("Token::InterpolatedString expected but gets {:?}", token)
                    }
                }

                loop {
                    let expr = match self.parse_block(input, state, lib, settings.level_up())? {
                        block @ Stmt::Block(..) => Expr::Stmt(Box::new(block.into())),
                        stmt => unreachable!("Stmt::Block expected but gets {:?}", stmt),
                    };
                    match expr {
                        Expr::StringConstant(s, ..) if s.is_empty() => (),
                        _ => segments.push(expr),
                    }

                    // Make sure to parse the following as text
                    state.tokenizer_control.borrow_mut().is_within_text = true;

                    match input.next().expect(NEVER_ENDS) {
                        (Token::StringConstant(s), pos) => {
                            if !s.is_empty() {
                                segments.push(Expr::StringConstant(s.into(), pos));
                            }
                            // End the interpolated string if it is terminated by a back-tick.
                            break;
                        }
                        (Token::InterpolatedString(s), pos) => {
                            if !s.is_empty() {
                                segments.push(Expr::StringConstant(s.into(), pos));
                            }
                        }
                        (Token::LexError(err), pos)
                            if matches!(*err, LexError::UnterminatedString) =>
                        {
                            return Err(err.into_err(pos))
                        }
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
                    Expr::InterpolatedString(segments.into(), settings.pos)
                }
            }

            // Array literal
            #[cfg(not(feature = "no_index"))]
            Token::LeftBracket => {
                self.parse_array_literal(input, state, lib, settings.level_up())?
            }

            // Map literal
            #[cfg(not(feature = "no_object"))]
            Token::MapStart => self.parse_map_literal(input, state, lib, settings.level_up())?,

            // Custom syntax.
            #[cfg(not(feature = "no_custom_syntax"))]
            Token::Custom(key) | Token::Reserved(key) | Token::Identifier(key)
                if !self.custom_syntax.is_empty() && self.custom_syntax.contains_key(&**key) =>
            {
                let (key, syntax) = self.custom_syntax.get_key_value(&**key).unwrap();
                let (.., pos) = input.next().expect(NEVER_ENDS);
                let settings2 = settings.level_up();
                self.parse_custom_syntax(input, state, lib, settings2, key, syntax, pos)?
            }

            // Identifier
            Token::Identifier(..) => {
                #[cfg(not(feature = "no_module"))]
                let ns = crate::ast::Namespace::NONE;
                #[cfg(feature = "no_module")]
                let ns = ();

                let s = match input.next().expect(NEVER_ENDS) {
                    (Token::Identifier(s), ..) => s,
                    token => unreachable!("Token::Identifier expected but gets {:?}", token),
                };

                match input.peek().expect(NEVER_ENDS).0 {
                    // Function call
                    Token::LeftParen | Token::Bang | Token::Unit => {
                        #[cfg(not(feature = "no_closure"))]
                        {
                            // Once the identifier consumed we must enable next variables capturing
                            state.allow_capture = true;
                        }
                        Expr::Variable(
                            (None, ns, 0, state.get_interned_string(s)).into(),
                            None,
                            settings.pos,
                        )
                    }
                    // Namespace qualification
                    #[cfg(not(feature = "no_module"))]
                    Token::DoubleColon => {
                        #[cfg(not(feature = "no_closure"))]
                        {
                            // Once the identifier consumed we must enable next variables capturing
                            state.allow_capture = true;
                        }
                        let name = state.get_interned_string(s);
                        Expr::Variable((None, ns, 0, name).into(), None, settings.pos)
                    }
                    // Normal variable access
                    _ => {
                        let (index, is_func) = state.access_var(&s, lib, settings.pos);

                        if !is_property
                            && !is_func
                            && index.is_none()
                            && settings.options.contains(LangOptions::STRICT_VAR)
                            && !state.scope.contains(&s)
                        {
                            return Err(
                                PERR::VariableUndefined(s.to_string()).into_err(settings.pos)
                            );
                        }

                        let short_index = index.and_then(|x| {
                            if x.get() <= u8::MAX as usize {
                                NonZeroU8::new(x.get() as u8)
                            } else {
                                None
                            }
                        });
                        let name = state.get_interned_string(s);
                        Expr::Variable((index, ns, 0, name).into(), short_index, settings.pos)
                    }
                }
            }

            // Reserved keyword or symbol
            Token::Reserved(..) => {
                #[cfg(not(feature = "no_module"))]
                let ns = crate::ast::Namespace::NONE;
                #[cfg(feature = "no_module")]
                let ns = ();

                let s = match input.next().expect(NEVER_ENDS) {
                    (Token::Reserved(s), ..) => s,
                    token => unreachable!("Token::Reserved expected but gets {:?}", token),
                };

                match input.peek().expect(NEVER_ENDS).0 {
                    // Function call is allowed to have reserved keyword
                    Token::LeftParen | Token::Bang | Token::Unit if is_keyword_function(&s) => {
                        Expr::Variable(
                            (None, ns, 0, state.get_interned_string(s)).into(),
                            None,
                            settings.pos,
                        )
                    }
                    // Access to `this` as a variable is OK within a function scope
                    #[cfg(not(feature = "no_function"))]
                    _ if &*s == KEYWORD_THIS && settings.in_fn_scope => Expr::Variable(
                        (None, ns, 0, state.get_interned_string(s)).into(),
                        None,
                        settings.pos,
                    ),
                    // Cannot access to `this` as a variable not in a function scope
                    _ if &*s == KEYWORD_THIS => {
                        let msg = format!("'{s}' can only be used in functions");
                        return Err(
                            LexError::ImproperSymbol(s.to_string(), msg).into_err(settings.pos)
                        );
                    }
                    _ => return Err(PERR::Reserved(s.to_string()).into_err(settings.pos)),
                }
            }

            Token::LexError(..) => match input.next().expect(NEVER_ENDS) {
                (Token::LexError(err), ..) => return Err(err.into_err(settings.pos)),
                token => unreachable!("Token::LexError expected but gets {:?}", token),
            },

            _ => {
                return Err(
                    LexError::UnexpectedInput(token.syntax().to_string()).into_err(settings.pos)
                )
            }
        };

        if !(state.expr_filter)(&input.peek().expect(NEVER_ENDS).0) {
            return Ok(root_expr);
        }

        self.parse_postfix(input, state, lib, root_expr, settings)
    }

    /// Tail processing of all possible postfix operators of a primary expression.
    fn parse_postfix(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        mut lhs: Expr,
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        let mut settings = settings;

        // Tail processing all possible postfix operators
        loop {
            let (tail_token, ..) = input.peek().expect(NEVER_ENDS);

            if !lhs.is_valid_postfix(tail_token) {
                break;
            }

            let (tail_token, tail_pos) = input.next().expect(NEVER_ENDS);
            settings.pos = tail_pos;

            lhs = match (lhs, tail_token) {
                // Qualified function call with !
                #[cfg(not(feature = "no_module"))]
                (Expr::Variable(x, ..), Token::Bang) if !x.1.is_empty() => {
                    return match input.peek().expect(NEVER_ENDS) {
                        (Token::LeftParen | Token::Unit, ..) => {
                            Err(LexError::UnexpectedInput(Token::Bang.syntax().to_string())
                                .into_err(tail_pos))
                        }
                        _ => Err(LexError::ImproperSymbol(
                            "!".to_string(),
                            "'!' cannot be used to call module functions".to_string(),
                        )
                        .into_err(tail_pos)),
                    };
                }
                // Function call with !
                (Expr::Variable(x, .., pos), Token::Bang) => {
                    match input.peek().expect(NEVER_ENDS) {
                        (Token::LeftParen | Token::Unit, ..) => (),
                        (_, pos) => {
                            return Err(PERR::MissingToken(
                                Token::LeftParen.syntax().into(),
                                "to start arguments list of function call".into(),
                            )
                            .into_err(*pos))
                        }
                    }

                    let no_args = input.next().expect(NEVER_ENDS).0 == Token::Unit;

                    let (.., _ns, _, name) = *x;
                    settings.pos = pos;
                    self.parse_fn_call(
                        input,
                        state,
                        lib,
                        name,
                        no_args,
                        true,
                        #[cfg(not(feature = "no_module"))]
                        _ns,
                        settings.level_up(),
                    )?
                }
                // Function call
                (Expr::Variable(x, .., pos), t @ (Token::LeftParen | Token::Unit)) => {
                    let (.., _ns, _, name) = *x;
                    settings.pos = pos;
                    self.parse_fn_call(
                        input,
                        state,
                        lib,
                        name,
                        t == Token::Unit,
                        false,
                        #[cfg(not(feature = "no_module"))]
                        _ns,
                        settings.level_up(),
                    )?
                }
                // module access
                #[cfg(not(feature = "no_module"))]
                (Expr::Variable(x, .., pos), Token::DoubleColon) => {
                    let (id2, pos2) = parse_var_name(input)?;
                    let (.., mut namespace, _, name) = *x;
                    let var_name_def = Ident { name, pos };

                    namespace.push(var_name_def);

                    Expr::Variable(
                        (None, namespace, 0, state.get_interned_string(id2)).into(),
                        None,
                        pos2,
                    )
                }
                // Indexing
                #[cfg(not(feature = "no_index"))]
                (expr, token @ (Token::LeftBracket | Token::QuestionBracket)) => {
                    let opt = match token {
                        Token::LeftBracket => ASTFlags::NONE,
                        Token::QuestionBracket => ASTFlags::NEGATED,
                        _ => unreachable!("`[` or `?[`"),
                    };
                    self.parse_index_chain(input, state, lib, expr, opt, true, settings.level_up())?
                }
                // Property access
                #[cfg(not(feature = "no_object"))]
                (expr, op @ (Token::Period | Token::Elvis)) => {
                    // Expression after dot must start with an identifier
                    match input.peek().expect(NEVER_ENDS) {
                        (Token::Identifier(..), ..) => {
                            #[cfg(not(feature = "no_closure"))]
                            {
                                // Prevents capturing of the object properties as vars: xxx.<var>
                                state.allow_capture = false;
                            }
                        }
                        (Token::Reserved(s), ..) if is_keyword_function(s) => (),
                        (.., pos) => return Err(PERR::PropertyExpected.into_err(*pos)),
                    }

                    let rhs = self.parse_primary(input, state, lib, true, settings.level_up())?;
                    let op_flags = match op {
                        Token::Period => ASTFlags::NONE,
                        Token::Elvis => ASTFlags::NEGATED,
                        _ => unreachable!("`.` or `?.`"),
                    };
                    Self::make_dot_expr(state, expr, rhs, ASTFlags::NONE, op_flags, tail_pos)?
                }
                // Unknown postfix operator
                (expr, token) => unreachable!(
                    "unknown postfix operator '{}' for {:?}",
                    token.syntax(),
                    expr
                ),
            }
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
                *hash = crate::calc_qualified_var_hash(namespace.iter().map(Ident::as_str), name);

                #[cfg(not(feature = "no_module"))]
                {
                    let root = namespace.root();
                    let index = state.find_module(root);

                    #[cfg(not(feature = "no_function"))]
                    #[cfg(not(feature = "no_module"))]
                    let is_global = root == crate::engine::KEYWORD_GLOBAL;
                    #[cfg(any(feature = "no_function", feature = "no_module"))]
                    let is_global = false;

                    if settings.options.contains(LangOptions::STRICT_VAR)
                        && index.is_none()
                        && !is_global
                        && !state.global_imports.iter().any(|m| m.as_str() == root)
                        && !self.global_sub_modules.contains_key(root)
                    {
                        return Err(
                            PERR::ModuleUndefined(root.to_string()).into_err(namespace.position())
                        );
                    }

                    namespace.set_index(index);
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
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let (token, token_pos) = input.peek().expect(NEVER_ENDS);

        if !(state.expr_filter)(token) {
            return Err(LexError::UnexpectedInput(token.syntax().to_string()).into_err(*token_pos));
        }

        let mut settings = settings;
        settings.pos = *token_pos;

        match token {
            // -expr
            Token::Minus | Token::UnaryMinus => {
                let token = token.clone();
                let pos = eat_token(input, token);

                match self.parse_unary(input, state, lib, settings.level_up())? {
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
                    expr => {
                        let mut args = StaticVec::new_const();
                        args.push(expr);
                        args.shrink_to_fit();

                        Ok(FnCallExpr {
                            name: state.get_interned_string("-"),
                            hashes: FnCallHashes::from_native(calc_fn_hash("-", 1)),
                            args,
                            pos,
                            ..Default::default()
                        }
                        .into_fn_call_expr(pos))
                    }
                }
            }
            // +expr
            Token::Plus | Token::UnaryPlus => {
                let token = token.clone();
                let pos = eat_token(input, token);

                match self.parse_unary(input, state, lib, settings.level_up())? {
                    expr @ Expr::IntegerConstant(..) => Ok(expr),
                    #[cfg(not(feature = "no_float"))]
                    expr @ Expr::FloatConstant(..) => Ok(expr),

                    // Call plus function
                    expr => {
                        let mut args = StaticVec::new_const();
                        args.push(expr);
                        args.shrink_to_fit();

                        Ok(FnCallExpr {
                            name: state.get_interned_string("+"),
                            hashes: FnCallHashes::from_native(calc_fn_hash("+", 1)),
                            args,
                            pos,
                            ..Default::default()
                        }
                        .into_fn_call_expr(pos))
                    }
                }
            }
            // !expr
            Token::Bang => {
                let pos = eat_token(input, Token::Bang);
                let mut args = StaticVec::new_const();
                args.push(self.parse_unary(input, state, lib, settings.level_up())?);
                args.shrink_to_fit();

                Ok(FnCallExpr {
                    name: state.get_interned_string("!"),
                    hashes: FnCallHashes::from_native(calc_fn_hash("!", 1)),
                    args,
                    pos,
                    ..Default::default()
                }
                .into_fn_call_expr(pos))
            }
            // <EOF>
            Token::EOF => Err(PERR::UnexpectedEOF.into_err(settings.pos)),
            // All other tokens
            _ => self.parse_primary(input, state, lib, false, settings.level_up()),
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
                        Expr::Property(..) if !options.contains(ASTFlags::BREAK) => {
                            check_lvalue(&x.rhs, matches!(expr, Expr::Dot(..)))
                        }
                        Expr::Property(..) => None,
                        // Anything other than a property after dotting (e.g. a method call) is not an l-value
                        ref e => Some(e.position()),
                    }
                }
                Expr::Index(x, options, ..) | Expr::Dot(x, options, ..) => match x.lhs {
                    Expr::Property(..) => unreachable!("unexpected Expr::Property in indexing"),
                    _ if !options.contains(ASTFlags::BREAK) => {
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

        let op_info = if let Some(op) = op {
            OpAssignment::new_op_assignment_from_token(op, op_pos)
        } else {
            OpAssignment::new_assignment(op_pos)
        };

        match lhs {
            // const_expr = rhs
            ref expr if expr.is_constant() => {
                Err(PERR::AssignmentToConstant("".into()).into_err(lhs.start_position()))
            }
            // var (non-indexed) = rhs
            Expr::Variable(ref x, None, _) if x.0.is_none() => {
                Ok(Stmt::Assignment((op_info, (lhs, rhs).into()).into()))
            }
            // var (indexed) = rhs
            Expr::Variable(ref x, i, var_pos) => {
                let (index, .., name) = &**x;
                let index = i.map_or_else(
                    || index.expect("either long or short index is `None`").get(),
                    |n| n.get() as usize,
                );
                match state
                    .stack
                    .get_mut_by_index(state.stack.len() - index)
                    .access_mode()
                {
                    AccessMode::ReadWrite => {
                        Ok(Stmt::Assignment((op_info, (lhs, rhs).into()).into()))
                    }
                    // Constant values cannot be assigned to
                    AccessMode::ReadOnly => {
                        Err(PERR::AssignmentToConstant(name.to_string()).into_err(var_pos))
                    }
                }
            }
            // xxx[???]... = rhs, xxx.prop... = rhs
            Expr::Index(ref x, options, ..) | Expr::Dot(ref x, options, ..) => {
                let valid_lvalue = if options.contains(ASTFlags::BREAK) {
                    None
                } else {
                    check_lvalue(&x.rhs, matches!(lhs, Expr::Dot(..)))
                };

                match valid_lvalue {
                    None => {
                        match x.lhs {
                            // var[???] = rhs, var.??? = rhs
                            Expr::Variable(..) => {
                                Ok(Stmt::Assignment((op_info, (lhs, rhs).into()).into()))
                            }
                            // expr[???] = rhs, expr.??? = rhs
                            ref expr => Err(PERR::AssignmentToInvalidLHS("".to_string())
                                .into_err(expr.position())),
                        }
                    }
                    Some(err_pos) => {
                        Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(err_pos))
                    }
                }
            }
            // ??? && ??? = rhs, ??? || ??? = rhs, xxx ?? xxx = rhs
            Expr::And(..) | Expr::Or(..) | Expr::Coalesce(..) => Err(LexError::ImproperSymbol(
                "=".to_string(),
                "Possibly a typo of '=='?".to_string(),
            )
            .into_err(op_pos)),
            // expr = rhs
            _ => Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(lhs.position())),
        }
    }

    /// Parse an operator-assignment expression (if any).
    fn parse_op_assignment_stmt(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        lhs: Expr,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let (op, pos) = match input.peek().expect(NEVER_ENDS) {
            // var = ...
            (Token::Equals, ..) => (None, eat_token(input, Token::Equals)),
            // var op= ...
            (token, ..) if token.is_op_assignment() => input
                .next()
                .map(|(op, pos)| (Some(op), pos))
                .expect(NEVER_ENDS),
            // Not op-assignment
            _ => return Ok(Stmt::Expr(lhs.into())),
        };

        let mut settings = settings;
        settings.pos = pos;

        let rhs = self.parse_expr(input, state, lib, settings.level_up())?;
        Self::make_assignment_stmt(op, state, lhs, rhs, pos)
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
            // lhs[idx_expr].rhs
            (Expr::Index(mut x, options, pos), rhs) => {
                x.rhs = Self::make_dot_expr(
                    state,
                    x.rhs,
                    rhs,
                    options | parent_options,
                    op_flags,
                    op_pos,
                )?;
                Ok(Expr::Index(x, ASTFlags::NONE, pos))
            }
            // lhs.module::id - syntax error
            #[cfg(not(feature = "no_module"))]
            (.., Expr::Variable(x, ..)) if !x.1.is_empty() => {
                Err(PERR::PropertyExpected.into_err(x.1.position()))
            }
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
            (.., Expr::FnCall(func, ..)) if func.is_qualified() => {
                Err(PERR::PropertyExpected.into_err(func.namespace.position()))
            }
            // lhs.Fn() or lhs.eval()
            (.., Expr::FnCall(func, func_pos))
                if func.args.is_empty()
                    && [crate::engine::KEYWORD_FN_PTR, crate::engine::KEYWORD_EVAL]
                        .contains(&func.name.as_str()) =>
            {
                let err_msg = format!(
                    "'{}' should not be called in method style. Try {}(...);",
                    func.name, func.name
                );
                Err(LexError::ImproperSymbol(func.name.to_string(), err_msg).into_err(func_pos))
            }
            // lhs.func!(...)
            (.., Expr::FnCall(func, func_pos)) if func.capture_parent_scope => {
                Err(PERR::MalformedCapture(
                    "method-call style does not support running within the caller's scope".into(),
                )
                .into_err(func_pos))
            }
            // lhs.func(...)
            (lhs, Expr::FnCall(mut func, func_pos)) => {
                // Recalculate hash
                func.hashes = FnCallHashes::from_all(
                    #[cfg(not(feature = "no_function"))]
                    calc_fn_hash(&func.name, func.args.len()),
                    calc_fn_hash(&func.name, func.args.len() + 1),
                );

                let rhs = Expr::MethodCall(func, func_pos);
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
                    Expr::Variable(x, ..) if !x.1.is_empty() => {
                        Err(PERR::PropertyExpected.into_err(x.1.position()))
                    }
                    // lhs.module::func().dot_rhs or lhs.module::func()[idx_rhs] - syntax error
                    #[cfg(not(feature = "no_module"))]
                    Expr::FnCall(func, ..) if func.is_qualified() => {
                        Err(PERR::PropertyExpected.into_err(func.namespace.position()))
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
                    Expr::FnCall(mut func, func_pos) => {
                        // Recalculate hash
                        func.hashes = FnCallHashes::from_all(
                            #[cfg(not(feature = "no_function"))]
                            calc_fn_hash(&func.name, func.args.len()),
                            calc_fn_hash(&func.name, func.args.len() + 1),
                        );

                        let new_lhs = BinaryExpr {
                            lhs: Expr::MethodCall(func, func_pos),
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
        parent_precedence: Option<Precedence>,
        lhs: Expr,
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;
        settings.pos = lhs.position();

        let mut root = lhs;

        loop {
            let (current_op, current_pos) = input.peek().expect(NEVER_ENDS);

            if !(state.expr_filter)(current_op) {
                return Ok(root);
            }

            let precedence = match current_op {
                #[cfg(not(feature = "no_custom_syntax"))]
                Token::Custom(c) => self
                    .custom_keywords
                    .get(c)
                    .copied()
                    .ok_or_else(|| PERR::Reserved(c.to_string()).into_err(*current_pos))?,
                Token::Reserved(c) if !is_valid_identifier(c.chars()) => {
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

            let (op_token, pos) = input.next().expect(NEVER_ENDS);

            let rhs = self.parse_unary(input, state, lib, settings)?;

            let (next_op, next_pos) = input.peek().expect(NEVER_ENDS);
            let next_precedence = match next_op {
                #[cfg(not(feature = "no_custom_syntax"))]
                Token::Custom(c) => self
                    .custom_keywords
                    .get(c)
                    .copied()
                    .ok_or_else(|| PERR::Reserved(c.to_string()).into_err(*next_pos))?,
                Token::Reserved(c) if !is_valid_identifier(c.chars()) => {
                    return Err(PERR::UnknownOperator(c.to_string()).into_err(*next_pos))
                }
                _ => next_op.precedence(),
            };

            // Bind to right if the next operator has higher precedence
            // If same precedence, then check if the operator binds right
            let rhs =
                if (precedence == next_precedence && bind_right) || precedence < next_precedence {
                    self.parse_binary_op(input, state, lib, precedence, rhs, settings)?
                } else {
                    // Otherwise bind to left (even if next operator has the same precedence)
                    rhs
                };

            settings = settings.level_up();
            settings.pos = pos;

            #[cfg(not(feature = "unchecked"))]
            settings.ensure_level_within_max_limit(state.max_expr_depth)?;

            let op = op_token.syntax();
            let hash = calc_fn_hash(&op, 2);

            let op_base = FnCallExpr {
                name: state.get_interned_string(op.as_ref()),
                hashes: FnCallHashes::from_native(hash),
                pos,
                ..Default::default()
            };

            let mut args = StaticVec::new_const();
            args.push(root);
            args.push(rhs);
            args.shrink_to_fit();

            root = match op_token {
                // '!=' defaults to true when passed invalid operands
                Token::NotEqualsTo => FnCallExpr { args, ..op_base }.into_fn_call_expr(pos),

                // Comparison operators default to false when passed invalid operands
                Token::EqualsTo
                | Token::LessThan
                | Token::LessThanEqualsTo
                | Token::GreaterThan
                | Token::GreaterThanEqualsTo => {
                    let pos = args[0].start_position();
                    FnCallExpr { args, ..op_base }.into_fn_call_expr(pos)
                }

                Token::Or => {
                    let rhs = args.pop().unwrap();
                    let current_lhs = args.pop().unwrap();
                    Expr::Or(
                        BinaryExpr {
                            lhs: current_lhs.ensure_bool_expr()?,
                            rhs: rhs.ensure_bool_expr()?,
                        }
                        .into(),
                        pos,
                    )
                }
                Token::And => {
                    let rhs = args.pop().unwrap();
                    let current_lhs = args.pop().unwrap();
                    Expr::And(
                        BinaryExpr {
                            lhs: current_lhs.ensure_bool_expr()?,
                            rhs: rhs.ensure_bool_expr()?,
                        }
                        .into(),
                        pos,
                    )
                }
                Token::DoubleQuestion => {
                    let rhs = args.pop().unwrap();
                    let current_lhs = args.pop().unwrap();
                    Expr::Coalesce(
                        BinaryExpr {
                            lhs: current_lhs,
                            rhs,
                        }
                        .into(),
                        pos,
                    )
                }
                Token::In => {
                    // Swap the arguments
                    let current_lhs = args.remove(0);
                    let pos = current_lhs.start_position();
                    args.push(current_lhs);
                    args.shrink_to_fit();

                    // Convert into a call to `contains`
                    FnCallExpr {
                        hashes: calc_fn_hash(OP_CONTAINS, 2).into(),
                        args,
                        name: state.get_interned_string(OP_CONTAINS),
                        ..op_base
                    }
                    .into_fn_call_expr(pos)
                }

                #[cfg(not(feature = "no_custom_syntax"))]
                Token::Custom(s)
                    if self
                        .custom_keywords
                        .get(s.as_str())
                        .map_or(false, Option::is_some) =>
                {
                    let hash = calc_fn_hash(&s, 2);
                    let pos = args[0].start_position();

                    FnCallExpr {
                        hashes: if is_valid_function_name(&s) {
                            hash.into()
                        } else {
                            FnCallHashes::from_native(hash)
                        },
                        args,
                        ..op_base
                    }
                    .into_fn_call_expr(pos)
                }

                _ => {
                    let pos = args[0].start_position();
                    FnCallExpr { args, ..op_base }.into_fn_call_expr(pos)
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
        settings: ParseSettings,
        key: impl Into<ImmutableString>,
        syntax: &crate::api::custom_syntax::CustomSyntax,
        pos: Position,
    ) -> ParseResult<Expr> {
        use crate::api::custom_syntax::markers::*;

        let mut settings = settings;
        let mut inputs = StaticVec::<Expr>::new();
        let mut segments = StaticVec::new_const();
        let mut tokens = StaticVec::new_const();

        // Adjust the variables stack
        if syntax.scope_may_be_changed {
            // Add a barrier variable to the stack so earlier variables will not be matched.
            // Variable searches stop at the first barrier.
            let marker = state.get_interned_string(SCOPE_SEARCH_BARRIER_MARKER);
            state.stack.push(marker, ());
        }

        let parse_func = &*syntax.parse;
        let mut required_token: ImmutableString = key.into();

        tokens.push(required_token.clone());
        segments.push(required_token.clone());

        loop {
            let (fwd_token, fwd_pos) = input.peek().expect(NEVER_ENDS);
            settings.pos = *fwd_pos;
            let settings = settings.level_up();

            required_token = match parse_func(&segments, &*fwd_token.syntax()) {
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

                    #[cfg(not(feature = "no_module"))]
                    let ns = crate::ast::Namespace::NONE;
                    #[cfg(feature = "no_module")]
                    let ns = ();

                    segments.push(name.clone().into());
                    tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_IDENT));
                    inputs.push(Expr::Variable((None, ns, 0, name).into(), None, pos));
                }
                CUSTOM_SYNTAX_MARKER_SYMBOL => {
                    let (symbol, pos) = parse_symbol(input)?;
                    let symbol = state.get_interned_string(symbol);
                    segments.push(symbol.clone());
                    tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_SYMBOL));
                    inputs.push(Expr::StringConstant(symbol, pos));
                }
                CUSTOM_SYNTAX_MARKER_EXPR => {
                    inputs.push(self.parse_expr(input, state, lib, settings)?);
                    let keyword = state.get_interned_string(CUSTOM_SYNTAX_MARKER_EXPR);
                    segments.push(keyword.clone().into());
                    tokens.push(keyword);
                }
                CUSTOM_SYNTAX_MARKER_BLOCK => {
                    match self.parse_block(input, state, lib, settings)? {
                        block @ Stmt::Block(..) => {
                            inputs.push(Expr::Stmt(Box::new(block.into())));
                            let keyword = state.get_interned_string(CUSTOM_SYNTAX_MARKER_BLOCK);
                            segments.push(keyword.clone().into());
                            tokens.push(keyword);
                        }
                        stmt => unreachable!("Stmt::Block expected but gets {:?}", stmt),
                    }
                }
                CUSTOM_SYNTAX_MARKER_BOOL => match input.next().expect(NEVER_ENDS) {
                    (b @ (Token::True | Token::False), pos) => {
                        inputs.push(Expr::BoolConstant(b == Token::True, pos));
                        segments.push(state.get_interned_string(b.literal_syntax()));
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_BOOL));
                    }
                    (.., pos) => {
                        return Err(
                            PERR::MissingSymbol("Expecting 'true' or 'false'".to_string())
                                .into_err(pos),
                        )
                    }
                },
                CUSTOM_SYNTAX_MARKER_INT => match input.next().expect(NEVER_ENDS) {
                    (Token::IntegerConstant(i), pos) => {
                        inputs.push(Expr::IntegerConstant(i, pos));
                        segments.push(i.to_string().into());
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_INT));
                    }
                    (.., pos) => {
                        return Err(
                            PERR::MissingSymbol("Expecting an integer number".to_string())
                                .into_err(pos),
                        )
                    }
                },
                #[cfg(not(feature = "no_float"))]
                CUSTOM_SYNTAX_MARKER_FLOAT => match input.next().expect(NEVER_ENDS) {
                    (Token::FloatConstant(f), pos) => {
                        inputs.push(Expr::FloatConstant(f, pos));
                        segments.push(f.to_string().into());
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_FLOAT));
                    }
                    (.., pos) => {
                        return Err(PERR::MissingSymbol(
                            "Expecting a floating-point number".to_string(),
                        )
                        .into_err(pos))
                    }
                },
                CUSTOM_SYNTAX_MARKER_STRING => match input.next().expect(NEVER_ENDS) {
                    (Token::StringConstant(s), pos) => {
                        let s = state.get_interned_string(s);
                        inputs.push(Expr::StringConstant(s.clone(), pos));
                        segments.push(s);
                        tokens.push(state.get_interned_string(CUSTOM_SYNTAX_MARKER_STRING));
                    }
                    (.., pos) => {
                        return Err(
                            PERR::MissingSymbol("Expecting a string".to_string()).into_err(pos)
                        )
                    }
                },
                s => match input.next().expect(NEVER_ENDS) {
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (t, ..) if &*t.syntax() == s => {
                        segments.push(required_token.clone());
                        tokens.push(required_token.clone().into());
                    }
                    (.., pos) => {
                        return Err(PERR::MissingToken(
                            s.to_string(),
                            format!("for '{}' expression", segments[0]),
                        )
                        .into_err(pos))
                    }
                },
            }
        }

        inputs.shrink_to_fit();
        tokens.shrink_to_fit();

        const KEYWORD_SEMICOLON: &str = Token::SemiColon.literal_syntax();
        const KEYWORD_CLOSE_BRACE: &str = Token::RightBrace.literal_syntax();

        let self_terminated = matches!(
            required_token.as_str(),
            // It is self-terminating if the last symbol is a block
            CUSTOM_SYNTAX_MARKER_BLOCK |
            // If the last symbol is `;` or `}`, it is self-terminating
            KEYWORD_SEMICOLON | KEYWORD_CLOSE_BRACE
        );

        Ok(Expr::Custom(
            crate::ast::CustomExpr {
                inputs,
                tokens,
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
        settings: ParseSettings,
    ) -> ParseResult<Expr> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;
        settings.pos = input.peek().expect(NEVER_ENDS).1;

        // Parse expression normally.
        let precedence = Precedence::new(1);
        let lhs = self.parse_unary(input, state, lib, settings.level_up())?;
        self.parse_binary_op(input, state, lib, precedence, lhs, settings.level_up())
    }

    /// Parse an if statement.
    fn parse_if(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // if ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::If);

        // if guard { if_body }
        ensure_not_statement_expr(input, "a boolean")?;
        let guard = self
            .parse_expr(input, state, lib, settings.level_up())?
            .ensure_bool_expr()?;
        ensure_not_assignment(input)?;
        let if_body = self.parse_block(input, state, lib, settings.level_up())?;

        // if guard { if_body } else ...
        let else_body = if match_token(input, Token::Else).0 {
            if let (Token::If, ..) = input.peek().expect(NEVER_ENDS) {
                // if guard { if_body } else if ...
                self.parse_if(input, state, lib, settings.level_up())?
            } else {
                // if guard { if_body } else { else-body }
                self.parse_block(input, state, lib, settings.level_up())?
            }
        } else {
            Stmt::Noop(Position::NONE)
        };

        Ok(Stmt::If(
            (guard, if_body.into(), else_body.into()).into(),
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
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;

        // while|loops ...
        let (guard, token_pos) = match input.next().expect(NEVER_ENDS) {
            (Token::While, pos) => {
                ensure_not_statement_expr(input, "a boolean")?;
                let expr = self
                    .parse_expr(input, state, lib, settings.level_up())?
                    .ensure_bool_expr()?;
                ensure_not_assignment(input)?;
                (expr, pos)
            }
            (Token::Loop, pos) => (Expr::Unit(Position::NONE), pos),
            token => unreachable!("Token::While or Token::Loop expected but gets {:?}", token),
        };
        settings.pos = token_pos;
        settings.is_breakable = true;

        let body = self.parse_block(input, state, lib, settings.level_up())?;

        Ok(Stmt::While((guard, body.into()).into(), settings.pos))
    }

    /// Parse a do loop.
    fn parse_do(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // do ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::Do);

        // do { body } [while|until] guard
        settings.is_breakable = true;
        let body = self.parse_block(input, state, lib, settings.level_up())?;

        let negated = match input.next().expect(NEVER_ENDS) {
            (Token::While, ..) => ASTFlags::NONE,
            (Token::Until, ..) => ASTFlags::NEGATED,
            (.., pos) => {
                return Err(
                    PERR::MissingToken(Token::While.into(), "for the do statement".into())
                        .into_err(pos),
                )
            }
        };

        settings.is_breakable = false;

        ensure_not_statement_expr(input, "a boolean")?;
        let guard = self
            .parse_expr(input, state, lib, settings.level_up())?
            .ensure_bool_expr()?;
        ensure_not_assignment(input)?;

        Ok(Stmt::Do((guard, body.into()).into(), negated, settings.pos))
    }

    /// Parse a for loop.
    fn parse_for(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // for ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::For);

        // for name ...
        let (name, name_pos, counter_name, counter_pos) = if match_token(input, Token::LeftParen).0
        {
            // ( name, counter )
            let (name, name_pos) = parse_var_name(input)?;
            let (has_comma, pos) = match_token(input, Token::Comma);
            if !has_comma {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "after the iteration variable name".into(),
                )
                .into_err(pos));
            }
            let (counter_name, counter_pos) = parse_var_name(input)?;

            if counter_name == name {
                return Err(
                    PERR::DuplicatedVariable(counter_name.to_string()).into_err(counter_pos)
                );
            }

            let (has_close_paren, pos) = match_token(input, Token::RightParen);
            if !has_close_paren {
                return Err(PERR::MissingToken(
                    Token::RightParen.into(),
                    "to close the iteration variable".into(),
                )
                .into_err(pos));
            }
            (name, name_pos, counter_name, counter_pos)
        } else {
            // name
            let (name, name_pos) = parse_var_name(input)?;
            (name, name_pos, Identifier::new_const(), Position::NONE)
        };

        // for name in ...
        match input.next().expect(NEVER_ENDS) {
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
            .parse_expr(input, state, lib, settings.level_up())?
            .ensure_iterable()?;

        let prev_stack_len = state.stack.len();

        if !counter_name.is_empty() {
            state.stack.push(name.clone(), ());
        }
        let counter_var = Ident {
            name: state.get_interned_string(counter_name),
            pos: counter_pos,
        };

        let loop_var = state.get_interned_string(name);
        state.stack.push(loop_var.clone(), ());
        let loop_var = Ident {
            name: loop_var,
            pos: name_pos,
        };

        settings.is_breakable = true;
        let body = self.parse_block(input, state, lib, settings.level_up())?;

        state.stack.rewind(prev_stack_len);

        Ok(Stmt::For(
            Box::new((loop_var, counter_var, expr, body.into())),
            settings.pos,
        ))
    }

    /// Parse a variable definition statement.
    fn parse_let(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        access: AccessMode,
        is_export: bool,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // let/const... (specified in `var_type`)
        let mut settings = settings;
        settings.pos = input.next().expect(NEVER_ENDS).1;

        // let name ...
        let (name, pos) = parse_var_name(input)?;

        if !self.allow_shadowing() && state.stack.iter().any(|(v, ..)| v == name) {
            return Err(PERR::VariableExists(name.to_string()).into_err(pos));
        }

        if let Some(ref filter) = self.def_var_filter {
            let will_shadow = state.stack.iter().any(|(v, ..)| v == name);
            let level = settings.level;
            let is_const = access == AccessMode::ReadOnly;
            let info = VarDefInfo {
                name: &name,
                is_const,
                nesting_level: level,
                will_shadow,
            };
            let mut this_ptr = None;
            let context = EvalContext::new(
                self,
                &mut state.stack,
                &mut state.global,
                None,
                &[],
                &mut this_ptr,
                level,
            );

            match filter(false, info, context) {
                Ok(true) => (),
                Ok(false) => return Err(PERR::ForbiddenVariable(name.to_string()).into_err(pos)),
                Err(err) => match *err {
                    EvalAltResult::ErrorParsing(perr, pos) => return Err(perr.into_err(pos)),
                    _ => return Err(PERR::ForbiddenVariable(name.to_string()).into_err(pos)),
                },
            }
        }

        let name = state.get_interned_string(name);

        // let name = ...
        let expr = if match_token(input, Token::Equals).0 {
            // let name = expr
            self.parse_expr(input, state, lib, settings.level_up())?
        } else {
            Expr::Unit(Position::NONE)
        };

        let export = if is_export {
            ASTFlags::EXPORTED
        } else {
            ASTFlags::NONE
        };

        let (existing, hit_barrier) = state.find_var(&name);
        let existing = if !hit_barrier && existing > 0 {
            let offset = state.stack.len() - existing;
            if offset < state.block_stack_len {
                // Defined in parent block
                None
            } else {
                Some(offset)
            }
        } else {
            None
        };

        let idx = if let Some(n) = existing {
            state.stack.get_mut_by_index(n).set_access_mode(access);
            Some(NonZeroUsize::new(state.stack.len() - n).unwrap())
        } else {
            state.stack.push_entry(name.as_str(), access, Dynamic::UNIT);
            None
        };

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
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // import ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::Import);

        // import expr ...
        let expr = self.parse_expr(input, state, lib, settings.level_up())?;

        // import expr;
        if !match_token(input, Token::As).0 {
            let empty = Ident {
                name: state.get_interned_string(""),
                pos: Position::NONE,
            };
            return Ok(Stmt::Import((expr, empty).into(), settings.pos));
        }

        // import expr as name ...
        let (name, pos) = parse_var_name(input)?;
        let name = state.get_interned_string(name);
        state.imports.push(name.clone());

        Ok(Stmt::Import(
            (expr, Ident { name, pos }).into(),
            settings.pos,
        ))
    }

    /// Parse an export statement.
    #[cfg(not(feature = "no_module"))]
    fn parse_export(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;
        settings.pos = eat_token(input, Token::Export);

        match input.peek().expect(NEVER_ENDS) {
            (Token::Let, pos) => {
                let pos = *pos;
                let mut stmt =
                    self.parse_let(input, state, lib, AccessMode::ReadWrite, true, settings)?;
                stmt.set_position(pos);
                return Ok(stmt);
            }
            (Token::Const, pos) => {
                let pos = *pos;
                let mut stmt =
                    self.parse_let(input, state, lib, AccessMode::ReadOnly, true, settings)?;
                stmt.set_position(pos);
                return Ok(stmt);
            }
            _ => (),
        }

        let (id, id_pos) = parse_var_name(input)?;

        let (alias, alias_pos) = if match_token(input, Token::As).0 {
            let (name, pos) = parse_var_name(input)?;
            (Some(name), pos)
        } else {
            (None, Position::NONE)
        };

        let export = (
            Ident {
                name: state.get_interned_string(id),
                pos: id_pos,
            },
            Ident {
                name: state.get_interned_string(alias.as_ref().map_or("", <_>::as_ref)),
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
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // Must start with {
        let mut settings = settings;
        settings.pos = match input.next().expect(NEVER_ENDS) {
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

        let mut statements = StaticVec::new_const();

        let prev_entry_stack_len = state.block_stack_len;
        state.block_stack_len = state.stack.len();

        #[cfg(not(feature = "no_module"))]
        let orig_imports_len = state.imports.len();

        let end_pos = loop {
            // Terminated?
            match input.peek().expect(NEVER_ENDS) {
                (Token::RightBrace, ..) => break eat_token(input, Token::RightBrace),
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
            settings.at_global_level = false;

            let stmt = self.parse_stmt(input, state, lib, settings.level_up())?;

            if stmt.is_noop() {
                continue;
            }

            // See if it needs a terminating semicolon
            let need_semicolon = !stmt.is_self_terminated();

            statements.push(stmt);

            match input.peek().expect(NEVER_ENDS) {
                // { ... stmt }
                (Token::RightBrace, ..) => break eat_token(input, Token::RightBrace),
                // { ... stmt;
                (Token::SemiColon, ..) if need_semicolon => {
                    eat_token(input, Token::SemiColon);
                }
                // { ... { stmt } ;
                (Token::SemiColon, ..) if !need_semicolon => {
                    eat_token(input, Token::SemiColon);
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

        state.stack.rewind(state.block_stack_len);
        state.block_stack_len = prev_entry_stack_len;

        #[cfg(not(feature = "no_module"))]
        state.imports.truncate(orig_imports_len);

        Ok((statements, settings.pos, end_pos).into())
    }

    /// Parse an expression as a statement.
    fn parse_expr_stmt(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;
        settings.pos = input.peek().expect(NEVER_ENDS).1;

        let expr = self.parse_expr(input, state, lib, settings.level_up())?;
        let stmt = self.parse_op_assignment_stmt(input, state, lib, expr, settings.level_up())?;
        Ok(stmt)
    }

    /// Parse a single statement.
    fn parse_stmt(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<Stmt> {
        use AccessMode::{ReadOnly, ReadWrite};

        let mut settings = settings;

        #[cfg(not(feature = "no_function"))]
        #[cfg(feature = "metadata")]
        let comments = {
            let mut comments = StaticVec::<SmartString>::new();
            let mut comments_pos = Position::NONE;

            // Handle doc-comments.
            while let (Token::Comment(ref comment), pos) = input.peek().expect(NEVER_ENDS) {
                if comments_pos.is_none() {
                    comments_pos = *pos;
                }

                if !crate::tokenizer::is_doc_comment(comment) {
                    unreachable!("doc-comment expected but gets {:?}", comment);
                }

                if !settings.at_global_level {
                    return Err(PERR::WrongDocComment.into_err(comments_pos));
                }

                match input.next().expect(NEVER_ENDS).0 {
                    Token::Comment(comment) => {
                        comments.push(comment);

                        match input.peek().expect(NEVER_ENDS) {
                            (Token::Fn | Token::Private, ..) => break,
                            (Token::Comment(..), ..) => (),
                            _ => return Err(PERR::WrongDocComment.into_err(comments_pos)),
                        }
                    }
                    token => unreachable!("Token::Comment expected but gets {:?}", token),
                }
            }

            comments
        };

        let (token, token_pos) = match input.peek().expect(NEVER_ENDS) {
            (Token::EOF, pos) => return Ok(Stmt::Noop(*pos)),
            (x, pos) => (x, *pos),
        };
        settings.pos = token_pos;

        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        match token {
            // ; - empty statement
            Token::SemiColon => {
                eat_token(input, Token::SemiColon);
                Ok(Stmt::Noop(token_pos))
            }

            // { - statements block
            Token::LeftBrace => Ok(self.parse_block(input, state, lib, settings.level_up())?),

            // fn ...
            #[cfg(not(feature = "no_function"))]
            Token::Fn if !settings.at_global_level => {
                Err(PERR::WrongFnDefinition.into_err(token_pos))
            }

            #[cfg(not(feature = "no_function"))]
            Token::Fn | Token::Private => {
                let access = if matches!(token, Token::Private) {
                    eat_token(input, Token::Private);
                    crate::FnAccess::Private
                } else {
                    crate::FnAccess::Public
                };

                match input.next().expect(NEVER_ENDS) {
                    (Token::Fn, pos) => {
                        // Build new parse state
                        let interned_strings = std::mem::take(&mut state.interned_strings);

                        let mut new_state = ParseState::new(
                            self,
                            state.scope,
                            interned_strings,
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
                            new_state
                                .global_imports
                                .extend(state.imports.iter().cloned());
                        }

                        #[cfg(not(feature = "unchecked"))]
                        {
                            new_state.max_expr_depth = self.max_function_expr_depth();
                        }

                        let mut options = self.options;
                        options.set(
                            LangOptions::STRICT_VAR,
                            settings.options.contains(LangOptions::STRICT_VAR),
                        );

                        let new_settings = ParseSettings {
                            at_global_level: false,
                            in_fn_scope: true,
                            #[cfg(not(feature = "no_closure"))]
                            in_closure: false,
                            is_breakable: false,
                            level: 0,
                            options,
                            pos,
                        };

                        let func = self.parse_fn(
                            input,
                            &mut new_state,
                            lib,
                            access,
                            new_settings,
                            #[cfg(not(feature = "no_function"))]
                            #[cfg(feature = "metadata")]
                            comments,
                        );

                        // Restore parse state
                        state.interned_strings = new_state.interned_strings;

                        let func = func?;

                        let hash = calc_fn_hash(&func.name, func.params.len());

                        if !lib.is_empty() && lib.contains_key(&hash) {
                            return Err(PERR::FnDuplicatedDefinition(
                                func.name.to_string(),
                                func.params.len(),
                            )
                            .into_err(pos));
                        }

                        lib.insert(hash, func.into());

                        Ok(Stmt::Noop(pos))
                    }

                    (.., pos) => Err(PERR::MissingToken(
                        Token::Fn.into(),
                        format!("following '{}'", Token::Private.syntax()),
                    )
                    .into_err(pos)),
                }
            }

            Token::If => self.parse_if(input, state, lib, settings.level_up()),
            Token::Switch => self.parse_switch(input, state, lib, settings.level_up()),
            Token::While | Token::Loop if self.allow_looping() => {
                self.parse_while_loop(input, state, lib, settings.level_up())
            }
            Token::Do if self.allow_looping() => {
                self.parse_do(input, state, lib, settings.level_up())
            }
            Token::For if self.allow_looping() => {
                self.parse_for(input, state, lib, settings.level_up())
            }

            Token::Continue if self.allow_looping() && settings.is_breakable => {
                let pos = eat_token(input, Token::Continue);
                Ok(Stmt::BreakLoop(ASTFlags::NONE, pos))
            }
            Token::Break if self.allow_looping() && settings.is_breakable => {
                let pos = eat_token(input, Token::Break);
                Ok(Stmt::BreakLoop(ASTFlags::BREAK, pos))
            }
            Token::Continue | Token::Break if self.allow_looping() => {
                Err(PERR::LoopBreak.into_err(token_pos))
            }

            Token::Return | Token::Throw => {
                let (return_type, token_pos) = input
                    .next()
                    .map(|(token, pos)| {
                        let flags = match token {
                            Token::Return => ASTFlags::NONE,
                            Token::Throw => ASTFlags::BREAK,
                            token => unreachable!(
                                "Token::Return or Token::Throw expected but gets {:?}",
                                token
                            ),
                        };
                        (flags, pos)
                    })
                    .expect(NEVER_ENDS);

                match input.peek().expect(NEVER_ENDS) {
                    // `return`/`throw` at <EOF>
                    (Token::EOF, ..) => Ok(Stmt::Return(None, return_type, token_pos)),
                    // `return`/`throw` at end of block
                    (Token::RightBrace, ..) if !settings.at_global_level => {
                        Ok(Stmt::Return(None, return_type, token_pos))
                    }
                    // `return;` or `throw;`
                    (Token::SemiColon, ..) => Ok(Stmt::Return(None, return_type, token_pos)),
                    // `return` or `throw` with expression
                    _ => {
                        let expr = self.parse_expr(input, state, lib, settings.level_up())?;
                        Ok(Stmt::Return(Some(expr.into()), return_type, token_pos))
                    }
                }
            }

            Token::Try => self.parse_try_catch(input, state, lib, settings.level_up()),

            Token::Let => self.parse_let(input, state, lib, ReadWrite, false, settings.level_up()),
            Token::Const => self.parse_let(input, state, lib, ReadOnly, false, settings.level_up()),

            #[cfg(not(feature = "no_module"))]
            Token::Import => self.parse_import(input, state, lib, settings.level_up()),

            #[cfg(not(feature = "no_module"))]
            Token::Export if !settings.at_global_level => {
                Err(PERR::WrongExport.into_err(token_pos))
            }

            #[cfg(not(feature = "no_module"))]
            Token::Export => self.parse_export(input, state, lib, settings.level_up()),

            _ => self.parse_expr_stmt(input, state, lib, settings.level_up()),
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
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        // try ...
        let mut settings = settings;
        settings.pos = eat_token(input, Token::Try);

        // try { try_block }
        let try_block = self.parse_block(input, state, lib, settings.level_up())?;

        // try { try_block } catch
        let (matched, catch_pos) = match_token(input, Token::Catch);

        if !matched {
            return Err(
                PERR::MissingToken(Token::Catch.into(), "for the 'try' statement".into())
                    .into_err(catch_pos),
            );
        }

        // try { try_block } catch (
        let catch_var = if match_token(input, Token::LeftParen).0 {
            let (name, pos) = parse_var_name(input)?;
            let (matched, err_pos) = match_token(input, Token::RightParen);

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
        let catch_block = self.parse_block(input, state, lib, settings.level_up())?;

        if !catch_var.is_empty() {
            // Remove the error variable from the stack
            state.stack.rewind(state.stack.len() - 1);
        }

        Ok(Stmt::TryCatch(
            TryCatchBlock {
                try_block: try_block.into(),
                catch_var,
                catch_block: catch_block.into(),
            }
            .into(),
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
        access: crate::FnAccess,
        settings: ParseSettings,
        #[cfg(not(feature = "no_function"))]
        #[cfg(feature = "metadata")]
        comments: StaticVec<SmartString>,
    ) -> ParseResult<ScriptFnDef> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;

        let (token, pos) = input.next().expect(NEVER_ENDS);

        let name = match token.into_function_name_for_override() {
            Ok(r) => r,
            Err(Token::Reserved(s)) => return Err(PERR::Reserved(s.to_string()).into_err(pos)),
            Err(_) => return Err(PERR::FnMissingName.into_err(pos)),
        };

        let no_params = match input.peek().expect(NEVER_ENDS) {
            (Token::LeftParen, ..) => {
                eat_token(input, Token::LeftParen);
                match_token(input, Token::RightParen).0
            }
            (Token::Unit, ..) => {
                eat_token(input, Token::Unit);
                true
            }
            (.., pos) => return Err(PERR::FnMissingParams(name.to_string()).into_err(*pos)),
        };

        let mut params = StaticVec::<(ImmutableString, _)>::new_const();

        if !no_params {
            let sep_err = format!("to separate the parameters of function '{name}'");

            loop {
                match input.next().expect(NEVER_ENDS) {
                    (Token::RightParen, ..) => break,
                    (Token::Identifier(s), pos) => {
                        if params.iter().any(|(p, _)| p.as_str() == &*s) {
                            return Err(PERR::FnDuplicatedParam(name.to_string(), s.to_string())
                                .into_err(pos));
                        }
                        let s = state.get_interned_string(s);
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

                match input.next().expect(NEVER_ENDS) {
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
        let body = match input.peek().expect(NEVER_ENDS) {
            (Token::LeftBrace, ..) => {
                settings.is_breakable = false;
                self.parse_block(input, state, lib, settings.level_up())?
            }
            (.., pos) => return Err(PERR::FnMissingBody(name.to_string()).into_err(*pos)),
        }
        .into();

        let mut params: StaticVec<_> = params.into_iter().map(|(p, ..)| p).collect();
        params.shrink_to_fit();

        Ok(ScriptFnDef {
            name: state.get_interned_string(name),
            access,
            params,
            body,
            #[cfg(not(feature = "no_module"))]
            environ: None,
            #[cfg(not(feature = "no_function"))]
            #[cfg(feature = "metadata")]
            comments: comments
                .into_iter()
                .map(|s| s.to_string().into_boxed_str())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        })
    }

    /// Creates a curried expression from a list of external variables
    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_closure"))]
    fn make_curry_from_externals(
        state: &mut ParseState,
        fn_expr: Expr,
        externals: StaticVec<crate::ast::Ident>,
        pos: Position,
    ) -> Expr {
        // If there are no captured variables, no need to curry
        if externals.is_empty() {
            return fn_expr;
        }

        let num_externals = externals.len();
        let mut args = StaticVec::with_capacity(externals.len() + 1);

        args.push(fn_expr);

        args.extend(
            externals
                .iter()
                .cloned()
                .map(|crate::ast::Ident { name, pos }| {
                    #[cfg(not(feature = "no_module"))]
                    let ns = crate::ast::Namespace::NONE;
                    #[cfg(feature = "no_module")]
                    let ns = ();

                    Expr::Variable((None, ns, 0, name).into(), None, pos)
                }),
        );

        let expr = FnCallExpr {
            name: state.get_interned_string(crate::engine::KEYWORD_FN_PTR_CURRY),
            hashes: FnCallHashes::from_native(calc_fn_hash(
                crate::engine::KEYWORD_FN_PTR_CURRY,
                num_externals + 1,
            )),
            args,
            pos,
            ..Default::default()
        }
        .into_fn_call_expr(pos);

        // Convert the entire expression into a statement block, then insert the relevant
        // [`Share`][Stmt::Share] statements.
        let mut statements = StaticVec::with_capacity(externals.len() + 1);
        statements.extend(
            externals
                .into_iter()
                .map(|crate::ast::Ident { name, pos }| Stmt::Share(name, pos)),
        );
        statements.push(Stmt::Expr(expr.into()));
        Expr::Stmt(crate::ast::StmtBlock::new(statements, pos, Position::NONE).into())
    }

    /// Parse an anonymous function definition.
    #[cfg(not(feature = "no_function"))]
    fn parse_anon_fn(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        lib: &mut FnLib,
        settings: ParseSettings,
    ) -> ParseResult<(Expr, ScriptFnDef)> {
        #[cfg(not(feature = "unchecked"))]
        settings.ensure_level_within_max_limit(state.max_expr_depth)?;

        let mut settings = settings;
        let mut params_list = StaticVec::<ImmutableString>::new_const();

        if input.next().expect(NEVER_ENDS).0 != Token::Or && !match_token(input, Token::Pipe).0 {
            loop {
                match input.next().expect(NEVER_ENDS) {
                    (Token::Pipe, ..) => break,
                    (Token::Identifier(s), pos) => {
                        if params_list.iter().any(|p| p.as_str() == &*s) {
                            return Err(PERR::FnDuplicatedParam("".to_string(), s.to_string())
                                .into_err(pos));
                        }
                        let s = state.get_interned_string(s);
                        state.stack.push(s.clone(), ());
                        params_list.push(s);
                    }
                    (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                    (.., pos) => {
                        return Err(PERR::MissingToken(
                            Token::Pipe.into(),
                            "to close the parameters list of anonymous function".into(),
                        )
                        .into_err(pos))
                    }
                }

                match input.next().expect(NEVER_ENDS) {
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
        settings.is_breakable = false;
        let body = self.parse_stmt(input, state, lib, settings.level_up())?;

        // External variables may need to be processed in a consistent order,
        // so extract them into a list.
        #[cfg(not(feature = "no_closure"))]
        let (mut params, externals) = {
            let externals: StaticVec<_> = state.external_vars.iter().cloned().collect();

            let mut params = StaticVec::with_capacity(params_list.len() + externals.len());
            params.extend(
                externals
                    .iter()
                    .map(|crate::ast::Ident { name, .. }| name.clone()),
            );

            (params, externals)
        };
        #[cfg(feature = "no_closure")]
        let mut params = StaticVec::with_capacity(params_list.len());

        params.append(&mut params_list);

        // Create unique function name by hashing the script body plus the parameters.
        let hasher = &mut get_hasher();
        params.iter().for_each(|p| p.hash(hasher));
        body.hash(hasher);
        let hash = hasher.finish();
        let fn_name = state.get_interned_string(make_anonymous_fn(hash));

        // Define the function
        let script = ScriptFnDef {
            name: fn_name.clone(),
            access: crate::FnAccess::Public,
            params,
            body: body.into(),
            #[cfg(not(feature = "no_module"))]
            environ: None,
            #[cfg(not(feature = "no_function"))]
            #[cfg(feature = "metadata")]
            comments: Box::default(),
        };

        let fn_ptr = crate::FnPtr::new_unchecked(fn_name, StaticVec::new_const());
        let expr = Expr::DynamicConstant(Box::new(fn_ptr.into()), settings.pos);

        #[cfg(not(feature = "no_closure"))]
        let expr = Self::make_curry_from_externals(state, expr, externals, settings.pos);

        Ok((expr, script))
    }

    /// Parse a global level expression.
    pub(crate) fn parse_global_expr(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        _optimization_level: OptimizationLevel,
    ) -> ParseResult<AST> {
        let mut functions = BTreeMap::new();

        let mut options = self.options;
        options.remove(LangOptions::IF_EXPR | LangOptions::SWITCH_EXPR | LangOptions::STMT_EXPR);
        #[cfg(not(feature = "no_function"))]
        options.remove(LangOptions::ANON_FN);

        let settings = ParseSettings {
            at_global_level: true,
            #[cfg(not(feature = "no_function"))]
            in_fn_scope: false,
            #[cfg(not(feature = "no_function"))]
            #[cfg(not(feature = "no_closure"))]
            in_closure: false,
            is_breakable: false,
            level: 0,
            options,
            pos: Position::NONE,
        };
        let expr = self.parse_expr(input, state, &mut functions, settings)?;

        assert!(functions.is_empty());

        match input.peek().expect(NEVER_ENDS) {
            (Token::EOF, ..) => (),
            // Return error if the expression doesn't end
            (token, pos) => {
                return Err(LexError::UnexpectedInput(token.syntax().to_string()).into_err(*pos))
            }
        }

        let mut statements = StmtBlockContainer::new_const();
        statements.push(Stmt::Expr(expr.into()));

        #[cfg(not(feature = "no_optimize"))]
        return Ok(crate::optimizer::optimize_into_ast(
            self,
            state.scope,
            statements,
            #[cfg(not(feature = "no_function"))]
            StaticVec::new_const(),
            _optimization_level,
        ));

        #[cfg(feature = "no_optimize")]
        return Ok(AST::new(
            statements,
            #[cfg(not(feature = "no_function"))]
            crate::Module::new(),
        ));
    }

    /// Parse the global level statements.
    fn parse_global_level(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
    ) -> ParseResult<(StmtBlockContainer, StaticVec<Shared<ScriptFnDef>>)> {
        let mut statements = StmtBlockContainer::new_const();
        let mut functions = BTreeMap::new();

        while !input.peek().expect(NEVER_ENDS).0.is_eof() {
            let settings = ParseSettings {
                at_global_level: true,
                #[cfg(not(feature = "no_function"))]
                in_fn_scope: false,
                #[cfg(not(feature = "no_function"))]
                #[cfg(not(feature = "no_closure"))]
                in_closure: false,
                is_breakable: false,
                options: self.options,
                level: 0,
                pos: Position::NONE,
            };

            let stmt = self.parse_stmt(input, state, &mut functions, settings)?;

            if stmt.is_noop() {
                continue;
            }

            let need_semicolon = !stmt.is_self_terminated();

            statements.push(stmt);

            match input.peek().expect(NEVER_ENDS) {
                // EOF
                (Token::EOF, ..) => break,
                // stmt ;
                (Token::SemiColon, ..) if need_semicolon => {
                    eat_token(input, Token::SemiColon);
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
        input: &mut TokenStream,
        state: &mut ParseState,
        _optimization_level: OptimizationLevel,
    ) -> ParseResult<AST> {
        let (statements, _lib) = self.parse_global_level(input, state)?;

        #[cfg(not(feature = "no_optimize"))]
        return Ok(crate::optimizer::optimize_into_ast(
            self,
            state.scope,
            statements,
            #[cfg(not(feature = "no_function"))]
            _lib,
            _optimization_level,
        ));

        #[cfg(feature = "no_optimize")]
        #[cfg(not(feature = "no_function"))]
        {
            let mut m = crate::Module::new();

            for fn_def in _lib {
                m.set_script_fn(fn_def);
            }

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
