use core::ops::{Range, RangeInclusive};

use rhai::{Array, Blob, Dynamic, Map, INT};

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
    // `is_float` follows rhai's own `no_float`; the feature matrix for the
    // device build is M6's problem, not this function's.
    if value.is_unit()
        || value.is_bool()
        || value.is_int()
        || value.is_char()
        || value.is_string()
        || value.is_float()
    {
        return true;
    }

    if value.is_array() {
        return value
            .read_lock::<Array>()
            .is_some_and(|array| array.iter().all(is_poolable));
    }

    if value.is_map() {
        return value
            .read_lock::<Map>()
            .is_some_and(|map| map.values().all(is_poolable));
    }

    if value.is_blob() {
        return value.read_lock::<Blob>().is_some();
    }

    // A range is a host type by representation but not by nature: rhai builds
    // one for `0..5` and indexes strings and arrays with it, and its `TypeId`
    // is one both sides can name. Without this every slice is a fragment.
    if value.is::<Range<INT>>() || value.is::<RangeInclusive<INT>>() {
        return true;
    }

    // Anything else — FnPtr, TimeStamp, Decimal, a host type, a shared cell.
    false
}
