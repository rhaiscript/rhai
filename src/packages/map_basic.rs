#![cfg(not(feature = "no_object"))]

use crate::any::Dynamic;
use crate::def_package;
use crate::engine::Map;
use crate::parser::{ImmutableString, INT};
use crate::plugin::*;

use crate::stdlib::vec::Vec;

def_package!(crate:BasicMapPackage:"Basic object map utilities.", lib, {
    lib.combine_flatten(exported_module!(map_functions));
});

#[export_module]
mod map_functions {
    #[inline(always)]
    pub fn has(map: &mut Map, prop: ImmutableString) -> bool {
        map.contains_key(&prop)
    }
    #[inline(always)]
    pub fn len(map: &mut Map) -> INT {
        map.len() as INT
    }
    #[rhai_fn(get = "len")]
    #[inline(always)]
    pub fn len_prop(map: &mut Map) -> INT {
        len(map)
    }
    #[inline(always)]
    pub fn clear(map: &mut Map) {
        map.clear();
    }
    #[inline]
    pub fn remove(x: &mut Map, name: ImmutableString) -> Dynamic {
        x.remove(&name).unwrap_or_else(|| ().into())
    }
    pub fn mixin(map1: &mut Map, map2: Map) {
        map2.into_iter().for_each(|(key, value)| {
            map1.insert(key, value);
        });
    }
    #[rhai_fn(name = "+=")]
    #[inline(always)]
    pub fn mixin_operator(map1: &mut Map, map2: Map) {
        mixin(map1, map2)
    }
    #[rhai_fn(name = "+")]
    pub fn merge(mut map1: Map, map2: Map) -> Map {
        map2.into_iter().for_each(|(key, value)| {
            map1.insert(key, value);
        });
        map1
    }
    pub fn fill_with(map1: &mut Map, map2: Map) {
        map2.into_iter().for_each(|(key, value)| {
            if !map1.contains_key(&key) {
                map1.insert(key, value);
            }
        });
    }

    #[cfg(not(feature = "no_index"))]
    pub mod indexing {
        pub fn keys(map: &mut Map) -> Vec<Dynamic> {
            map.iter().map(|(k, _)| k.clone().into()).collect()
        }
        pub fn values(map: &mut Map) -> Vec<Dynamic> {
            map.iter().map(|(_, v)| v.clone()).collect()
        }
    }
}
