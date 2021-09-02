//! Main module defining the lexer and parser.

use crate::ast::{
    BinaryExpr, CustomExpr, Expr, FnCallExpr, FnCallHashes, Ident, OpAssignment, ReturnType,
    ScriptFnDef, Stmt, StmtBlock, AST_OPTION_FLAGS::*,
};
use crate::custom_syntax::{
    CustomSyntax, CUSTOM_SYNTAX_MARKER_BLOCK, CUSTOM_SYNTAX_MARKER_BOOL, CUSTOM_SYNTAX_MARKER_EXPR,
    CUSTOM_SYNTAX_MARKER_IDENT, CUSTOM_SYNTAX_MARKER_INT, CUSTOM_SYNTAX_MARKER_STRING,
};
use crate::dynamic::AccessMode;
use crate::engine::{Precedence, KEYWORD_THIS, OP_CONTAINS};
use crate::fn_hash::get_hasher;
use crate::module::NamespaceRef;
use crate::optimize::{optimize_into_ast, OptimizationLevel};
use crate::token::{
    is_keyword_function, is_valid_identifier, Token, TokenStream, TokenizerControl,
};
use crate::{
    calc_fn_hash, calc_qualified_fn_hash, calc_qualified_var_hash, Engine, Identifier, LexError,
    ParseError, ParseErrorType, Position, Scope, Shared, StaticVec, AST,
};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    num::{NonZeroU8, NonZeroUsize},
};

#[cfg(not(feature = "no_float"))]
use crate::{custom_syntax::CUSTOM_SYNTAX_MARKER_FLOAT, FLOAT};

#[cfg(not(feature = "no_function"))]
use crate::FnAccess;

type PERR = ParseErrorType;

type FunctionsLib = BTreeMap<u64, Shared<ScriptFnDef>>;

/// Invalid variable name that acts as a search barrier in a [`Scope`].
const SCOPE_SEARCH_BARRIER_MARKER: &str = "$BARRIER$";

/// The message: never fails because `TokenStream` never ends
const NEVER_ENDS: &str = "never fails because `TokenStream` never ends";

/// A factory of identifiers from text strings.
///
/// When [`SmartString`](https://crates.io/crates/smartstring) is used as [`Identifier`],
/// this just returns a copy because most identifiers in Rhai are short and ASCII-based.
///
/// When [`ImmutableString`] is used as [`Identifier`], this type acts as an interner which keeps a
/// collection of strings and returns shared instances, only creating a new string when it is not
/// yet interned.
#[derive(Debug, Clone, Default, Hash)]
pub struct IdentifierBuilder(
    #[cfg(feature = "no_smartstring")] std::collections::BTreeSet<Identifier>,
);

impl IdentifierBuilder {
    /// Get an identifier from a text string.
    #[inline]
    #[must_use]
    pub fn get(&mut self, text: impl AsRef<str> + Into<Identifier>) -> Identifier {
        #[cfg(not(feature = "no_smartstring"))]
        return text.into();

        #[cfg(feature = "no_smartstring")]
        return self.0.get(text.as_ref()).cloned().unwrap_or_else(|| {
            let s: Identifier = text.into();
            self.0.insert(s.clone());
            s
        });
    }
}

/// A type that encapsulates the current state of the parser.
#[derive(Debug)]
pub struct ParseState<'e> {
    /// Reference to the scripting [`Engine`].
    engine: &'e Engine,
    /// Input stream buffer containing the next character to read.
    tokenizer_control: TokenizerControl,
    /// Interned strings.
    interned_strings: IdentifierBuilder,
    /// Encapsulates a local stack with variable names to simulate an actual runtime scope.
    stack: Vec<(Identifier, AccessMode)>,
    /// Size of the local variables stack upon entry of the current block scope.
    entry_stack_len: usize,
    /// Tracks a list of external variables (variables that are not explicitly declared in the scope).
    #[cfg(not(feature = "no_closure"))]
    external_vars: BTreeMap<Identifier, Position>,
    /// An indicator that disables variable capturing into externals one single time
    /// up until the nearest consumed Identifier token.
    /// If set to false the next call to [`access_var`][ParseState::access_var] will not capture the variable.
    /// All consequent calls to [`access_var`][ParseState::access_var] will not be affected
    #[cfg(not(feature = "no_closure"))]
    allow_capture: bool,
    /// Encapsulates a local stack with imported [module][crate::Module] names.
    #[cfg(not(feature = "no_module"))]
    modules: StaticVec<Identifier>,
    /// Maximum levels of expression nesting.
    #[cfg(not(feature = "unchecked"))]
    max_expr_depth: Option<NonZeroUsize>,
    /// Maximum levels of expression nesting in functions.
    #[cfg(not(feature = "unchecked"))]
    #[cfg(not(feature = "no_function"))]
    max_function_expr_depth: Option<NonZeroUsize>,
}

impl<'e> ParseState<'e> {
    /// Create a new [`ParseState`].
    #[inline(always)]
    #[must_use]
    pub fn new(engine: &'e Engine, tokenizer_control: TokenizerControl) -> Self {
        Self {
            engine,
            tokenizer_control,
            #[cfg(not(feature = "unchecked"))]
            max_expr_depth: NonZeroUsize::new(engine.max_expr_depth()),
            #[cfg(not(feature = "unchecked"))]
            #[cfg(not(feature = "no_function"))]
            max_function_expr_depth: NonZeroUsize::new(engine.max_function_expr_depth()),
            #[cfg(not(feature = "no_closure"))]
            external_vars: Default::default(),
            #[cfg(not(feature = "no_closure"))]
            allow_capture: true,
            interned_strings: Default::default(),
            stack: Vec::with_capacity(16),
            entry_stack_len: 0,
            #[cfg(not(feature = "no_module"))]
            modules: Default::default(),
        }
    }

    /// Find explicitly declared variable by name in the [`ParseState`], searching in reverse order.
    ///
    /// If the variable is not present in the scope adds it to the list of external variables
    ///
    /// The return value is the offset to be deducted from `ParseState::stack::len()`,
    /// i.e. the top element of [`ParseState`]'s variables stack is offset 1.
    ///
    /// Return `None` when the variable name is not found in the `stack`.
    #[inline]
    pub fn access_var(&mut self, name: &str, pos: Position) -> Option<NonZeroUsize> {
        let mut barrier = false;
        let _pos = pos;

        let index = self
            .stack
            .iter()
            .rev()
            .enumerate()
            .find(|(_, (n, _))| {
                if n == SCOPE_SEARCH_BARRIER_MARKER {
                    // Do not go beyond the barrier
                    barrier = true;
                    false
                } else {
                    n == name
                }
            })
            .and_then(|(i, _)| NonZeroUsize::new(i + 1));

        #[cfg(not(feature = "no_closure"))]
        if self.allow_capture {
            if index.is_none() && !self.external_vars.contains_key(name) {
                self.external_vars.insert(name.into(), _pos);
            }
        } else {
            self.allow_capture = true
        }

        if barrier {
            None
        } else {
            index
        }
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
        self.modules
            .iter()
            .rev()
            .enumerate()
            .find(|&(_, n)| n == name)
            .and_then(|(i, _)| NonZeroUsize::new(i + 1))
    }

    /// Get an interned string, creating one if it is not yet interned.
    #[inline(always)]
    #[must_use]
    pub fn get_identifier(&mut self, text: impl AsRef<str> + Into<Identifier>) -> Identifier {
        self.interned_strings.get(text)
    }
}

/// A type that encapsulates all the settings for a particular parsing function.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct ParseSettings {
    /// Current position.
    pos: Position,
    /// Is the construct being parsed located at global level?
    is_global: bool,
    /// Is the construct being parsed located at function definition level?
    is_function_scope: bool,
    /// Is the current position inside a loop?
    is_breakable: bool,
    /// Is anonymous function allowed?
    allow_anonymous_fn: bool,
    /// Is if-expression allowed?
    allow_if_expr: bool,
    /// Is switch expression allowed?
    allow_switch_expr: bool,
    /// Is statement-expression allowed?
    allow_stmt_expr: bool,
    /// Current expression nesting level.
    level: usize,
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
    #[cfg(not(feature = "unchecked"))]
    #[inline]
    pub fn ensure_level_within_max_limit(
        &self,
        limit: Option<NonZeroUsize>,
    ) -> Result<(), ParseError> {
        if let Some(limit) = limit {
            if self.level > limit.get() {
                return Err(PERR::ExprTooDeep.into_err(self.pos));
            }
        }
        Ok(())
    }
}

impl Expr {
    /// Convert a [`Variable`][Expr::Variable] into a [`Property`][Expr::Property].
    /// All other variants are untouched.
    #[cfg(not(feature = "no_object"))]
    #[inline]
    #[must_use]
    fn into_property(self, state: &mut ParseState) -> Self {
        match self {
            Self::Variable(_, pos, x) if x.1.is_none() => {
                let ident = x.2;
                let getter = state.get_identifier(crate::engine::make_getter(&ident));
                let hash_get = calc_fn_hash(&getter, 1);
                let setter = state.get_identifier(crate::engine::make_setter(&ident));
                let hash_set = calc_fn_hash(&setter, 2);

                Self::Property(Box::new((
                    (getter, hash_get),
                    (setter, hash_set),
                    (state.get_identifier(ident).into(), pos),
                )))
            }
            _ => self,
        }
    }
    /// Raise an error if the expression can never yield a boolean value.
    fn ensure_bool_expr(self) -> Result<Expr, ParseError> {
        let type_name = match self {
            Expr::Unit(_) => "()",
            Expr::DynamicConstant(ref v, _) if !v.is::<bool>() => v.type_name(),
            Expr::IntegerConstant(_, _) => "a number",
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_, _) => "a floating-point number",
            Expr::CharConstant(_, _) => "a character",
            Expr::StringConstant(_, _) => "a string",
            Expr::InterpolatedString(_, _) => "a string",
            Expr::Array(_, _) => "an array",
            Expr::Map(_, _) => "an object map",
            _ => return Ok(self),
        };

        Err(
            PERR::MismatchedType("a boolean expression".to_string(), type_name.to_string())
                .into_err(self.position()),
        )
    }
    /// Raise an error if the expression can never yield an iterable value.
    fn ensure_iterable(self) -> Result<Expr, ParseError> {
        let type_name = match self {
            Expr::Unit(_) => "()",
            Expr::BoolConstant(_, _) => "a boolean",
            Expr::IntegerConstant(_, _) => "a number",
            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_, _) => "a floating-point number",
            Expr::CharConstant(_, _) => "a character",
            Expr::StringConstant(_, _) => "a string",
            Expr::InterpolatedString(_, _) => "a string",
            Expr::Map(_, _) => "an object map",
            _ => return Ok(self),
        };

        Err(
            PERR::MismatchedType("an iterable value".to_string(), type_name.to_string())
                .into_err(self.position()),
        )
    }
}

