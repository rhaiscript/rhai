pub mod code;

mod chain;
mod chunk;
mod strings;
mod switch;
mod op;
mod positions;
mod verify;

pub use chain::{Chain, Root, Step, Tail};
pub use strings::{BadTable, Strings};
pub use switch::{probe, Switch, SwitchCase, SwitchRange};
pub use chunk::Chunk;
pub use code::{assemble, disassemble, resolve_switch_targets, AssembleError, Code};
pub use op::{AssignOp, Op, Receiver};
pub use positions::{Positions, TableError};
pub use verify::{verify, Pools, VerifyError};
