//! Main module defining the lexer and parser.

use crate::ast::{
    BinaryExpr, CustomExpr, Expr, FnCallInfo, Ident, IdentX, ReturnType, ScriptFnDef, Stmt, AST,
};
use crate::dynamic::{Dynamic, Union};
use crate::engine::{Engine, KEYWORD_THIS, MARKER_BLOCK, MARKER_EXPR, MARKER_IDENT};
use crate::module::ModuleRef;
use crate::optimize::{optimize_into_ast, OptimizationLevel};
use crate::parse_error::{LexError, ParseError, ParseErrorType};
use crate::scope::{EntryType as ScopeEntryType, Scope};
use crate::syntax::CustomSyntax;
use crate::token::{
    is_keyword_function, is_valid_identifier, Position, Token, TokenStream, NO_POS,
};
use crate::utils::StraightHasherBuilder;
use crate::{calc_script_fn_hash, StaticVec};

#[cfg(not(feature = "no_float"))]
use crate::ast::FloatWrapper;

#[cfg(not(feature = "no_object"))]
use crate::engine::{make_getter, make_setter, KEYWORD_EVAL, KEYWORD_FN_PTR};

#[cfg(not(feature = "no_function"))]
use crate::{
    ast::FnAccess,
    engine::{FN_ANONYMOUS, KEYWORD_FN_PTR_CURRY},
    utils::ImmutableString,
};

use crate::stdlib::{
    borrow::Cow,
    boxed::Box,
    collections::HashMap,
    format,
    hash::Hash,
    iter::empty,
    num::NonZeroUsize,
    string::{String, ToString},
    vec,
    vec::Vec,
};

#[cfg(not(feature = "no_function"))]
use crate::stdlib::hash::Hasher;

#[cfg(not(feature = "no_closure"))]
use crate::stdlib::collections::HashSet;

#[cfg(not(feature = "no_std"))]
#[cfg(not(feature = "no_function"))]
use crate::stdlib::collections::hash_map::DefaultHasher;

#[cfg(feature = "no_std")]
#[cfg(not(feature = "no_function"))]
use ahash::AHasher;

type PERR = ParseErrorType;

type FunctionsLib = HashMap<u64, ScriptFnDef, StraightHasherBuilder>;

#[derive(Clone)]
struct ParseState<'e> {
    /// Reference to the scripting `Engine`.
    engine: &'e Engine,
    /// Encapsulates a local stack with variable names to simulate an actual runtime scope.
    stack: Vec<(String, ScopeEntryType)>,
    /// Tracks a list of external variables (variables that are not explicitly declared in the scope).
    #[cfg(not(feature = "no_closure"))]
    externals: HashMap<String, Position>,
    /// An indicator that disables variable capturing into externals one single time
    /// up until the nearest consumed Identifier token.
    /// If set to false the next call to `access_var` will not capture the variable.
    /// All consequent calls to `access_var` will not be affected
    #[cfg(not(feature = "no_closure"))]
    allow_capture: bool,
    /// Encapsulates a local stack with imported module names.
    #[cfg(not(feature = "no_module"))]
    modules: Vec<String>,
    /// Maximum levels of expression nesting.
    #[cfg(not(feature = "unchecked"))]
    max_expr_depth: usize,
    /// Maximum levels of expression nesting in functions.
    #[cfg(not(feature = "unchecked"))]
    #[cfg(not(feature = "no_function"))]
    max_function_expr_depth: usize,
}

impl<'e> ParseState<'e> {
    /// Create a new `ParseState`.
    #[inline(always)]
    pub fn new(
        engine: &'e Engine,
        #[cfg(not(feature = "unchecked"))] max_expr_depth: usize,
        #[cfg(not(feature = "unchecked"))]
        #[cfg(not(feature = "no_function"))]
        max_function_expr_depth: usize,
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
            stack: Default::default(),
            #[cfg(not(feature = "no_module"))]
            modules: Default::default(),
        }
    }

    /// Find explicitly declared variable by name in the `ParseState`, searching in reverse order.
    ///
    /// If the variable is not present in the scope adds it to the list of external variables
    ///
    /// The return value is the offset to be deducted from `Stack::len`,
    /// i.e. the top element of the `ParseState` is offset 1.
    /// Return `None` when the variable name is not found in the `stack`.
    #[inline]
    fn access_var(&mut self, name: &str, _pos: Position) -> Option<NonZeroUsize> {
        let index = self
            .stack
            .iter()
            .rev()
            .enumerate()
            .find(|(_, (n, _))| *n == name)
            .and_then(|(i, _)| NonZeroUsize::new(i + 1));

        #[cfg(not(feature = "no_closure"))]
        if self.allow_capture {
            if index.is_none() && !self.externals.contains_key(name) {
                self.externals.insert(name.to_string(), _pos);
            }
        } else {
            self.allow_capture = true
        }

        index
    }

    /// Find a module by name in the `ParseState`, searching in reverse.
    ///
    /// Returns the offset to be deducted from `Stack::len`,
    /// i.e. the top element of the `ParseState` is offset 1.
    ///
    /// Returns `None` when the variable name is not found in the `ParseState`.
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
            .find(|(_, n)| *n == name)
            .and_then(|(i, _)| NonZeroUsize::new(i + 1))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
/// A type that encapsulates all the settings for a particular parsing function.
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
    #[inline]
    pub fn ensure_level_within_max_limit(&self, limit: usize) -> Result<(), ParseError> {
        if limit == 0 {
            Ok(())
        } else if self.level > limit {
            Err(PERR::ExprTooDeep.into_err(self.pos))
        } else {
            Ok(())
        }
    }
}
/// Consume a particular token, checking that it is the expected one.
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