/// Make sure that the next expression is not a statement expression (i.e. wrapped in `{}`).
#[inline]
fn ensure_not_statement_expr(input: &mut TokenStream, type_name: &str) -> Result<(), ParseError> {
    match input.peek().expect(NEVER_ENDS) {
        (Token::LeftBrace, pos) => Err(PERR::ExprExpected(type_name.to_string()).into_err(*pos)),
        _ => Ok(()),
    }
}

/// Make sure that the next expression is not a mis-typed assignment (i.e. `a = b` instead of `a == b`).
#[inline]
fn ensure_not_assignment(input: &mut TokenStream) -> Result<(), ParseError> {
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
#[inline]
fn eat_token(input: &mut TokenStream, token: Token) -> Position {
    let (t, pos) = input.next().expect(NEVER_ENDS);

    if t != token {
        unreachable!(
            "expecting {} (found {}) at {}",
            token.syntax(),
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
fn parse_var_name(input: &mut TokenStream) -> Result<(String, Position), ParseError> {
    match input.next().expect(NEVER_ENDS) {
        // Variable name
        (Token::Identifier(s), pos) => Ok((s, pos)),
        // Reserved keyword
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            Err(PERR::Reserved(s).into_err(pos))
        }
        // Bad identifier
        (Token::LexError(err), pos) => Err(err.into_err(pos)),
        // Not a variable name
        (_, pos) => Err(PERR::VariableExpected.into_err(pos)),
    }
}

/// Parse `(` expr `)`
fn parse_paren_expr(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // ( ...
    settings.pos = eat_token(input, Token::LeftParen);

    if match_token(input, Token::RightParen).0 {
        return Ok(Expr::Unit(settings.pos));
    }

    let expr = parse_expr(input, state, lib, settings.level_up())?;

    match input.next().expect(NEVER_ENDS) {
        // ( xxx )
        (Token::RightParen, _) => Ok(expr),
        // ( <error>
        (Token::LexError(err), pos) => Err(err.into_err(pos)),
        // ( xxx ???
        (_, pos) => Err(PERR::MissingToken(
            Token::RightParen.into(),
            "for a matching ( in this expression".into(),
        )
        .into_err(pos)),
    }
}

/// Parse a function call.
fn parse_fn_call(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    id: Identifier,
    capture: bool,
    namespace: Option<NamespaceRef>,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, token_pos) = input.peek().expect(NEVER_ENDS);

    let mut namespace = namespace;
    let mut args = StaticVec::new();

    match token {
        // id( <EOF>
        Token::EOF => {
            return Err(PERR::MissingToken(
                Token::RightParen.into(),
                format!("to close the arguments list of this function call '{}'", id),
            )
            .into_err(*token_pos))
        }
        // id( <error>
        Token::LexError(err) => return Err(err.clone().into_err(*token_pos)),
        // id()
        Token::RightParen => {
            eat_token(input, Token::RightParen);

            let hash = namespace.as_mut().map_or_else(
                || calc_fn_hash(&id, 0),
                |modules| {
                    #[cfg(not(feature = "no_module"))]
                    modules.set_index(state.find_module(&modules[0].name));

                    calc_qualified_fn_hash(modules.iter().map(|m| m.name.as_str()), &id, 0)
                },
            );

            let hashes = if is_valid_identifier(id.chars()) {
                FnCallHashes::from_script(hash)
            } else {
                FnCallHashes::from_native(hash)
            };

            args.shrink_to_fit();

            return Ok(FnCallExpr {
                name: state.get_identifier(id),
                capture,
                namespace,
                hashes,
                args,
                ..Default::default()
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
            (Token::RightParen, _) => (),
            _ => args.push(parse_expr(input, state, lib, settings)?),
        }

        match input.peek().expect(NEVER_ENDS) {
            // id(...args)
            (Token::RightParen, _) => {
                eat_token(input, Token::RightParen);

                let hash = namespace.as_mut().map_or_else(
                    || calc_fn_hash(&id, args.len()),
                    |modules| {
                        #[cfg(not(feature = "no_module"))]
                        modules.set_index(state.find_module(&modules[0].name));

                        calc_qualified_fn_hash(
                            modules.iter().map(|m| m.name.as_str()),
                            &id,
                            args.len(),
                        )
                    },
                );

                let hashes = if is_valid_identifier(id.chars()) {
                    FnCallHashes::from_script(hash)
                } else {
                    FnCallHashes::from_native(hash)
                };

                args.shrink_to_fit();

                return Ok(FnCallExpr {
                    name: state.get_identifier(id),
                    capture,
                    namespace,
                    hashes,
                    args,
                    ..Default::default()
                }
                .into_fn_call_expr(settings.pos));
            }
            // id(...args,
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            // id(...args <EOF>
            (Token::EOF, pos) => {
                return Err(PERR::MissingToken(
                    Token::RightParen.into(),
                    format!("to close the arguments list of this function call '{}'", id),
                )
                .into_err(*pos))
            }
            // id(...args <error>
            (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
            // id(...args ???
            (_, pos) => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    format!("to separate the arguments to function call '{}'", id),
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
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    lhs: Expr,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let idx_expr = parse_expr(input, state, lib, settings.level_up())?;

    // Check type of indexing - must be integer or string
    match idx_expr {
        Expr::IntegerConstant(_, pos) => match lhs {
            Expr::IntegerConstant(_, _)
            | Expr::Array(_, _)
            | Expr::StringConstant(_, _)
            | Expr::InterpolatedString(_, _) => (),

            Expr::Map(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Object map access expects string index, not a number".into(),
                )
                .into_err(pos))
            }

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            Expr::CharConstant(_, _)
            | Expr::And(_, _)
            | Expr::Or(_, _)
            | Expr::BoolConstant(_, _)
            | Expr::Unit(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            _ => (),
        },

        // lhs[string]
        Expr::StringConstant(_, _) | Expr::InterpolatedString(_, _) => match lhs {
            Expr::Map(_, _) => (),

            Expr::Array(_, _) | Expr::StringConstant(_, _) | Expr::InterpolatedString(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Array or string expects numeric index, not a string".into(),
                )
                .into_err(idx_expr.position()))
            }

            #[cfg(not(feature = "no_float"))]
            Expr::FloatConstant(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            Expr::CharConstant(_, _)
            | Expr::And(_, _)
            | Expr::Or(_, _)
            | Expr::BoolConstant(_, _)
            | Expr::Unit(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            _ => (),
        },

        // lhs[float]
        #[cfg(not(feature = "no_float"))]
        x @ Expr::FloatConstant(_, _) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a float".into(),
            )
            .into_err(x.position()))
        }
        // lhs[char]
        x @ Expr::CharConstant(_, _) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a character".into(),
            )
            .into_err(x.position()))
        }
        // lhs[()]
        x @ Expr::Unit(_) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not ()".into(),
            )
            .into_err(x.position()))
        }
        // lhs[??? && ???], lhs[??? || ???]
        x @ Expr::And(_, _) | x @ Expr::Or(_, _) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // lhs[true], lhs[false]
        x @ Expr::BoolConstant(_, _) => {
            return Err(PERR::MalformedIndexExpr(
                "Array access expects integer index, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // All other expressions
        _ => (),
    }

    // Check if there is a closing bracket
    match input.peek().expect(NEVER_ENDS) {
        (Token::RightBracket, _) => {
            eat_token(input, Token::RightBracket);

            // Any more indexing following?
            match input.peek().expect(NEVER_ENDS) {
                // If another indexing level, right-bind it
                (Token::LeftBracket, _) => {
                    let prev_pos = settings.pos;
                    settings.pos = eat_token(input, Token::LeftBracket);
                    // Recursively parse the indexing chain, right-binding each
                    let idx_expr =
                        parse_index_chain(input, state, lib, idx_expr, settings.level_up())?;
                    // Indexing binds to right
                    Ok(Expr::Index(
                        BinaryExpr { lhs, rhs: idx_expr }.into(),
                        false,
                        prev_pos,
                    ))
                }
                // Otherwise terminate the indexing chain
                _ => Ok(Expr::Index(
                    BinaryExpr { lhs, rhs: idx_expr }.into(),
                    true,
                    settings.pos,
                )),
            }
        }
        (Token::LexError(err), pos) => Err(err.clone().into_err(*pos)),
        (_, pos) => Err(PERR::MissingToken(
            Token::RightBracket.into(),
            "for a matching [ in this index expression".into(),
        )
        .into_err(*pos)),
    }
}

/// Parse an array literal.
#[cfg(not(feature = "no_index"))]
fn parse_array_literal(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // [ ...
    settings.pos = eat_token(input, Token::LeftBracket);

    let mut arr = StaticVec::new();

    loop {
        const MISSING_RBRACKET: &str = "to end this array literal";

        #[cfg(not(feature = "unchecked"))]
        if state.engine.max_array_size() > 0 && arr.len() >= state.engine.max_array_size() {
            return Err(PERR::LiteralTooLarge(
                "Size of array literal".to_string(),
                state.engine.max_array_size(),
            )
            .into_err(input.peek().expect(NEVER_ENDS).1));
        }

        match input.peek().expect(NEVER_ENDS) {
            (Token::RightBracket, _) => {
                eat_token(input, Token::RightBracket);
                break;
            }
            (Token::EOF, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBracket.into(), MISSING_RBRACKET.into())
                        .into_err(*pos),
                )
            }
            _ => {
                let expr = parse_expr(input, state, lib, settings.level_up())?;
                arr.push(expr);
            }
        }

        match input.peek().expect(NEVER_ENDS) {
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            (Token::RightBracket, _) => (),
            (Token::EOF, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBracket.into(), MISSING_RBRACKET.into())
                        .into_err(*pos),
                )
            }
            (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
            (_, pos) => {
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
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // #{ ...
    settings.pos = eat_token(input, Token::MapStart);

    let mut map: StaticVec<(Ident, Expr)> = Default::default();
    let mut template: BTreeMap<Identifier, crate::Dynamic> = Default::default();

    loop {
        const MISSING_RBRACE: &str = "to end this object map literal";

        match input.peek().expect(NEVER_ENDS) {
            (Token::RightBrace, _) => {
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
            (Token::Identifier(s), pos) | (Token::StringConstant(s), pos) => {
                if map.iter().any(|(p, _)| p.name == s) {
                    return Err(PERR::DuplicatedProperty(s).into_err(pos));
                }
                (s, pos)
            }
            (Token::InterpolatedString(_), pos) => return Err(PERR::PropertyExpected.into_err(pos)),
            (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                return Err(PERR::Reserved(s).into_err(pos));
            }
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (Token::EOF, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(pos),
                );
            }
            (_, pos) if map.is_empty() => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(pos),
                );
            }
            (_, pos) => return Err(PERR::PropertyExpected.into_err(pos)),
        };

        match input.next().expect(NEVER_ENDS) {
            (Token::Colon, _) => (),
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (_, pos) => {
                return Err(PERR::MissingToken(
                    Token::Colon.into(),
                    format!(
                        "to follow the property '{}' in this object map literal",
                        name
                    ),
                )
                .into_err(pos))
            }
        };

        #[cfg(not(feature = "unchecked"))]
        if state.engine.max_map_size() > 0 && map.len() >= state.engine.max_map_size() {
            return Err(PERR::LiteralTooLarge(
                "Number of properties in object map literal".to_string(),
                state.engine.max_map_size(),
            )
            .into_err(input.peek().expect(NEVER_ENDS).1));
        }

        let expr = parse_expr(input, state, lib, settings.level_up())?;
        let name = state.get_identifier(name);
        template.insert(name.clone(), Default::default());
        map.push((Ident { name, pos }, expr));

        match input.peek().expect(NEVER_ENDS) {
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            (Token::RightBrace, _) => (),
            (Token::Identifier(_), pos) => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "to separate the items of this object map literal".into(),
                )
                .into_err(*pos))
            }
            (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
            (_, pos) => {
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
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // switch ...
    settings.pos = eat_token(input, Token::Switch);

    let item = parse_expr(input, state, lib, settings.level_up())?;

    match input.next().expect(NEVER_ENDS) {
        (Token::LeftBrace, _) => (),
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => {
            return Err(PERR::MissingToken(
                Token::LeftBrace.into(),
                "to start a switch block".into(),
            )
            .into_err(pos))
        }
    }

    let mut table = BTreeMap::<u64, Box<(Option<Expr>, StmtBlock)>>::new();
    let mut def_pos = Position::NONE;
    let mut def_stmt = None;

    loop {
        const MISSING_RBRACE: &str = "to end this switch block";

        let (expr, condition) = match input.peek().expect(NEVER_ENDS) {
            (Token::RightBrace, _) => {
                eat_token(input, Token::RightBrace);
                break;
            }
            (Token::EOF, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(*pos),
                )
            }
            (Token::Underscore, pos) if def_stmt.is_none() => {
                def_pos = *pos;
                eat_token(input, Token::Underscore);

                let (if_clause, if_pos) = match_token(input, Token::If);

                if if_clause {
                    return Err(PERR::WrongSwitchCaseCondition.into_err(if_pos));
                }

                (None, None)
            }
            (Token::Underscore, pos) => return Err(PERR::DuplicatedSwitchCase.into_err(*pos)),
            _ if def_stmt.is_some() => return Err(PERR::WrongSwitchDefaultCase.into_err(def_pos)),

            _ => {
                let case_expr = Some(parse_expr(input, state, lib, settings.level_up())?);

                let condition = if match_token(input, Token::If).0 {
                    Some(parse_expr(input, state, lib, settings.level_up())?)
                } else {
                    None
                };
                (case_expr, condition)
            }
        };

        let hash = if let Some(expr) = expr {
            if let Some(value) = expr.get_literal_value() {
                let hasher = &mut get_hasher();
                value.hash(hasher);
                let hash = hasher.finish();

                if table.contains_key(&hash) {
                    return Err(PERR::DuplicatedSwitchCase.into_err(expr.position()));
                }

                Some(hash)
            } else {
                return Err(PERR::ExprExpected("a literal".to_string()).into_err(expr.position()));
            }
        } else {
            None
        };

        match input.next().expect(NEVER_ENDS) {
            (Token::DoubleArrow, _) => (),
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (_, pos) => {
                return Err(PERR::MissingToken(
                    Token::DoubleArrow.into(),
                    "in this switch case".to_string(),
                )
                .into_err(pos))
            }
        };

        let stmt = parse_stmt(input, state, lib, settings.level_up())?;

        let need_comma = !stmt.is_self_terminated();

        def_stmt = if let Some(hash) = hash {
            table.insert(hash, (condition, stmt.into()).into());
            None
        } else {
            Some(stmt.into())
        };

        match input.peek().expect(NEVER_ENDS) {
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            (Token::RightBrace, _) => (),
            (Token::EOF, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightParen.into(), MISSING_RBRACE.into())
                        .into_err(*pos),
                )
            }
            (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
            (_, pos) if need_comma => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "to separate the items in this switch block".into(),
                )
                .into_err(*pos))
            }
            (_, _) => (),
        }
    }

    let def_stmt_block = def_stmt.unwrap_or_else(|| Stmt::Noop(Position::NONE).into());

    Ok(Stmt::Switch(
        item,
        (table, def_stmt_block).into(),
        settings.pos,
    ))
}

