use core::ops::{Range, RangeInclusive};

#[cfg(not(feature = "no_object"))]
use rhai::Map;
#[cfg(not(feature = "no_index"))]
use rhai::{Array, Blob};
use rhai::{Dynamic, INT};
#[cfg(feature = "decimal")]
use rust_decimal::Decimal;

/// Whether a constant can live in the artifact's constant pool.
///
/// Two constraints happen to coincide here, so one check enforces both.
///
/// The artifact must be loadable in another process, which rules out anything
/// carrying a host `TypeId`, a live `Rc`, or a clock reading: `Variant`,
/// `Shared`, `TimeStamp`.
///
/// And `FnPtr` is not a value the VM may simply clone into place, even in the
/// same process. Rhai attaches the calling environment when it reads a
/// function pointer out of a constant (`ast/expr.rs:471-482`), so a pointer
/// copied straight from the pool would be missing the module library it was
/// created against. Keeping it out of the pool leaves it as a fragment, which
/// evaluates through the path that does the attaching.
pub(crate) fn is_poolable(value: &Dynamic) -> bool {
    // A shared cell first, because every question below sees through one:
    // `is_array` and friends unwrap `Union::Shared`, so a shared array of ints
    // would answer yes and be pooled by cloning the `Rc` — leaving the pool
    // aliasing a cell the host can still write to. Nothing a `Program` owns may
    // alias mutable state: the pool is immutable after `Program::new`, and that
    // is what makes the reference graph among programs acyclic.
    #[cfg(not(feature = "no_closure"))]
    if value.is_shared() {
        return false;
    }

    // Under `no_float` Rhai has no float type and no `is_float` to ask, so
    // there is nothing here for the question to be about.
    #[cfg(not(feature = "no_float"))]
    if value.is_float() {
        return true;
    }

    if value.is_unit() || value.is_bool() || value.is_int() || value.is_char() || value.is_string()
    {
        return true;
    }

    // Arrays and blobs go with `no_index`, maps with `no_object` — the same
    // reason as the float above: no type, and no question to ask about one.
    #[cfg(not(feature = "no_index"))]
    if value.is_array() {
        return value
            .read_lock::<Array>()
            .map_or(false, |array| array.iter().all(is_poolable));
    }

    #[cfg(not(feature = "no_object"))]
    if value.is_map() {
        return value
            .read_lock::<Map>()
            .map_or(false, |map| map.values().all(is_poolable));
    }

    #[cfg(not(feature = "no_index"))]
    if value.is_blob() {
        return value.read_lock::<Blob>().is_some();
    }

    // Decimals are enabled by `decimal`.
    #[cfg(feature = "decimal")]
    if value.is_decimal() {
        return value.read_lock::<Decimal>().is_some();
    }

    // Disregard the embedded environment in the FnPtr because it most likely
    // will simply be the collection of all functions in the program.
    if value.is_fnptr() {
        return value.read_lock::<rhai::FnPtr>().is_some();
    }

    // A range is a host type by representation but not by nature: Rhai builds
    // one for `0..5` and indexes strings and arrays with it, and its `TypeId`
    // is one both sides can name. Without this every slice is a fragment.
    if value.is::<Range<INT>>() || value.is::<RangeInclusive<INT>>() {
        return true;
    }

    // Anything else — TimeStamp, a custom type, a shared cell.
    false
}
