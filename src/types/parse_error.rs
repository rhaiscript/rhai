//! Module containing error definitions for the parsing process.

use crate::tokenizer::is_valid_identifier;
use crate::{Position, RhaiError, ERR};
#[cfg(feature = "no_std")]
use core_error::Error;
#[cfg(not(feature = "no_std"))]
use std::error::Error;
use std::fmt;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Error encountered when tokenizing the script text.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
#[non_exhaustive]
#[must_use]
pub enum LexError {
    /// An unexpected symbol is encountered when tokenizing the script text.
    UnexpectedInput(String),
    /// A string literal is not terminated before a new-line or EOF.
    UnterminatedString,
    /// An identifier or string literal is longer than the maximum allowed length.
    StringTooLong(usize),
    /// An string/character/numeric escape sequence is in an invalid format.
    MalformedEscapeSequence(String),
    /// An numeric literal is in an invalid format.
    MalformedNumber(String),
    /// An character literal is in an invalid format.
    MalformedChar(String),
    /// An identifier is in an invalid format.
    MalformedIdentifier(String),
    /// Bad symbol encountered when tokenizing the script text.
    ImproperSymbol(String, String),
}

impl Error for LexError {}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedInput(s) => write!(f, "Unexpected '{s}'"),
            Self::MalformedEscapeSequence(s) => write!(f, "Invalid escape sequence: '{s}'"),
            Self::MalformedNumber(s) => write!(f, "Invalid number: '{s}'"),
            Self::MalformedChar(s) => write!(f, "Invalid character: '{s}'"),
            Self::MalformedIdentifier(s) => write!(f, "Variable name is not proper: '{s}'"),
            Self::UnterminatedString => f.write_str("Open string is not terminated"),
            Self::StringTooLong(max) => write!(f, "String is too long (max {max})"),
            Self::ImproperSymbol(s, d) if d.is_empty() => {
                write!(f, "Invalid symbol encountered: '{s}'")
            }
            Self::ImproperSymbol(.., d) => f.write_str(d),
        }
    }
}

impl LexError {
    /// Convert a [`LexError`] into a [`ParseError`].
    #[cold]
    #[inline(never)]
    pub fn into_err(self, pos: Position) -> ParseError {
        ParseError(Box::new(self.into()), pos)
    }
}

