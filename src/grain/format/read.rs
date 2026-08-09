use crate::{tokenizer::Token, Dynamic, ImmutableString};
use core::convert::{TryFrom, TryInto};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::grain::bytecode::{
    AssignOp, BadTable, Chain, Chunk, Positions, Root, Step, Strings, Switch, SwitchCase,
    SwitchRange, TableError, Tail, VerifyError,
};
use crate::grain::format::abi::{Abi, AbiMismatch};
use crate::grain::format::{constant, root_tag, step_tag, tail_tag, Cursor, MAGIC, VERSION};
use crate::grain::program::{Function, Parts, Program};

/// How deeply a constant may nest.
///
/// Decoding an array or a map recurses, so an artifact claiming a few thousand
/// nested arrays would overflow the stack of whatever loads it. Rhai's own
/// parser caps expression depth for the same reason; this is the loader's
/// version, and it is well past anything a literal in real source reaches.
const MAX_CONSTANT_DEPTH: usize = 64;

/// Why an artifact could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Not a rhaigrain artifact at all.
    BadMagic,
    /// Written by a format this build does not know how to read.
    UnsupportedVersion {
        /// The version the artifact claims
        found: u16,
        /// The version this build reads
        supported: u16,
    },
    /// Written against a different value representation. Loading anyway would
    /// decode integers or floats as the wrong type.
    Abi(AbiMismatch),
    /// The input ended mid-value.
    Truncated,
    /// A varint that never terminates, or one too wide for its field.
    MalformedVarint,
    /// A string that is not UTF-8.
    BadUtf8,
    /// A tag this build has no meaning for.
    UnknownTag {
        /// Which section it was read from
        section: &'static str,
        /// The tag itself
        tag: u8,
    },
    /// An operator syntax rhai does not recognise.
    UnknownToken {
        /// The syntax that was read
        syntax: String,
    },
    /// The artifact's `switch` case hashes were computed by a differently
    /// seeded hasher, so none of them would ever match.
    HashSeedMismatch {
        /// The seed the writer used
        artifact: u64,
        /// The seed this build uses
        host: u64,
    },
    /// Constants nested past `MAX_CONSTANT_DEPTH`.
    ConstantTooDeep,
    /// Bytes left over after the last section, so the file is not what it
    /// claims to be even though every field parsed.
    TrailingBytes {
        /// How many bytes are left over
        count: usize,
    },
    /// The chunk parsed but does not agree with itself.
    Unverifiable(VerifyError),
    /// The position table is malformed, or belongs to a different program.
    Positions(TableError),
    /// The name table's spans do not fit its blob.
    Names(BadTable),
}

impl core::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadMagic => f.write_str("not a rhaigrain artifact"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "artifact is format version {found}, and this build reads {supported}"
            ),
            Self::Abi(mismatch) => write!(f, "{mismatch}"),
            Self::Truncated => f.write_str("artifact ends mid-value"),
            Self::MalformedVarint => f.write_str("malformed varint"),
            Self::BadUtf8 => f.write_str("a string is not valid UTF-8"),
            Self::UnknownTag { section, tag } => {
                write!(f, "unknown {section} tag {tag:#04x}")
            }
            Self::UnknownToken { syntax } => write!(f, "`{syntax}` is not an operator"),
            Self::HashSeedMismatch { artifact, host } => write!(
                f,
                "this artifact's `switch` cases were hashed with a different seed \
                 ({artifact:#018x} against {host:#018x}), so none of them could match — \
                 call `rhai::config::hashing::set_hashing_seed` with the same seed \
                 wherever this was compiled and wherever it is loaded"
            ),
            Self::ConstantTooDeep => write!(
                f,
                "a constant nests deeper than {MAX_CONSTANT_DEPTH} levels"
            ),
            Self::TrailingBytes { count } => {
                write!(f, "{count} byte(s) follow the last section")
            }
            Self::Unverifiable(err) => write!(f, "chunk failed verification: {err:?}"),
            Self::Positions(err) => write!(f, "{err}"),
            Self::Names(err) => write!(f, "name table is malformed: {err:?}"),
        }
    }
}

