#![cfg(not(feature = "no_object"))]

use crate::engine::OP_EQUALS;
use crate::plugin::*;
use crate::{def_package, Dynamic, ImmutableString, Map, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_index"))]
use crate::Array;

def_package!(crate:BasicMapPackage:"Basic object map utilities.", lib, {
    lib.standard = true;

    combine_with_exported_module!(lib, "map", map_functions);
});

#[export_module]
mod map_functions {
    #[rhai_fn(name = "has", pure)]
    pub fn contains(map: &mut Map, prop: ImmutableString) -> bool {
        if map.is_empty() {
            false
        } else {
            map.contains_key(prop.as_str())
        }
    }
    #[rhai_fn(pure)]
    pub fn len(map: &mut Map) -> INT {
        map.len() as INT
    }
    pub fn clear(map: &mut Map) {
        if !map.is_empty() {
            map.clear();
        }
    }
    pub fn remove(map: &mut Map, name: ImmutableString) -> Dynamic {
        if !map.is_empty() {
            map.remove(name.as_str()).unwrap_or_else(|| Dynamic::UNIT)
        } else {
            Dynamic::UNIT
        }
    }
    #[rhai_fn(name = "mixin", name = "+=")]
    pub fn mixin(map: &mut Map, map2: Map) {
        if !map2.is_empty() {
            map.extend(map2.into_iter());
        }
    }
    #[rhai_fn(name = "+")]
    pub fn merge(map1: Map, map2: Map) -> Map {
        if map2.is_empty() {
            map1
        } else if map1.is_empty() {
            map2
        } else {
            let mut map1 = map1;
            map1.extend(map2.into_iter());
            map1
        }
    }
    pub fn fill_with(map: &mut Map, map2: Map) {
        if !map2.is_empty() {
            if map.is_empty() {
                *map = map2;
            } else {
                map2.into_iter().for_each(|(key, value)| {
                    map.entry(key).or_insert(value);
                });
            }
        }
    }
    #[rhai_fn(name = "==", return_raw, pure)]
    pub fn equals(
        ctx: NativeCallContext,
        map1: &mut Map,
        map2: Map,
    ) -> Result<bool, Box<EvalAltResult>> {
        if map1.len() != map2.len() {
            return Ok(false);
        }

        if !map1.is_empty() {
            let mut map2 = map2;

            for (m1, v1) in map1.iter_mut() {
                if let Some(v2) = map2.get_mut(m1) {
                    let equals = ctx
                        .call_fn_dynamic_raw(OP_EQUALS, true, &mut [v1, v2])
                        .map(|v| v.as_bool().unwrap_or(false))?;

                    if !equals {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
    #[rhai_fn(name = "!=", return_raw, pure)]
    pub fn not_equals(
        ctx: NativeCallContext,
        map1: &mut Map,
        map2: Map,
    ) -> Result<bool, Box<EvalAltResult>> {
        equals(ctx, map1, map2).map(|r| !r)
    }

    #[cfg(not(feature = "no_index"))]
    pub mod indexing {
        #[rhai_fn(pure)]
        pub fn keys(map: &mut Map) -> Array {
            if map.is_empty() {
                Array::new()
            } else {
                map.keys().cloned().map(Into::<Dynamic>::into).collect()
            }
        }
        #[rhai_fn(pure)]
        pub fn values(map: &mut Map) -> Array {
            if map.is_empty() {
                Array::new()
            } else {
                map.values().cloned().collect()
            }
        }
    }
}
