use rhai::ImmutableString;

/// Assigns a slot to every local, mirroring how rhai's own `Scope` grows.
///
/// Locals live in the caller's `Scope`, and rhai pushes an entry per
/// declaration and truncates back on block exit. So a slot is just the entry's
/// index, and a block boundary is a mark-and-truncate on this side too.
///
/// Shadowing needs no special handling: a second `let x` pushes a second entry,
/// and resolution scans backwards, so the newer slot wins while the older one
/// stays addressable by anything compiled before it — which is exactly rhai's
/// behaviour.
#[derive(Debug, Default)]
pub(crate) struct Slots {
    names: Vec<ImmutableString>,
}

impl Slots {
    /// Slot count, which is also the scope depth a block should unwind to.
    pub(crate) fn depth(&self) -> usize {
        self.names.len()
    }

    /// Declare a local and return its slot.
    pub(crate) fn declare(&mut self, name: ImmutableString) -> u16 {
        let slot = self.names.len();
        self.names.push(name);
        u16::try_from(slot).expect("slot count is bounded by the compiler's own limit")
    }

    /// Resolve a name to the most recent slot holding it.
    pub(crate) fn resolve(&self, name: &str) -> Option<u16> {
        self.names
            .iter()
            .rposition(|candidate| candidate.as_str() == name)
            .map(|slot| u16::try_from(slot).expect("slot count is bounded"))
    }

    /// Drop every local declared since `depth`.
    pub(crate) fn unwind_to(&mut self, depth: usize) {
        self.names.truncate(depth);
    }

    /// Whether another local would overflow the slot encoding.
    pub(crate) fn is_full(&self) -> bool {
        self.names.len() >= u16::MAX as usize
    }
}