pub(super) fn read(bytes: &[u8]) -> Result<Program<'_>, ReadError> {
    let mut cursor = Cursor::new(bytes);

    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(ReadError::BadMagic);
    }

    let version = u16::from_le_bytes(cursor.take(2)?.try_into().expect("two bytes"));
    if version != VERSION {
        return Err(ReadError::UnsupportedVersion {
            found: version,
            supported: VERSION,
        });
    }

    // Before anything is decoded: past here every value is read as a type the
    // fingerprint just promised.
    let abi = Abi {
        int_bytes: cursor.byte()?,
        float_bytes: cursor.byte()?,
        flags: u32::from_le_bytes(cursor.take(4)?.try_into().expect("four bytes")),
    };
    if let Some(mismatch) = abi.incompatible_with(Abi::host()) {
        return Err(ReadError::Abi(mismatch));
    }

    let source = cursor.str()?;
    let source = (!source.is_empty()).then(|| ImmutableString::from(source));

    // Borrowed: the spans are read, the blob is sliced, and nothing per-name
    // is allocated.
    let count = cursor.count()?;
    let mut starts = Vec::with_capacity(count + 1);
    starts.push(0u32);
    for _ in 0..count {
        starts.push(cursor.index()?);
    }
    let blob_len = usize::try_from(cursor.uvarint()?).map_err(|_| ReadError::Truncated)?;
    let names = Strings::borrowed(cursor.take(blob_len)?, starts)?;

    let mut consts = Vec::new();
    for _ in 0..cursor.uvarint()? {
        consts.push(get_constant(&mut cursor, 0)?);
    }

    let mut tokens = Vec::new();
    for _ in 0..cursor.uvarint()? {
        tokens.push(get_token(&mut cursor)?);
    }

    let mut assign_ops = Vec::new();
    for _ in 0..cursor.uvarint()? {
        assign_ops.push(AssignOp {
            op_assign: get_token(&mut cursor)?,
            op_assign_name: cursor.index()?,
            op: get_token(&mut cursor)?,
            op_name: cursor.index()?,
        });
    }

    let mut chains = Vec::new();
    for _ in 0..cursor.uvarint()? {
        chains.push(get_chain(&mut cursor)?);
    }

    let switches = get_switches(&mut cursor)?;

    let main = get_chunk(&mut cursor)?;

    let mut functions = Vec::new();
    for _ in 0..cursor.uvarint()? {
        let name = cursor.index()?;
        // Zero is "untyped"; anything else is an index one higher.
        let this_type = match cursor.uvarint()? {
            0 => None,
            raw => Some(u32::try_from(raw - 1).map_err(|_| ReadError::Truncated)?),
        };
        let mut params = Vec::new();
        for _ in 0..cursor.uvarint()? {
            params.push(cursor.index()?);
        }
        functions.push(Function {
            name,
            this_type,
            params,
            // Not encoded: derived from the chunk by `Program::new`, so a loaded
            // program and a compiled one cannot disagree about it.
            takes_this: false,
            chunk: get_chunk(&mut cursor)?,
        });
    }

    // Borrowed, not copied. The VM dispatches on these bytes where they lie.
    let code_len = usize::try_from(cursor.uvarint()?).map_err(|_| ReadError::Truncated)?;
    let code = cursor.take(code_len)?;

    // Absent means stripped, which is the normal shape for something that
    // reached a device. `from_table` refuses a table belonging to another
    // program, so a mismatched pair fails here rather than misreporting later.
    let table_len = usize::try_from(cursor.uvarint()?).map_err(|_| ReadError::Truncated)?;
    let positions = if table_len == 0 {
        Positions::Stripped
    } else {
        Positions::from_table(cursor.take(table_len)?, code_len)?
    };

    if !cursor.at_end() {
        return Err(ReadError::TrailingBytes {
            count: bytes.len() - cursor.pos,
        });
    }

    let program = Program::new(
        code.into(),
        main,
        functions,
        Parts {
            positions,
            residuals: Vec::new(),
            consts,
            names,
            tokens,
            assign_ops,
            chains,
            switches,
            // Script functions are still ASTs, so `write` refuses a program
            // that has any and a loaded one never does.
            lib: None,
            #[cfg(not(feature = "no_module"))]
            resolver: None,
            source,
        },
    );

    // An artifact is untrusted input, so the chunk is checked before it is
    // handed to a `Vm`. Nothing that fails here is constructible by the
    // compiler — and because the VM executes these bytes in place, this is what
    // stands between a corrupt file and an operand read as an opcode.
    program.verify()?;

    Ok(program)
}

