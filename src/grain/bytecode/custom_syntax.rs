use crate::grain::bytecode::Chunk;

#[cfg(feature = "no_std")]
use std::prelude::v1::*;

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
    /// Tuple is `(chunk, literal)`.
    pub inputs: Vec<(Chunk, Option<u32>)>,
    /// Address to jump to when running the site fails with a `break` or
    /// `continue` pseudo-error (see [`Op::BreakFlow`](crate::grain::bytecode::Op::BreakFlow)), with the break value
    /// and an `is_break` flag pushed.
    ///
    /// `None` when no loop in the same frame encloses the site, in which case
    /// the pseudo-error propagates unchanged.
    pub on_loop_flow: Option<u32>,
}

impl CustomSyntaxSite {
    /// Dump the disassembly of the operation.
    #[cfg(feature = "internals")]
    pub fn disassemble(&self, program: &crate::grain::Program) -> String {
        match self.on_loop_flow {
            Some(target) => format!(
                "{} [{} input(s)] on_loop_flow={target}",
                program.name(self.key).unwrap_or("?"),
                self.inputs.len(),
            ),
            None => format!(
                "{} [{} input(s)]",
                program.name(self.key).unwrap_or("?"),
                self.inputs.len(),
            ),
        }
    }
}