/// Parse a primary expression.
fn parse_primary(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, token_pos) = input.peek().expect(NEVER_ENDS);
    settings.pos = *token_pos;

    let mut root_expr = match token {
        Token::EOF => return Err(PERR::UnexpectedEOF.into_err(settings.pos)),

        Token::IntegerConstant(_)
        | Token::CharConstant(_)
        | Token::StringConstant(_)
        | Token::True
        | Token::False => match input.next().expect(NEVER_ENDS).0 {
            Token::IntegerConstant(x) => Expr::IntegerConstant(x, settings.pos),
            Token::CharConstant(c) => Expr::CharConstant(c, settings.pos),
            Token::StringConstant(s) => {
                Expr::StringConstant(state.get_identifier(s).into(), settings.pos)
            }
            Token::True => Expr::BoolConstant(true, settings.pos),
            Token::False => Expr::BoolConstant(false, settings.pos),
            _ => unreachable!(),
        },
        #[cfg(not(feature = "no_float"))]
        Token::FloatConstant(x) => {
            let x = *x;
            input.next().expect(NEVER_ENDS);
            Expr::FloatConstant(x, settings.pos)
        }
        #[cfg(feature = "decimal")]
        Token::DecimalConstant(x) => {
            let x = (*x).into();
            input.next().expect(NEVER_ENDS);
            Expr::DynamicConstant(Box::new(x), settings.pos)
        }

        // { - block statement as expression
        Token::LeftBrace if settings.allow_stmt_expr => {
            match parse_block(input, state, lib, settings.level_up())? {
                block @ Stmt::Block(_, _) => Expr::Stmt(Box::new(block.into())),
                stmt => unreachable!("expecting Stmt::Block, but gets {:?}", stmt),
            }
        }
        // ( - grouped expression
        Token::LeftParen => parse_paren_expr(input, state, lib, settings.level_up())?,

        // If statement is allowed to act as expressions
        Token::If if settings.allow_if_expr => Expr::Stmt(Box::new(
            parse_if(input, state, lib, settings.level_up())?.into(),
        )),
        // Switch statement is allowed to act as expressions
        Token::Switch if settings.allow_switch_expr => Expr::Stmt(Box::new(
            parse_switch(input, state, lib, settings.level_up())?.into(),
        )),

        // | ...
        #[cfg(not(feature = "no_function"))]
        Token::Pipe | Token::Or if settings.allow_anonymous_fn => {
            let mut new_state = ParseState::new(state.engine, state.tokenizer_control.clone());

            #[cfg(not(feature = "unchecked"))]
            {
                new_state.max_expr_depth = new_state.max_function_expr_depth;
            }

            let settings = ParseSettings {
                allow_if_expr: true,
                allow_switch_expr: true,
                allow_stmt_expr: true,
                allow_anonymous_fn: true,
                is_global: false,
                is_function_scope: true,
                is_breakable: false,
                level: 0,
                pos: settings.pos,
            };

            let (expr, func) = parse_anon_fn(input, &mut new_state, lib, settings)?;

            #[cfg(not(feature = "no_closure"))]
            new_state.external_vars.iter().for_each(|(closure, &pos)| {
                state.access_var(closure, pos);
            });

            let hash_script = calc_fn_hash(&func.name, func.params.len());
            lib.insert(hash_script, func.into());

            expr
        }

        // Interpolated string
        Token::InterpolatedString(_) => {
            let mut segments: StaticVec<Expr> = Default::default();

            if let (Token::InterpolatedString(s), pos) = input.next().expect(NEVER_ENDS) {
                segments.push(Expr::StringConstant(s.into(), pos));
            } else {
                unreachable!();
            }

            loop {
                let expr = match parse_block(input, state, lib, settings.level_up())? {
                    block @ Stmt::Block(_, _) => Expr::Stmt(Box::new(block.into())),
                    stmt => unreachable!("expecting Stmt::Block, but gets {:?}", stmt),
                };
                segments.push(expr);

                // Make sure to parse the following as text
                let mut control = state.tokenizer_control.get();
                control.is_within_text = true;
                state.tokenizer_control.set(control);

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
                    (Token::LexError(err @ LexError::UnterminatedString), pos) => {
                        return Err(err.into_err(pos))
                    }
                    (token, _) => unreachable!(
                        "expected a string within an interpolated string literal, but gets {:?}",
                        token
                    ),
                }
            }

            segments.shrink_to_fit();
            Expr::InterpolatedString(segments.into(), settings.pos)
        }

        // Array literal
        #[cfg(not(feature = "no_index"))]
        Token::LeftBracket => parse_array_literal(input, state, lib, settings.level_up())?,

        // Map literal
        #[cfg(not(feature = "no_object"))]
        Token::MapStart => parse_map_literal(input, state, lib, settings.level_up())?,

        // Identifier
        Token::Identifier(_) => {
            let s = match input.next().expect(NEVER_ENDS) {
                (Token::Identifier(s), _) => s,
                _ => unreachable!(),
            };

            match input.peek().expect(NEVER_ENDS).0 {
                // Function call
                Token::LeftParen | Token::Bang => {
                    #[cfg(not(feature = "no_closure"))]
                    {
                        // Once the identifier consumed we must enable next variables capturing
                        state.allow_capture = true;
                    }
                    Expr::Variable(
                        None,
                        settings.pos,
                        (None, None, state.get_identifier(s)).into(),
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
                    Expr::Variable(
                        None,
                        settings.pos,
                        (None, None, state.get_identifier(s)).into(),
                    )
                }
                // Normal variable access
                _ => {
                    let index = state.access_var(&s, settings.pos);
                    let short_index = index.and_then(|x| {
                        if x.get() <= u8::MAX as usize {
                            NonZeroU8::new(x.get() as u8)
                        } else {
                            None
                        }
                    });
                    Expr::Variable(
                        short_index,
                        settings.pos,
                        (index, None, state.get_identifier(s)).into(),
                    )
                }
            }
        }

        // Reserved keyword or symbol
        Token::Reserved(_) => {
            let s = match input.next().expect(NEVER_ENDS) {
                (Token::Reserved(s), _) => s,
                _ => unreachable!(),
            };

            match input.peek().expect(NEVER_ENDS).0 {
                // Function call is allowed to have reserved keyword
                Token::LeftParen | Token::Bang if is_keyword_function(&s) => Expr::Variable(
                    None,
                    settings.pos,
                    (None, None, state.get_identifier(s)).into(),
                ),
                // Access to `this` as a variable is OK within a function scope
                _ if s == KEYWORD_THIS && settings.is_function_scope => Expr::Variable(
                    None,
                    settings.pos,
                    (None, None, state.get_identifier(s)).into(),
                ),
                // Cannot access to `this` as a variable not in a function scope
                _ if s == KEYWORD_THIS => {
                    let msg = format!("'{}' can only be used in functions", s);
                    return Err(LexError::ImproperSymbol(s, msg).into_err(settings.pos));
                }
                _ if is_valid_identifier(s.chars()) => {
                    return Err(PERR::Reserved(s).into_err(settings.pos))
                }
                _ => return Err(LexError::UnexpectedInput(s).into_err(settings.pos)),
            }
        }

        Token::LexError(_) => match input.next().expect(NEVER_ENDS) {
            (Token::LexError(err), _) => return Err(err.into_err(settings.pos)),
            _ => unreachable!(),
        },

        _ => {
            return Err(LexError::UnexpectedInput(token.syntax().to_string()).into_err(settings.pos))
        }
    };

    // Tail processing all possible postfix operators
    loop {
        let (tail_token, _) = input.peek().expect(NEVER_ENDS);

        if !root_expr.is_valid_postfix(tail_token) {
            break;
        }

        let (tail_token, tail_pos) = input.next().expect(NEVER_ENDS);
        settings.pos = tail_pos;

        root_expr = match (root_expr, tail_token) {
            // Qualified function call with !
            (Expr::Variable(_, _, x), Token::Bang) if x.1.is_some() => {
                return Err(if !match_token(input, Token::LeftParen).0 {
                    LexError::UnexpectedInput(Token::Bang.syntax().to_string()).into_err(tail_pos)
                } else {
                    LexError::ImproperSymbol(
                        "!".to_string(),
                        "'!' cannot be used to call module functions".to_string(),
                    )
                    .into_err(tail_pos)
                });
            }
            // Function call with !
            (Expr::Variable(_, var_pos, x), Token::Bang) => {
                let (matched, pos) = match_token(input, Token::LeftParen);
                if !matched {
                    return Err(PERR::MissingToken(
                        Token::LeftParen.syntax().into(),
                        "to start arguments list of function call".into(),
                    )
                    .into_err(pos));
                }

                let (_, namespace, name) = *x;
                settings.pos = var_pos;
                let ns = namespace.map(|(ns, _)| ns);
                parse_fn_call(input, state, lib, name, true, ns, settings.level_up())?
            }
            // Function call
            (Expr::Variable(_, var_pos, x), Token::LeftParen) => {
                let (_, namespace, name) = *x;
                settings.pos = var_pos;
                let ns = namespace.map(|(ns, _)| ns);
                parse_fn_call(input, state, lib, name, false, ns, settings.level_up())?
            }
            // module access
            (Expr::Variable(_, var_pos, x), Token::DoubleColon) => {
                let (id2, pos2) = parse_var_name(input)?;
                let (_, mut namespace, var_name) = *x;
                let var_name_def = Ident {
                    name: var_name,
                    pos: var_pos,
                };

                if let Some((ref mut namespace, _)) = namespace {
                    namespace.push(var_name_def);
                } else {
                    let mut ns: NamespaceRef = Default::default();
                    ns.push(var_name_def);
                    namespace = Some((ns, 42));
                }

                Expr::Variable(
                    None,
                    pos2,
                    (None, namespace, state.get_identifier(id2)).into(),
                )
            }
            // Indexing
            #[cfg(not(feature = "no_index"))]
            (expr, Token::LeftBracket) => {
                parse_index_chain(input, state, lib, expr, settings.level_up())?
            }
            // Property access
            #[cfg(not(feature = "no_object"))]
            (expr, Token::Period) => {
                // Expression after dot must start with an identifier
                match input.peek().expect(NEVER_ENDS) {
                    (Token::Identifier(_), _) => {
                        #[cfg(not(feature = "no_closure"))]
                        {
                            // Prevents capturing of the object properties as vars: xxx.<var>
                            state.allow_capture = false;
                        }
                    }
                    (Token::Reserved(s), _) if is_keyword_function(s) => (),
                    (_, pos) => return Err(PERR::PropertyExpected.into_err(*pos)),
                }

                let rhs = parse_primary(input, state, lib, settings.level_up())?;

                make_dot_expr(state, expr, false, rhs, tail_pos)?
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
    let namespaced_variable = match root_expr {
        Expr::Variable(_, _, ref mut x) if x.1.is_some() => Some(x.as_mut()),
        Expr::Index(ref mut x, _, _) | Expr::Dot(ref mut x, _, _) => match x.lhs {
            Expr::Variable(_, _, ref mut x) if x.1.is_some() => Some(x.as_mut()),
            _ => None,
        },
        _ => None,
    };

    if let Some(x) = namespaced_variable {
        match x {
            (_, Some((namespace, hash)), name) => {
                *hash = calc_qualified_var_hash(namespace.iter().map(|v| v.name.as_str()), name);

                #[cfg(not(feature = "no_module"))]
                namespace.set_index(state.find_module(&namespace[0].name));
            }
            _ => unreachable!("expecting namespace-qualified variable access"),
        }
    }

    // Make sure identifiers are valid
    Ok(root_expr)
}

/// Parse a potential unary operator.
fn parse_unary(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    let (token, token_pos) = input.peek().expect(NEVER_ENDS);
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    match token {
        // -expr
        Token::UnaryMinus => {
            let pos = eat_token(input, Token::UnaryMinus);

            match parse_unary(input, state, lib, settings.level_up())? {
                // Negative integer
                Expr::IntegerConstant(num, _) => num
                    .checked_neg()
                    .map(|i| Expr::IntegerConstant(i, pos))
                    .or_else(|| {
                        #[cfg(not(feature = "no_float"))]
                        return Some(Expr::FloatConstant((-(num as FLOAT)).into(), pos));
                        #[cfg(feature = "no_float")]
                        return None;
                    })
                    .ok_or_else(|| LexError::MalformedNumber(format!("-{}", num)).into_err(pos)),

                // Negative float
                #[cfg(not(feature = "no_float"))]
                Expr::FloatConstant(x, _) => Ok(Expr::FloatConstant((-(*x)).into(), pos)),

                // Call negative function
                expr => {
                    let mut args = StaticVec::new();
                    args.push(expr);
                    args.shrink_to_fit();

                    Ok(FnCallExpr {
                        name: state.get_identifier("-"),
                        hashes: FnCallHashes::from_native(calc_fn_hash("-", 1)),
                        args,
                        ..Default::default()
                    }
                    .into_fn_call_expr(pos))
                }
            }
        }
        // +expr
        Token::UnaryPlus => {
            let pos = eat_token(input, Token::UnaryPlus);

            match parse_unary(input, state, lib, settings.level_up())? {
                expr @ Expr::IntegerConstant(_, _) => Ok(expr),
                #[cfg(not(feature = "no_float"))]
                expr @ Expr::FloatConstant(_, _) => Ok(expr),

                // Call plus function
                expr => {
                    let mut args = StaticVec::new();
                    args.push(expr);
                    args.shrink_to_fit();

                    Ok(FnCallExpr {
                        name: state.get_identifier("+"),
                        hashes: FnCallHashes::from_native(calc_fn_hash("+", 1)),
                        args,
                        ..Default::default()
                    }
                    .into_fn_call_expr(pos))
                }
            }
        }
        // !expr
        Token::Bang => {
            let pos = eat_token(input, Token::Bang);
            let mut args = StaticVec::new();
            args.push(parse_unary(input, state, lib, settings.level_up())?);
            args.shrink_to_fit();

            Ok(FnCallExpr {
                name: state.get_identifier("!"),
                hashes: FnCallHashes::from_native(calc_fn_hash("!", 1)),
                args,
                ..Default::default()
            }
            .into_fn_call_expr(pos))
        }
        // <EOF>
        Token::EOF => Err(PERR::UnexpectedEOF.into_err(settings.pos)),
        // All other tokens
        _ => parse_primary(input, state, lib, settings.level_up()),
    }
}

/// Make an assignment statement.
fn make_assignment_stmt(
    op: Option<Token>,
    state: &mut ParseState,
    lhs: Expr,
    rhs: Expr,
    op_pos: Position,
) -> Result<Stmt, ParseError> {
    #[must_use]
    fn check_lvalue(expr: &Expr, parent_is_dot: bool) -> Option<Position> {
        match expr {
            Expr::Index(x, _, _) | Expr::Dot(x, _, _) if parent_is_dot => match x.lhs {
                Expr::Property(_) => check_lvalue(&x.rhs, matches!(expr, Expr::Dot(_, _, _))),
                ref e => Some(e.position()),
            },
            Expr::Index(x, _, _) | Expr::Dot(x, _, _) => match x.lhs {
                Expr::Property(_) => unreachable!("unexpected Expr::Property in indexing"),
                _ => check_lvalue(&x.rhs, matches!(expr, Expr::Dot(_, _, _))),
            },
            Expr::Property(_) if parent_is_dot => None,
            Expr::Property(_) => unreachable!("unexpected Expr::Property in indexing"),
            e if parent_is_dot => Some(e.position()),
            _ => None,
        }
    }

    let op_info = op.map(OpAssignment::new);

    match lhs {
        // const_expr = rhs
        ref expr if expr.is_constant() => {
            Err(PERR::AssignmentToConstant("".into()).into_err(lhs.position()))
        }
        // var (non-indexed) = rhs
        Expr::Variable(None, _, ref x) if x.0.is_none() => {
            Ok(Stmt::Assignment((lhs, op_info, rhs).into(), op_pos))
        }
        // var (indexed) = rhs
        Expr::Variable(i, var_pos, ref x) => {
            let (index, _, name) = x.as_ref();
            let index = i.map_or_else(
                || {
                    index
                        .expect(
                            "never fails because the long index is `Some` when the short index is `None`",
                        )
                        .get()
                },
                |n| n.get() as usize,
            );
            match state.stack[state.stack.len() - index].1 {
                AccessMode::ReadWrite => Ok(Stmt::Assignment((lhs, op_info, rhs).into(), op_pos)),
                // Constant values cannot be assigned to
                AccessMode::ReadOnly => {
                    Err(PERR::AssignmentToConstant(name.to_string()).into_err(var_pos))
                }
            }
        }
        // xxx[???]... = rhs, xxx.prop... = rhs
        Expr::Index(ref x, _, _) | Expr::Dot(ref x, _, _) => {
            match check_lvalue(&x.rhs, matches!(lhs, Expr::Dot(_, _, _))) {
                None => match x.lhs {
                    // var[???] = rhs, var.??? = rhs
                    Expr::Variable(_, _, _) => {
                        Ok(Stmt::Assignment((lhs, op_info, rhs).into(), op_pos))
                    }
                    // expr[???] = rhs, expr.??? = rhs
                    ref expr => {
                        Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(expr.position()))
                    }
                },
                Some(pos) => Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(pos)),
            }
        }
        // ??? && ??? = rhs, ??? || ??? = rhs
        Expr::And(_, _) | Expr::Or(_, _) => Err(LexError::ImproperSymbol(
            "=".to_string(),
            "Possibly a typo of '=='?".to_string(),
        )
        .into_err(op_pos)),
        // expr = rhs
        _ => Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(lhs.position())),
    }
}