/// Error encountered when parsing a script.
///
/// Some errors never appear when certain features are turned on.
/// They still exist so that the application can turn features on and off without going through
/// massive code changes to remove/add back enum variants in match statements.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
#[non_exhaustive]
#[must_use]
pub enum ParseErrorType {
    /// The script ends prematurely.
    UnexpectedEOF,
    /// Error in the script text. Wrapped value is the lex error.
    BadInput(LexError),
    /// An unknown operator is encountered. Wrapped value is the operator.
    UnknownOperator(String),
    /// Expecting a particular token but not finding one. Wrapped values are the token and description.
    MissingToken(String, String),
    /// Expecting a particular symbol but not finding one. Wrapped value is the description.
    MissingSymbol(String),
    /// An expression in function call arguments `()` has syntax error. Wrapped value is the error
    /// description (if any).
    ///
    /// # Deprecated
    ///
    /// This error is no longer used and will be removed in the next major version.
    #[deprecated(
        since = "1.16.0",
        note = "This error variant is no longer used and will be removed in the next major version."
    )]
    MalformedCallExpr(String),
    /// An expression in indexing brackets `[]` has syntax error. Wrapped value is the error
    /// description (if any).
    MalformedIndexExpr(String),
    /// An expression in an `in` expression has syntax error. Wrapped value is the error description (if any).
    ///
    /// # Deprecated
    ///
    /// This error is no longer used and will be removed in the next major version.
    #[deprecated(
        since = "1.16.0",
        note = "This error variant is no longer used and will be removed in the next major version."
    )]
    MalformedInExpr(String),
    /// A capturing  has syntax error. Wrapped value is the error description (if any).
    MalformedCapture(String),
    /// A map definition has duplicated property names. Wrapped value is the property name.
    DuplicatedProperty(String),
    /// A `switch` case is duplicated.
    ///
    /// # Deprecated
    ///
    /// This error is no longer used and will be removed in the next major version.
    #[deprecated(
        since = "1.9.0",
        note = "This error variant is no longer used and will be removed in the next major version."
    )]
    DuplicatedSwitchCase,
    /// A variable name is duplicated. Wrapped value is the variable name.
    DuplicatedVariable(String),
    /// A numeric case of a `switch` statement is in an appropriate place.
    WrongSwitchIntegerCase,
    /// The default case of a `switch` statement is in an appropriate place.
    WrongSwitchDefaultCase,
    /// The case condition of a `switch` statement is not appropriate.
    WrongSwitchCaseCondition,
    /// Missing a property name for custom types and maps.
    PropertyExpected,
    /// Missing a variable name after the `let`, `const`, `for` or `catch` keywords.
    VariableExpected,
    /// Forbidden variable name.  Wrapped value is the variable name.
    ForbiddenVariable(String),
    /// An identifier is a reserved symbol.
    Reserved(String),
    /// An expression is of the wrong type.
    /// Wrapped values are the type requested and type of the actual result.
    MismatchedType(String, String),
    /// Missing an expression. Wrapped value is the expression type.
    ExprExpected(String),
    /// Defining a doc-comment in an appropriate place (e.g. not at global level).
    WrongDocComment,
    /// Defining a function `fn` in an appropriate place (e.g. inside another function).
    WrongFnDefinition,
    /// Defining a function with a name that conflicts with an existing function.
    /// Wrapped values are the function name and number of parameters.
    FnDuplicatedDefinition(String, usize),
    /// Missing a function name after the `fn` keyword.
    FnMissingName,
    /// A function definition is missing the parameters list. Wrapped value is the function name.
    FnMissingParams(String),
    /// A function definition has duplicated parameters. Wrapped values are the function name and
    /// parameter name.
    FnDuplicatedParam(String, String),
    /// A function definition is missing the body. Wrapped value is the function name.
    FnMissingBody(String),
    /// Export statement not at global level.
    WrongExport,
    /// Assignment to an a constant variable. Wrapped value is the constant variable name.
    AssignmentToConstant(String),
    /// Assignment to an inappropriate LHS (left-hand-side) expression.
    /// Wrapped value is the error message (if any).
    AssignmentToInvalidLHS(String),
    /// A variable is already defined.
    ///
    /// Only appears when variables shadowing is disabled.
    VariableExists(String),
    /// A variable is not found.
    ///
    /// Only appears when strict variables mode is enabled.
    VariableUndefined(String),
    /// An imported module is not found.
    ///
    /// Only appears when strict variables mode is enabled.
    ModuleUndefined(String),
    /// Expression exceeding the maximum levels of complexity.
    ExprTooDeep,
    /// Number of scripted functions over maximum limit.
    TooManyFunctions,
    /// Literal exceeding the maximum size. Wrapped values are the data type name and the maximum size.
    LiteralTooLarge(String, usize),
    /// Break statement not inside a loop.
    LoopBreak,
}

