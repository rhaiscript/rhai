//! _(internals)_ The instruction set, the pools it indexes,
//! and the checks a [`Chunk`] must pass before it runs.
//! Exported under the `internals` feature only.

mod chain;
mod chunk;
pub(crate) mod code;
mod custom_syntax;
mod op;
mod positions;
pub(crate) mod sites;
mod strings;
mod switch;
mod verify;

pub(crate) use chain::{Chain, Root, Step, StepFlags, Tail};
pub(crate) use code::Code;
pub(crate) use custom_syntax::CustomSyntaxSite;
pub(crate) use op::AssignOp;
pub(crate) use positions::site_to_position;
pub(crate) use positions::{Positions, TableError};
pub(crate) use strings::{BadTable, Strings};
pub(crate) use switch::{Switch, SwitchRange};
pub(crate) use verify::{verify, Pools, VerifyError};

#[crate::expose_under_internals]
use chunk::Chunk;

#[crate::expose_under_internals]
use code::disassemble;

#[crate::expose_under_internals]
use op::{Op, Receiver};
