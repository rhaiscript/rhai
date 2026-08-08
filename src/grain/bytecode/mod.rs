//! The instruction set, the pools it indexes, and the checks a chunk must pass
//! before it runs.

pub mod code;

mod chain;
mod chunk;
mod op;
mod positions;
mod strings;
mod switch;
mod verify;

pub use chain::{Chain, Root, Step, Tail};
pub use chunk::Chunk;
pub use code::{assemble, disassemble, resolve_switch_targets, AssembleError, Code};
pub use op::{AssignOp, Op, Receiver};
pub use positions::{Positions, TableError};
pub use strings::{BadTable, Strings};
pub use switch::{probe, Switch, SwitchCase, SwitchRange};
pub use verify::{verify, Pools, VerifyError};