/// Parse an operator-assignment expression.
fn parse_op_assignment_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    lhs: Expr,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, token_pos) = input.peek().expect(NEVER_ENDS);
    settings.pos = *token_pos;

    let (op, pos) = match token {
        Token::Equals => (None, input.next().expect(NEVER_ENDS).1),
        _ if token.is_op_assignment() => input
            .next()
            .map(|(op, pos)| (Some(op), pos))
            .expect(NEVER_ENDS),
        _ => return Ok(Stmt::Expr(lhs)),
    };

    let rhs = parse_expr(input, state, lib, settings.level_up())?;
    make_assignment_stmt(op, state, lhs, rhs, pos)
}

/// Make a dot expression.
#[cfg(not(feature = "no_object"))]
fn make_dot_expr(
    state: &mut ParseState,
    lhs: Expr,
    terminate_chaining: bool,
    rhs: Expr,
    op_pos: Position,
) -> Result<Expr, ParseError> {
    Ok(match (lhs, rhs) {
        // lhs[???]...[???].rhs
        (Expr::Index(mut x, false, pos), rhs) if !terminate_chaining => {
            // Attach dot chain to the bottom level of indexing chain
            x.rhs = make_dot_expr(state, x.rhs, false, rhs, op_pos)?;
            Expr::Index(x, false, pos)
        }
        // lhs[idx_expr].rhs
        (Expr::Index(mut x, _, pos), rhs) => {
            x.rhs = make_dot_expr(state, x.rhs, true, rhs, op_pos)?;
            Expr::Index(x, false, pos)
        }
        // lhs.id
        (lhs, var_expr @ Expr::Variable(_, _, _)) if var_expr.is_variable_access(true) => {
            let rhs = var_expr.into_property(state);
            Expr::Dot(BinaryExpr { lhs, rhs }.into(), false, op_pos)
        }
        // lhs.module::id - syntax error
        (_, Expr::Variable(_, _, x)) => {
            return Err(PERR::PropertyExpected
                .into_err(x.1.expect("never fails because the namespace is `Some`").0[0].pos))
        }
        // lhs.prop
        (lhs, prop @ Expr::Property(_)) => {
            Expr::Dot(BinaryExpr { lhs, rhs: prop }.into(), false, op_pos)
        }
        // lhs.dot_lhs.dot_rhs or lhs.dot_lhs[idx_rhs]
        (lhs, rhs @ Expr::Dot(_, _, _)) | (lhs, rhs @ Expr::Index(_, _, _)) => {
            let (x, term, pos, is_dot) = match rhs {
                Expr::Dot(x, term, pos) => (x, term, pos, true),
                Expr::Index(x, term, pos) => (x, term, pos, false),
                _ => unreachable!(),
            };

            match x.lhs {
                Expr::Variable(_, _, _) | Expr::Property(_) => {
                    let new_lhs = BinaryExpr {
                        lhs: x.lhs.into_property(state),
                        rhs: x.rhs,
                    }
                    .into();

                    let rhs = if is_dot {
                        Expr::Dot(new_lhs, term, pos)
                    } else {
                        Expr::Index(new_lhs, term, pos)
                    };
                    Expr::Dot(BinaryExpr { lhs, rhs }.into(), false, op_pos)
                }
                Expr::FnCall(mut func, func_pos) => {
                    // Recalculate hash
                    func.hashes = FnCallHashes::from_script_and_native(
                        calc_fn_hash(&func.name, func.args.len()),
                        calc_fn_hash(&func.name, func.args.len() + 1),
                    );

                    let new_lhs = BinaryExpr {
                        lhs: Expr::FnCall(func, func_pos),
                        rhs: x.rhs,
                    }
                    .into();

                    let rhs = if is_dot {
                        Expr::Dot(new_lhs, term, pos)
                    } else {
                        Expr::Index(new_lhs, term, pos)
                    };
                    Expr::Dot(BinaryExpr { lhs, rhs }.into(), false, op_pos)
                }
                _ => unreachable!("invalid dot expression: {:?}", x.lhs),
            }
        }
        // lhs.nnn::func(...)
        (_, Expr::FnCall(x, _)) if x.is_qualified() => {
            unreachable!("method call should not be namespace-qualified")
        }
        // lhs.Fn() or lhs.eval()
        (_, Expr::FnCall(x, pos))
            if x.args.is_empty()
                && [crate::engine::KEYWORD_FN_PTR, crate::engine::KEYWORD_EVAL]
                    .contains(&x.name.as_ref()) =>
        {
            return Err(LexError::ImproperSymbol(
                x.name.to_string(),
                format!(
                    "'{}' should not be called in method style. Try {}(...);",
                    x.name, x.name
                ),
            )
            .into_err(pos))
        }
        // lhs.func!(...)
        (_, Expr::FnCall(x, pos)) if x.capture => {
            return Err(PERR::MalformedCapture(
                "method-call style does not support capturing".into(),
            )
            .into_err(pos))
        }
        // lhs.func(...)
        (lhs, Expr::FnCall(mut func, func_pos)) => {
            // Recalculate hash
            func.hashes = FnCallHashes::from_script_and_native(
                calc_fn_hash(&func.name, func.args.len()),
                calc_fn_hash(&func.name, func.args.len() + 1),
            );
            let rhs = Expr::FnCall(func, func_pos);
            Expr::Dot(BinaryExpr { lhs, rhs }.into(), false, op_pos)
        }
        // lhs.rhs
        (_, rhs) => return Err(PERR::PropertyExpected.into_err(rhs.position())),
    })
}

