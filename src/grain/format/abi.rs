//! What a `Dynamic` is, on the machine that wrote the artifact.
//!
//! Rhai's feature flags change the value representation rather than just what
//! is available: `f32_float` makes `FLOAT` an `f32`, `only_i32` narrows `INT`,
//! `sync` swaps `Rc` for `Arc`. Loading an artifact across one of those is not
//! a missing feature, it is a value decoded as the wrong type — so the header
//! carries a fingerprint and the loader refuses a mismatch by name.
//!
//! ## What the fingerprint can and cannot see
//!
//! Widths are *measured*, so they are right no matter how rhai was configured.
//! The booleans are read from this crate's own features, which is why the
//! manifest mirrors them — `cfg!(feature = "no_object")` here does not consult
//! rhai's manifest.
//!
//! That leaves one gap: enabling a restriction on rhai directly, bypassing the
//! mirror. The cross-checks below close it wherever rust can prove the
//! disagreement, turning it into a compile error rather than a wrong
//! fingerprint. They cannot close it everywhere, which is what the mirror is
//! documented for.

/// Restrictions that are not visible in a width.
///
/// Order is the wire order and must never change; append only. A flag's name
/// is what the loader reports, so it has to match rhai's own spelling.
const FLAGS: &[(&str, bool)] = &[
    ("sync", cfg!(feature = "sync")),
    ("decimal", cfg!(feature = "decimal")),
    ("no_index", cfg!(feature = "no_index")),
    ("no_object", cfg!(feature = "no_object")),
    ("no_closure", cfg!(feature = "no_closure")),
    ("no_function", cfg!(feature = "no_function")),
    ("no_module", cfg!(feature = "no_module")),
    ("no_position", cfg!(feature = "no_position")),
    ("no_custom_syntax", cfg!(feature = "no_custom_syntax")),
    ("no_time", cfg!(feature = "no_time")),
    ("unchecked", cfg!(feature = "unchecked")),
];

/// `Engine` is only `Send + Sync` when rhai is built with `sync`, so claiming
/// the flag without rhai agreeing fails to compile.
#[cfg(feature = "sync")]
const _: () = {
    const fn assert_sync<T: Send + Sync>() {}
    let _ = assert_sync::<rhai::Engine>;
};

/// The value representation an artifact was written against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abi {
    /// `size_of::<rhai::INT>()`. Measured, so `only_i32` set on rhai alone is
    /// still caught.
    pub int_bytes: u8,
    /// `size_of::<rhai::FLOAT>()`, or 0 under `no_float`.
    pub float_bytes: u8,
    /// [`FLAGS`] as a bitmask, low bit first.
    pub flags: u32,
}

/// How two fingerprints differ.
///
/// Naming the difference is the whole point: "artifact was built for a
/// different rhai" is not something a user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiMismatch {
    /// A width differs, which means integers or floats would decode wrong.
    Width {
        what: &'static str,
        artifact: u8,
        host: u8,
    },
    /// A restriction differs. `artifact` is whether the writer had it on.
    Flag {
        flag: &'static str,
        artifact: bool,
    },
}

impl core::fmt::Display for AbiMismatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Width {
                what,
                artifact,
                host,
            } => write!(
                f,
                "artifact was written with a {artifact}-byte {what}, but this build has {host}"
            ),
            Self::Flag { flag, artifact } => {
                let (writer, reader) = if *artifact { ("on", "off") } else { ("off", "on") };
                write!(
                    f,
                    "artifact was written with `{flag}` {writer}, but this build has it {reader}"
                )
            }
        }
    }
}

impl Abi {
    /// The fingerprint of the running build.
    #[must_use]
    pub fn host() -> Self {
        #[cfg(not(feature = "no_float"))]
        let float_bytes = core::mem::size_of::<rhai::FLOAT>() as u8;
        #[cfg(feature = "no_float")]
        let float_bytes = 0u8;

        let mut flags = 0u32;
        for (bit, (_, on)) in FLAGS.iter().enumerate() {
            if *on {
                flags |= 1 << bit;
            }
        }

        Self {
            int_bytes: core::mem::size_of::<rhai::INT>() as u8,
            float_bytes,
            flags,
        }
    }

    /// Why this fingerprint cannot be loaded by `host`, if it cannot.
    ///
    /// Widths first: they are measured rather than declared, so they are the
    /// claim least likely to be lying.
    #[must_use]
    pub fn incompatible_with(self, host: Self) -> Option<AbiMismatch> {
        if self.int_bytes != host.int_bytes {
            return Some(AbiMismatch::Width {
                what: "INT",
                artifact: self.int_bytes,
                host: host.int_bytes,
            });
        }
        if self.float_bytes != host.float_bytes {
            return Some(AbiMismatch::Width {
                what: "FLOAT",
                artifact: self.float_bytes,
                host: host.float_bytes,
            });
        }

        let differing = self.flags ^ host.flags;
        if differing != 0 {
            let bit = differing.trailing_zeros() as usize;
            // A bit past the table means the writer knew a flag this build does
            // not. Reporting it as unknown beats indexing out of bounds.
            let flag = FLAGS.get(bit).map_or("an unknown restriction", |(name, _)| *name);
            return Some(AbiMismatch::Flag {
                flag,
                artifact: self.flags & (1 << bit) != 0,
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
        assert_eq!(Abi::host().incompatible_with(Abi::host()), None);
    }

    #[test]
    fn the_build_measures_the_widths_it_should() {
        let abi = Abi::host();
        // Against the aliases rather than concrete types, so this holds under
        // only_i32 and f32_float as well — the point is that the fingerprint
        // reports the build it was taken on, whichever build that is.
        assert_eq!(
            abi.int_bytes as usize,
            core::mem::size_of::<rhai::INT>(),
        );
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
        let narrow = Abi {
            int_bytes: 4,
            ..Abi::host()
        };
        assert_eq!(
            narrow.incompatible_with(Abi::host()),
            Some(AbiMismatch::Width {
                what: "INT",
                artifact: 4,
                host: 8,
            }),
        );
    }

    /// The message has to name the flag; that is the difference between an
    /// error a user can act on and one they cannot.
    #[test]
    fn a_differing_restriction_is_refused_by_name() {
        let host = Abi::host();
        let restricted = Abi {
            flags: host.flags ^ (1 << 3),
            ..host
        };

        let Some(mismatch @ AbiMismatch::Flag { flag, .. }) = restricted.incompatible_with(host)
        else {
            panic!("a differing flag must be refused");
        };
        assert_eq!(flag, "no_object");
        assert!(mismatch.to_string().contains("no_object"));
    }

    #[test]
    fn a_flag_this_build_has_never_heard_of_does_not_panic() {
        let host = Abi::host();
        let future = Abi {
            flags: host.flags ^ (1 << 31),
            ..host
        };
        assert!(matches!(
            future.incompatible_with(host),
            Some(AbiMismatch::Flag { .. }),
        ));
    }
}
