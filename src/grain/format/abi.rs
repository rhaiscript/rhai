//! Capabilities that a script requires to run.

use bitflags::bitflags;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

bitflags! {
    /// Capability flags.
    ///
    /// Order is the wire order and must never change; append only.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Caps: u32 {
        /// The script uses floating-point numbers, which are not available under `no_float`.
        const FLOAT = 1<<0;
        /// The script uses arrays, which are not available under `no_index`.
        const ARRAY = 1<<1;
        /// The script uses BLOB's, which are not available under `no_index`.
        const BLOB = 1<<2;
        /// The script uses object maps, which are not available under `no_object`.
        const MAP = 1<<3;
        /// The script uses decimal numbers, which are only available under `decimal`.
        const DECIMAL = 1<<4;
        /// The script defines functions, which are not available under `no_function`.
        const FUNCTION = 1<<5;
        /// The script uses function pointers.
        const FN_PTR = 1<<6;
        /// The script uses currying on function pointers.
        const CURRYING = 1<<7;
        /// The script employs indexing, which is not available under `no_index`.
        const INDEXING = 1<<8;
        /// The script accesses properties, which are not available under `no_object`.
        const PROPERTY = 1<<9;
        /// The script uses method calling style, which is not available under `no_object`.
        const METHOD = 1<<10;
        /// The script uses `this`, which is not available under `no_function`.
        const THIS = 1<<11;
        /// The script uses shared values, which is not available under `no_closure`.
        const SHARING = 1<<12;
        /// The script uses the `import` statement to import modules, which is not available under `no_module`.
        const IMPORT = 1<<13;
        /// The script uses the `export` statement to export in modules, which is not available under `no_module`.
        const EXPORT = 1<<14;
        /// The script uses the custom syntax, which is not available under `no_custom_syntax`.
        const CUSTOM_SYNTAX = 1<<15;
    }
}

impl std::fmt::Display for Caps {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let description = CAP_FLAGS
            .iter()
            .filter(|(cap, _, _)| self.contains(*cap))
            .map(|(_, name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ");

        f.write_str(&description)?;

        // A bit not in the table means the writer knows a capability this build does not.
        // Reporting it as unknown.
        if !(*self - Self::all()).is_empty() {
            if !description.is_empty() {
                f.write_str(", ")?;
            }
            f.write_str("requires an unknown capability")?;
        }

        if description.is_empty() {
            f.write_str("requires nothing")?;
        }

        Ok(())
    }
}

/// A table of all capabilities, their human-readable names,
/// and whether this build has them.
#[rustfmt::skip]
const CAP_FLAGS: &[(Caps, &'static str, bool)] = &[
    (Caps::FLOAT,       "uses floating-point numbers",      !cfg!(feature = "no_float")),
    (Caps::ARRAY,       "uses arrays",                      !cfg!(feature = "no_index")),
    (Caps::BLOB,        "uses BLOB's",                      !cfg!(feature = "no_index")),
    (Caps::MAP,         "uses object maps",                 !cfg!(feature = "no_object")),
    (Caps::DECIMAL,     "uses decimal numbers",              cfg!(feature = "decimal")),
    (Caps::FUNCTION,    "defines functions",                !cfg!(feature = "no_function")),
    (Caps::FN_PTR,      "uses function pointers",            true),
    (Caps::CURRYING,    "uses currying",                     true),
    (Caps::INDEXING,    "uses indexing",                    !cfg!(feature = "no_index")),
    (Caps::PROPERTY,    "accesses properties",              !cfg!(feature = "no_object")),
    (Caps::METHOD,      "uses method calling style",        !cfg!(feature = "no_object")),
    (Caps::THIS,        "uses `this`",                      !cfg!(feature = "no_function")),
    (Caps::SHARING,     "uses shared values",               !cfg!(feature = "no_closure")),
    (Caps::IMPORT,      "imports modules",                  !cfg!(feature = "no_module")),
    (Caps::EXPORT,      "exports data in modules",          !cfg!(feature = "no_module")),
    // Unsupported features below
    (Caps::CUSTOM_SYNTAX, "uses custom syntax",              false && !cfg!(feature = "no_custom_syntax")),
];

/// The value representation an artifact was written against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abi {
    /// `size_of::<rhai::INT>()`. Measured, so `only_i32` set on Rhai alone is
    /// still caught.
    pub int_bytes: u8,
    /// `size_of::<rhai::FLOAT>()`, or 0 under `no_float`.
    pub float_bytes: u8,
    /// current build's feature flags.
    pub caps: Caps,
}

/// How two fingerprints differ.
///
/// Naming the difference is the whole point: "artifact was built for a
/// different rhai" is not something a user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiMismatch {
    /// A width differs, which means integers or floats would decode wrong.
    DataWidth {
        /// Which width differs
        what: &'static str,
        /// What the writer used
        artifact: u8,
        /// What this build uses
        host: u8,
    },
    /// Missing capabilities, which means the artifact uses features this build does not have.
    MissingCaps {
        /// Which capabilities are missing.
        caps: String,
    },
}