/// Parse a binary expression.
fn parse_binary_op(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    parent_precedence: Option<Precedence>,
    lhs: Expr,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = lhs.position();

    let mut root = lhs;

    loop {
        let (current_op, current_pos) = input.peek().expect(NEVER_ENDS);
        let precedence = match current_op {
            Token::Custom(c) => state
                .engine
                .custom_keywords
                .get(c.as_str())
                .cloned()
                .ok_or_else(|| PERR::Reserved(c.clone()).into_err(*current_pos))?,
            Token::Reserved(c) if !is_valid_identifier(c.chars()) => {
                return Err(PERR::UnknownOperator(c.into()).into_err(*current_pos))
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

        let rhs = parse_unary(input, state, lib, settings)?;

        let (next_op, next_pos) = input.peek().expect(NEVER_ENDS);
        let next_precedence = match next_op {
            Token::Custom(c) => state
                .engine
                .custom_keywords
                .get(c.as_str())
                .cloned()
                .ok_or_else(|| PERR::Reserved(c.clone()).into_err(*next_pos))?,
            Token::Reserved(c) if !is_valid_identifier(c.chars()) => {
                return Err(PERR::UnknownOperator(c.into()).into_err(*next_pos))
            }
            _ => next_op.precedence(),
        };

        // Bind to right if the next operator has higher precedence
        // If same precedence, then check if the operator binds right
        let rhs = if (precedence == next_precedence && bind_right) || precedence < next_precedence {
            parse_binary_op(input, state, lib, precedence, rhs, settings)?
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
            name: state.get_identifier(op.as_ref()),
            hashes: FnCallHashes::from_native(hash),
            capture: false,
            ..Default::default()
        };

        let mut args = StaticVec::new();
        args.push(root);
        args.push(rhs);
        args.shrink_to_fit();

        root = match op_token {
            Token::Plus
            | Token::Minus
            | Token::Multiply
            | Token::Divide
            | Token::LeftShift
            | Token::RightShift
            | Token::Modulo
            | Token::PowerOf
            | Token::Ampersand
            | Token::Pipe
            | Token::XOr => FnCallExpr { args, ..op_base }.into_fn_call_expr(pos),

            // '!=' defaults to true when passed invalid operands
            Token::NotEqualsTo => FnCallExpr { args, ..op_base }.into_fn_call_expr(pos),

            // Comparison operators default to false when passed invalid operands
            Token::EqualsTo
            | Token::LessThan
            | Token::LessThanEqualsTo
            | Token::GreaterThan
            | Token::GreaterThanEqualsTo => FnCallExpr { args, ..op_base }.into_fn_call_expr(pos),

            Token::Or => {
                let rhs = args
                    .pop()
                    .expect("never fails because `||` has two arguments");
                let current_lhs = args
                    .pop()
                    .expect("never fails because `||` has two arguments");
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
                let rhs = args
                    .pop()
                    .expect("never fails because `&&` has two arguments");
                let current_lhs = args
                    .pop()
                    .expect("never fails because `&&` has two arguments");
                Expr::And(
                    BinaryExpr {
                        lhs: current_lhs.ensure_bool_expr()?,
                        rhs: rhs.ensure_bool_expr()?,
                    }
                    .into(),
                    pos,
                )
            }
            Token::In => {
                // Swap the arguments
                let current_lhs = args.remove(0);
                args.push(current_lhs);
                args.shrink_to_fit();

                // Convert into a call to `contains`
                FnCallExpr {
                    hashes: FnCallHashes::from_script(calc_fn_hash(OP_CONTAINS, 2)),
                    args,
                    name: state.get_identifier(OP_CONTAINS),
                    ..op_base
                }
                .into_fn_call_expr(pos)
            }

            Token::Custom(s)
                if state
                    .engine
                    .custom_keywords
                    .get(s.as_str())
                    .map_or(false, Option::is_some) =>
            {
                let hash = calc_fn_hash(&s, 2);

                FnCallExpr {
                    hashes: if is_valid_identifier(s.chars()) {
                        FnCallHashes::from_script(hash)
                    } else {
                        FnCallHashes::from_native(hash)
                    },
                    args,
                    ..op_base
                }
                .into_fn_call_expr(pos)
            }

            op_token => return Err(PERR::UnknownOperator(op_token.into()).into_err(pos)),
        };
    }
}

/// Parse a custom syntax.
fn parse_custom_syntax(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
    key: &str,
    syntax: &CustomSyntax,
    pos: Position,
) -> Result<Expr, ParseError> {
    let mut settings = settings;
    let mut keywords: StaticVec<Expr> = Default::default();
    let mut segments: StaticVec<_> = Default::default();
    let mut tokens: StaticVec<_> = Default::default();

    // Adjust the variables stack
    if syntax.scope_may_be_changed {
        // Add a barrier variable to the stack so earlier variables will not be matched.
        // Variable searches stop at the first barrier.
        let empty = state.get_identifier(SCOPE_SEARCH_BARRIER_MARKER);
        state.stack.push((empty, AccessMode::ReadWrite));
    }

    let parse_func = syntax.parse.as_ref();

    segments.push(key.into());
    tokens.push(key.into());

    loop {
        let (fwd_token, fwd_pos) = input.peek().expect(NEVER_ENDS);
        settings.pos = *fwd_pos;
        let settings = settings.level_up();

        let required_token = match parse_func(&segments, fwd_token.syntax().as_ref()) {
            Ok(Some(seg)) => seg,
            Ok(None) => break,
            Err(err) => return Err(err.0.into_err(settings.pos)),
        };

        match required_token.as_str() {
            CUSTOM_SYNTAX_MARKER_IDENT => {
                let (name, pos) = parse_var_name(input)?;
                let name = state.get_identifier(name);
                segments.push(name.clone().into());
                tokens.push(state.get_identifier(CUSTOM_SYNTAX_MARKER_IDENT));
                keywords.push(Expr::Variable(None, pos, (None, None, name).into()));
            }
            CUSTOM_SYNTAX_MARKER_EXPR => {
                keywords.push(parse_expr(input, state, lib, settings)?);
                let keyword = state.get_identifier(CUSTOM_SYNTAX_MARKER_EXPR);
                segments.push(keyword.clone().into());
                tokens.push(keyword);
            }
            CUSTOM_SYNTAX_MARKER_BLOCK => match parse_block(input, state, lib, settings)? {
                block @ Stmt::Block(_, _) => {
                    keywords.push(Expr::Stmt(Box::new(block.into())));
                    let keyword = state.get_identifier(CUSTOM_SYNTAX_MARKER_BLOCK);
                    segments.push(keyword.clone().into());
                    tokens.push(keyword);
                }
                stmt => unreachable!("expecting Stmt::Block, but gets {:?}", stmt),
            },
            CUSTOM_SYNTAX_MARKER_BOOL => match input.next().expect(NEVER_ENDS) {
                (b @ Token::True, pos) | (b @ Token::False, pos) => {
                    keywords.push(Expr::BoolConstant(b == Token::True, pos));
                    let keyword = state.get_identifier(CUSTOM_SYNTAX_MARKER_BOOL);
                    segments.push(keyword.clone().into());
                    tokens.push(keyword);
                }
                (_, pos) => {
                    return Err(
                        PERR::MissingSymbol("Expecting 'true' or 'false'".to_string())
                            .into_err(pos),
                    )
                }
            },
            CUSTOM_SYNTAX_MARKER_INT => match input.next().expect(NEVER_ENDS) {
                (Token::IntegerConstant(i), pos) => {
                    keywords.push(Expr::IntegerConstant(i, pos));
                    let keyword = state.get_identifier(CUSTOM_SYNTAX_MARKER_INT);
                    segments.push(keyword.clone().into());
                    tokens.push(keyword);
                }
                (_, pos) => {
                    return Err(
                        PERR::MissingSymbol("Expecting an integer number".to_string())
                            .into_err(pos),
                    )
                }
            },
            #[cfg(not(feature = "no_float"))]
            CUSTOM_SYNTAX_MARKER_FLOAT => match input.next().expect(NEVER_ENDS) {
                (Token::FloatConstant(f), pos) => {
                    keywords.push(Expr::FloatConstant(f, pos));
                    let keyword = state.get_identifier(CUSTOM_SYNTAX_MARKER_FLOAT);
                    segments.push(keyword.clone().into());
                    tokens.push(keyword);
                }
                (_, pos) => {
                    return Err(PERR::MissingSymbol(
                        "Expecting a floating-point number".to_string(),
                    )
                    .into_err(pos))
                }
            },
            CUSTOM_SYNTAX_MARKER_STRING => match input.next().expect(NEVER_ENDS) {
                (Token::StringConstant(s), pos) => {
                    keywords.push(Expr::StringConstant(state.get_identifier(s).into(), pos));
                    let keyword = state.get_identifier(CUSTOM_SYNTAX_MARKER_STRING);
                    segments.push(keyword.clone().into());
                    tokens.push(keyword);
                }
                (_, pos) => {
                    return Err(PERR::MissingSymbol("Expecting a string".to_string()).into_err(pos))
                }
            },
            s => match input.next().expect(NEVER_ENDS) {
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (t, _) if t.syntax().as_ref() == s => {
                    segments.push(required_token.clone());
                    tokens.push(required_token.clone().into());
                }
                (_, pos) => {
                    return Err(PERR::MissingToken(
                        s.to_string(),
                        format!("for '{}' expression", segments[0]),
                    )
                    .into_err(pos))
                }
            },
        }
    }

    keywords.shrink_to_fit();
    tokens.shrink_to_fit();

    Ok(Expr::Custom(
        CustomExpr {
            keywords,
            tokens,
            scope_may_be_changed: syntax.scope_may_be_changed,
        }
        .into(),
        pos,
    ))
}

/// Parse an expression.
fn parse_expr(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = input.peek().expect(NEVER_ENDS).1;

    // Check if it is a custom syntax.
    if !state.engine.custom_syntax.is_empty() {
        let (token, pos) = input.peek().expect(NEVER_ENDS);
        let token_pos = *pos;

        match token {
            Token::Custom(key) | Token::Reserved(key) | Token::Identifier(key) => {
                if let Some((key, syntax)) = state.engine.custom_syntax.get_key_value(key.as_str())
                {
                    input.next().expect(NEVER_ENDS);
                    return parse_custom_syntax(
                        input, state, lib, settings, key, syntax, token_pos,
                    );
                }
            }
            _ => (),
        }
    }

    // Parse expression normally.
    let lhs = parse_unary(input, state, lib, settings.level_up())?;
    parse_binary_op(
        input,
        state,
        lib,
        Precedence::new(1),
        lhs,
        settings.level_up(),
    )
}

/// Parse an if statement.
fn parse_if(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // if ...
    settings.pos = eat_token(input, Token::If);

    // if guard { if_body }
    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, state, lib, settings.level_up())?.ensure_bool_expr()?;
    ensure_not_assignment(input)?;
    let if_body = parse_block(input, state, lib, settings.level_up())?;

    // if guard { if_body } else ...
    let else_body = if match_token(input, Token::Else).0 {
        if let (Token::If, _) = input.peek().expect(NEVER_ENDS) {
            // if guard { if_body } else if ...
            parse_if(input, state, lib, settings.level_up())?
        } else {
            // if guard { if_body } else { else-body }
            parse_block(input, state, lib, settings.level_up())?
        }
    } else {
        Stmt::Noop(Position::NONE)
    };

    Ok(Stmt::If(
        guard,
        (if_body.into(), else_body.into()).into(),
        settings.pos,
    ))
}

/// Parse a while loop.
fn parse_while_loop(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // while|loops ...
    let (guard, token_pos) = match input.next().expect(NEVER_ENDS) {
        (Token::While, pos) => {
            ensure_not_statement_expr(input, "a boolean")?;
            let expr = parse_expr(input, state, lib, settings.level_up())?.ensure_bool_expr()?;
            ensure_not_assignment(input)?;
            (expr, pos)
        }
        (Token::Loop, pos) => (Expr::Unit(Position::NONE), pos),
        _ => unreachable!(),
    };
    settings.pos = token_pos;
    settings.is_breakable = true;

    let body = parse_block(input, state, lib, settings.level_up())?;

    Ok(Stmt::While(guard, Box::new(body.into()), settings.pos))
}

/// Parse a do loop.
fn parse_do(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // do ...
    settings.pos = eat_token(input, Token::Do);

    // do { body } [while|until] guard
    settings.is_breakable = true;
    let body = parse_block(input, state, lib, settings.level_up())?;

    let negated = match input.next().expect(NEVER_ENDS) {
        (Token::While, _) => AST_OPTION_NONE,
        (Token::Until, _) => AST_OPTION_NEGATED,
        (_, pos) => {
            return Err(
                PERR::MissingToken(Token::While.into(), "for the do statement".into())
                    .into_err(pos),
            )
        }
    };

    settings.is_breakable = false;

    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, state, lib, settings.level_up())?.ensure_bool_expr()?;
    ensure_not_assignment(input)?;

    Ok(Stmt::Do(
        Box::new(body.into()),
        guard,
        negated,
        settings.pos,
    ))
}

