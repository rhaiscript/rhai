use core::ops::{Range, RangeInclusive};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use rhai::{tokenizer::Token, Array, Blob, Dynamic, Map, INT};

use crate::grain::bytecode::{AssignOp, Chain, Root, Step, Tail};
use crate::grain::format::abi::Abi;
use crate::grain::format::{
    constant, put_ivarint, put_str, put_uvarint, root_tag, step_tag, tail_tag, MAGIC, VERSION,
};
use crate::grain::program::Program;

/// Why a program cannot be written out.
///
/// Every variant names the construct that blocked it. A serializer that only
/// says "no" leaves the author guessing which line to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// The program still hands fragments to rhai's walker, and a fragment is a
    /// real `Expr` tree.
    ///
    /// Names the first construct responsible and where it is, because a caller
    /// deciding whether to ship source instead needs to know what it is
    /// falling back for.
    HasResiduals {
        /// How many fragments the program still has
        count: usize,
        /// What the first of them is
        construct: &'static str,
        /// Where it is in the source
        pos: rhai::Position,
    },
    /// The program still carries rhai's own function library rather than
    /// chunks, so its functions are ASTs an artifact cannot hold.
    HasScriptFunctions,
    /// A pooled constant carries something that has no meaning in another
    /// process — a host type, a function pointer, a clock reading.
    UnserializableConstant {
        /// Index into the constant pool
        index: usize,
        /// What the constant holds
        type_name: String,
    },
    /// An operator token that does not survive `syntax -> token`. Storing it
    /// would silently change which built-in the VM reaches.
    AmbiguousToken {
        /// The token's syntax
        token: String,
    },
}

impl core::fmt::Display for WriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::HasResiduals {
                count,
                construct,
                pos,
            } => write!(
                f,
                "{construct} at {pos} is not compiled yet, so this program still has \
                 {count} fragment(s) that only rhai's walker can evaluate"
            ),
            Self::HasScriptFunctions => {
                f.write_str("script functions are still ASTs and cannot be written")
            }
            Self::UnserializableConstant { index, type_name } => write!(
                f,
                "constant {index} is a `{type_name}`, which has no meaning in another process"
            ),
            Self::AmbiguousToken { token } => {
                write!(f, "operator token `{token}` does not survive a round trip")
            }
        }
    }
}

/// Whether an artifact carries its own diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Positions {
    Keep,
    Strip,
}

pub(super) fn write(program: &Program, positions: Positions) -> Result<Vec<u8>, WriteError> {
    // Refuse before encoding anything, so a rejection cannot leave a caller
    // holding a half-written buffer that happens to parse.
    if program.residual_count() > 0 {
        let (construct, pos) = program
            .first_unsupported()
            .unwrap_or(("an unlowered expression", rhai::Position::NONE));
        return Err(WriteError::HasResiduals {
            count: program.residual_count(),
            construct,
            pos,
        });
    }
    if program.lib().is_some() {
        return Err(WriteError::HasScriptFunctions);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());

    let abi = Abi::host();
    out.push(abi.int_bytes);
    out.push(abi.float_bytes);
    out.extend_from_slice(&abi.flags.to_le_bytes());

    put_str(&mut out, program.source().map_or("", |s| s.as_str()));

    // One blob and a list of spans, so a loader can point at the names where
    // they lie instead of allocating one box per name.
    let names = program.names();
    put_uvarint(&mut out, names.len() as u64);
    for start in names.starts().iter().skip(1) {
        put_uvarint(&mut out, u64::from(*start));
    }
    put_uvarint(&mut out, names.blob().len() as u64);
    out.extend_from_slice(names.blob());

    put_uvarint(&mut out, program.consts().len() as u64);
    for (index, value) in program.consts().iter().enumerate() {
        put_constant(&mut out, value)
            .map_err(|type_name| WriteError::UnserializableConstant { index, type_name })?;
    }

    put_uvarint(&mut out, program.tokens().len() as u64);
    for token in program.tokens() {
        put_token(&mut out, token)?;
    }

    put_uvarint(&mut out, program.assign_ops().len() as u64);
    for entry in program.assign_ops() {
        put_assign_op(&mut out, entry)?;
    }

    put_uvarint(&mut out, program.chains().len() as u64);
    for chain in program.chains() {
        put_chain_spec(&mut out, chain);
    }

    put_switches(&mut out, program.switches());

    // Chunks: main first, then one per compiled function. Entry offsets are
    // into the single code buffer below.
    put_chunk(&mut out, program.main());
    put_uvarint(&mut out, program.functions().len() as u64);
    for function in program.functions() {
        put_uvarint(&mut out, u64::from(function.name));
        put_uvarint(&mut out, function.params.len() as u64);
        for param in &function.params {
            put_uvarint(&mut out, u64::from(*param));
        }
        put_chunk(&mut out, &function.chunk);
    }

    // Verbatim. This is the whole point of the byte encoding: what a loader
    // hands the VM is a slice of the artifact, not something rebuilt from it.
    let code = program.code();
    put_uvarint(&mut out, code.len() as u64);
    out.extend_from_slice(code);

    // Last, and length-prefixed, so removing it is a truncation rather than a
    // re-encode — and so a reader that finds nothing there is reading a
    // deliberately stripped artifact, not a damaged one.
    let table = match positions {
        Positions::Keep => program.positions().to_table(),
        Positions::Strip => Vec::new(),
    };
    put_uvarint(&mut out, table.len() as u64);
    out.extend_from_slice(&table);

    Ok(out)
}

