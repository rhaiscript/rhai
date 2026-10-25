//! Module defining the operations meter used by [`Engine::on_operation_deadline`][crate::Engine::on_operation_deadline].
#![cfg(not(feature = "unchecked"))]
#![cfg(any(not(feature = "sync"), target_has_atomic = "64"))]

#[cfg(not(feature = "sync"))]
type Counter = std::cell::Cell<u64>;
#[cfg(feature = "sync")]
type Counter = std::sync::atomic::AtomicU64;

/// The number of operations performed so far, and the deadline at which the [`Engine`][crate::Engine]
/// calls the callback registered with [`Engine::on_operation_deadline`][crate::Engine::on_operation_deadline].
///
/// The meter is shared between the [`Engine`][crate::Engine] and the host, which can read the
/// number of operations and move the deadline at any time, including from inside a native
/// function while a script runs.
///
/// A meter is meant to be used by one evaluation at a time: under `sync`, moving the deadline
/// with [`bring_deadline_nearer`][OperationMeter::bring_deadline_nearer] from another thread
/// while a script runs is not atomic.
///
/// Not available under `unchecked`.
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
        #[cfg(not(feature = "sync"))]
        return self.count.get();
        #[cfg(feature = "sync")]
        return self.count.load(std::sync::atomic::Ordering::Relaxed);
    }
    /// Set the number of operations.
    #[inline(always)]
    pub(crate) fn set_count(&self, count: u64) {
        #[cfg(not(feature = "sync"))]
        self.count.set(count);
        #[cfg(feature = "sync")]
        self.count
            .store(count, std::sync::atomic::Ordering::Relaxed);
    }
    /// Get the number of operations at which the callback is called next.
    #[inline(always)]
    #[must_use]
    pub fn deadline(&self) -> u64 {
        #[cfg(not(feature = "sync"))]
        return self.deadline.get();
        #[cfg(feature = "sync")]
        return self.deadline.load(std::sync::atomic::Ordering::Relaxed);
    }
    /// Set the number of operations at which the callback is called next.
    ///
    /// A deadline at or below the current number of operations calls the callback on the next
    /// operation.
    #[inline(always)]
    pub fn set_deadline(&self, operations: u64) {
        #[cfg(not(feature = "sync"))]
        self.deadline.set(operations);
        #[cfg(feature = "sync")]
        self.deadline
            .store(operations, std::sync::atomic::Ordering::Relaxed);
    }
    /// Bring the deadline `operations` nearer (but not below zero), as when a native function
    /// does work that it wants to count as operations.
    #[inline(always)]
    pub fn bring_deadline_nearer(&self, operations: u64) {
        self.set_deadline(self.deadline().saturating_sub(operations));
    }
}