/// Parse a for loop.
fn parse_for(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // for ...
    settings.pos = eat_token(input, Token::For);

    // for name ...
    let (name, name_pos, counter_name, counter_pos) = if match_token(input, Token::LeftParen).0 {
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
            return Err(PERR::DuplicatedVariable(counter_name).into_err(counter_pos));
        }

        let (has_close_paren, pos) = match_token(input, Token::RightParen);
        if !has_close_paren {
            return Err(PERR::MissingToken(
                Token::RightParen.into(),
                "to close the iteration variable".into(),
            )
            .into_err(pos));
        }
        (name, name_pos, Some(counter_name), Some(counter_pos))
    } else {
        // name
        let (name, name_pos) = parse_var_name(input)?;
        (name, name_pos, None, None)
    };

    // for name in ...
    match input.next().expect(NEVER_ENDS) {
        (Token::In, _) => (),
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => {
            return Err(
                PERR::MissingToken(Token::In.into(), "after the iteration variable".into())
                    .into_err(pos),
            )
        }
    }

    // for name in expr { body }
    ensure_not_statement_expr(input, "a boolean")?;
    let expr = parse_expr(input, state, lib, settings.level_up())?.ensure_iterable()?;

    let prev_stack_len = state.stack.len();

    let counter_var = if let Some(name) = counter_name {
        let counter_var = state.get_identifier(name);
        state
            .stack
            .push((counter_var.clone(), AccessMode::ReadWrite));
        Some(counter_var)
    } else {
        None
    };
    let loop_var = state.get_identifier(name);
    state.stack.push((loop_var.clone(), AccessMode::ReadWrite));

    settings.is_breakable = true;
    let body = parse_block(input, state, lib, settings.level_up())?;

    state.stack.truncate(prev_stack_len);

    Ok(Stmt::For(
        expr,
        Box::new((
            Ident {
                name: loop_var,
                pos: name_pos,
            },
            counter_var.map(|name| Ident {
                name,
                pos: counter_pos.expect("never fails because `counter_var` is `Some`"),
            }),
            body.into(),
        )),
        settings.pos,
    ))
}

/// Parse a variable definition statement.
fn parse_let(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    var_type: AccessMode,
    is_export: bool,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // let/const... (specified in `var_type`)
    settings.pos = input.next().expect(NEVER_ENDS).1;

    // let name ...
    let (name, pos) = parse_var_name(input)?;

    let name = state.get_identifier(name);
    let var_def = Ident {
        name: name.clone(),
        pos,
    };

    // let name = ...
    let expr = if match_token(input, Token::Equals).0 {
        // let name = expr
        parse_expr(input, state, lib, settings.level_up())?
    } else {
        Expr::Unit(Position::NONE)
    };

    state.stack.push((name, var_type));

    let export = if is_export {
        AST_OPTION_EXPORTED
    } else {
        AST_OPTION_NONE
    };

    match var_type {
        // let name = expr
        AccessMode::ReadWrite => Ok(Stmt::Var(expr, var_def.into(), export, settings.pos)),
        // const name = { expr:constant }
        AccessMode::ReadOnly => Ok(Stmt::Var(
            expr,
            var_def.into(),
            AST_OPTION_CONSTANT + export,
            settings.pos,
        )),
    }
}

/// Parse an import statement.
#[cfg(not(feature = "no_module"))]
fn parse_import(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // import ...
    settings.pos = eat_token(input, Token::Import);

    // import expr ...
    let expr = parse_expr(input, state, lib, settings.level_up())?;

    // import expr as ...
    if !match_token(input, Token::As).0 {
        return Ok(Stmt::Import(expr, None, settings.pos));
    }

    // import expr as name ...
    let (name, name_pos) = parse_var_name(input)?;
    let name = state.get_identifier(name);
    state.modules.push(name.clone());

    Ok(Stmt::Import(
        expr,
        Some(
            Ident {
                name,
                pos: name_pos,
            }
            .into(),
        ),
        settings.pos,
    ))
}

