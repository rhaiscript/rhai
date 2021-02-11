//! Main module defining the lexer and parser.

use crate::ast::{BinaryExpr, CustomExpr, Expr, FnCallExpr, Ident, ReturnType, ScriptFnDef, Stmt};
use crate::dynamic::{AccessMode, Union};
use crate::engine::KEYWORD_THIS;
use crate::module::NamespaceRef;
use crate::optimize::optimize_into_ast;
use crate::optimize::OptimizationLevel;
use crate::stdlib::{
    borrow::Cow,
    boxed::Box,
    collections::HashMap,
    format,
    hash::{Hash, Hasher},
    iter::empty,
    num::{NonZeroU64, NonZeroUsize},
    string::{String, ToString},
    vec,
    vec::Vec,
};
use crate::syntax::{CustomSyntax, MARKER_BLOCK, MARKER_EXPR, MARKER_IDENT};
use crate::token::{is_keyword_function, is_valid_identifier, Token, TokenStream};
use crate::utils::{get_hasher, StraightHasherBuilder};
use crate::{
    calc_script_fn_hash, Dynamic, Engine, ImmutableString, LexError, ParseError, ParseErrorType,
    Position, Scope, StaticVec, AST,
};

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(not(feature = "no_function"))]
use crate::FnAccess;

type PERR = ParseErrorType;

type FunctionsLib = HashMap<NonZeroU64, ScriptFnDef, StraightHasherBuilder>;