/// Match a particular token, consuming it if matched.
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
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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
    id: String,
    capture: bool,
    mut namespace: Option<Box<ModuleRef>>,
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let (token, token_pos) = input.peek().unwrap();

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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
        Token::LexError(err) => return Err(err.into_err(*token_pos)),
        // id()
        Token::RightParen => {
            eat_token(input, Token::RightParen);

            let hash_script = if let Some(modules) = namespace.as_mut() {
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

            return Ok(Expr::FnCall(
                Box::new(FnCallInfo {
                    name: id.into(),
                    capture,
                    namespace,
                    hash: hash_script,
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

                let hash_script = if let Some(modules) = namespace.as_mut() {
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

                return Ok(Expr::FnCall(
                    Box::new(FnCallInfo {
                        name: id.into(),
                        capture,
                        namespace,
                        hash: hash_script,
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
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
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
            Expr::Array(_, _) | Expr::StringConstant(_) => (),

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
            | Expr::True(_)
            | Expr::False(_)
            | Expr::Unit(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Only arrays, object maps and strings can be indexed".into(),
                )
                .into_err(lhs.position()))
            }

            _ => (),
        },

        // lhs[string]
        Expr::StringConstant(x) => match lhs {
            Expr::Map(_, _) => (),

            Expr::Array(_, _) | Expr::StringConstant(_) => {
                return Err(PERR::MalformedIndexExpr(
                    "Array or string expects numeric index, not a string".into(),
                )
                .into_err(x.pos))
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
            | Expr::True(_)
            | Expr::False(_)
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
        x @ Expr::True(_) | x @ Expr::False(_) => {
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
                _ => {
                    match idx_expr {
                        // Terminate with an `Expr::Expr` wrapper to prevent the last index expression
                        // inside brackets to be mis-parsed as another level of indexing, or a
                        // dot expression/function call to be mis-parsed as following the indexing chain.
                        Expr::Index(_, _) | Expr::Dot(_, _) | Expr::FnCall(_, _) => {
                            Ok(Expr::Index(
                                Box::new(BinaryExpr {
                                    lhs,
                                    rhs: Expr::Expr(Box::new(idx_expr)),
                                }),
                                settings.pos,
                            ))
                        }
                        _ => Ok(Expr::Index(
                            Box::new(BinaryExpr { lhs, rhs: idx_expr }),
                            settings.pos,
                        )),
                    }
                }
            }
        }
        (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
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
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut arr = StaticVec::new();

    while !input.peek().unwrap().0.is_eof() {
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
                return Err(PERR::MissingToken(
                    Token::RightBracket.into(),
                    "to end this array literal".into(),
                )
                .into_err(*pos))
            }
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
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
    settings: ParseSettings,
) -> Result<Expr, ParseError> {
    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut map = StaticVec::new();

    while !input.peek().unwrap().0.is_eof() {
        const MISSING_RBRACE: &str = "to end this object map literal";

        match input.peek().unwrap() {
            (Token::RightBrace, _) => {
                eat_token(input, Token::RightBrace);
                break;
            }
            _ => (),
        }

        let (name, pos) = match input.next().unwrap() {
            (Token::Identifier(s), pos) => (s, pos),
            (Token::StringConstant(s), pos) => (s, pos),
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
        map.push((IdentX::new(name, pos), expr));

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
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
            (_, pos) => {
                return Err(
                    PERR::MissingToken(Token::RightBrace.into(), MISSING_RBRACE.into())
                        .into_err(*pos),
                )
            }
        }
    }

    // Check for duplicating properties
    map.iter()
        .enumerate()
        .try_for_each(|(i, (IdentX { name: k1, .. }, _))| {
            map.iter()
                .skip(i + 1)
                .find(|(IdentX { name: k2, .. }, _)| k2 == k1)
                .map_or_else(|| Ok(()), |(IdentX { name: k2, pos }, _)| Err((k2, *pos)))
        })
        .map_err(|(key, pos)| PERR::DuplicatedProperty(key.to_string()).into_err(pos))?;

    Ok(Expr::Map(Box::new(map), settings.pos))
}

/// Parse a primary expression.
fn parse_primary(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Expr, ParseError> {
    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let (token, _) = match token {
        // { - block statement as expression
        Token::LeftBrace if settings.allow_stmt_expr => {
            return parse_block(input, state, lib, settings.level_up()).map(|block| match block {
                Stmt::Block(statements, pos) => Expr::Stmt(Box::new(statements.into()), pos),
                _ => unreachable!(),
            })
        }
        Token::EOF => return Err(PERR::UnexpectedEOF.into_err(settings.pos)),
        _ => input.next().unwrap(),
    };

    let (next_token, _) = input.peek().unwrap();

    let mut root_expr = match token {
        Token::IntegerConstant(x) => Expr::IntegerConstant(x, settings.pos),
        #[cfg(not(feature = "no_float"))]
        Token::FloatConstant(x) => Expr::FloatConstant(FloatWrapper(x), settings.pos),
        Token::CharConstant(c) => Expr::CharConstant(c, settings.pos),
        Token::StringConstant(s) => Expr::StringConstant(Box::new(IdentX::new(s, settings.pos))),

        // Function call
        Token::Identifier(s) if *next_token == Token::LeftParen || *next_token == Token::Bang => {
            // Once the identifier consumed we must enable next variables capturing
            #[cfg(not(feature = "no_closure"))]
            {
                state.allow_capture = true;
            }
            Expr::Variable(Box::new((None, None, 0, Ident::new(s, settings.pos))))
        }
        // Module qualification
        #[cfg(not(feature = "no_module"))]
        Token::Identifier(s) if *next_token == Token::DoubleColon => {
            // Once the identifier consumed we must enable next variables capturing
            #[cfg(not(feature = "no_closure"))]
            {
                state.allow_capture = true;
            }
            Expr::Variable(Box::new((None, None, 0, Ident::new(s, settings.pos))))
        }
        // Normal variable access
        Token::Identifier(s) => {
            let index = state.access_var(&s, settings.pos);
            Expr::Variable(Box::new((index, None, 0, Ident::new(s, settings.pos))))
        }

        // Function call is allowed to have reserved keyword
        Token::Reserved(s) if *next_token == Token::LeftParen || *next_token == Token::Bang => {
            if is_keyword_function(&s) {
                Expr::Variable(Box::new((None, None, 0, Ident::new(s, settings.pos))))
            } else {
                return Err(PERR::Reserved(s).into_err(settings.pos));
            }
        }

        // Access to `this` as a variable is OK
        Token::Reserved(s) if s == KEYWORD_THIS && *next_token != Token::LeftParen => {
            if !settings.is_function_scope {
                return Err(PERR::BadInput(LexError::ImproperSymbol(format!(
                    "'{}' can only be used in functions",
                    s
                )))
                .into_err(settings.pos));
            } else {
                Expr::Variable(Box::new((None, None, 0, Ident::new(s, settings.pos))))
            }
        }

        Token::Reserved(s) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(settings.pos));
        }

        Token::LeftParen => parse_paren_expr(input, state, lib, settings.level_up())?,
        #[cfg(not(feature = "no_index"))]
        Token::LeftBracket => parse_array_literal(input, state, lib, settings.level_up())?,
        #[cfg(not(feature = "no_object"))]
        Token::MapStart => parse_map_literal(input, state, lib, settings.level_up())?,
        Token::True => Expr::True(settings.pos),
        Token::False => Expr::False(settings.pos),
        Token::LexError(err) => return Err(err.into_err(settings.pos)),

        _ => {
            return Err(
                PERR::BadInput(LexError::UnexpectedInput(token.syntax().to_string()))
                    .into_err(settings.pos),
            );
        }
    };

    // Tail processing all possible postfix operators
    loop {
        let (token, _) = input.peek().unwrap();

        if !root_expr.is_valid_postfix(token) {
            break;
        }

        let (token, token_pos) = input.next().unwrap();
        settings.pos = token_pos;

        root_expr = match (root_expr, token) {
            // Qualified function call with !
            (Expr::Variable(x), Token::Bang) if x.1.is_some() => {
                return Err(if !match_token(input, Token::LeftParen).0 {
                    LexError::UnexpectedInput(Token::Bang.syntax().to_string()).into_err(token_pos)
                } else {
                    PERR::BadInput(LexError::ImproperSymbol(
                        "'!' cannot be used to call module functions".to_string(),
                    ))
                    .into_err(token_pos)
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

                let (_, modules, _, Ident { name, pos }) = *x;
                settings.pos = pos;
                parse_fn_call(input, state, lib, name, true, modules, settings.level_up())?
            }
            // Function call
            (Expr::Variable(x), Token::LeftParen) => {
                let (_, modules, _, Ident { name, pos }) = *x;
                settings.pos = pos;
                parse_fn_call(input, state, lib, name, false, modules, settings.level_up())?
            }
            (Expr::Property(_), _) => unreachable!(),
            // module access
            (Expr::Variable(x), Token::DoubleColon) => match input.next().unwrap() {
                (Token::Identifier(id2), pos2) => {
                    let (index, mut modules, _, var_name_def) = *x;

                    if let Some(ref mut modules) = modules {
                        modules.push(var_name_def);
                    } else {
                        let mut m: ModuleRef = Default::default();
                        m.push(var_name_def);
                        modules = Some(Box::new(m));
                    }

                    Expr::Variable(Box::new((index, modules, 0, Ident::new(id2, pos2))))
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
            // Unknown postfix operator
            (expr, token) => unreachable!(
                "unknown postfix operator '{}' for {:?}",
                token.syntax(),
                expr
            ),
        }
    }

    match &mut root_expr {
        // Cache the hash key for module-qualified variables
        Expr::Variable(x) if x.1.is_some() => {
            let (_, modules, hash, Ident { name, .. }) = x.as_mut();
            let modules = modules.as_mut().unwrap();

            // Qualifiers + variable name
            *hash = calc_script_fn_hash(modules.iter().map(|v| v.name.as_str()), name, 0);

            #[cfg(not(feature = "no_module"))]
            modules.set_index(state.find_module(&modules[0].name));
        }
        _ => (),
    }

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
        // If statement is allowed to act as expressions
        Token::If if settings.allow_if_expr => {
            let mut block: StaticVec<_> = Default::default();
            block.push(parse_if(input, state, lib, settings.level_up())?);
            Ok(Expr::Stmt(Box::new(block), settings.pos))
        }
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
                        return Some(Expr::FloatConstant(-Into::<FloatWrapper>::into(num), pos));
                        #[cfg(feature = "no_float")]
                        return None;
                    })
                    .ok_or_else(|| LexError::MalformedNumber(format!("-{}", num)).into_err(pos)),

                // Negative float
                #[cfg(not(feature = "no_float"))]
                Expr::FloatConstant(x, pos) => Ok(Expr::FloatConstant(-x, pos)),

                // Call negative function
                expr => {
                    let op = "-";
                    let hash = calc_script_fn_hash(empty(), op, 1);
                    let mut args = StaticVec::new();
                    args.push(expr);

                    Ok(Expr::FnCall(
                        Box::new(FnCallInfo {
                            name: op.into(),
                            native_only: true,
                            namespace: None,
                            hash,
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
            eat_token(input, Token::UnaryPlus);
            parse_unary(input, state, lib, settings.level_up())
        }
        // !expr
        Token::Bang => {
            let pos = eat_token(input, Token::Bang);
            let mut args = StaticVec::new();
            let expr = parse_primary(input, state, lib, settings.level_up())?;
            args.push(expr);

            let op = "!";
            let hash = calc_script_fn_hash(empty(), op, 1);

            Ok(Expr::FnCall(
                Box::new(FnCallInfo {
                    name: op.into(),
                    native_only: true,
                    hash,
                    args,
                    def_value: Some(false), // NOT operator, when operating on invalid operand, defaults to false
                    ..Default::default()
                }),
                pos,
            ))
        }
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
                allow_stmt_expr: true,
                allow_anonymous_fn: true,
                is_global: false,
                is_function_scope: true,
                is_breakable: false,
                level: 0,
                pos: *token_pos,
            };

            let (expr, func) = parse_anon_fn(input, &mut new_state, lib, settings)?;

            #[cfg(not(feature = "no_closure"))]
            new_state.externals.iter().for_each(|(closure, pos)| {
                state.access_var(closure, *pos);
            });

            // Qualifiers (none) + function name + number of arguments.
            let hash = calc_script_fn_hash(empty(), &func.name, func.params.len());

            lib.insert(hash, func);

            Ok(expr)
        }
        // <EOF>
        Token::EOF => Err(PERR::UnexpectedEOF.into_err(settings.pos)),
        // All other tokens
        _ => parse_primary(input, state, lib, settings.level_up()),
    }
}

fn make_assignment_stmt<'a>(
    fn_name: Cow<'static, str>,
    state: &mut ParseState,
    lhs: Expr,
    rhs: Expr,
    pos: Position,
) -> Result<Stmt, ParseError> {
    match &lhs {
        // var (non-indexed) = rhs
        Expr::Variable(x) if x.0.is_none() => {
            Ok(Stmt::Assignment(Box::new((lhs, fn_name.into(), rhs)), pos))
        }
        // var (indexed) = rhs
        Expr::Variable(x) => {
            let (
                index,
                _,
                _,
                Ident {
                    name,
                    pos: name_pos,
                },
            ) = x.as_ref();
            match state.stack[(state.stack.len() - index.unwrap().get())].1 {
                ScopeEntryType::Normal => {
                    Ok(Stmt::Assignment(Box::new((lhs, fn_name.into(), rhs)), pos))
                }
                // Constant values cannot be assigned to
                ScopeEntryType::Constant => {
                    Err(PERR::AssignmentToConstant(name.clone()).into_err(*name_pos))
                }
            }
        }
        // xxx[???] = rhs, xxx.??? = rhs
        Expr::Index(x, _) | Expr::Dot(x, _) => match &x.lhs {
            // var[???] (non-indexed) = rhs, var.??? (non-indexed) = rhs
            Expr::Variable(x) if x.0.is_none() => {
                Ok(Stmt::Assignment(Box::new((lhs, fn_name.into(), rhs)), pos))
            }
            // var[???] (indexed) = rhs, var.??? (indexed) = rhs
            Expr::Variable(x) => {
                let (
                    index,
                    _,
                    _,
                    Ident {
                        name,
                        pos: name_pos,
                    },
                ) = x.as_ref();
                match state.stack[(state.stack.len() - index.unwrap().get())].1 {
                    ScopeEntryType::Normal => {
                        Ok(Stmt::Assignment(Box::new((lhs, fn_name.into(), rhs)), pos))
                    }
                    // Constant values cannot be assigned to
                    ScopeEntryType::Constant => {
                        Err(PERR::AssignmentToConstant(name.clone()).into_err(*name_pos))
                    }
                }
            }
            // expr[???] = rhs, expr.??? = rhs
            _ => Err(PERR::AssignmentToInvalidLHS("".to_string()).into_err(x.lhs.position())),
        },
        // const_expr = rhs
        expr if expr.is_constant() => {
            Err(PERR::AssignmentToConstant("".into()).into_err(lhs.position()))
        }
        // ??? && ??? = rhs, ??? || ??? = rhs
        Expr::And(_, _) | Expr::Or(_, _) => Err(PERR::BadInput(LexError::ImproperSymbol(
            "Possibly a typo of '=='?".to_string(),
        ))
        .into_err(pos)),
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
    let (token, token_pos) = input.peek().unwrap();
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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
fn make_dot_expr(lhs: Expr, rhs: Expr, op_pos: Position) -> Result<Expr, ParseError> {
    Ok(match (lhs, rhs) {
        // idx_lhs[idx_expr].rhs
        // Attach dot chain to the bottom level of indexing chain
        (Expr::Index(mut x, pos), rhs) => {
            x.rhs = make_dot_expr(x.rhs, rhs, op_pos)?;
            Expr::Index(x, pos)
        }
        // lhs.id
        (lhs, Expr::Variable(x)) if x.1.is_none() => {
            let ident = x.3;
            let getter = make_getter(&ident.name);
            let setter = make_setter(&ident.name);
            let rhs = Expr::Property(Box::new(((getter, setter), ident.into())));

            Expr::Dot(Box::new(BinaryExpr { lhs, rhs }), op_pos)
        }
        // lhs.module::id - syntax error
        (_, Expr::Variable(x)) if x.1.is_some() => {
            return Err(PERR::PropertyExpected.into_err(x.1.unwrap()[0].pos));
        }
        // lhs.prop
        (lhs, prop @ Expr::Property(_)) => {
            Expr::Dot(Box::new(BinaryExpr { lhs, rhs: prop }), op_pos)
        }
        // lhs.dot_lhs.dot_rhs
        (lhs, Expr::Dot(x, pos)) => {
            let rhs = Expr::Dot(
                Box::new(BinaryExpr {
                    lhs: x.lhs.into_property(),
                    rhs: x.rhs,
                }),
                pos,
            );
            Expr::Dot(Box::new(BinaryExpr { lhs, rhs }), op_pos)
        }
        // lhs.idx_lhs[idx_rhs]
        (lhs, Expr::Index(x, pos)) => {
            let rhs = Expr::Index(
                Box::new(BinaryExpr {
                    lhs: x.lhs.into_property(),
                    rhs: x.rhs,
                }),
                pos,
            );
            Expr::Dot(Box::new(BinaryExpr { lhs, rhs }), op_pos)
        }
        // lhs.Fn() or lhs.eval()
        (_, Expr::FnCall(x, pos))
            if x.args.len() == 0 && [KEYWORD_FN_PTR, KEYWORD_EVAL].contains(&x.name.as_ref()) =>
        {
            return Err(PERR::BadInput(LexError::ImproperSymbol(format!(
                "'{}' should not be called in method style. Try {}(...);",
                x.name, x.name
            )))
            .into_err(pos));
        }
        // lhs.func!(...)
        (_, Expr::FnCall(x, pos)) if x.capture => {
            return Err(PERR::MalformedCapture(
                "method-call style does not support capturing".into(),
            )
            .into_err(pos));
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
        | (_, x @ Expr::True(_))
        | (_, x @ Expr::False(_))
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
        (Expr::StringConstant(_), Expr::StringConstant(_))
        | (Expr::CharConstant(_, _), Expr::StringConstant(_)) => (),

        // 123.456 in "xxxx"
        #[cfg(not(feature = "no_float"))]
        (x @ Expr::FloatConstant(_, _), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a float".into(),
            )
            .into_err(x.position()))
        }
        // 123 in "xxxx"
        (x @ Expr::IntegerConstant(_, _), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a number".into(),
            )
            .into_err(x.position()))
        }
        // (??? && ???) in "xxxx", (??? || ???) in "xxxx", (??? in ???) in "xxxx",
        //  true in "xxxx", false in "xxxx"
        (x @ Expr::And(_, _), Expr::StringConstant(_))
        | (x @ Expr::Or(_, _), Expr::StringConstant(_))
        | (x @ Expr::In(_, _), Expr::StringConstant(_))
        | (x @ Expr::True(_), Expr::StringConstant(_))
        | (x @ Expr::False(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not a boolean".into(),
            )
            .into_err(x.position()))
        }
        // [???, ???, ???] in "xxxx"
        (x @ Expr::Array(_, _), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not an array".into(),
            )
            .into_err(x.position()))
        }
        // #{...} in "xxxx"
        (x @ Expr::Map(_, _), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not an object map".into(),
            )
            .into_err(x.position()))
        }
        // () in "xxxx"
        (x @ Expr::Unit(_), Expr::StringConstant(_)) => {
            return Err(PERR::MalformedInExpr(
                "'in' expression for a string expects a string, not ()".into(),
            )
            .into_err(x.position()))
        }

        // "xxx" in #{...}, 'x' in #{...} - OK!
        (Expr::StringConstant(_), Expr::Map(_, _))
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
        | (x @ Expr::True(_), Expr::Map(_, _))
        | (x @ Expr::False(_), Expr::Map(_, _)) => {
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
    settings.pos = lhs.position();

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    let mut root = lhs;

    loop {
        let (current_op, current_pos) = input.peek().unwrap();
        let precedence = if let Token::Custom(c) = current_op {
            // Custom operators
            if let Some(Some(p)) = state.engine.custom_keywords.get(c) {
                *p
            } else {
                return Err(PERR::Reserved(c.clone()).into_err(*current_pos));
            }
        } else {
            current_op.precedence()
        };
        let bind_right = current_op.is_bind_right();

        // Bind left to the parent lhs expression if precedence is higher
        // If same precedence, then check if the operator binds right
        if precedence < parent_precedence || (precedence == parent_precedence && !bind_right) {
            return Ok(root);
        }

        let (op_token, pos) = input.next().unwrap();

        if cfg!(not(feature = "no_object")) && op_token == Token::Period {
            if let (Token::Identifier(_), _) = input.peek().unwrap() {
                // prevents capturing of the object properties as vars: xxx.<var>
                #[cfg(not(feature = "no_closure"))]
                {
                    state.allow_capture = false;
                }
            }
        }

        let rhs = parse_unary(input, state, lib, settings)?;

        let (next_op, next_pos) = input.peek().unwrap();
        let next_precedence = if let Token::Custom(c) = next_op {
            // Custom operators
            if let Some(Some(p)) = state.engine.custom_keywords.get(c) {
                *p
            } else {
                return Err(PERR::Reserved(c.clone()).into_err(*next_pos));
            }
        } else {
            next_op.precedence()
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

        let cmp_def = Some(false);
        let op = op_token.syntax();
        let hash = calc_script_fn_hash(empty(), &op, 2);

        let op_base = FnCallInfo {
            name: op,
            native_only: true,
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
            | Token::Ampersand
            | Token::Pipe
            | Token::XOr => Expr::FnCall(
                Box::new(FnCallInfo {
                    hash,
                    args,
                    ..op_base
                }),
                pos,
            ),

            // '!=' defaults to true when passed invalid operands
            Token::NotEqualsTo => Expr::FnCall(
                Box::new(FnCallInfo {
                    hash,
                    args,
                    def_value: Some(true),
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
                Box::new(FnCallInfo {
                    hash,
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

            #[cfg(not(feature = "no_object"))]
            Token::Period => {
                let rhs = args.pop().unwrap();
                let current_lhs = args.pop().unwrap();
                make_dot_expr(current_lhs, rhs, pos)?
            }

            Token::Custom(s) if state.engine.custom_keywords.contains_key(&s) => {
                // Accept non-native functions for custom operators
                Expr::FnCall(
                    Box::new(FnCallInfo {
                        hash,
                        args,
                        native_only: false,
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
    let mut exprs: StaticVec<Expr> = Default::default();

    // Adjust the variables stack
    match syntax.scope_delta {
        delta if delta > 0 => {
            state.stack.resize(
                state.stack.len() + delta as usize,
                ("".to_string(), ScopeEntryType::Normal),
            );
        }
        delta if delta < 0 && state.stack.len() <= delta.abs() as usize => state.stack.clear(),
        delta if delta < 0 => state
            .stack
            .truncate(state.stack.len() - delta.abs() as usize),
        _ => (),
    }

    let parse_func = &syntax.parse;

    let mut segments: StaticVec<_> = Default::default();
    segments.push(key.to_string());

    loop {
        settings.pos = input.peek().unwrap().1;
        let settings = settings.level_up();

        let token =
            if let Some(seg) = parse_func(&segments).map_err(|err| err.0.into_err(settings.pos))? {
                seg
            } else {
                break;
            };

        match token.as_str() {
            MARKER_IDENT => match input.next().unwrap() {
                (Token::Identifier(s), pos) => {
                    segments.push(s.clone());
                    exprs.push(Expr::Variable(Box::new((
                        None,
                        None,
                        0,
                        Ident::new(s, pos),
                    ))));
                }
                (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                    return Err(PERR::Reserved(s).into_err(pos));
                }
                (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
            },
            MARKER_EXPR => {
                exprs.push(parse_expr(input, state, lib, settings)?);
                segments.push(MARKER_EXPR.into());
            }
            MARKER_BLOCK => match parse_block(input, state, lib, settings)? {
                Stmt::Block(statements, pos) => {
                    exprs.push(Expr::Stmt(Box::new(statements.into()), pos));
                    segments.push(MARKER_BLOCK.into());
                }
                _ => unreachable!(),
            },
            s => match input.next().unwrap() {
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (t, _) if t.syntax().as_ref() == s => {
                    segments.push(t.syntax().into_owned());
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
            keywords: exprs,
            func: syntax.func.clone(),
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
    settings.pos = input.peek().unwrap().1;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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
        (Token::Equals, pos) => Err(PERR::BadInput(LexError::ImproperSymbol(
            "Possibly a typo of '=='?".to_string(),
        ))
        .into_err(*pos)),
        (Token::PlusAssign, pos)
        | (Token::MinusAssign, pos)
        | (Token::MultiplyAssign, pos)
        | (Token::DivideAssign, pos)
        | (Token::LeftShiftAssign, pos)
        | (Token::RightShiftAssign, pos)
        | (Token::ModuloAssign, pos)
        | (Token::PowerOfAssign, pos)
        | (Token::AndAssign, pos)
        | (Token::OrAssign, pos)
        | (Token::XOrAssign, pos) => Err(PERR::BadInput(LexError::ImproperSymbol(
            "Expecting a boolean expression, not an assignment".to_string(),
        ))
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
    // if ...
    let token_pos = eat_token(input, Token::If);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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

    Ok(Stmt::IfThenElse(
        guard,
        Box::new((if_body, else_body)),
        token_pos,
    ))
}

/// Parse a while loop.
fn parse_while(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // while ...
    let token_pos = eat_token(input, Token::While);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // while guard { body }
    ensure_not_statement_expr(input, "a boolean")?;
    let guard = parse_expr(input, state, lib, settings.level_up())?;
    ensure_not_assignment(input)?;

    settings.is_breakable = true;
    let body = Box::new(parse_block(input, state, lib, settings.level_up())?);

    Ok(Stmt::While(guard, body, token_pos))
}

/// Parse a loop statement.
fn parse_loop(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // loop ...
    let token_pos = eat_token(input, Token::Loop);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // loop { body }
    settings.is_breakable = true;
    let body = Box::new(parse_block(input, state, lib, settings.level_up())?);

    Ok(Stmt::Loop(body, token_pos))
}

/// Parse a for loop.
fn parse_for(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // for ...
    let token_pos = eat_token(input, Token::For);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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

    let prev_stack_len = state.stack.len();
    state.stack.push((name.clone(), ScopeEntryType::Normal));

    settings.is_breakable = true;
    let body = parse_block(input, state, lib, settings.level_up())?;

    state.stack.truncate(prev_stack_len);

    Ok(Stmt::For(expr, Box::new((name, body)), token_pos))
}

/// Parse a variable definition statement.
fn parse_let(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    var_type: ScopeEntryType,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // let/const... (specified in `var_type`)
    let token_pos = input.next().unwrap().1;
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // let name ...
    let (name, pos) = match input.next().unwrap() {
        (Token::Identifier(s), pos) => (s, pos),
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    // let name = ...
    let init_value = if match_token(input, Token::Equals).0 {
        // let name = expr
        Some(parse_expr(input, state, lib, settings.level_up())?)
    } else {
        None
    };

    match var_type {
        // let name = expr
        ScopeEntryType::Normal => {
            state.stack.push((name.clone(), ScopeEntryType::Normal));
            Ok(Stmt::Let(
                Box::new(Ident::new(name, pos)),
                init_value,
                token_pos,
            ))
        }
        // const name = { expr:constant }
        ScopeEntryType::Constant => {
            state.stack.push((name.clone(), ScopeEntryType::Constant));
            Ok(Stmt::Const(
                Box::new(Ident::new(name, pos)),
                init_value,
                token_pos,
            ))
        }
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
    // import ...
    let token_pos = eat_token(input, Token::Import);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    // import expr ...
    let expr = parse_expr(input, state, lib, settings.level_up())?;

    // import expr as ...
    if !match_token(input, Token::As).0 {
        return Ok(Stmt::Import(expr, None, token_pos));
    }

    // import expr as name ...
    let (name, _) = match input.next().unwrap() {
        (Token::Identifier(s), pos) => (s, pos),
        (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
            return Err(PERR::Reserved(s).into_err(pos));
        }
        (Token::LexError(err), pos) => return Err(err.into_err(pos)),
        (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
    };

    state.modules.push(name.clone());

    Ok(Stmt::Import(
        expr,
        Some(Box::new(IdentX::new(name, settings.pos))),
        token_pos,
    ))
}

/// Parse an export statement.
#[cfg(not(feature = "no_module"))]
fn parse_export(
    input: &mut TokenStream,
    _state: &mut ParseState,
    _lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    let token_pos = eat_token(input, Token::Export);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(_state.max_expr_depth)?;

    let mut exports = Vec::new();

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
                (Token::Identifier(s), pos) => Some(Ident::new(s.clone(), pos)),
                (Token::Reserved(s), pos) if is_valid_identifier(s.chars()) => {
                    return Err(PERR::Reserved(s).into_err(pos));
                }
                (Token::LexError(err), pos) => return Err(err.into_err(pos)),
                (_, pos) => return Err(PERR::VariableExpected.into_err(pos)),
            }
        } else {
            None
        };

        exports.push((Ident::new(id, id_pos), rename));

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

    // Check for duplicating parameters
    exports
        .iter()
        .enumerate()
        .try_for_each(|(i, (Ident { name: id1, .. }, _))| {
            exports
                .iter()
                .skip(i + 1)
                .find(|(Ident { name: id2, .. }, _)| id2 == id1)
                .map_or_else(|| Ok(()), |(Ident { name: id2, pos }, _)| Err((id2, *pos)))
        })
        .map_err(|(id2, pos)| PERR::DuplicatedExport(id2.to_string()).into_err(pos))?;

    Ok(Stmt::Export(exports, token_pos))
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

    let mut statements = Vec::new();
    let prev_stack_len = state.stack.len();

    #[cfg(not(feature = "no_module"))]
    let prev_mods_len = state.modules.len();

    while !match_token(input, Token::RightBrace).0 {
        // Parse statements inside the block
        settings.is_global = false;

        let stmt = match parse_stmt(input, state, lib, settings.level_up())? {
            Some(s) => s,
            None => continue,
        };

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
            (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
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

    state.stack.truncate(prev_stack_len);

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
    settings.pos = input.peek().unwrap().1;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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
) -> Result<Option<Stmt>, ParseError> {
    use ScopeEntryType::{Constant, Normal};

    let (token, token_pos) = match input.peek().unwrap() {
        (Token::EOF, pos) => return Ok(Some(Stmt::Noop(*pos))),
        x => x,
    };
    settings.pos = *token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

    match token {
        // Semicolon - empty statement
        Token::SemiColon => Ok(Some(Stmt::Noop(settings.pos))),

        Token::LeftBrace => parse_block(input, state, lib, settings.level_up()).map(Some),

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
                        allow_stmt_expr: true,
                        allow_anonymous_fn: true,
                        is_global: false,
                        is_function_scope: true,
                        is_breakable: false,
                        level: 0,
                        pos: pos,
                    };

                    let func = parse_fn(input, &mut new_state, lib, access, settings)?;

                    // Qualifiers (none) + function name + number of arguments.
                    let hash = calc_script_fn_hash(empty(), &func.name, func.params.len());

                    lib.insert(hash, func);

                    Ok(None)
                }

                (_, pos) => Err(PERR::MissingToken(
                    Token::Fn.into(),
                    format!("following '{}'", Token::Private.syntax()),
                )
                .into_err(pos)),
            }
        }

        Token::If => parse_if(input, state, lib, settings.level_up()).map(Some),
        Token::While => parse_while(input, state, lib, settings.level_up()).map(Some),
        Token::Loop => parse_loop(input, state, lib, settings.level_up()).map(Some),
        Token::For => parse_for(input, state, lib, settings.level_up()).map(Some),

        Token::Continue if settings.is_breakable => {
            let pos = eat_token(input, Token::Continue);
            Ok(Some(Stmt::Continue(pos)))
        }
        Token::Break if settings.is_breakable => {
            let pos = eat_token(input, Token::Break);
            Ok(Some(Stmt::Break(pos)))
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
                .unwrap();

            match input.peek().unwrap() {
                // `return`/`throw` at <EOF>
                (Token::EOF, pos) => Ok(Some(Stmt::ReturnWithVal(
                    (return_type, token_pos),
                    None,
                    *pos,
                ))),
                // `return;` or `throw;`
                (Token::SemiColon, _) => Ok(Some(Stmt::ReturnWithVal(
                    (return_type, token_pos),
                    None,
                    settings.pos,
                ))),
                // `return` or `throw` with expression
                (_, _) => {
                    let expr = parse_expr(input, state, lib, settings.level_up())?;
                    let pos = expr.position();
                    Ok(Some(Stmt::ReturnWithVal(
                        (return_type, token_pos),
                        Some(expr),
                        pos,
                    )))
                }
            }
        }

        Token::Try => parse_try_catch(input, state, lib, settings.level_up()).map(Some),

        Token::Let => parse_let(input, state, lib, Normal, settings.level_up()).map(Some),
        Token::Const => parse_let(input, state, lib, Constant, settings.level_up()).map(Some),

        #[cfg(not(feature = "no_module"))]
        Token::Import => parse_import(input, state, lib, settings.level_up()).map(Some),

        #[cfg(not(feature = "no_module"))]
        Token::Export if !settings.is_global => Err(PERR::WrongExport.into_err(settings.pos)),

        #[cfg(not(feature = "no_module"))]
        Token::Export => parse_export(input, state, lib, settings.level_up()).map(Some),

        _ => parse_expr_stmt(input, state, lib, settings.level_up()).map(Some),
    }
}

/// Parse a try/catch statement.
fn parse_try_catch(
    input: &mut TokenStream,
    state: &mut ParseState,
    lib: &mut FunctionsLib,
    mut settings: ParseSettings,
) -> Result<Stmt, ParseError> {
    // try ...
    let token_pos = eat_token(input, Token::Try);
    settings.pos = token_pos;

    #[cfg(not(feature = "unchecked"))]
    settings.ensure_level_within_max_limit(state.max_expr_depth)?;

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
            (Token::Identifier(s), pos) => Ident::new(s, pos),
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
        token_pos,
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

    let mut params = Vec::new();

    if !match_token(input, Token::RightParen).0 {
        let sep_err = format!("to separate the parameters of function '{}'", name);

        loop {
            match input.next().unwrap() {
                (Token::RightParen, _) => break,
                (Token::Identifier(s), pos) => {
                    state.stack.push((s.clone(), ScopeEntryType::Normal));
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

    // Check for duplicating parameters
    params
        .iter()
        .enumerate()
        .try_for_each(|(i, (p1, _))| {
            params
                .iter()
                .skip(i + 1)
                .find(|(p2, _)| p2 == p1)
                .map_or_else(|| Ok(()), |(p2, pos)| Err((p2, *pos)))
        })
        .map_err(|(p, pos)| {
            PERR::FnDuplicatedParam(name.to_string(), p.to_string()).into_err(pos)
        })?;

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
    let externals: HashSet<_> = state
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
        externals: if externals.is_empty() {
            None
        } else {
            Some(Box::new(externals))
        },
        body,
        lib: None,
    })
}

/// Creates a curried expression from a list of external variables
#[cfg(not(feature = "no_function"))]
fn make_curry_from_externals(fn_expr: Expr, externals: StaticVec<Ident>, pos: Position) -> Expr {
    if externals.is_empty() {
        return fn_expr;
    }

    let num_externals = externals.len();
    let mut args: StaticVec<_> = Default::default();

    args.push(fn_expr);

    #[cfg(not(feature = "no_closure"))]
    externals.iter().for_each(|x| {
        args.push(Expr::Variable(Box::new((None, None, 0, x.clone()))));
    });

    #[cfg(feature = "no_closure")]
    externals.into_iter().for_each(|x| {
        args.push(Expr::Variable(Box::new((None, None, 0, x.clone()))));
    });

    let hash = calc_script_fn_hash(empty(), KEYWORD_FN_PTR_CURRY, num_externals + 1);

    let expr = Expr::FnCall(
        Box::new(FnCallInfo {
            name: KEYWORD_FN_PTR_CURRY.into(),
            hash,
            args,
            ..Default::default()
        }),
        pos,
    );

    // If there are captured variables, convert the entire expression into a statement block,
    // then insert the relevant `Share` statements.
    #[cfg(not(feature = "no_closure"))]
    {
        // Statement block
        let mut statements: StaticVec<_> = Default::default();
        // Insert `Share` statements
        statements.extend(externals.into_iter().map(|x| Stmt::Share(Box::new(x))));
        // Final expression
        statements.push(Stmt::Expr(expr));
        Expr::Stmt(Box::new(statements), pos)
    }

    #[cfg(feature = "no_closure")]
    return expr;
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

    let mut params = Vec::new();

    if input.next().unwrap().0 != Token::Or {
        if !match_token(input, Token::Pipe).0 {
            loop {
                match input.next().unwrap() {
                    (Token::Pipe, _) => break,
                    (Token::Identifier(s), pos) => {
                        state.stack.push((s.clone(), ScopeEntryType::Normal));
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

    // Check for duplicating parameters
    params
        .iter()
        .enumerate()
        .try_for_each(|(i, (p1, _))| {
            params
                .iter()
                .skip(i + 1)
                .find(|(p2, _)| p2 == p1)
                .map_or_else(|| Ok(()), |(p2, pos)| Err((p2, *pos)))
        })
        .map_err(|(p, pos)| PERR::FnDuplicatedParam("".to_string(), p.to_string()).into_err(pos))?;

    // Parse function body
    settings.is_breakable = false;
    let pos = input.peek().unwrap().1;
    let body = parse_stmt(input, state, lib, settings.level_up())
        .map(|stmt| stmt.unwrap_or_else(|| Stmt::Noop(pos)))?;

    // External variables may need to be processed in a consistent order,
    // so extract them into a list.
    let externals: StaticVec<Ident> = {
        #[cfg(not(feature = "no_closure"))]
        {
            state
                .externals
                .iter()
                .map(|(k, &v)| Ident::new(k.clone(), v))
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

    // Calculate hash
    #[cfg(feature = "no_std")]
    let mut s: AHasher = Default::default();
    #[cfg(not(feature = "no_std"))]
    let mut s = DefaultHasher::new();

    s.write_usize(params.len());
    params.iter().for_each(|a| a.hash(&mut s));
    body.hash(&mut s);
    let hash = s.finish();

    // Create unique function name
    let fn_name: ImmutableString = format!("{}{:016x}", FN_ANONYMOUS, hash).into();

    // Define the function
    let script = ScriptFnDef {
        name: fn_name.clone(),
        access: FnAccess::Public,
        params,
        #[cfg(not(feature = "no_closure"))]
        externals: Default::default(),
        body,
        lib: None,
    };

    let expr = Expr::FnPointer(Box::new(IdentX::new(fn_name, settings.pos)));

    let expr = if cfg!(not(feature = "no_closure")) {
        make_curry_from_externals(expr, externals, settings.pos)
    } else {
        expr
    };

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
            self.max_expr_depth(),
            #[cfg(not(feature = "unchecked"))]
            #[cfg(not(feature = "no_function"))]
            self.max_function_expr_depth(),
        );

        let settings = ParseSettings {
            allow_if_expr: false,
            allow_stmt_expr: false,
            allow_anonymous_fn: false,
            is_global: true,
            is_function_scope: false,
            is_breakable: false,
            level: 0,
            pos: NO_POS,
        };
        let expr = parse_expr(input, &mut state, &mut functions, settings)?;

        assert!(functions.is_empty());

        match input.peek().unwrap() {
            (Token::EOF, _) => (),
            // Return error if the expression doesn't end
            (token, pos) => {
                return Err(
                    PERR::BadInput(LexError::UnexpectedInput(token.syntax().to_string()))
                        .into_err(*pos),
                )
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
        let mut statements: Vec<Stmt> = Default::default();
        let mut functions = Default::default();
        let mut state = ParseState::new(
            self,
            #[cfg(not(feature = "unchecked"))]
            self.max_expr_depth(),
            #[cfg(not(feature = "unchecked"))]
            #[cfg(not(feature = "no_function"))]
            self.max_function_expr_depth(),
        );

        while !input.peek().unwrap().0.is_eof() {
            let settings = ParseSettings {
                allow_if_expr: true,
                allow_stmt_expr: true,
                allow_anonymous_fn: true,
                is_global: true,
                is_function_scope: false,
                is_breakable: false,
                level: 0,
                pos: NO_POS,
            };

            let stmt = match parse_stmt(input, &mut state, &mut functions, settings)? {
                Some(s) => s,
                None => continue,
            };

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
                (Token::LexError(err), pos) => return Err(err.into_err(*pos)),
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
        Union::Float(value) => Some(Expr::FloatConstant(FloatWrapper(value), pos)),

        Union::Unit(_) => Some(Expr::Unit(pos)),
        Union::Int(value) => Some(Expr::IntegerConstant(value, pos)),
        Union::Char(value) => Some(Expr::CharConstant(value, pos)),
        Union::Str(value) => Some(Expr::StringConstant(Box::new(IdentX::new(value, pos)))),
        Union::Bool(true) => Some(Expr::True(pos)),
        Union::Bool(false) => Some(Expr::False(pos)),
        #[cfg(not(feature = "no_index"))]
        Union::Array(array) => {
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
        Union::Map(map) => {
            let items: Vec<_> = map
                .into_iter()
                .map(|(k, v)| (IdentX::new(k, pos), map_dynamic_to_expr(v, pos)))
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