/// A step's position, which travels with the step rather than in the position
/// table — see [`Step::pos`]. Line zero means none, because rhai's own line
/// numbers start at one.
fn put_position(out: &mut Vec<u8>, pos: rhai::Position) {
    put_uvarint(out, pos.line().unwrap_or(0) as u64);
    put_uvarint(out, pos.position().unwrap_or(0) as u64);
}

fn put_chain_spec(out: &mut Vec<u8>, chain: &Chain) {
    match chain.root {
        Root::Local { slot, name } => {
            out.push(root_tag::LOCAL);
            put_uvarint(out, u64::from(slot));
            put_uvarint(out, u64::from(name));
        }
        Root::Named { name, pos } => {
            out.push(root_tag::NAMED);
            put_uvarint(out, u64::from(name));
            put_position(out, pos);
        }
        Root::This { pos } => {
            out.push(root_tag::THIS);
            put_position(out, pos);
        }
        Root::Temporary => out.push(root_tag::TEMPORARY),
    }
    put_uvarint(out, u64::from(chain.operands));

    put_uvarint(out, chain.steps.len() as u64);
    for step in &chain.steps {
        match step {
            Step::Index {
                operand,
                pos,
                bracket,
            } => {
                out.push(step_tag::INDEX);
                put_uvarint(out, u64::from(*operand));
                put_position(out, *pos);
                put_position(out, *bracket);
            }
            Step::Property {
                name,
                getter,
                setter,
                pos,
            } => {
                out.push(step_tag::PROPERTY);
                put_uvarint(out, u64::from(*name));
                put_uvarint(out, u64::from(*getter));
                put_uvarint(out, u64::from(*setter));
                put_position(out, *pos);
            }
            Step::Method {
                name,
                argc,
                operand,
                pos,
            } => {
                out.push(step_tag::METHOD);
                put_uvarint(out, u64::from(*name));
                out.push(*argc);
                put_uvarint(out, u64::from(*operand));
                put_position(out, *pos);
            }
        }
    }

    match &chain.tail {
        Tail::Read => out.push(tail_tag::READ),
        Tail::Assign { op: None } => out.push(tail_tag::ASSIGN),
        Tail::Assign { op: Some(op) } => {
            out.push(tail_tag::ASSIGN_OP);
            put_uvarint(out, u64::from(*op));
        }
    }
}

/// Write the switch tables, behind a probe that says whether their hashes will
/// still mean anything on the other side.
///
/// Rhai's parser keeps only the *hash* of a case value (`ast/stmt.rs:336`), so
/// there is nothing here to re-hash at load — and by default those hashes do
/// not survive the trip, because rhai's default features include
/// `ahash/runtime-rng` and the seed is drawn per process. An artifact with a
/// `switch` in it therefore requires `config::hashing::set_hashing_seed` with
/// the same seed on both sides.
///
/// The probe is what turns that from a silent wrong answer — every subject
/// dispatched to the default — into a refusal to load. It goes here rather
/// than in the ABI fingerprint because it only constrains artifacts that
/// actually contain a `switch`; making every program agree about a hashing
/// seed would be a restriction bought for nothing.
fn put_switches(out: &mut Vec<u8>, switches: &[crate::grain::bytecode::Switch]) {
    put_uvarint(out, switches.len() as u64);
    if switches.is_empty() {
        return;
    }
    out.extend_from_slice(&crate::grain::bytecode::probe().to_le_bytes());

    for switch in switches {
        put_uvarint(out, switch.cases.len() as u64);
        for case in &switch.cases {
            // Fixed width: a hash is eight bytes of noise, which a varint
            // would spend ten on.
            out.extend_from_slice(&case.hash.to_le_bytes());
            put_uvarint(out, u64::from(case.target));
        }

        put_uvarint(out, switch.ranges.len() as u64);
        for range in &switch.ranges {
            #[allow(clippy::useless_conversion)]
            put_ivarint(out, i64::from(range.from));
            #[allow(clippy::useless_conversion)]
            put_ivarint(out, i64::from(range.to));
            out.push(u8::from(range.inclusive));
            put_uvarint(out, u64::from(range.target));
        }

        put_uvarint(out, u64::from(switch.default));
    }
}

