//! Module defining the operations meter used by [`Engine::on_operation_deadline`][crate::Engine::on_operation_deadline].
#![cfg(not(feature = "unchecked"))]
#![cfg(any(not(feature = "sync"), target_has_atomic = "64"))]

#[cfg(not(feature = "sync"))]
type Counter = std::cell::Cell<u64>;
#[cfg(feature = "sync")]
type Counter = std::sync::atomic::AtomicU64;

#[inline(always)]
fn load(counter: &Counter) -> u64 {
    #[cfg(not(feature = "sync"))]
    return counter.get();
    #[cfg(feature = "sync")]
    return counter.load(std::sync::atomic::Ordering::Relaxed);
}

#[inline(always)]
fn store(counter: &Counter, value: u64) {
    #[cfg(not(feature = "sync"))]
    counter.set(value);
    #[cfg(feature = "sync")]
    counter.store(value, std::sync::atomic::Ordering::Relaxed);
}

/// The number of operations of an evaluation, and the deadline at which the
/// [`Engine`][crate::Engine] calls the callback registered with
/// [`Engine::on_operation_deadline`][crate::Engine::on_operation_deadline].
///
/// The meter is shared between the [`Engine`][crate::Engine] and the host, which can read the
/// number of operations and move the deadline at any time, including from inside a native
/// function while a script runs.
///
/// A meter is meant to be used by one evaluation at a time: under `sync`, [`charge`][OperationMeter::charge]
/// from another thread while a script runs is not atomic.
///
/// Not available under `unchecked`, or under `sync` on targets without 64-bit atomics.
#[derive(Debug)]
pub struct OperationMeter {
    count: Counter,
    deadline: Counter,
}

impl Default for OperationMeter {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl OperationMeter {
    /// Create a new [`OperationMeter`] with no operations and a deadline that is never reached.
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: Counter::new(0),
            deadline: Counter::new(u64::MAX),
        }
    }
    /// Get the number of operations of the running evaluation, or of the last one.
    #[inline(always)]
    #[must_use]
    pub fn count(&self) -> u64 {
        load(&self.count)
    }
    /// Set the number of operations.
    #[inline(always)]
    pub(crate) fn set_count(&self, count: u64) {
        store(&self.count, count);
    }
    /// Get the number of operations at which the callback is called next.
    #[inline(always)]
    #[must_use]
    pub fn deadline(&self) -> u64 {
        load(&self.deadline)
    }
    /// Set the number of operations at which the callback is called next.
    ///
    /// A deadline at or below the current number of operations calls the callback on the next
    /// operation.
    #[inline(always)]
    pub fn set_deadline(&self, operations: u64) {
        store(&self.deadline, operations);
    }
    /// Count `operations` of the host's own work, as from a native function, against the
    /// deadline: the deadline comes that many operations nearer (but not below zero).
    /// [`count`][OperationMeter::count] stays the engine's own count.
    #[inline(always)]
    pub fn charge(&self, operations: u64) {
        self.set_deadline(self.deadline().saturating_sub(operations));
    }
}