/// Parse an export statement.
#[cfg(not(feature = "no_module"))]
fn parse_export(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = eat_token(input, Token::Export);

    match input.peek().expect(NEVER_ENDS) {
        (Token::Let, pos) => {
            let pos = *pos;
            let mut stmt = parse_let(input, state, lib, AccessMode::ReadWrite, true, settings)?;
            stmt.set_position(pos);
            return Ok(stmt);
        }
        (Token::Const, pos) => {
            let pos = *pos;
            let mut stmt = parse_let(input, state, lib, AccessMode::ReadOnly, true, settings)?;
            stmt.set_position(pos);
            return Ok(stmt);
        }
        _ => (),
    }

    let mut exports = Vec::<(Ident, Ident)>::with_capacity(4);

    loop {
        let (id, id_pos) = parse_var_name(input)?;

        let (rename, rename_pos) = if match_token(input, Token::As).0 {
            let (name, pos) = parse_var_name(input)?;
            if exports.iter().any(|(_, alias)| alias.name == name) {
                return Err(PERR::DuplicatedVariable(name).into_err(pos));
            }
            (name, pos)
        } else {
            (Default::default(), Position::NONE)
        };

        exports.push((
            Ident {
                name: state.get_identifier(id),
                pos: id_pos,
            },
            Ident {
                name: state.get_identifier(rename),
                pos: rename_pos,
            },
        ));

        match input.peek().expect(NEVER_ENDS) {
            (Token::Comma, _) => {
                eat_token(input, Token::Comma);
            }
            (Token::Identifier(_), pos) => {
                return Err(PERR::MissingToken(
                    Token::Comma.into(),
                    "to separate the list of exports".into(),
                )
                .into_err(*pos))
            }
            _ => break,
        }
    }

    Ok(Stmt::Export(exports.into_boxed_slice(), settings.pos))
}

/// Parse a statement block.
fn parse_block(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    // Must start with {
    settings.pos = match input.next().expect(NEVER_ENDS) {
        (Token::LeftBrace, pos) => pos,
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => {
            return Err(PERR::MissingToken(
                Token::LeftBrace.into(),
                "to start a statement block".into(),
            )
            .into_err(pos))
        }
    };

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut statements = Vec::with_capacity(8);

    let prev_entry_stack_len = state.entry_stack_len;
    state.entry_stack_len = state.stack.len();

    #[cfg(not(feature = "no_module"))]
    let prev_mods_len = state.modules.len();

    loop {
        // Terminated?
        match input.peek().expect(NEVER_ENDS) {
            (Token::RightBrace, _) => {
                eat_token(input, Token::RightBrace);
                break;
            }
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
        settings.is_global = false;

        let stmt = parse_stmt(input, state, lib, settings.level_up())?;

        if stmt.is_noop() {
            continue;
        }

        // See if it needs a terminating semicolon
        let need_semicolon = !stmt.is_self_terminated();

        statements.push(stmt);

        match input.peek().expect(NEVER_ENDS) {
            // { ... stmt }
            (Token::RightBrace, _) => {
                eat_token(input, Token::RightBrace);
                break;
            }
            // { ... stmt;
            (Token::SemiColon, _) if need_semicolon => {
                eat_token(input, Token::SemiColon);
            }
            // { ... { stmt } ;
            (Token::SemiColon, _) if !need_semicolon => {
                eat_token(input, Token::SemiColon);
            }
            // { ... { stmt } ???
            (_, _) if !need_semicolon => (),
            // { ... stmt <error>
            (Token::LexError(err), err_pos) => return Err(err.clone().into_err(*err_pos)),
            // { ... stmt ???
            (_, pos) => {
                // Semicolons are not optional between statements
                return Err(PERR::MissingToken(
                    Token::SemiColon.into(),
                    "to terminate this statement".into(),
                )
                .into_err(*pos));
            }
        }
    }

    state.stack.truncate(state.entry_stack_len);
    state.entry_stack_len = prev_entry_stack_len;

    #[cfg(not(feature = "no_module"))]
    state.modules.truncate(prev_mods_len);

    Ok(Stmt::Block(statements.into_boxed_slice(), settings.pos))
}

/// Parse an expression as a statement.
fn parse_expr_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = input.peek().expect(NEVER_ENDS).1;

    let expr = parse_expr(input, state, lib, settings.level_up())?;
    let stmt = parse_op_assignment_stmt(input, state, lib, expr, settings.level_up())?;
    Ok(stmt)
}

/// Parse a single statement.
fn parse_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    use AccessMode::{ReadOnly, ReadWrite};

    let mut settings = settings;

    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "metadata")]
    let comments = {
        let mut comments: StaticVec<String> = Default::default();
        let mut comments_pos = Position::NONE;

        // Handle doc-comments.
        while let (Token::Comment(ref comment), pos) = input.peek().expect(NEVER_ENDS) {
            if comments_pos.is_none() {
                comments_pos = *pos;
            }

            if !crate::token::is_doc_comment(comment) {
                unreachable!("expecting doc-comment, but gets {:?}", comment);
            }

            if !settings.is_global {
                return Err(PERR::WrongDocComment.into_err(comments_pos));
            }

            match input.next().expect(NEVER_ENDS).0 {
                Token::Comment(comment) => {
                    comments.push(comment);

                    match input.peek().expect(NEVER_ENDS) {
                        (Token::Fn, _) | (Token::Private, _) => break,
                        (Token::Comment(_), _) => (),
                        _ => return Err(PERR::WrongDocComment.into_err(comments_pos)),
                    }
                }
                _ => unreachable!(),
            }
        }

        comments
    };

    let (token, token_pos) = match input.peek().expect(NEVER_ENDS) {
        (Token::EOF, pos) => return Ok(Stmt::Noop(*pos)),
        x => x,
    };
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    match token {
        // ; - empty statement
        Token::SemiColon => {
            eat_token(input, Token::SemiColon);
            Ok(Stmt::Noop(settings.pos))
        }

        // { - statements block
        Token::LeftBrace => Ok(parse_block(input, state, lib, settings.level_up())?),

        // fn ...
        #[cfg(not(feature = "no_function"))]
        Token::Fn if !settings.is_global => Err(PERR::WrongFnDefinition.into_err(settings.pos)),

        #[cfg(not(feature = "no_function"))]
        Token::Fn | Token::Private => {
            let access = if matches!(token, Token::Private) {
                eat_token(input, Token::Private);
                FnAccess::Private
            } else {
                FnAccess::Public
            };

            match input.next().expect(NEVER_ENDS) {
                (Token::Fn, pos) => {
                    let mut new_state =
                        ParseState::new(state.engine, state.tokenizer_control.clone());

                    #[cfg(not(feature = "unchecked"))]
                    {
                        new_state.max_expr_depth = new_state.max_function_expr_depth;
                    }

                    let settings = ParseSettings {
                        allow_if_expr: true,
                        allow_switch_expr: true,
                        allow_stmt_expr: true,
                        allow_anonymous_fn: true,
                        is_global: false,
                        is_function_scope: true,
                        is_breakable: false,
                        level: 0,
                        pos,
                    };

                    let func = parse_fn(
                        input,
                        &mut new_state,
                        lib,
                        access,
                        settings,
                        #[cfg(not(feature = "no_function"))]
                        #[cfg(feature = "metadata")]
                        comments,
                    )?;

                    let hash = calc_fn_hash(&func.name, func.params.len());

                    if lib.contains_key(&hash) {
                        return Err(PERR::FnDuplicatedDefinition(
                            func.name.to_string(),
                            func.params.len(),
                        )
                        .into_err(pos));
                    }

                    lib.insert(hash, func.into());

                    Ok(Stmt::Noop(pos))
                }

                (_, pos) => Err(PERR::MissingToken(
                    Token::Fn.into(),
                    format!("following '{}'", Token::Private.syntax()),
                )
                .into_err(pos)),
            }
        }

        Token::If => parse_if(input, state, lib, settings.level_up()),
        Token::Switch => parse_switch(input, state, lib, settings.level_up()),
        Token::While | Token::Loop => parse_while_loop(input, state, lib, settings.level_up()),
        Token::Do => parse_do(input, state, lib, settings.level_up()),
        Token::For => parse_for(input, state, lib, settings.level_up()),

        Token::Continue if settings.is_breakable => {
            let pos = eat_token(input, Token::Continue);
            Ok(Stmt::Continue(pos))
        }
        Token::Break if settings.is_breakable => {
            let pos = eat_token(input, Token::Break);
            Ok(Stmt::Break(pos))
        }
        Token::Continue | Token::Break => Err(PERR::LoopBreak.into_err(settings.pos)),

        Token::Return | Token::Throw => {
            let (return_type, token_pos) = input
                .next()
                .map(|(token, pos)| {
                    (
                        match token {
                            Token::Return => ReturnType::Return,
                            Token::Throw => ReturnType::Exception,
                            _ => unreachable!(),
                        },
                        pos,
                    )
                })
                .expect(NEVER_ENDS);

            match input.peek().expect(NEVER_ENDS) {
                // `return`/`throw` at <EOF>
                (Token::EOF, _) => Ok(Stmt::Return(return_type, None, token_pos)),
                // `return`/`throw` at end of block
                (Token::RightBrace, _) if !settings.is_global => {
                    Ok(Stmt::Return(return_type, None, token_pos))
                }
                // `return;` or `throw;`
                (Token::SemiColon, _) => Ok(Stmt::Return(return_type, None, token_pos)),
                // `return` or `throw` with expression
                (_, _) => {
                    let expr = parse_expr(input, state, lib, settings.level_up())?;
                    Ok(Stmt::Return(return_type, Some(expr), token_pos))
                }
            }
        }

        Token::Try => parse_try_catch(input, state, lib, settings.level_up()),

        Token::Let => parse_let(input, state, lib, ReadWrite, false, settings.level_up()),
        Token::Const => parse_let(input, state, lib, ReadOnly, false, settings.level_up()),

        #[cfg(not(feature = "no_module"))]
        Token::Import => parse_import(input, state, lib, settings.level_up()),

        #[cfg(not(feature = "no_module"))]
        Token::Export if !settings.is_global => Err(PERR::WrongExport.into_err(settings.pos)),

        #[cfg(not(feature = "no_module"))]
        Token::Export => parse_export(input, state, lib, settings.level_up()),

        _ => parse_expr_stmt(input, state, lib, settings.level_up()),
    }
}