/// A type that encapsulates the current state of the parser.
#[derive(Debug)]
struct ParseState<'e> {
    /// Reference to the scripting [`Engine`].
    engine: &'e Engine,
    /// Interned strings.
    strings: HashMap<String, ImmutableString>,
    /// Encapsulates a local stack with variable names to simulate an actual runtime scope.
    stack: Vec<(ImmutableString, AccessMode)>,
    /// Size of the local variables stack upon entry of the current block scope.
    entry_stack_len: usize,
    /// Tracks a list of external variables (variables that are not explicitly declared in the scope).
    #[cfg(not(feature = "no_closure"))]
    externals: HashMap<ImmutableString, Position>,
    /// An indicator that disables variable capturing into externals one single time
    /// up until the nearest consumed Identifier token.
    /// If set to false the next call to `access_var` will not capture the variable.
    /// All consequent calls to `access_var` will not be affected
    #[cfg(not(feature = "no_closure"))]
    allow_capture: bool,
    /// Encapsulates a local stack with imported [module][crate::Module] names.
    #[cfg(not(feature = "no_module"))]
    modules: StaticVec<ImmutableString>,
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
    pub fn new(
        engine: &'e Engine,
        #[cfg(not(feature = "unchecked"))] max_expr_depth: Option<NonZeroUsize>,
        #[cfg(not(feature = "unchecked"))]
        #[cfg(not(feature = "no_function"))]
        max_function_expr_depth: Option<NonZeroUsize>,
    ) -> Self {
        Self {
            engine,
            #[cfg(not(feature = "unchecked"))]
            max_expr_depth,
            #[cfg(not(feature = "unchecked"))]
            #[cfg(not(feature = "no_function"))]
            max_function_expr_depth,
            #[cfg(not(feature = "no_closure"))]
            externals: Default::default(),
            #[cfg(not(feature = "no_closure"))]
            allow_capture: true,
            strings: HashMap::with_capacity(64),
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
    /// The return value is the offset to be deducted from `Stack::len`,
    /// i.e. the top element of the [`ParseState`] is offset 1.
    ///
    /// Return `None` when the variable name is not found in the `stack`.
    #[inline]
    fn access_var(&mut self, name: &str, _pos: Position) -> Option<NonZeroUsize> {
        let mut barrier = false;

        let index = self
            .stack
            .iter()
            .rev()
            .enumerate()
            .find(|(_, (n, _))| {
                if n.is_empty() {
                    // Do not go beyond empty variable names
                    barrier = true;
                    false
                } else {
                    *n == name
                }
            })
            .and_then(|(i, _)| NonZeroUsize::new(i + 1));

        #[cfg(not(feature = "no_closure"))]
        if self.allow_capture {
            if index.is_none() && !self.externals.contains_key(name) {
                self.externals.insert(name.into(), _pos);
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
    #[inline(always)]
    pub fn find_module(&self, name: &str) -> Option<NonZeroUsize> {
        self.modules
            .iter()
            .rev()
            .enumerate()
            .find(|(_, n)| **n == name)
            .and_then(|(i, _)| NonZeroUsize::new(i + 1))
    }

    /// Get an interned string, creating one if it is not yet interned.
    pub fn get_interned_string(
        &mut self,
        text: impl AsRef<str> + Into<ImmutableString>,
    ) -> ImmutableString {
        #[allow(clippy::map_entry)]
        if !self.strings.contains_key(text.as_ref()) {
            let value = text.into();
            self.strings.insert(value.clone().into(), value.clone());
            value
        } else {
            self.strings.get(text.as_ref()).unwrap().clone()
        }
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
    pub fn level_up(&self) -> Self {
        Self {
            level: self.level + 1,
            ..*self
        }
    }
    /// Make sure that the current level of expression nesting is within the maximum limit.
    #[cfg(not(feature = "unchecked"))]
    #[inline(always)]
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
    fn into_property(self, state: &mut ParseState) -> Self {
        match self {
            Self::Variable(x) if x.1.is_none() => {
                let ident = x.2;
                let getter = state.get_interned_string(crate::engine::make_getter(&ident.name));
                let setter = state.get_interned_string(crate::engine::make_setter(&ident.name));
                Self::Property(Box::new((getter, setter, ident.into())))
            }
            _ => self,
        }
    }
}

/// Consume a particular [token][Token], checking that it is the expected one.
fn eat_token(input: &mut TokenStream, token: Token) -> Position {
    let (t, pos) = input.next().unwrap();

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
fn match_token(input: &mut TokenStream, token: Token) -> (bool, Position) {
    let (t, pos) = input.peek().unwrap();
    if *t == token {
        (true, eat_token(input, token))
    } else {
        (false, *pos)
    }
}

/// Parse ( expr )
fn parse_paren_expr(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // ( ...
    settings.pos = eat_token(input, Token::LeftParen);

    if match_token(input, Token::RightParen).0 {
        return Ok(Expr::Unit(settings.pos));
    }

    let expr = parse_expr(input, state, lib, settings.level_up())?;

    match input.next().unwrap() {
        // ( xxx )
        (Token::RightParen, _) => Ok(expr),
        // ( <error>
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
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
    id: ImmutableString,
    capture: bool,
    mut namespace: Option<NamespaceRef>,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, token_pos) = input.peek().unwrap();

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

            let mut hash_script = if let Some(ref mut modules) = namespace {
                #[cfg(not(feature = "no_module"))]
                modules.set_index(state.find_module(&modules[0].name));

                // Rust functions are indexed in two steps:
                // 1) Calculate a hash in a similar manner to script-defined functions,
                //    i.e. qualifiers + function name + number of arguments.
                // 2) Calculate a second hash with no qualifiers, empty function name,
                //    zero number of arguments, and the actual list of argument `TypeId`'s.
                // 3) The final hash is the XOR of the two hashes.
                let qualifiers = modules.iter().map(|m| m.name.as_str());
                calc_script_fn_hash(qualifiers, &id, 0)
            } else {
                // Qualifiers (none) + function name + no parameters.
                calc_script_fn_hash(empty(), &id, 0)
            };

            // script functions can only be valid identifiers
            if !is_valid_identifier(id.chars()) {
                hash_script = None;
            }

            return Ok(Expr::FnCall(
                Box::new(FnCallExpr {
                    name: id.to_string().into(),
                    capture,
                    namespace,
                    hash_script,
                    args,
                    ..Default::default()
                }),
                settings.pos,
            ));
        }
        // id...
        _ => (),
    }

    let settings = settings.level_up();

    loop {
        match input.peek().unwrap() {
            // id(...args, ) - handle trailing comma
            (Token::RightParen, _) => (),
            _ => args.push(parse_expr(input, state, lib, settings)?),
        }

        match input.peek().unwrap() {
            // id(...args)
            (Token::RightParen, _) => {
                eat_token(input, Token::RightParen);

                let mut hash_script = if let Some(modules) = namespace.as_mut() {
                    #[cfg(not(feature = "no_module"))]
                    modules.set_index(state.find_module(&modules[0].name));

                    // Rust functions are indexed in two steps:
                    // 1) Calculate a hash in a similar manner to script-defined functions,
                    //    i.e. qualifiers + function name + number of arguments.
                    // 2) Calculate a second hash with no qualifiers, empty function name,
                    //    zero number of arguments, and the actual list of argument `TypeId`'s.
                    // 3) The final hash is the XOR of the two hashes.
                    let qualifiers = modules.iter().map(|m| m.name.as_str());
                    calc_script_fn_hash(qualifiers, &id, args.len())
                } else {
                    // Qualifiers (none) + function name + number of arguments.
                    calc_script_fn_hash(empty(), &id, args.len())
                };

                // script functions can only be valid identifiers
                if !is_valid_identifier(id.chars()) {
                    hash_script = None;
                }

                return Ok(Expr::FnCall(
                    Box::new(FnCallExpr {
                        name: id.to_string().into(),
                        capture,
                        namespace,
                        hash_script,
                        args,
                        ..Default::default()
                    }),
                    settings.pos,
                ));
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
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let idx_expr = parse_expr(input, state, lib, settings.level_up())?;

    // Check type of indexing - must be integer or string
    match &idx_expr {
        // lhs[int]
        Expr::IntegerConstant(x, pos) if *x < 0 => {
            return Err(PERR::MalformedIndexExpr(format!(
                "Array access expects non-negative index: {} < 0",
                *x
            ))
            .into_err(*pos))
        }
        Expr::IntegerConstant(_, pos) => match lhs {
            Expr::Array(_, _) | Expr::StringConstant(_, _) => (),

            Expr::Map(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Object map access expects string index, not a number".into(),
                )
                .into_err(*pos))
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
            | Expr::In(_, _)
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
        Expr::StringConstant(_, pos) => match lhs {
            Expr::Map(_, _) => (),

            Expr::Array(_, _) | Expr::StringConstant(_, _) => {
                return Err(PERR::MalformedIndexExpr(
                    "Array or string expects numeric index, not a string".into(),
                )
                .into_err(*pos))
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
            | Expr::In(_, _)
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
        // lhs[??? && ???], lhs[??? || ???], lhs[??? in ???]
        x @ Expr::And(_, _) | x @ Expr::Or(_, _) | x @ Expr::In(_, _) => {
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
    match input.peek().unwrap() {
        (Token::RightBracket, _) => {
            eat_token(input, Token::RightBracket);

            // Any more indexing following?
            match input.peek().unwrap() {
                // If another indexing level, right-bind it
                (Token::LeftBracket, _) => {
                    let prev_pos = settings.pos;
                    settings.pos = eat_token(input, Token::LeftBracket);
                    // Recursively parse the indexing chain, right-binding each
                    let idx_expr =
                        parse_index_chain(input, state, lib, idx_expr, settings.level_up())?;
                    // Indexing binds to right
                    Ok(Expr::Index(
                        Box::new(BinaryExpr { lhs, rhs: idx_expr }),
                        prev_pos,
                    ))
                }
                // Otherwise terminate the indexing chain
                _ => Ok(Expr::Index(
                    Box::new(BinaryExpr { lhs, rhs: idx_expr }),
                    settings.pos,
                )),
            }
        }
        (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
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
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
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
            .into_err(input.peek().unwrap().1));
        }

        match input.peek().unwrap() {
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

        match input.peek().unwrap() {
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

    Ok(Expr::Array(Box::new(arr), settings.pos))
}

/// Parse a map literal.
#[cfg(not(feature = "no_object"))]
fn parse_map_literal(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // #{ ...
    settings.pos = eat_token(input, Token::MapStart);

    let mut map: StaticVec<(Ident, Expr)> = Default::default();

    loop {
        const MISSING_RBRACE: &str = "to end this object map literal";

        match input.peek().unwrap() {
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

        let (name, pos) = match input.next().unwrap() {
            (Token::Identifier(s), pos) | (Token::StringConstant(s), pos) => {
                if map.iter().any(|(p, _)| p.name == &s) {
                    return Err(PERR::DuplicatedProperty(s).into_err(pos));
                }
                (s, pos)
            }
            (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                return Err(PERR::Reserved(s).into_err(pos));
            }
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (_, pos) if map.is_empty() => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(pos),
                );
            }
            (Token::EOF, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(pos),
                );
            }
            (_, pos) => return Err(PERR::PropertyExpected.into_err(pos)),
        };

        match input.next().unwrap() {
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
            .into_err(input.peek().unwrap().1));
        }

        let expr = parse_expr(input, state, lib, settings.level_up())?;
        let name = state.get_interned_string(name);
        map.push((Ident { name, pos }, expr));

        match input.peek().unwrap() {
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

    Ok(Expr::Map(Box::new(map), settings.pos))
}

/// Parse a switch expression.
fn parse_switch(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // switch ...
    settings.pos = eat_token(input, Token::Switch);

    let item = parse_expr(input, state, lib, settings.level_up())?;

    match input.next().unwrap() {
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

    let mut table = HashMap::new();
    let mut def_stmt = None;

    loop {
        const MISSING_RBRACE: &str = "to end this switch block";

        let expr = match input.peek().unwrap() {
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
            (Token::Underscore, _) if def_stmt.is_none() => {
                eat_token(input, Token::Underscore);
                None
            }
            (Token::Underscore, pos) => return Err(PERR::DuplicatedSwitchCase.into_err(*pos)),
            _ => Some(parse_expr(input, state, lib, settings.level_up())?),
        };

        let hash = if let Some(expr) = expr {
            if let Some(value) = expr.get_constant_value() {
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

        match input.next().unwrap() {
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
            table.insert(hash, stmt);
            None
        } else {
            Some(stmt)
        };

        match input.peek().unwrap() {
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

    let mut final_table = HashMap::with_capacity_and_hasher(table.len(), StraightHasherBuilder);
    final_table.extend(table.into_iter());

    Ok(Stmt::Switch(
        item,
        Box::new((final_table.into(), def_stmt)),
        settings.pos,
    ))
}

/// Parse a primary expression.
fn parse_primary(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    let mut root_expr = match token {
        Token::EOF => return Err(PERR::UnexpectedEOF.into_err(settings.pos)),

        Token::IntegerConstant(_)
        | Token::CharConstant(_)
        | Token::StringConstant(_)
        | Token::True
        | Token::False => match input.next().unwrap().0 {
            Token::IntegerConstant(x) => Expr::IntegerConstant(x, settings.pos),
            Token::CharConstant(c) => Expr::CharConstant(c, settings.pos),
            Token::StringConstant(s) => {
                Expr::StringConstant(state.get_interned_string(s), settings.pos)
            }
            Token::True => Expr::BoolConstant(true, settings.pos),
            Token::False => Expr::BoolConstant(false, settings.pos),
            t => unreachable!("unexpected token: {:?}", t),
        },
        #[cfg(not(feature = "no_float"))]
        Token::FloatConstant(x) => {
            let x = (*x).into();
            input.next().unwrap();
            Expr::FloatConstant(x, settings.pos)
        }

        // { - block statement as expression
        Token::LeftBrace if settings.allow_stmt_expr => {
            match parse_block(input, state, lib, settings.level_up())? {
                Stmt::Block(statements, pos) => Expr::Stmt(Box::new(statements.into()), pos),
                stmt => unreachable!("expecting Stmt::Block, but gets {:?}", stmt),
            }
        }
        // ( - grouped expression
        Token::LeftParen => parse_paren_expr(input, state, lib, settings.level_up())?,

        // If statement is allowed to act as expressions
        Token::If if settings.allow_if_expr => Expr::Stmt(
            Box::new(vec![parse_if(input, state, lib, settings.level_up())?].into()),
            settings.pos,
        ),
        // Switch statement is allowed to act as expressions
        Token::Switch if settings.allow_switch_expr => Expr::Stmt(
            Box::new(vec![parse_switch(input, state, lib, settings.level_up())?].into()),
            settings.pos,
        ),
        // | ...
        #[cfg(not(feature = "no_function"))]
        Token::Pipe | Token::Or if settings.allow_anonymous_fn => {
            let mut new_state = ParseState::new(
                state.engine,
                #[cfg(not(feature = "unchecked"))]
                state.max_function_expr_depth,
                #[cfg(not(feature = "unchecked"))]
                state.max_function_expr_depth,
            );

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
            new_state.externals.iter().for_each(|(closure, pos)| {
                state.access_var(closure, *pos);
            });

            lib.insert(
                // Qualifiers (none) + function name + number of arguments.
                calc_script_fn_hash(empty(), &func.name, func.params.len()).unwrap(),
                func,
            );

            expr
        }

        // Array literal
        #[cfg(not(feature = "no_index"))]
        Token::LeftBracket => parse_array_literal(input, state, lib, settings.level_up())?,

        // Map literal
        #[cfg(not(feature = "no_object"))]
        Token::MapStart => parse_map_literal(input, state, lib, settings.level_up())?,

        // Identifier
        Token::Identifier(_) => {
            let s = match input.next().unwrap().0 {
                Token::Identifier(s) => s,
                t => unreachable!("expecting Token::Identifier, but gets {:?}", t),
            };

            match input.peek().unwrap().0 {
                // Function call
                Token::LeftParen | Token::Bang => {
                    #[cfg(not(feature = "no_closure"))]
                    {
                        // Once the identifier consumed we must enable next variables capturing
                        state.allow_capture = true;
                    }
                    let var_name_def = Ident {
                        name: state.get_interned_string(s),
                        pos: settings.pos,
                    };
                    Expr::Variable(Box::new((None, None, var_name_def)))
                }
                // Namespace qualification
                #[cfg(not(feature = "no_module"))]
                Token::DoubleColon => {
                    #[cfg(not(feature = "no_closure"))]
                    {
                        // Once the identifier consumed we must enable next variables capturing
                        state.allow_capture = true;
                    }
                    let var_name_def = Ident {
                        name: state.get_interned_string(s),
                        pos: settings.pos,
                    };
                    Expr::Variable(Box::new((None, None, var_name_def)))
                }
                // Normal variable access
                _ => {
                    let index = state.access_var(&s, settings.pos);
                    let var_name_def = Ident {
                        name: state.get_interned_string(s),
                        pos: settings.pos,
                    };
                    Expr::Variable(Box::new((index, None, var_name_def)))
                }
            }
        }

        // Reserved keyword or symbol
        Token::Reserved(_) => {
            let s = match input.next().unwrap().0 {
                Token::Reserved(s) => s,
                t => unreachable!("expecting Token::Reserved, but gets {:?}", t),
            };

            match input.peek().unwrap().0 {
                // Function call is allowed to have reserved keyword
                Token::LeftParen | Token::Bang if is_keyword_function(&s) => {
                    let var_name_def = Ident {
                        name: state.get_interned_string(s),
                        pos: settings.pos,
                    };
                    Expr::Variable(Box::new((None, None, var_name_def)))
                }
                // Access to `this` as a variable is OK within a function scope
                _ if s == KEYWORD_THIS && settings.is_function_scope => {
                    let var_name_def = Ident {
                        name: state.get_interned_string(s),
                        pos: settings.pos,
                    };
                    Expr::Variable(Box::new((None, None, var_name_def)))
                }
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

        Token::LexError(_) => {
            let err = match input.next().unwrap().0 {
                Token::LexError(err) => err,
                t => unreachable!("expecting Token::LexError, but gets {:?}", t),
            };

            return Err(err.into_err(settings.pos));
        }

        _ => {
            return Err(LexError::UnexpectedInput(token.syntax().to_string()).into_err(settings.pos))
        }
    };

    // Tail processing all possible postfix operators
    loop {
        let (tail_token, _) = input.peek().unwrap();

        if !root_expr.is_valid_postfix(tail_token) {
            break;
        }

        let (tail_token, tail_pos) = input.next().unwrap();
        settings.pos = tail_pos;

        root_expr = match (root_expr, tail_token) {
            // Qualified function call with !
            (Expr::Variable(x), Token::Bang) if x.1.is_some() => {
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
            (Expr::Variable(x), Token::Bang) => {
                let (matched, pos) = match_token(input, Token::LeftParen);
                if !matched {
                    return Err(PERR::MissingToken(
                        Token::LeftParen.syntax().into(),
                        "to start arguments list of function call".into(),
                    )
                    .into_err(pos));
                }

                let (_, namespace, Ident { name, pos }) = *x;
                settings.pos = pos;
                let ns = namespace.map(|(_, ns)| ns);
                parse_fn_call(input, state, lib, name, true, ns, settings.level_up())?
            }
            // Function call
            (Expr::Variable(x), Token::LeftParen) => {
                let (_, namespace, Ident { name, pos }) = *x;
                settings.pos = pos;
                let ns = namespace.map(|(_, ns)| ns);
                parse_fn_call(input, state, lib, name, false, ns, settings.level_up())?
            }
            // module access
            (Expr::Variable(x), Token::DoubleColon) => match input.next().unwrap() {
                (Token::Identifier(id2), pos2) => {
                    let (index, mut namespace, var_name_def) = *x;

                    if let Some((_, ref mut namespace)) = namespace {
                        namespace.push(var_name_def);
                    } else {
                        let mut ns: NamespaceRef = Default::default();
                        ns.push(var_name_def);
                        let index = NonZeroU64::new(42).unwrap(); // Dummy
                        namespace = Some((index, ns));
                    }

                    let var_name_def = Ident {
                        name: state.get_interned_string(id2),
                        pos: pos2,
                    };
                    Expr::Variable(Box::new((index, namespace, var_name_def)))
                }
                (Token::Reserved(id2), pos2) if is_valid_identifier(id2.chars()) => {
                    return Err(PERR::Reserved(id2).into_err(pos2));
                }
                (_, pos2) => return Err(PERR::VariableExpected.into_err(pos2)),
            },
            // Indexing
            #[cfg(not(feature = "no_index"))]
            (expr, Token::LeftBracket) => {
                parse_index_chain(input, state, lib, expr, settings.level_up())?
            }
            // Property access
            #[cfg(not(feature = "no_object"))]
            (expr, Token::Period) => {
                // Expression after dot must start with an identifier
                match input.peek().unwrap() {
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

                make_dot_expr(state, expr, rhs, tail_pos)?
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
    match &mut root_expr {
        Expr::Variable(x) if x.1.is_some() => Some(x),
        Expr::Index(x, _) | Expr::Dot(x, _) => match &mut x.lhs {
            Expr::Variable(x) if x.1.is_some() => Some(x),
            _ => None,
        },
        _ => None,
    }
    .map(|x| match x.as_mut() {
        (_, Some((ref mut hash, ref mut namespace)), Ident { name, .. }) => {
            // Qualifiers + variable name
            *hash =
                calc_script_fn_hash(namespace.iter().map(|v| v.name.as_str()), name, 0).unwrap();

            #[cfg(not(feature = "no_module"))]
            namespace.set_index(state.find_module(&namespace[0].name));
        }
        _ => unreachable!("expecting namespace-qualified variable access"),
    });

    // Make sure identifiers are valid
    Ok(root_expr)
}

/// Parse a potential unary operator.
fn parse_unary(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    match token {
        // -expr
        Token::UnaryMinus => {
            let pos = eat_token(input, Token::UnaryMinus);

            match parse_unary(input, state, lib, settings.level_up())? {
                // Negative integer
                Expr::IntegerConstant(num, pos) => num
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
                Expr::FloatConstant(x, pos) => Ok(Expr::FloatConstant((-(*x)).into(), pos)),

                // Call negative function
                expr => {
                    let op = "-";
                    let mut args = StaticVec::new();
                    args.push(expr);

                    Ok(Expr::FnCall(
                        Box::new(FnCallExpr {
                            name: op.into(),
                            args,
                            ..Default::default()
                        }),
                        pos,
                    ))
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
                    let op = "+";
                    let mut args = StaticVec::new();
                    args.push(expr);

                    Ok(Expr::FnCall(
                        Box::new(FnCallExpr {
                            name: op.into(),
                            args,
                            ..Default::default()
                        }),
                        pos,
                    ))
                }
            }
        }
        // !expr
        Token::Bang => {
            let pos = eat_token(input, Token::Bang);
            let mut args = StaticVec::new();
            let expr = parse_unary(input, state, lib, settings.level_up())?;
            args.push(expr);

            let op = "!";

            Ok(Expr::FnCall(
                Box::new(FnCallExpr {
                    name: op.into(),
                    args,
                    def_value: Some(false.into()), // NOT operator, when operating on invalid operand, defaults to false
                    ..Default::default()
                }),
                pos,
            ))
        }
        // <EOF>
        Token::EOF => Err(PERR::UnexpectedEOF.into_err(settings.pos)),
        // All other tokens
        _ => parse_primary(input, state, lib, settings.level_up()),
    }
}

/// Make an assignment statement.
fn make_assignment_stmt<'a>(
    fn_name: Cow<'static, str>,
    state: &mut ParseState,
    lhs: Expr,
    rhs: Expr,
    op_pos: Position,
) -> Result<Stmt, ParseError> {
    fn check_lvalue(expr: &Expr, parent_is_dot: bool) -> Position {
        match expr {
            Expr::Index(x, _) | Expr::Dot(x, _) if parent_is_dot => match x.lhs {
                Expr::Property(_) => check_lvalue(&x.rhs, matches!(expr, Expr::Dot(_, _))),
                ref e => e.position(),
            },
            Expr::Index(x, _) | Expr::Dot(x, _) => match x.lhs {
                Expr::Property(_) => unreachable!("unexpected Expr::Property in indexing"),
                _ => check_lvalue(&x.rhs, matches!(expr, Expr::Dot(_, _))),
            },
            Expr::Property(_) if parent_is_dot => Position::NONE,
            Expr::Property(_) => unreachable!("unexpected Expr::Property in indexing"),
            e if parent_is_dot => e.position(),
            _ => Position::NONE,
        }
    }

    match &lhs {
        // const_expr = rhs
        expr if expr.is_constant() => {
            Err(PERR::AssignmentToConstant("".into()).into_err(lhs.position()))
        }
        // var (non-indexed) = rhs
        Expr::Variable(x) if x.0.is_none() => Ok(Stmt::Assignment(
            Box::new((lhs, fn_name.into(), rhs)),
            op_pos,
        )),
        // var (indexed) = rhs
        Expr::Variable(x) => {
            let (index, _, Ident { name, pos }) = x.as_ref();
            match state.stack[(state.stack.len() - index.unwrap().get())].1 {
                AccessMode::ReadWrite => Ok(Stmt::Assignment(
                    Box::new((lhs, fn_name.into(), rhs)),
                    op_pos,
                )),
                // Constant values cannot be assigned to
                AccessMode::ReadOnly => {
                    Err(PERR::AssignmentToConstant(name.to_string()).into_err(*pos))
                }
            }
        }
        // xxx[???]... = rhs, xxx.prop... = rhs
        Expr::Index(x, _) | Expr::Dot(x, _) => {
            match check_lvalue(&x.rhs, matches!(lhs, Expr::Dot(_, _))) {
                Position::NONE => match &x.lhs {
                    // var[???] (non-indexed) = rhs, var.??? (non-indexed) = rhs
                    Expr::Variable(x) if x.0.is_none() => Ok(Stmt::Assignment(
                        Box::new((lhs, fn_name.into(), rhs)),
                        op_pos,
                    )),
                    // var[???] (indexed) = rhs, var.??? (indexed) = rhs
                    Expr::Variable(x) => {
                        let (index, _, Ident { name, pos }) = x.as_ref();
                        match state.stack[(state.stack.len() - index.unwrap().get())].1 {
                            AccessMode::ReadWrite => Ok(Stmt::Assignment(
                                Box::new((lhs, fn_name.into(), rhs)),
                                op_pos,
                            )),
                            // Constant values cannot be assigned to
                            AccessMode::ReadOnly => {
                                Err(PERR::AssignmentToConstant(name.to_string()).into_err(*pos))
                            }
                        }
                    }
                    // expr[???] = rhs, expr.??? = rhs
                    expr => {
                        Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(expr.position()))
                    }
                },
                pos => Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(pos)),
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
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    let op = match token {
        Token::Equals => "".into(),

        Token::PlusAssign
        | Token::MinusAssign
        | Token::MultiplyAssign
        | Token::DivideAssign
        | Token::LeftShiftAssign
        | Token::RightShiftAssign
        | Token::ModuloAssign
        | Token::PowerOfAssign
        | Token::AndAssign
        | Token::OrAssign
        | Token::XOrAssign => token.syntax(),

        _ => return Ok(Stmt::Expr(lhs)),
    };

    let (_, pos) = input.next().unwrap();
    let rhs = parse_expr(input, state, lib, settings.level_up())?;
    make_assignment_stmt(op, state, lhs, rhs, pos)
}

/// Make a dot expression.
#[cfg(not(feature = "no_object"))]
fn make_dot_expr(
    state: &mut ParseState,
    lhs: Expr,
    rhs: Expr,
    op_pos: Position,
) -> Result<Expr, ParseError> {
    Ok(match (lhs, rhs) {
        // idx_lhs[idx_expr].rhs
        // Attach dot chain to the bottom level of indexing chain
        (Expr::Index(mut x, pos), rhs) => {
            x.rhs = make_dot_expr(state, x.rhs, rhs, op_pos)?;
            Expr::Index(x, pos)
        }
        // lhs.id
        (lhs, Expr::Variable(x)) if x.1.is_none() => {
            let ident = x.2;
            let getter = state.get_interned_string(crate::engine::make_getter(&ident.name));
            let setter = state.get_interned_string(crate::engine::make_setter(&ident.name));
            let rhs = Expr::Property(Box::new((getter, setter, ident)));

            Expr::Dot(Box::new(BinaryExpr { lhs, rhs }), op_pos)
        }
        // lhs.module::id - syntax error
        (_, Expr::Variable(x)) if x.1.is_some() => {
            return Err(PERR::PropertyExpected.into_err(x.1.unwrap().1[0].pos))
        }
        // lhs.prop
        (lhs, prop @ Expr::Property(_)) => {
            Expr::Dot(Box::new(BinaryExpr { lhs, rhs: prop }), op_pos)
        }
        // lhs.dot_lhs.dot_rhs
        (lhs, Expr::Dot(x, pos)) => match x.lhs {
            Expr::Variable(_) | Expr::Property(_) | Expr::FnCall(_, _) => {
                let rhs = Expr::Dot(
                    Box::new(BinaryExpr {
                        lhs: x.lhs.into_property(state),
                        rhs: x.rhs,
                    }),
                    pos,
                );
                Expr::Dot(Box::new(BinaryExpr { lhs, rhs }), op_pos)
            }
            _ => unreachable!("invalid dot expression: {:?}", x.lhs),
        },
        // lhs.idx_lhs[idx_rhs]
        (lhs, Expr::Index(x, pos)) => {
            let rhs = Expr::Index(
                Box::new(BinaryExpr {
                    lhs: x.lhs.into_property(state),
                    rhs: x.rhs,
                }),
                pos,
            );
            Expr::Dot(Box::new(BinaryExpr { lhs, rhs }), op_pos)
        }
        // lhs.Fn() or lhs.eval()
        (_, Expr::FnCall(x, pos))
            if x.args.len() == 0
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
        (lhs, func @ Expr::FnCall(_, _)) => {
            Expr::Dot(Box::new(BinaryExpr { lhs, rhs: func }), op_pos)
        }
        // lhs.rhs
        (_, rhs) => return Err(PERR::PropertyExpected.into_err(rhs.position())),
    })
}

/// Make an 'in' expression.
fn make_in_expr(lhs: Expr, rhs: Expr, op_pos: Position) -> Result<Expr, ParseError> {
    match (&lhs, &rhs) {
        (_, x @ Expr::IntegerConstant(_, _))
        | (_, x @ Expr::And(_, _))
        | (_, x @ Expr::Or(_, _))
        | (_, x @ Expr::In(_, _))
        | (_, x @ Expr::BoolConstant(_, _))
        | (_, x @ Expr::Unit(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression expects a string, array or object map".into(),
            )
            .into_err(x.position()))
        }

        #[cfg(not(feature = "no_float"))]
        (_, x @ Expr::FloatConstant(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression expects a string, array or object map".into(),
            )
            .into_err(x.position()))
        }

        // "xxx" in "xxxx", 'x' in "xxxx" - OK!
        (Expr::StringConstant(_, _), Expr::StringConstant(_, _))
        | (Expr::CharConstant(_, _), Expr::StringConstant(_, _)) => (),

        // 123.456 in "xxxx"
        #[cfg(not(feature = "no_float"))]
        (x @ Expr::FloatConstant(_, _), Expr::StringConstant(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a float".into(),
            )
            .into_err(x.position()))
        }
        // 123 in "xxxx"
        (x @ Expr::IntegerConstant(_, _), Expr::StringConstant(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a number".into(),
            )
            .into_err(x.position()))
        }
        // (??? && ???) in "xxxx", (??? || ???) in "xxxx", (??? in ???) in "xxxx",
        //  true in "xxxx", false in "xxxx"
        (x @ Expr::And(_, _), Expr::StringConstant(_, _))
        | (x @ Expr::Or(_, _), Expr::StringConstant(_, _))
        | (x @ Expr::In(_, _), Expr::StringConstant(_, _))
        | (x @ Expr::BoolConstant(_, _), Expr::StringConstant(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // [???, ???, ???] in "xxxx"
        (x @ Expr::Array(_, _), Expr::StringConstant(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not an array".into(),
            )
            .into_err(x.position()))
        }
        // #{...} in "xxxx"
        (x @ Expr::Map(_, _), Expr::StringConstant(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not an object map".into(),
            )
            .into_err(x.position()))
        }
        // () in "xxxx"
        (x @ Expr::Unit(_), Expr::StringConstant(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not ()".into(),
            )
            .into_err(x.position()))
        }

        // "xxx" in #{...}, 'x' in #{...} - OK!
        (Expr::StringConstant(_, _), Expr::Map(_, _))
        | (Expr::CharConstant(_, _), Expr::Map(_, _)) => (),

        // 123.456 in #{...}
        #[cfg(not(feature = "no_float"))]
        (x @ Expr::FloatConstant(_, _), Expr::Map(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not a float".into(),
            )
            .into_err(x.position()))
        }
        // 123 in #{...}
        (x @ Expr::IntegerConstant(_, _), Expr::Map(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not a number".into(),
            )
            .into_err(x.position()))
        }
        // (??? && ???) in #{...}, (??? || ???) in #{...}, (??? in ???) in #{...},
        // true in #{...}, false in #{...}
        (x @ Expr::And(_, _), Expr::Map(_, _))
        | (x @ Expr::Or(_, _), Expr::Map(_, _))
        | (x @ Expr::In(_, _), Expr::Map(_, _))
        | (x @ Expr::BoolConstant(_, _), Expr::Map(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // [???, ???, ???] in #{..}
        (x @ Expr::Array(_, _), Expr::Map(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not an array".into(),
            )
            .into_err(x.position()))
        }
        // #{...} in #{..}
        (x @ Expr::Map(_, _), Expr::Map(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not an object map".into(),
            )
            .into_err(x.position()))
        }
        // () in #{...}
        (x @ Expr::Unit(_), Expr::Map(_, _)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for an object map expects a string, not ()".into(),
            )
            .into_err(x.position()))
        }

        _ => (),
    }

    Ok(Expr::In(Box::new(BinaryExpr { lhs, rhs }), op_pos))
}

/// Parse a binary expression.
fn parse_binary_op(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    parent_precedence: u8,
    lhs: Expr,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = lhs.position();

    let mut root = lhs;

    loop {
        let (current_op, current_pos) = input.peek().unwrap();
        let precedence = match current_op {
            Token::Custom(c) => {
                if state
                    .engine
                    .custom_keywords
                    .get(c)
                    .map_or(false, Option::is_some)
                {
                    state.engine.custom_keywords.get(c).unwrap().unwrap().get()
                } else {
                    return Err(PERR::Reserved(c.clone()).into_err(*current_pos));
                }
            }
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

        let (op_token, pos) = input.next().unwrap();

        let rhs = parse_unary(input, state, lib, settings)?;

        let (next_op, next_pos) = input.peek().unwrap();
        let next_precedence = match next_op {
            Token::Custom(c) => {
                if state
                    .engine
                    .custom_keywords
                    .get(c)
                    .map_or(false, Option::is_some)
                {
                    state.engine.custom_keywords.get(c).unwrap().unwrap().get()
                } else {
                    return Err(PERR::Reserved(c.clone()).into_err(*next_pos));
                }
            }
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

        let cmp_def = Some(false.into());
        let op = op_token.syntax();

        let op_base = FnCallExpr {
            name: op,
            capture: false,
            ..Default::default()
        };

        let mut args = StaticVec::new();
        args.push(root);
        args.push(rhs);

        root = match op_token {
            Token::Plus
            | Token::Minus
            | Token::Multiply
            | Token::Divide
            | Token::LeftShift
            | Token::RightShift
            | Token::Modulo
            | Token::PowerOf
            | Token::Scinot
            | Token::Ampersand
            | Token::Pipe
            | Token::XOr => Expr::FnCall(Box::new(FnCallExpr { args, ..op_base }), pos),

            // '!=' defaults to true when passed invalid operands
            Token::NotEqualsTo => Expr::FnCall(
                Box::new(FnCallExpr {
                    args,
                    def_value: Some(true.into()),
                    ..op_base
                }),
                pos,
            ),

            // Comparison operators default to false when passed invalid operands
            Token::EqualsTo
            | Token::LessThan
            | Token::LessThanEqualsTo
            | Token::GreaterThan
            | Token::GreaterThanEqualsTo => Expr::FnCall(
                Box::new(FnCallExpr {
                    args,
                    def_value: cmp_def,
                    ..op_base
                }),
                pos,
            ),

            Token::Or => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                Expr::Or(
                    Box::new(BinaryExpr {
                        lhs: current_lhs,
                        rhs,
                    }),
                    pos,
                )
            }
            Token::And => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                Expr::And(
                    Box::new(BinaryExpr {
                        lhs: current_lhs,
                        rhs,
                    }),
                    pos,
                )
            }
            Token::In => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                make_in_expr(current_lhs, rhs, pos)?
            }

            Token::Custom(s)
                if state
                    .engine
                    .custom_keywords
                    .get(&s)
                    .map_or(false, Option::is_some) =>
            {
                let hash_script = if is_valid_identifier(s.chars()) {
                    // Accept non-native functions for custom operators
                    calc_script_fn_hash(empty(), &s, 2)
                } else {
                    None
                };

                Expr::FnCall(
                    Box::new(FnCallExpr {
                        hash_script,
                        args,
                        ..op_base
                    }),
                    pos,
                )
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
    mut settings: ParseSettings,
    key: &str,
    syntax: &CustomSyntax,
    pos: Position,
) -> Result<Expr, ParseError> {
    let mut keywords: StaticVec<Expr> = Default::default();
    let mut segments: StaticVec<_> = Default::default();
    let mut tokens: Vec<_> = Default::default();

    // Adjust the variables stack
    match syntax.scope_delta {
        delta if delta > 0 => {
            // Add enough empty variable names to the stack.
            // Empty variable names act as a barrier so earlier variables will not be matched.
            // Variable searches stop at the first empty variable name.
            state.stack.resize(
                state.stack.len() + delta as usize,
                ("".into(), AccessMode::ReadWrite),
            );
        }
        delta if delta < 0 && state.stack.len() <= delta.abs() as usize => state.stack.clear(),
        delta if delta < 0 => state
            .stack
            .truncate(state.stack.len() - delta.abs() as usize),
        _ => (),
    }

    let parse_func = &syntax.parse;

    segments.push(key.into());
    tokens.push(key.into());

    loop {
        let (fwd_token, fwd_pos) = input.peek().unwrap();
        settings.pos = *fwd_pos;
        let settings = settings.level_up();

        let required_token = if let Some(seg) = parse_func(&segments, fwd_token.syntax().as_ref())
            .map_err(|err| err.0.into_err(settings.pos))?
        {
            seg
        } else {
            break;
        };

        match required_token.as_str() {
            MARKER_IDENT => match input.next().unwrap() {
                (Token::Identifier(s), pos) => {
                    let name = state.get_interned_string(s);
                    segments.push(name.clone());
                    tokens.push(state.get_interned_string(MARKER_IDENT));
                    let var_name_def = Ident { name, pos };
                    keywords.push(Expr::Variable(Box::new((None, None, var_name_def))));
                }
                (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                    return Err(PERR::Reserved(s).into_err(pos));
                }
                (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
            },
            MARKER_EXPR => {
                keywords.push(parse_expr(input, state, lib, settings)?);
                let keyword = state.get_interned_string(MARKER_EXPR);
                segments.push(keyword.clone());
                tokens.push(keyword);
            }
            MARKER_BLOCK => match parse_block(input, state, lib, settings)? {
                Stmt::Block(statements, pos) => {
                    keywords.push(Expr::Stmt(Box::new(statements.into()), pos));
                    let keyword = state.get_interned_string(MARKER_BLOCK);
                    segments.push(keyword.clone());
                    tokens.push(keyword);
                }
                stmt => unreachable!("expecting Stmt::Block, but gets {:?}", stmt),
            },
            s => match input.next().unwrap() {
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (t, _) if t.syntax().as_ref() == s => {
                    segments.push(required_token.clone());
                    tokens.push(required_token.clone());
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

    Ok(Expr::Custom(
        Box::new(CustomExpr {
            keywords,
            tokens,
            scope_delta: syntax.scope_delta,
        }),
        pos,
    ))
}

/// Parse an expression.
fn parse_expr(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = input.peek().unwrap().1;

    // Check if it is a custom syntax.
    if !state.engine.custom_syntax.is_empty() {
        let (token, pos) = input.peek().unwrap();
        let token_pos = *pos;

        match token {
            Token::Custom(key) | Token::Reserved(key) | Token::Identifier(key) => {
                match state.engine.custom_syntax.get_key_value(key) {
                    Some((key, syntax)) => {
                        input.next().unwrap();
                        return parse_custom_syntax(
                            input, state, lib, settings, key, syntax, token_pos,
                        );
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }

    // Parse expression normally.
    let lhs = parse_unary(input, state, lib, settings.level_up())?;
    parse_binary_op(input, state, lib, 1, lhs, settings.level_up())
}

/// Make sure that the expression is not a statement expression (i.e. wrapped in `{}`).
fn ensure_not_statement_expr(input: &mut TokenStream, type_name: &str) -> Result<(), ParseError> {
    match input.peek().unwrap() {
        // Disallow statement expressions
        (Token::LeftBrace, pos) | (Token::EOF, pos) => {
            Err(PERR::ExprExpected(type_name.to_string()).into_err(*pos))
        }
        // No need to check for others at this time - leave it for the expr parser
        _ => Ok(()),
    }
}

/// Make sure that the expression is not a mis-typed assignment (i.e. `a = b` instead of `a == b`).
fn ensure_not_assignment(input: &mut TokenStream) -> Result<(), ParseError> {
    match input.peek().unwrap() {
        (Token::Equals, pos) => Err(LexError::ImproperSymbol(
            "=".to_string(),
            "Possibly a typo of '=='?".to_string(),
        )
        .into_err(*pos)),
        (token @ Token::PlusAssign, pos)
        | (token @ Token::MinusAssign, pos)
        | (token @ Token::MultiplyAssign, pos)
        | (token @ Token::DivideAssign, pos)
        | (token @ Token::LeftShiftAssign, pos)
        | (token @ Token::RightShiftAssign, pos)
        | (token @ Token::ModuloAssign, pos)
        | (token @ Token::PowerOfAssign, pos)
        | (token @ Token::AndAssign, pos)
        | (token @ Token::OrAssign, pos)
        | (token @ Token::XOrAssign, pos) => Err(LexError::ImproperSymbol(
            token.syntax().to_string(),
            "Expecting a boolean expression, not an assignment".to_string(),
        )
        .into_err(*pos)),

        _ => Ok(()),
    }
}

/// Parse an if statement.
fn parse_if(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // if ...
    settings.pos = eat_token(input, Token::If);

    // if guard { if_body }
    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, state, lib, settings.level_up())?;
    ensure_not_assignment(input)?;
    let if_body = parse_block(input, state, lib, settings.level_up())?;

    // if guard { if_body } else ...
    let else_body = if match_token(input, Token::Else).0 {
        Some(if let (Token::If, _) = input.peek().unwrap() {
            // if guard { if_body } else if ...
            parse_if(input, state, lib, settings.level_up())?
        } else {
            // if guard { if_body } else { else-body }
            parse_block(input, state, lib, settings.level_up())?
        })
    } else {
        None
    };

    Ok(Stmt::If(
        guard,
        Box::new((if_body, else_body)),
        settings.pos,
    ))
}

/// Parse a while loop.
fn parse_while_loop(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // while|loops ...
    let (guard, token_pos) = match input.next().unwrap() {
        (Token::While, pos) => {
            ensure_not_statement_expr(input, "a boolean")?;
            (parse_expr(input, state, lib, settings.level_up())?, pos)
        }
        (Token::Loop, pos) => (Expr::BoolConstant(true, pos), pos),
        (t, _) => unreachable!("expecting Token::While or Token::Loop, but gets {:?}", t),
    };
    settings.pos = token_pos;

    ensure_not_assignment(input)?;
    settings.is_breakable = true;
    let body = Box::new(parse_block(input, state, lib, settings.level_up())?);

    Ok(Stmt::While(guard, body, settings.pos))
}

/// Parse a do loop.
fn parse_do(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // do ...
    settings.pos = eat_token(input, Token::Do);

    // do { body } [while|until] guard
    settings.is_breakable = true;
    let body = Box::new(parse_block(input, state, lib, settings.level_up())?);

    let is_while = match input.next().unwrap() {
        (Token::While, _) => true,
        (Token::Until, _) => false,
        (_, pos) => {
            return Err(
                PERR::MissingToken(Token::While.into(), "for the do statement".into())
                    .into_err(pos),
            )
        }
    };

    ensure_not_statement_expr(input, "a boolean")?;
    settings.is_breakable = false;
    let guard = parse_expr(input, state, lib, settings.level_up())?;
    ensure_not_assignment(input)?;

    Ok(Stmt::Do(body, guard, is_while, settings.pos))
}

/// Parse a for loop.
fn parse_for(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // for ...
    settings.pos = eat_token(input, Token::For);

    // for name ...
    let name = match input.next().unwrap() {
        // Variable name
        (Token::Identifier(s), _) => s,
        // Reserved keyword
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        // Bad identifier
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        // Not a variable name
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    // for name in ...
    match input.next().unwrap() {
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
    let expr = parse_expr(input, state, lib, settings.level_up())?;

    let loop_var = state.get_interned_string(name.clone());
    let prev_stack_len = state.stack.len();
    state.stack.push((loop_var, AccessMode::ReadWrite));

    settings.is_breakable = true;
    let body = parse_block(input, state, lib, settings.level_up())?;

    state.stack.truncate(prev_stack_len);

    Ok(Stmt::For(expr, Box::new((name, body)), settings.pos))
}

/// Parse a variable definition statement.
fn parse_let(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    var_type: AccessMode,
    export: bool,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // let/const... (specified in `var_type`)
    settings.pos = input.next().unwrap().1;

    // let name ...
    let (name, pos) = match input.next().unwrap() {
        (Token::Identifier(s), pos) => (s, pos),
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    let name = state.get_interned_string(name);
    let var_def = Ident {
        name: name.clone(),
        pos,
    };

    // let name = ...
    let expr = if match_token(input, Token::Equals).0 {
        // let name = expr
        Some(parse_expr(input, state, lib, settings.level_up())?)
    } else {
        None
    };

    state.stack.push((name, var_type));

    match var_type {
        // let name = expr
        AccessMode::ReadWrite => Ok(Stmt::Let(Box::new(var_def), expr, export, settings.pos)),
        // const name = { expr:constant }
        AccessMode::ReadOnly => Ok(Stmt::Const(Box::new(var_def), expr, export, settings.pos)),
    }
}

/// Parse an import statement.
#[cfg(not(feature = "no_module"))]
fn parse_import(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
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
    let (name, name_pos) = match input.next().unwrap() {
        (Token::Identifier(s), pos) => (s, pos),
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    let name = state.get_interned_string(name);
    state.modules.push(name.clone());

    Ok(Stmt::Import(
        expr,
        Some(Box::new(Ident {
            name,
            pos: name_pos,
        })),
        settings.pos,
    ))
}

/// Parse an export statement.
#[cfg(not(feature = "no_module"))]
fn parse_export(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = eat_token(input, Token::Export);

    match input.peek().unwrap() {
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

    let mut exports = Vec::with_capacity(4);

    loop {
        let (id, id_pos) = match input.next().unwrap() {
            (Token::Identifier(s), pos) => (s.clone(), pos),
            (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                return Err(PERR::Reserved(s).into_err(pos));
            }
            (Token::LexError(err), pos) => return Err(err.into_err(pos)),
            (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
        };

        let rename = if match_token(input, Token::As).0 {
            match input.next().unwrap() {
                (Token::Identifier(s), pos) => Some(Ident {
                    name: state.get_interned_string(s),
                    pos,
                }),
                (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                    return Err(PERR::Reserved(s).into_err(pos));
                }
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
            }
        } else {
            None
        };

        exports.push((
            Ident {
                name: state.get_interned_string(id),
                pos: id_pos,
            },
            rename,
        ));

        match input.peek().unwrap() {
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

    Ok(Stmt::Export(exports, settings.pos))
}

/// Parse a statement block.
fn parse_block(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // Must start with {
    settings.pos = match input.next().unwrap() {
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

    while !match_token(input, Token::RightBrace).0 {
        // Parse statements inside the block
        settings.is_global = false;

        let stmt = parse_stmt(input, state, lib, settings.level_up())?;

        if stmt.is_noop() {
            continue;
        }

        // See if it needs a terminating semicolon
        let need_semicolon = !stmt.is_self_terminated();

        statements.push(stmt);

        match input.peek().unwrap() {
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
            (Token::SemiColon, _) if !need_semicolon => (),
            // { ... { stmt } ???
            (_, _) if !need_semicolon => (),
            // { ... stmt <error>
            (Token::LexError(err), pos) => return Err(err.clone().into_err(*pos)),
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

    Ok(Stmt::Block(statements, settings.pos))
}

/// Parse an expression as a statement.
fn parse_expr_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    settings.pos = input.peek().unwrap().1;

    let expr = parse_expr(input, state, lib, settings.level_up())?;
    let stmt = parse_op_assignment_stmt(input, state, lib, expr, settings.level_up())?;
    Ok(stmt)
}

/// Parse a single statement.
fn parse_stmt(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    use AccessMode::{ReadOnly, ReadWrite};

    let mut _comments: Vec<String> = Default::default();

    #[cfg(not(feature = "no_function"))]
    {
        let mut comments_pos = Position::NONE;

        // Handle doc-comments.
        while let (Token::Comment(ref comment), pos) = input.peek().unwrap() {
            if comments_pos.is_none() {
                comments_pos = *pos;
            }

            if !crate::token::is_doc_comment(comment) {
                unreachable!("expecting doc-comment, but gets {:?}", comment);
            }

            if !settings.is_global {
                return Err(PERR::WrongDocComment.into_err(comments_pos));
            }

            match input.next().unwrap().0 {
                Token::Comment(comment) => {
                    _comments.push(comment);

                    match input.peek().unwrap() {
                        (Token::Fn, _) | (Token::Private, _) => break,
                        (Token::Comment(_), _) => (),
                        _ => return Err(PERR::WrongDocComment.into_err(comments_pos)),
                    }
                }
                t => unreachable!("expecting Token::Comment, but gets {:?}", t),
            }
        }
    }

    let (token, token_pos) = match input.peek().unwrap() {
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
        Token::Fn if !settings.is_global => Err(PERR::FnWrongDefinition.into_err(settings.pos)),

        #[cfg(not(feature = "no_function"))]
        Token::Fn | Token::Private => {
            let access = if matches!(token, Token::Private) {
                eat_token(input, Token::Private);
                FnAccess::Private
            } else {
                FnAccess::Public
            };

            match input.next().unwrap() {
                (Token::Fn, pos) => {
                    let mut new_state = ParseState::new(
                        state.engine,
                        #[cfg(not(feature = "unchecked"))]
                        state.max_function_expr_depth,
                        #[cfg(not(feature = "unchecked"))]
                        state.max_function_expr_depth,
                    );

                    let settings = ParseSettings {
                        allow_if_expr: true,
                        allow_switch_expr: true,
                        allow_stmt_expr: true,
                        allow_anonymous_fn: true,
                        is_global: false,
                        is_function_scope: true,
                        is_breakable: false,
                        level: 0,
                        pos: pos,
                    };

                    let func = parse_fn(input, &mut new_state, lib, access, settings, _comments)?;

                    // Qualifiers (none) + function name + number of arguments.
                    let hash = calc_script_fn_hash(empty(), &func.name, func.params.len()).unwrap();

                    if lib.contains_key(&hash) {
                        return Err(PERR::FnDuplicatedDefinition(
                            func.name.into_owned(),
                            func.params.len(),
                        )
                        .into_err(pos));
                    }

                    lib.insert(hash, func);

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
                            t => unreachable!(
                                "expecting Token::Return or Token::Throw, but gets {:?}",
                                t
                            ),
                        },
                        pos,
                    )
                })
                .unwrap();

            match input.peek().unwrap() {
                // `return`/`throw` at <EOF>
                (Token::EOF, pos) => Ok(Stmt::Return((return_type, token_pos), None, *pos)),
                // `return;` or `throw;`
                (Token::SemiColon, _) => {
                    Ok(Stmt::Return((return_type, token_pos), None, settings.pos))
                }
                // `return` or `throw` with expression
                (_, _) => {
                    let expr = parse_expr(input, state, lib, settings.level_up())?;
                    let pos = expr.position();
                    Ok(Stmt::Return((return_type, token_pos), Some(expr), pos))
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
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
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
        let id = match input.next().unwrap() {
            (Token::Identifier(s), pos) => Ident {
                name: state.get_interned_string(s),
                pos,
            },
            (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
        };

        let (matched, pos) = match_token(input, Token::RightParen);

        if !matched {
            return Err(PERR::MissingToken(
                Token::RightParen.into(),
                "to enclose the catch variable".into(),
            )
            .into_err(pos));
        }

        Some(id)
    } else {
        None
    };

    // try { body } catch ( var ) { catch_block }
    let catch_body = parse_block(input, state, lib, settings.level_up())?;

    Ok(Stmt::TryCatch(
        Box::new((body, var_def, catch_body)),
        settings.pos,
        catch_pos,
    ))
}

/// Parse a function definition.
#[cfg(not(feature = "no_function"))]
fn parse_fn(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    access: FnAccess,
    mut settings: ParseSettings,
    comments: Vec<String>,
) -> Result<ScriptFnDef, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, pos) = input.next().unwrap();

    let name = token
        .into_function_name_for_override()
        .map_err(|t| match t {
            Token::Reserved(s) => PERR::Reserved(s).into_err(pos),
            _ => PERR::FnMissingName.into_err(pos),
        })?;

    match input.peek().unwrap() {
        (Token::LeftParen, _) => eat_token(input, Token::LeftParen),
        (_, pos) => return Err(PERR::FnMissingParams(name).into_err(*pos)),
    };

    let mut params: StaticVec<_> = Default::default();

    if !match_token(input, Token::RightParen).0 {
        let sep_err = format!("to separate the parameters of function '{}'", name);

        loop {
            match input.next().unwrap() {
                (Token::RightParen, _) => break,
                (Token::Identifier(s), pos) => {
                    if params.iter().any(|(p, _)| p == &s) {
                        return Err(PERR::FnDuplicatedParam(name, s).into_err(pos));
                    }
                    let s = state.get_interned_string(s);
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

            match input.next().unwrap() {
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
    let body = match input.peek().unwrap() {
        (Token::LeftBrace, _) => {
            settings.is_breakable = false;
            parse_block(input, state, lib, settings.level_up())?
        }
        (_, pos) => return Err(PERR::FnMissingBody(name).into_err(*pos)),
    };

    let params: StaticVec<_> = params.into_iter().map(|(p, _)| p).collect();

    #[cfg(not(feature = "no_closure"))]
    let externals = state
        .externals
        .iter()
        .map(|(name, _)| name)
        .filter(|name| !params.contains(name))
        .cloned()
        .collect();

    Ok(ScriptFnDef {
        name: name.into(),
        access,
        params,
        #[cfg(not(feature = "no_closure"))]
        externals,
        body,
        lib: None,
        #[cfg(not(feature = "no_module"))]
        mods: Default::default(),
        comments,
    })
}

/// Creates a curried expression from a list of external variables
#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_closure"))]
fn make_curry_from_externals(fn_expr: Expr, externals: StaticVec<Ident>, pos: Position) -> Expr {
    // If there are no captured variables, no need to curry
    if externals.is_empty() {
        return fn_expr;
    }

    let num_externals = externals.len();
    let mut args: StaticVec<_> = Default::default();

    args.push(fn_expr);

    externals.iter().for_each(|x| {
        args.push(Expr::Variable(Box::new((None, None, x.clone()))));
    });

    let curry_func = crate::engine::KEYWORD_FN_PTR_CURRY;

    let hash_script = calc_script_fn_hash(empty(), curry_func, num_externals + 1);

    let expr = Expr::FnCall(
        Box::new(FnCallExpr {
            name: curry_func.into(),
            hash_script,
            args,
            ..Default::default()
        }),
        pos,
    );

    // Convert the entire expression into a statement block, then insert the relevant
    // [`Share`][Stmt::Share] statements.
    let mut statements: StaticVec<_> = Default::default();
    statements.extend(externals.into_iter().map(Stmt::Share));
    statements.push(Stmt::Expr(expr));
    Expr::Stmt(Box::new(statements), pos)
}

/// Parse an anonymous function definition.
#[cfg(not(feature = "no_function"))]
fn parse_anon_fn(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<(Expr, ScriptFnDef), ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut params: StaticVec<_> = Default::default();

    if input.next().unwrap().0 != Token::Or {
        if !match_token(input, Token::Pipe).0 {
            loop {
                match input.next().unwrap() {
                    (Token::Pipe, _) => break,
                    (Token::Identifier(s), pos) => {
                        if params.iter().any(|(p, _)| p == &s) {
                            return Err(PERR::FnDuplicatedParam("".to_string(), s).into_err(pos));
                        }
                        let s = state.get_interned_string(s);
                        state.stack.push((s.clone(), AccessMode::ReadWrite));
                        params.push((s, pos))
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

                match input.next().unwrap() {
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
    }

    // Parse function body
    settings.is_breakable = false;
    let body = parse_stmt(input, state, lib, settings.level_up())?;

    // External variables may need to be processed in a consistent order,
    // so extract them into a list.
    let externals: StaticVec<Ident> = {
        #[cfg(not(feature = "no_closure"))]
        {
            state
                .externals
                .iter()
                .map(|(name, &pos)| Ident {
                    name: name.clone(),
                    pos,
                })
                .collect()
        }
        #[cfg(feature = "no_closure")]
        Default::default()
    };

    let params: StaticVec<_> = if cfg!(not(feature = "no_closure")) {
        externals
            .iter()
            .map(|k| k.name.clone())
            .chain(params.into_iter().map(|(v, _)| v))
            .collect()
    } else {
        params.into_iter().map(|(v, _)| v).collect()
    };

    // Create unique function name by hashing the script body plus the parameters.
    let hasher = &mut get_hasher();
    params.iter().for_each(|p| p.as_str().hash(hasher));
    body.hash(hasher);
    let hash = hasher.finish();

    let fn_name: ImmutableString = format!("{}{:016x}", crate::engine::FN_ANONYMOUS, hash).into();

    // Define the function
    let script = ScriptFnDef {
        name: fn_name.clone(),
        access: FnAccess::Public,
        params,
        #[cfg(not(feature = "no_closure"))]
        externals: Default::default(),
        body,
        lib: None,
        #[cfg(not(feature = "no_module"))]
        mods: Default::default(),
        comments: Default::default(),
    };

    let expr = Expr::FnPointer(fn_name, settings.pos);

    #[cfg(not(feature = "no_closure"))]
    let expr = make_curry_from_externals(expr, externals, settings.pos);

    Ok((expr, script))
}

impl Engine {
    pub(crate) fn parse_global_expr(
        &self,
        input: &mut TokenStream,
        scope: &Scope,
        optimization_level: OptimizationLevel,
    ) -> Result<AST, ParseError> {
        let mut functions = Default::default();
        let mut state = ParseState::new(
            self,
            #[cfg(not(feature = "unchecked"))]
            NonZeroUsize::new(self.max_expr_depth()),
            #[cfg(not(feature = "unchecked"))]
            #[cfg(not(feature = "no_function"))]
            NonZeroUsize::new(self.max_function_expr_depth()),
        );

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
        let expr = parse_expr(input, &mut state, &mut functions, settings)?;

        assert!(functions.is_empty());

        match input.peek().unwrap() {
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
    ) -> Result<(Vec<Stmt>, Vec<ScriptFnDef>), ParseError> {
        let mut statements = Vec::with_capacity(16);
        let mut functions = HashMap::with_capacity_and_hasher(16, StraightHasherBuilder);
        let mut state = ParseState::new(
            self,
            #[cfg(not(feature = "unchecked"))]
            NonZeroUsize::new(self.max_expr_depth()),
            #[cfg(not(feature = "unchecked"))]
            #[cfg(not(feature = "no_function"))]
            NonZeroUsize::new(self.max_function_expr_depth()),
        );

        while !input.peek().unwrap().0.is_eof() {
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

            let stmt = parse_stmt(input, &mut state, &mut functions, settings)?;

            if stmt.is_noop() {
                continue;
            }

            let need_semicolon = !stmt.is_self_terminated();

            statements.push(stmt);

            match input.peek().unwrap() {
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
        scope: &Scope,
        optimization_level: OptimizationLevel,
    ) -> Result<AST, ParseError> {
        let (statements, lib) = self.parse_global_level(input)?;

        Ok(
            // Optimize AST
            optimize_into_ast(self, scope, statements, lib, optimization_level),
        )
    }
}

/// Map a `Dynamic` value to an expression.
///
/// Returns Some(expression) if conversion is successful.  Otherwise None.
pub fn map_dynamic_to_expr(value: Dynamic, pos: Position) -> Option<Expr> {
    match value.0 {
        #[cfg(not(feature = "no_float"))]
        Union::Float(value, _) => Some(Expr::FloatConstant(value, pos)),

        Union::Unit(_, _) => Some(Expr::Unit(pos)),
        Union::Int(value, _) => Some(Expr::IntegerConstant(value, pos)),
        Union::Char(value, _) => Some(Expr::CharConstant(value, pos)),
        Union::Str(value, _) => Some(Expr::StringConstant(value, pos)),
        Union::Bool(value, _) => Some(Expr::BoolConstant(value, pos)),
        #[cfg(not(feature = "no_index"))]
        Union::Array(array, _) => {
            let items: Vec<_> = array
                .into_iter()
                .map(|x| map_dynamic_to_expr(x, pos))
                .collect();

            if items.iter().all(Option::is_some) {
                Some(Expr::Array(
                    Box::new(items.into_iter().map(Option::unwrap).collect()),
                    pos,
                ))
            } else {
                None
            }
        }
        #[cfg(not(feature = "no_object"))]
        Union::Map(map, _) => {
            let items: Vec<_> = map
                .into_iter()
                .map(|(name, value)| (Ident { name, pos }, map_dynamic_to_expr(value, pos)))
                .collect();

            if items.iter().all(|(_, expr)| expr.is_some()) {
                Some(Expr::Map(
                    Box::new(
                        items
                            .into_iter()
                            .map(|(k, expr)| (k, expr.unwrap()))
                            .collect(),
                    ),
                    pos,
                ))
            } else {
                None
            }
        }

        _ => None,
    }
}