/// A chain step's own position. Line zero means none.
fn get_position(cursor: &mut Cursor) -> Result<rhai::Position, ReadError> {
    let line = cursor.small()?;
    let column = cursor.small()?;
    Ok(if line == 0 {
        rhai::Position::NONE
    } else {
        rhai::Position::new(line, column)
    })
}

fn get_chain(cursor: &mut Cursor) -> Result<Chain, ReadError> {
    let root = match cursor.byte()? {
        root_tag::LOCAL => Root::Local {
            slot: cursor.small()?,
            name: cursor.index()?,
        },
        root_tag::NAMED => Root::Named {
            name: cursor.index()?,
            pos: get_position(cursor)?,
        },
        root_tag::THIS => Root::This {
            pos: get_position(cursor)?,
        },
        root_tag::TEMPORARY => Root::Temporary,
        tag => {
            return Err(ReadError::UnknownTag {
                section: "chain root",
                tag,
            })
        }
    };
    let operands = cursor.small()?;

    let mut steps = Vec::new();
    for _ in 0..cursor.uvarint()? {
        steps.push(match cursor.byte()? {
            step_tag::INDEX => Step::Index {
                operand: cursor.small()?,
                pos: get_position(cursor)?,
                bracket: get_position(cursor)?,
            },
            step_tag::PROPERTY => Step::Property {
                name: cursor.index()?,
                getter: cursor.index()?,
                setter: cursor.index()?,
                pos: get_position(cursor)?,
            },
            step_tag::METHOD => Step::Method {
                name: cursor.index()?,
                argc: cursor.byte()?,
                operand: cursor.small()?,
                pos: get_position(cursor)?,
            },
            tag => {
                return Err(ReadError::UnknownTag {
                    section: "chain step",
                    tag,
                })
            }
        });
    }

    let tail = match cursor.byte()? {
        tail_tag::READ => Tail::Read,
        tail_tag::ASSIGN => Tail::Assign { op: None },
        tail_tag::ASSIGN_OP => Tail::Assign {
            op: Some(cursor.index()?),
        },
        tag => {
            return Err(ReadError::UnknownTag {
                section: "chain tail",
                tag,
            })
        }
    };

    Ok(Chain {
        root,
        steps,
        tail,
        operands,
    })
}

/// Read the switch tables, refusing them if their case hashes were made by a
/// hasher this process cannot reproduce.
///
/// The check is not belt and braces: without it a seed mismatch loads cleanly
/// and every `switch` silently takes its default, which is a wrong answer
/// rather than a failure. See `write::put_switches`.
fn get_switches(cursor: &mut Cursor) -> Result<Vec<Switch>, ReadError> {
    let count = cursor.uvarint()?;
    if count == 0 {
        return Ok(Vec::new());
    }

    let artifact = u64::from_le_bytes(cursor.take(8)?.try_into().expect("eight bytes"));
    let host = crate::grain::bytecode::probe();
    if artifact != host {
        return Err(ReadError::HashSeedMismatch { artifact, host });
    }

    let mut switches = Vec::new();
    for _ in 0..count {
        let mut cases = Vec::new();
        for _ in 0..cursor.uvarint()? {
            cases.push(SwitchCase {
                hash: u64::from_le_bytes(cursor.take(8)?.try_into().expect("eight bytes")),
                target: cursor.index()?,
            });
        }

        let mut ranges = Vec::new();
        for _ in 0..cursor.uvarint()? {
            ranges.push(SwitchRange {
                from: bounded_int(cursor.ivarint()?)?,
                to: bounded_int(cursor.ivarint()?)?,
                inclusive: cursor.byte()? != 0,
                target: cursor.index()?,
            });
        }

        switches.push(Switch {
            cases,
            ranges,
            default: cursor.index()?,
        });
    }
    Ok(switches)
}