impl fmt::Display for ParseErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadInput(err) => write!(f, "{err}"),

            Self::UnknownOperator(s) => write!(f, "Unknown operator: '{s}'"),

            Self::MalformedIndexExpr(s) if s.is_empty() => f.write_str("Invalid index in indexing expression"),
            Self::MalformedIndexExpr(s) =>  f.write_str(s),

            Self::MalformedCapture(s) if s.is_empty()  => f.write_str("Invalid capturing"),
            Self::MalformedCapture(s) => f.write_str(s),

            Self::FnDuplicatedDefinition(s, n) => {
                write!(f, "Function {s} with ")?;
                match n {
                    0 => f.write_str("no parameters already exists"),
                    1 => f.write_str("1 parameter already exists"),
                    _ => write!(f, "{n} parameters already exists"),
                }
            }

            Self::FnMissingBody(s) if s.is_empty()  => f.write_str("Expecting body statement block for anonymous function"),
            Self::FnMissingBody(s) =>  write!(f, "Expecting body statement block for function {s}"),

            Self::FnMissingParams(s) => write!(f, "Expecting parameters for function {s}"),
            Self::FnDuplicatedParam(s, arg) => write!(f, "Duplicated parameter {arg} for function {s}"),

            Self::DuplicatedProperty(s) => write!(f, "Duplicated property for object map literal: {s}"),
            Self::DuplicatedVariable(s) => write!(f, "Duplicated variable name: {s}"),

            Self::VariableExists(s) => write!(f, "Variable already defined: {s}"),
            Self::VariableUndefined(s) => write!(f, "Undefined variable: {s}"),
            Self::ModuleUndefined(s) => write!(f, "Undefined module: {s}"),

            Self::MismatchedType(r, a) => write!(f, "Expecting {r}, not {a}"),
            Self::ExprExpected(s) => write!(f, "Expecting {s} expression"),
            Self::MissingToken(token, s) => write!(f, "Expecting '{token}' {s}"),

            Self::MissingSymbol(s) if s.is_empty() => f.write_str("Expecting a symbol"),
            Self::MissingSymbol(s) => f.write_str(s),

            Self::AssignmentToConstant(s) if s.is_empty() => f.write_str("Cannot assign to a constant value"),
            Self::AssignmentToConstant(s) =>  write!(f, "Cannot assign to constant {s}"),

            Self::AssignmentToInvalidLHS(s) if s.is_empty() => f.write_str("Expression cannot be assigned to"),
            Self::AssignmentToInvalidLHS(s) => f.write_str(s),

            Self::LiteralTooLarge(typ, max) => write!(f, "{typ} exceeds the maximum limit ({max})"),
            Self::Reserved(s) if is_valid_identifier(s) => write!(f, "'{s}' is a reserved keyword"),
            Self::Reserved(s) => write!(f, "'{s}' is a reserved symbol"),
            Self::UnexpectedEOF => f.write_str("Script is incomplete"),
            Self::WrongSwitchIntegerCase => f.write_str("Numeric switch case cannot follow a range case"),
            Self::WrongSwitchDefaultCase => f.write_str("Default switch case must be the last"),
            Self::WrongSwitchCaseCondition => f.write_str("This switch case cannot have a condition"),
            Self::PropertyExpected => f.write_str("Expecting name of a property"),
            Self::VariableExpected => f.write_str("Expecting name of a variable"),
            Self::ForbiddenVariable(s) => write!(f, "Forbidden variable name: {s}"),
            Self::WrongFnDefinition => f.write_str("Function definitions must be at global level and cannot be inside a block or another function"),
            Self::FnMissingName => f.write_str("Expecting function name in function declaration"),
            Self::WrongDocComment => f.write_str("Doc-comment must be followed immediately by a function definition"),
            Self::WrongExport => f.write_str("Export statement can only appear at global level"),
            Self::ExprTooDeep => f.write_str("Expression exceeds maximum complexity"),
            Self::TooManyFunctions => f.write_str("Number of functions defined exceeds maximum limit"),
            Self::LoopBreak => f.write_str("Break statement should only be used inside a loop"),

            #[allow(deprecated)]
            Self::DuplicatedSwitchCase => f.write_str("Duplicated switch case"),

            #[allow(deprecated)]
            Self::MalformedCallExpr(s)  if s.is_empty() => f.write_str(s),
            #[allow(deprecated)]
            Self::MalformedCallExpr(..) => f.write_str("Invalid expression in function call arguments"),

            #[allow(deprecated)]
            Self::MalformedInExpr(s) if s.is_empty() => f.write_str("Invalid 'in' expression"),
            #[allow(deprecated)]
            Self::MalformedInExpr(s) =>  f.write_str(s),
        }
    }
}

impl From<LexError> for ParseErrorType {
    #[cold]
    #[inline(never)]
    fn from(err: LexError) -> Self {
        match err {
            LexError::StringTooLong(max) => {
                Self::LiteralTooLarge("Length of string".to_string(), max)
            }
            _ => Self::BadInput(err),
        }
    }
}

/// Error when parsing a script.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
#[must_use]
pub struct ParseError(
    /// Parse error type.
    pub Box<ParseErrorType>,
    /// [Position] of the parse error.
    pub Position,
);

impl Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)?;

        // Do not write any position if None
        if !self.1.is_none() {
            write!(f, " ({})", self.1)?;
        }

        Ok(())
    }
}

impl ParseError {
    /// Get the [type][ParseErrorType] of this parse error.
    #[cold]
    #[inline(never)]
    pub const fn err_type(&self) -> &ParseErrorType {
        &self.0
    }
    /// Get the [position][Position] of this parse error.
    #[cold]
    #[inline(never)]
    #[must_use]
    pub const fn position(&self) -> Position {
        self.1
    }
}

impl From<ParseErrorType> for RhaiError {
    #[cold]
    #[inline(never)]
    fn from(err: ParseErrorType) -> Self {
        Self::new(err.into())
    }
}

impl From<ParseErrorType> for ERR {
    #[cold]
    #[inline(never)]
    fn from(err: ParseErrorType) -> Self {
        Self::ErrorParsing(err, Position::NONE)
    }
}

impl From<ParseError> for RhaiError {
    #[cold]
    #[inline(never)]
    fn from(err: ParseError) -> Self {
        Self::new(err.into())
    }
}

impl From<ParseError> for ERR {
    #[cold]
    #[inline(never)]
    fn from(err: ParseError) -> Self {
        Self::ErrorParsing(*err.0, err.1)
    }
}
