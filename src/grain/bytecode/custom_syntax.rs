use crate::grain::bytecode::Chunk;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// One `$expr$`/`$block$` input to a custom syntax invocation, as the
/// compiler lowered it.
#[derive(Debug, Clone)]
pub enum CustomInput {
    /// A lowered chunk, callable like a zero-parameter function that reuses
    /// the enclosing frame's `Slots`/scope base rather than starting a fresh
    /// one of its own.
    Chunk(Chunk, Option<u32>),
    /// The compiler could not lower this one input, so it stays an unlowered
    /// [`AST`][crate::AST] fragment in the residual pool.
    ///
    /// Never present in a *written* artifact: [`Program::write`] refuses any
    /// program that still has residuals, whether from a whole custom-syntax
    /// node or from one input like this.
    ///
    /// [`Program::write`]: crate::grain::Program::write
    #[cfg(not(feature = "no_ast"))]
    Residual(u32),
}

/// One [`Op::CustomSyntax`](crate::grain::bytecode::Op::CustomSyntax) site:
/// the registered custom syntax to invoke, plus its (possibly only partially
/// lowered) inputs.
#[derive(Debug, Clone)]
pub struct CustomSyntaxSite {
    /// Name-pool index of the custom syntax's key -- its first token, which
    /// is what [`Engine::custom_syntax`](crate::Engine) is keyed on.
    pub key: u32,
    /// Const-pool index of the custom syntax's cloned `state` value.
    pub state: u32,
    /// Each `$expr$`/`$block$` input, in source order.
    pub inputs: Vec<CustomInput>,
}

impl CustomSyntaxSite {
    /// Dump the disassembly of the operation.
    #[cfg(feature = "internals")]
    pub fn disassemble(&self, program: &crate::grain::Program) -> String {
        format!(
            "{} [{} input(s)]",
            program.name(self.key).unwrap_or("?"),
            self.inputs.len(),
        )
    }
}