/// Narrow a written bound back to this build's `INT`.
///
/// The ABI fingerprint has already promised the widths agree, so this can only
/// fail on a corrupt file — but a range bound is compared against a subject,
/// and a silently truncated one would match the wrong values.
fn bounded_int(value: i64) -> Result<rhai::INT, ReadError> {
    rhai::INT::try_from(value).map_err(|_| ReadError::MalformedVarint)
}

/// Read a chunk's span. The verifier is what checks it names real code.
fn get_chunk(cursor: &mut Cursor) -> Result<Chunk, ReadError> {
    let entry = cursor.index()?;
    let end = cursor.index()?;
    Ok(Chunk::new(entry, end, cursor.small()?))
}

fn get_token(cursor: &mut Cursor) -> Result<Token, ReadError> {
    let syntax = cursor.str()?;
    Token::lookup_symbol_from_syntax(syntax).ok_or_else(|| ReadError::UnknownToken {
        syntax: syntax.to_string(),
    })
}

fn get_constant(cursor: &mut Cursor, depth: usize) -> Result<Dynamic, ReadError> {
    if depth > MAX_CONSTANT_DEPTH {
        return Err(ReadError::ConstantTooDeep);
    }

    Ok(match cursor.byte()? {
        constant::UNIT => Dynamic::UNIT,
        constant::FALSE => Dynamic::from(false),
        constant::TRUE => Dynamic::from(true),

        constant::INT => {
            let value = cursor.ivarint()?;
            Dynamic::from(rhai::INT::try_from(value).map_err(|_| ReadError::MalformedVarint)?)
        }

        #[cfg(not(feature = "no_float"))]
        constant::FLOAT => {
            let width = core::mem::size_of::<rhai::FLOAT>();
            let bits = cursor.take(width)?;
            Dynamic::from(rhai::FLOAT::from_le_bytes(
                bits.try_into().expect("width matches the fingerprint"),
            ))
        }

        constant::CHAR => {
            let code = cursor.index()?;
            Dynamic::from(char::from_u32(code).ok_or(ReadError::MalformedVarint)?)
        }

        constant::STRING => Dynamic::from(ImmutableString::from(cursor.str()?)),

        // A build with no type for one cannot decode it into anything; the tag
        // falls through to the unknown-tag arm, which is the truthful answer.
        #[cfg(not(feature = "no_index"))]
        constant::ARRAY => {
            // The declared length is untrusted, so nothing is reserved from it;
            // a short file runs out of bytes instead of out of memory.
            let count = cursor.uvarint()?;
            let mut array = rhai::Array::new();
            for _ in 0..count {
                array.push(get_constant(cursor, depth + 1)?);
            }
            Dynamic::from(array)
        }

        #[cfg(not(feature = "no_object"))]
        constant::MAP => {
            let count = cursor.uvarint()?;
            let mut map = rhai::Map::new();
            for _ in 0..count {
                let key = cursor.str()?.into();
                map.insert(key, get_constant(cursor, depth + 1)?);
            }
            Dynamic::from(map)
        }

        constant::RANGE => {
            let start = bounded_int(cursor.ivarint()?)?;
            Dynamic::from(start..bounded_int(cursor.ivarint()?)?)
        }

        constant::RANGE_INCLUSIVE => {
            let start = bounded_int(cursor.ivarint()?)?;
            Dynamic::from(start..=bounded_int(cursor.ivarint()?)?)
        }

        #[cfg(not(feature = "no_index"))]
        constant::BLOB => {
            let len = usize::try_from(cursor.uvarint()?).map_err(|_| ReadError::Truncated)?;
            Dynamic::from(cursor.take(len)?.to_vec())
        }

        tag => {
            return Err(ReadError::UnknownTag {
                section: "constant",
                tag,
            })
        }
    })
}