impl core::fmt::Display for AbiMismatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DataWidth {
                what,
                artifact,
                host,
            } => write!(
                f,
                "artifact cannot load because it was written with a {artifact}-byte {what}, but this build has {host}"
            ),
            Self::MissingCaps {
                caps: capability,
            } => {
                write!(
                    f,
                    "artifact cannot load because {capability}, but this build does not have the necessary features enabled"
                )
            }
        }
    }
}

impl Abi {
    /// The fingerprint of the running build.
    #[must_use]
    pub fn host() -> Self {
        let mut caps = Caps::empty();

        CAP_FLAGS
            .iter()
            .for_each(|(flag, _, on)| caps.set(*flag, *on));

        Self {
            int_bytes: core::mem::size_of::<rhai::INT>() as u8,
            #[cfg(not(feature = "no_float"))]
            float_bytes: core::mem::size_of::<rhai::FLOAT>() as u8,
            #[cfg(feature = "no_float")]
            float_bytes: 0,
            caps,
        }
    }

    /// Why this fingerprint cannot be loaded by `host`, if it cannot.
    ///
    /// Widths first: they are measured rather than declared, so they are the
    /// claim least likely to be lying.
    #[must_use]
    pub fn is_incompatible_with(self, host: Self) -> Option<AbiMismatch> {
        if self.int_bytes != host.int_bytes {
            return Some(AbiMismatch::DataWidth {
                what: "integer",
                artifact: self.int_bytes,
                host: host.int_bytes,
            });
        }
        if self.float_bytes != host.float_bytes {
            return Some(AbiMismatch::DataWidth {
                what: "floating-point number",
                artifact: self.float_bytes,
                host: host.float_bytes,
            });
        }

        let missing = self.caps - host.caps;
        if !missing.is_empty() {
            return Some(AbiMismatch::MissingCaps {
                caps: missing.to_string(),
            });
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_can_load_its_own_artifacts() {
        assert_eq!(Abi::host().is_incompatible_with(Abi::host()), None);
    }

    #[test]
    fn the_build_measures_the_widths_it_should() {
        let abi = Abi::host();
        // Against the aliases rather than concrete types, so this holds under
        // only_i32 and f32_float as well — the point is that the fingerprint
        // reports the build it was taken on, whichever build that is.
        assert_eq!(abi.int_bytes as usize, core::mem::size_of::<rhai::INT>(),);
        #[cfg(not(feature = "no_float"))]
        assert_eq!(
            abi.float_bytes as usize,
            core::mem::size_of::<rhai::FLOAT>(),
        );
        #[cfg(feature = "no_float")]
        assert_eq!(abi.float_bytes, 0);
    }

    #[test]
    fn a_narrower_int_is_refused_by_name() {
        let host = Abi::host();
        // Halved rather than named: `only_i32` makes 4 the host's own width,
        // and an artifact agreeing with the host is not a mismatch to report.
        let narrow = Abi {
            int_bytes: host.int_bytes / 2,
            ..host
        };
        assert_eq!(
            narrow.is_incompatible_with(host),
            Some(AbiMismatch::DataWidth {
                what: "integer",
                artifact: host.int_bytes / 2,
                host: host.int_bytes,
            }),
        );
    }

    /// The message has to name the flag; that is the difference between an
    /// error a user can act on and one they cannot.
    #[test]
    #[cfg(not(feature = "no_object"))]
    fn a_differing_restriction_is_refused_by_name() {
        let cap = Caps::MAP;
        let desc = CAP_FLAGS
            .iter()
            .find(|(flag, _, _)| *flag == cap)
            .expect("the test is broken")
            .1;

        // Let's say we need all caps on the host.
        let needed = Abi::host();

        // But the host has one less than we need, so it cannot load the artifact.
        let host = Abi {
            caps: needed.caps - cap,
            ..needed
        };

        let Some(AbiMismatch::MissingCaps { caps: missing, .. }) =
            needed.is_incompatible_with(host)
        else {
            panic!("a missing cap must be refused");
        };
        assert_eq!(missing, desc);
    }

    #[test]
    fn a_flag_this_build_has_never_heard_of_does_not_panic() {
        let host = Abi::host();
        let future = Abi {
            caps: host.caps ^ Caps::from_bits_retain(1_u32 << 31),
            ..host
        };
        assert!(matches!(
            future.is_incompatible_with(host),
            Some(AbiMismatch::MissingCaps { .. }),
        ));
    }
}