/// Parse a try/catch statement.
fn parse_try_catch(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // try ...
    settings.pos = eat_token(input, Token::Try);

    // try { body }
    let body = parse_block(input, state, lib, settings.level_up())?;

    // try { body } catch
    let (matched, catch_pos) = match_token(input, Token::Catch);

    if !matched {
        return Err(
            PERR::MissingToken(Token::Catch.into(), "for the 'try' statement".into())
                .into_err(catch_pos),
        );
    }

    // try { body } catch (
    let var_def = if match_token(input, Token::LeftParen).0 {
        let (name, pos) = parse_var_name(input)?;
        let (matched, err_pos) = match_token(input, Token::RightParen);

        if !matched {
            return Err(PERR::MissingToken(
                Token::RightParen.into(),
                "to enclose the catch variable".into(),
            )
            .into_err(err_pos));
        }

        let name = state.get_identifier(name);
        state.stack.push((name.clone(), AccessMode::ReadWrite));
        Some(Ident { name, pos })
    } else {
        None
    };

    // try { body } catch ( var ) { catch_block }
    let catch_body = parse_block(input, state, lib, settings.level_up())?;

    Ok(Stmt::TryCatch(
        (body.into(), var_def, catch_body.into()).into(),
        settings.pos,
    ))
}

/// Parse a function definition.
#[cfg(not(feature = "no_function"))]
fn parse_fn(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    access: FnAccess,
    settings: ParseSettings,
    #[cfg(not(feature = "no_function"))]
    #[cfg(feature = "metadata")]
    comments: StaticVec<String>,
) -> Result<ScriptFnDef, ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, pos) = input.next().expect(NEVER_ENDS);

    let name = match token.into_function_name_for_override() {
        Ok(r) => r,
        Err(Token::Reserved(s)) => return Err(PERR::Reserved(s).into_err(pos)),
        Err(_) => return Err(PERR::FnMissingName.into_err(pos)),
    };

    match input.peek().expect(NEVER_ENDS) {
        (Token::LeftParen, _) => eat_token(input, Token::LeftParen),
        (_, pos) => return Err(PERR::FnMissingParams(name).into_err(*pos)),
    };

    let mut params: StaticVec<_> = Default::default();

    if !match_token(input, Token::RightParen).0 {
        let sep_err = format!("to separate the parameters of function '{}'", name);

        loop {
            match input.next().expect(NEVER_ENDS) {
                (Token::RightParen, _) => break,
                (Token::Identifier(s), pos) => {
                    if params.iter().any(|(p, _)| p == &s) {
                        return Err(PERR::FnDuplicatedParam(name, s).into_err(pos));
                    }
                    let s = state.get_identifier(s);
                    state.stack.push((s.clone(), AccessMode::ReadWrite));
                    params.push((s, pos))
                }
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => {
                    return Err(PERR::MissingToken(
                        Token::RightParen.into(),
                        format!("to close the parameters list of function '{}'", name),
                    )
                    .into_err(pos))
                }
            }

            match input.next().expect(NEVER_ENDS) {
                (Token::RightParen, _) => break,
                (Token::Comma, _) => (),
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => {
                    return Err(PERR::MissingToken(Token::Comma.into(), sep_err).into_err(pos))
                }
            }
        }
    }

    // Parse function body
    let body = match input.peek().expect(NEVER_ENDS) {
        (Token::LeftBrace, _) => {
            settings.is_breakable = false;
            parse_block(input, state, lib, settings.level_up())?
        }
        (_, pos) => return Err(PERR::FnMissingBody(name).into_err(*pos)),
    }
    .into();

    let mut params: StaticVec<_> = params.into_iter().map(|(p, _)| p).collect();
    params.shrink_to_fit();

    #[cfg(not(feature = "no_closure"))]
    let externals = state
        .external_vars
        .iter()
        .map(|(name, _)| name)
        .filter(|name| !params.contains(name))
        .cloned()
        .collect();

    Ok(ScriptFnDef {
        name: state.get_identifier(&name),
        access,
        params,
        #[cfg(not(feature = "no_closure"))]
        externals,
        body,
        lib: None,
        #[cfg(not(feature = "no_module"))]
        mods: Default::default(),
        #[cfg(not(feature = "no_function"))]
        #[cfg(feature = "metadata")]
        comments,
    })
}

/// Creates a curried expression from a list of external variables
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_closure"))]
fn make_curry_from_externals(
    state: &mut ParseState,
    fn_expr: Expr,
    externals: StaticVec<Identifier>,
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
            .map(|x| Expr::Variable(None, Position::NONE, (None, None, x).into())),
    );

    let expr = FnCallExpr {
        name: state.get_identifier(crate::engine::KEYWORD_FN_PTR_CURRY),
        hashes: FnCallHashes::from_native(calc_fn_hash(
            crate::engine::KEYWORD_FN_PTR_CURRY,
            num_externals + 1,
        )),
        args,
        ..Default::default()
    }
    .into_fn_call_expr(pos);

    // Convert the entire expression into a statement block, then insert the relevant
    // [`Share`][Stmt::Share] statements.
    let mut statements = StaticVec::with_capacity(externals.len() + 1);
    statements.extend(externals.into_iter().map(Stmt::Share));
    statements.push(Stmt::Expr(expr));
    Expr::Stmt(StmtBlock::new(statements, pos).into())
}

/// Parse an anonymous function definition.
#[cfg(not(feature = "no_function"))]
fn parse_anon_fn(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    settings: ParseSettings,
) -> Result<(Expr, ScriptFnDef), ParseError> {
    let mut settings = settings;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut params_list: StaticVec<_> = Default::default();

    if input.next().expect(NEVER_ENDS).0 != Token::Or && !match_token(input, Token::Pipe).0 {
        loop {
            match input.next().expect(NEVER_ENDS) {
                (Token::Pipe, _) => break,
                (Token::Identifier(s), pos) => {
                    if params_list.iter().any(|p| p == &s) {
                        return Err(PERR::FnDuplicatedParam("".to_string(), s).into_err(pos));
                    }
                    let s = state.get_identifier(s);
                    state.stack.push((s.clone(), AccessMode::ReadWrite));
                    params_list.push(s)
                }
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => {
                    return Err(PERR::MissingToken(
                        Token::Pipe.into(),
                        "to close the parameters list of anonymous function".into(),
                    )
                    .into_err(pos))
                }
            }

            match input.next().expect(NEVER_ENDS) {
                (Token::Pipe, _) => break,
                (Token::Comma, _) => (),
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => {
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
    let body = parse_stmt(input, state, lib, settings.level_up())?;

    // External variables may need to be processed in a consistent order,
    // so extract them into a list.
    #[cfg(not(feature = "no_closure"))]
    let externals: StaticVec<Identifier> = state
        .external_vars
        .iter()
        .map(|(name, _)| name.clone())
        .collect();

    #[cfg(not(feature = "no_closure"))]
    let mut params = StaticVec::with_capacity(params_list.len() + externals.len());
    #[cfg(feature = "no_closure")]
    let mut params = StaticVec::with_capacity(params_list.len());

    #[cfg(not(feature = "no_closure"))]
    params.extend(externals.iter().cloned());

    params.append(&mut params_list);

    // Create unique function name by hashing the script body plus the parameters.
    let hasher = &mut get_hasher();
    params.iter().for_each(|p| p.hash(hasher));
    body.hash(hasher);
    let hash = hasher.finish();

    let fn_name = state.get_identifier(&(format!("{}{:016x}", crate::engine::FN_ANONYMOUS, hash)));

    // Define the function
    let script = ScriptFnDef {
        name: fn_name.clone(),
        access: FnAccess::Public,
        params,
        #[cfg(not(feature = "no_closure"))]
        externals: Default::default(),
        body: body.into(),
        lib: None,
        #[cfg(not(feature = "no_module"))]
        mods: Default::default(),
        #[cfg(not(feature = "no_function"))]
        #[cfg(feature = "metadata")]
        comments: Default::default(),
    };

    let fn_ptr = crate::FnPtr::new_unchecked(fn_name, Default::default());
    let expr = Expr::DynamicConstant(Box::new(fn_ptr.into()), settings.pos);

    #[cfg(not(feature = "no_closure"))]
    let expr = make_curry_from_externals(state, expr, externals, settings.pos);

    Ok((expr, script))
}

impl Engine {
    /// Parse a global level expression.
    pub(crate) fn parse_global_expr(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        scope: &Scope,
        optimization_level: OptimizationLevel,
    ) -> Result<AST, ParseError> {
        let mut functions = Default::default();

        let settings = ParseSettings {
            allow_if_expr: false,
            allow_switch_expr: false,
            allow_stmt_expr: false,
            allow_anonymous_fn: false,
            is_global: true,
            is_function_scope: false,
            is_breakable: false,
            level: 0,
            pos: Position::NONE,
        };
        let expr = parse_expr(input, state, &mut functions, settings)?;

        assert!(functions.is_empty());

        match input.peek().expect(NEVER_ENDS) {
            (Token::EOF, _) => (),
            // Return error if the expression doesn't end
            (token, pos) => {
                return Err(LexError::UnexpectedInput(token.syntax().to_string()).into_err(*pos))
            }
        }

        let expr = vec![Stmt::Expr(expr)];

        Ok(
            // Optimize AST
            optimize_into_ast(self, scope, expr, Default::default(), optimization_level),
        )
    }

    /// Parse the global level statements.
    fn parse_global_level(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
    ) -> Result<(Vec<Stmt>, Vec<Shared<ScriptFnDef>>), ParseError> {
        let mut statements = Vec::with_capacity(16);
        let mut functions = BTreeMap::new();

        while !input.peek().expect(NEVER_ENDS).0.is_eof() {
            let settings = ParseSettings {
                allow_if_expr: true,
                allow_switch_expr: true,
                allow_stmt_expr: true,
                allow_anonymous_fn: true,
                is_global: true,
                is_function_scope: false,
                is_breakable: false,
                level: 0,
                pos: Position::NONE,
            };

            let stmt = parse_stmt(input, state, &mut functions, settings)?;

            if stmt.is_noop() {
                continue;
            }

            let need_semicolon = !stmt.is_self_terminated();

            statements.push(stmt);

            match input.peek().expect(NEVER_ENDS) {
                // EOF
                (Token::EOF, _) => break,
                // stmt ;
                (Token::SemiColon, _) if need_semicolon => {
                    eat_token(input, Token::SemiColon);
                }
                // stmt ;
                (Token::SemiColon, _) if !need_semicolon => (),
                // { stmt } ???
                (_, _) if !need_semicolon => (),
                // stmt <error>
                (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
                // stmt ???
                (_, pos) => {
                    // Semicolons are not optional between statements
                    return Err(PERR::MissingToken(
                        Token::SemiColon.into(),
                        "to terminate this statement".into(),
                    )
                    .into_err(*pos));
                }
            }
        }

        Ok((statements, functions.into_iter().map(|(_, v)| v).collect()))
    }

    /// Run the parser on an input stream, returning an AST.
    #[inline(always)]
    pub(crate) fn parse(
        &self,
        input: &mut TokenStream,
        state: &mut ParseState,
        scope: &Scope,
        optimization_level: OptimizationLevel,
    ) -> Result<AST, ParseError> {
        let (statements, lib) = self.parse_global_level(input, state)?;

        Ok(
            // Optimize AST
            optimize_into_ast(self, scope, statements, lib, optimization_level),
        )
    }
}