fn put_chunk(out: &mut Vec<u8>, chunk: &crate::grain::bytecode::Chunk) {
    put_uvarint(out, u64::from(chunk.entry()));
    put_uvarint(out, u64::from(chunk.end()));
    put_uvarint(out, u64::from(chunk.max_stack()));
}

/// Store an operator token as its syntax, and prove that reading it back gives
/// the same token.
///
/// The check is not ceremony. `Plus` and `UnaryPlus` share the syntax `"+"`,
/// so the reverse lookup collapses them — and the token is what the built-in
/// operator lookup keys on, so a collapsed one would quietly reach a different
/// implementation.
fn put_token(out: &mut Vec<u8>, token: &Token) -> Result<(), WriteError> {
    let ambiguous = || WriteError::AmbiguousToken {
        token: format!("{token:?}"),
    };

    if !token.is_literal() {
        return Err(ambiguous());
    }
    let syntax = token.literal_syntax();
    if Token::lookup_symbol_from_syntax(syntax).as_ref() != Some(token) {
        return Err(WriteError::AmbiguousToken {
            token: syntax.to_string(),
        });
    }
    put_str(out, syntax);
    Ok(())
}

fn put_assign_op(out: &mut Vec<u8>, entry: &AssignOp) -> Result<(), WriteError> {
    put_token(out, &entry.op_assign)?;
    put_uvarint(out, u64::from(entry.op_assign_name));
    put_token(out, &entry.op)?;
    put_uvarint(out, u64::from(entry.op_name));
    Ok(())
}

/// Encode a constant, or name the type that stopped it.
///
/// The accepted set is `compile::poolable::is_poolable`'s, which the compiler
/// already applies when filling the pool — so a rejection here means the two
/// have drifted apart, not that a script did something exotic.
fn put_constant(out: &mut Vec<u8>, value: &Dynamic) -> Result<(), String> {
    if value.is_unit() {
        out.push(constant::UNIT);
        return Ok(());
    }
    if let Ok(flag) = value.as_bool() {
        out.push(if flag {
            constant::TRUE
        } else {
            constant::FALSE
        });
        return Ok(());
    }
    if let Ok(number) = value.as_int() {
        out.push(constant::INT);
        // `INT` narrows to `i32` under `only_i32`, so the widening is real
        // there even though it is a no-op here. The fingerprint is what stops
        // the reader from decoding the wrong width back.
        #[allow(clippy::useless_conversion)]
        put_ivarint(out, i64::from(number));
        return Ok(());
    }
    #[cfg(not(feature = "no_float"))]
    if let Ok(number) = value.as_float() {
        out.push(constant::FLOAT);
        out.extend_from_slice(&number.to_le_bytes());
        return Ok(());
    }
    if let Ok(character) = value.as_char() {
        out.push(constant::CHAR);
        put_uvarint(out, u32::from(character).into());
        return Ok(());
    }
    if value.is_string() {
        let text = value
            .read_lock::<rhai::ImmutableString>()
            .ok_or_else(|| value.type_name().to_string())?;
        out.push(constant::STRING);
        put_str(out, text.as_str());
        return Ok(());
    }
    if value.is_array() {
        let array = value
            .read_lock::<Array>()
            .ok_or_else(|| value.type_name().to_string())?;
        out.push(constant::ARRAY);
        put_uvarint(out, array.len() as u64);
        for item in array.iter() {
            put_constant(out, item)?;
        }
        return Ok(());
    }
    if value.is_map() {
        let map = value
            .read_lock::<Map>()
            .ok_or_else(|| value.type_name().to_string())?;
        out.push(constant::MAP);
        put_uvarint(out, map.len() as u64);
        for (key, item) in map.iter() {
            put_str(out, key.as_str());
            put_constant(out, item)?;
        }
        return Ok(());
    }
    if value.is_blob() {
        let blob = value
            .read_lock::<Blob>()
            .ok_or_else(|| value.type_name().to_string())?;
        out.push(constant::BLOB);
        put_uvarint(out, blob.len() as u64);
        out.extend_from_slice(&blob);
        return Ok(());
    }

    // Behind the array and map checks because those are far more common, and
    // after everything cheap because reaching one means two failed downcasts.
    if let Some(range) = value.read_lock::<Range<INT>>() {
        out.push(constant::RANGE);
        put_range(out, range.start, range.end);
        return Ok(());
    }
    if let Some(range) = value.read_lock::<RangeInclusive<INT>>() {
        out.push(constant::RANGE_INCLUSIVE);
        put_range(out, *range.start(), *range.end());
        return Ok(());
    }

    Err(value.type_name().to_string())
}

/// `INT` widens to `i64` on the wire; the fingerprint is what stops a reader
/// narrowing it back to something else.
fn put_range(out: &mut Vec<u8>, start: INT, end: INT) {
    #[allow(clippy::useless_conversion)]
    put_ivarint(out, i64::from(start));
    #[allow(clippy::useless_conversion)]
    put_ivarint(out, i64::from(end));
}
