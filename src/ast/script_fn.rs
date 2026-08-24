//! Module defining script-defined functions.
#![cfg(not(feature = "no_function"))]

use super::{FnAccess, StmtBlock};
use crate::{FnArgsVec, ImmutableString, Position};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;
use std::{fmt, hash::Hash};

/// A type containing the body of a script-defined function.
#[derive(Debug, Clone)]
pub enum ScriptFuncPayload {
    /// Normal statements block.
    Statements(StmtBlock),
    /// _(grain)_ A Rhai Grain VM to run the function body.
    #[cfg(feature = "grain")]
    GrainVM {
        program: crate::grain::SharedProgram,
        params: FnArgsVec<u32>,
        chunk: crate::grain::bytecode::Chunk,
    },
}

impl Default for ScriptFuncPayload {
    #[inline(always)]
    fn default() -> Self {
        Self::Statements(StmtBlock::NONE)
    }
}

impl ScriptFuncPayload {
    /// Get the start position.
    #[inline(always)]
    #[must_use]
    pub const fn position(&self) -> Position {
        match self {
            Self::Statements(block) => block.position(),
            #[cfg(feature = "grain")]
            ScriptFuncPayload::GrainVM { .. } => Position::NONE,
        }
    }
    /// Get the end position.
    #[inline(always)]
    #[must_use]
    pub const fn end_position(&self) -> Position {
        match self {
            Self::Statements(block) => block.end_position(),
            #[cfg(feature = "grain")]
            ScriptFuncPayload::GrainVM { .. } => Position::NONE,
        }
    }
}

/// _(internals)_ A type containing information on a script-defined function.
/// Exported under the `internals` feature only.
#[derive(Debug, Clone)]
pub struct ScriptFuncDef {
    /// Function body.
    pub body: ScriptFuncPayload,
    /// Function name.
    pub name: ImmutableString,
    /// Function access mode.
    pub access: FnAccess,
    /// Type of `this` pointer, if any.
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    pub this_type: Option<ImmutableString>,
    /// Names of function parameters.
    pub params: FnArgsVec<ImmutableString>,
    /// _(metadata)_ Function doc-comments (if any). Exported under the `metadata` feature only.
    ///
    /// Doc-comments are comment lines beginning with `///` or comment blocks beginning with `/**`,
    /// placed immediately before a function definition.
    ///
    /// Block doc-comments are kept in a single string with line-breaks within.
    ///
    /// Line doc-comments are merged, with line-breaks, into a single string without a termination line-break.
    ///
    /// Leading white-spaces are stripped, and each string always starts with the corresponding
    /// doc-comment leader: `///` or `/**`.
    ///
    /// Each line in non-block doc-comments starts with `///`.
    #[cfg(feature = "metadata")]
    pub comments: crate::StaticVec<crate::SmartString>,
}

impl ScriptFuncDef {
    /// Clone this [`ScriptFuncDef`] but with only signature-related info.
    ///
    /// The body of the function is removed, as well as comments (if any).
    #[allow(dead_code)]
    pub(crate) fn clone_function_signatures(&self) -> Self {
        Self {
            name: self.name.clone(),
            access: self.access,
            body: <_>::default(),
            #[cfg(not(feature = "no_object"))]
            this_type: self.this_type.clone(),
            params: self.params.clone(),
            #[cfg(feature = "metadata")]
            comments: <_>::default(),
        }
    }
}

impl fmt::Display for ScriptFuncDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_object"))]
        let this_type = self
            .this_type
            .as_ref()
            .map_or(String::new(), |s| format!("{s:?}."));

        #[cfg(feature = "no_object")]
        let this_type = "";

        write!(
            f,
            "{}{}{}({})",
            match self.access {
                FnAccess::Public => "",
                FnAccess::Private => "private ",
            },
            this_type,
            self.name,
            self.params
                .iter()
                .map(ImmutableString::as_str)
                .collect::<FnArgsVec<_>>()
                .join(", ")
        )
    }
}

/// A type containing the metadata of a script-defined function.
///
/// Not available under `no_function`.
///
/// Created by [`AST::iter_functions`][super::AST::iter_functions].
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
#[non_exhaustive]
pub struct ScriptFnMetadata<'a> {
    /// Function name.
    pub name: &'a str,
    /// Function parameters (if any).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub params: Vec<&'a str>,
    /// Function access mode.
    pub access: FnAccess,
    /// Type of `this` pointer, if any.
    /// Not available under `no_object`.
    #[cfg(not(feature = "no_object"))]
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub this_type: Option<&'a str>,
    /// _(metadata)_ Function doc-comments (if any).
    /// Exported under the `metadata` feature only.
    ///
    /// Doc-comments are comment lines beginning with `///` or comment blocks beginning with `/**`,
    /// placed immediately before a function definition.
    ///
    /// Block doc-comments are kept in a single string slice with line-breaks within.
    ///
    /// Line doc-comments are merged, with line-breaks, into a single string slice without a termination line-break.
    ///
    /// Leading white-spaces are stripped, and each string slice always starts with the
    /// corresponding doc-comment leader: `///` or `/**`.
    ///
    /// Each line in non-block doc-comments starts with `///`.
    #[cfg(feature = "metadata")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<&'a str>,
}

impl fmt::Display for ScriptFnMetadata<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(feature = "no_object"))]
        let this_type = self
            .this_type
            .as_ref()
            .map_or(String::new(), |s| format!("{s:?}."));

        #[cfg(feature = "no_object")]
        let this_type = "";

        write!(
            f,
            "{}{}{}({})",
            match self.access {
                FnAccess::Public => "",
                FnAccess::Private => "private ",
            },
            this_type,
            self.name,
            self.params
                .iter()
                .copied()
                .collect::<FnArgsVec<_>>()
                .join(", ")
        )
    }
}

impl<'a> From<&'a ScriptFuncDef> for ScriptFnMetadata<'a> {
    #[inline]
    fn from(value: &'a ScriptFuncDef) -> Self {
        Self {
            name: &value.name,
            params: value.params.iter().map(ImmutableString::as_str).collect(),
            access: value.access,
            #[cfg(not(feature = "no_object"))]
            this_type: value.this_type.as_deref(),
            #[cfg(feature = "metadata")]
            comments: value.comments.iter().map(<_>::as_ref).collect(),
        }
    }
}
