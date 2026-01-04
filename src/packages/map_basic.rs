#![cfg(not(feature = "no_object"))]

use crate::engine::OP_EQUALS;
use crate::plugin::*;
use crate::{
    def_package, Dynamic, FnPtr, ImmutableString, Map, NativeCallContext, RhaiResultOf, INT,
};
use std::convert::TryFrom;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_index"))]
use crate::Array;

def_package! {
    /// Package of basic object map utilities.
    pub BasicMapPackage(lib) {
        lib.set_standard_lib(true);

        combine_with_exported_module!(lib, "map", map_functions);
    }
}

#[export_module]
mod map_functions {
    /// Return the number of properties in the object map.
    #[rhai_fn(pure)]
    pub fn len(map: &mut Map) -> INT {
        INT::try_from(map.len()).unwrap_or(INT::MAX)
    }
    /// Return true if the map is empty.
    #[rhai_fn(pure)]
    pub fn is_empty(map: &mut Map) -> bool {
        map.len() == 0
    }
    /// Returns `true` if the object map contains a specified property.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a: 1, b: 2, c: 3};
    ///
    /// print(m.contains("b"));     // prints true
    ///
    /// print(m.contains("x"));     // prints false
    /// ```
    #[rhai_fn(pure)]
    pub fn contains(map: &mut Map, property: &str) -> bool {
        map.contains_key(property)
    }
    /// Get the value of the `property` in the object map and return a copy.
    ///
    /// If `property` does not exist in the object map, `()` is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a: 1, b: 2, c: 3};
    ///
    /// print(m.get("b"));      // prints 2
    ///
    /// print(m.get("x"));      // prints empty (for '()')
    /// ```
    #[rhai_fn(pure)]
    pub fn get(map: &mut Map, property: &str) -> Dynamic {
        if map.is_empty() {
            return Dynamic::UNIT;
        }

        map.get(property).cloned().unwrap_or(Dynamic::UNIT)
    }
    /// Set the value of the `property` in the object map to a new `value`.
    ///
    /// If `property` does not exist in the object map, it is added.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a: 1, b: 2, c: 3};
    ///
    /// m.set("b", 42)'
    ///
    /// print(m);           // prints "#{a: 1, b: 42, c: 3}"
    ///
    /// x.set("x", 0);
    ///
    /// print(m);           // prints "#{a: 1, b: 42, c: 3, x: 0}"
    /// ```
    pub fn set(map: &mut Map, property: &str, value: Dynamic) {
        match map.get_mut(property) {
            Some(value_ref) => *value_ref = value,
            None => {
                map.insert(property.into(), value);
            }
        }
    }
    /// Clear the object map.
    pub fn clear(map: &mut Map) {
        if map.is_empty() {
            return;
        }

        map.clear();
    }
    /// Remove any property of the specified `name` from the object map, returning its value.
    ///
    /// If the property does not exist, `()` is returned.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a:1, b:2, c:3};
    ///
    /// let x = m.remove("b");
    ///
    /// print(x);       // prints 2
    ///
    /// print(m);       // prints "#{a:1, c:3}"
    /// ```
    pub fn remove(map: &mut Map, property: &str) -> Dynamic {
        if map.is_empty() {
            return Dynamic::UNIT;
        }

        map.remove(property).unwrap_or(Dynamic::UNIT)
    }
    /// Add all property values of another object map into the object map.
    /// Existing property values of the same names are replaced.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a:1, b:2, c:3};
    /// let n = #{a: 42, d:0};
    ///
    /// m.mixin(n);
    ///
    /// print(m);       // prints "#{a:42, b:2, c:3, d:0}"
    /// ```
    #[rhai_fn(name = "mixin", name = "+=")]
    pub fn mixin(map: &mut Map, map2: Map) {
        if map2.is_empty() {
            return;
        }

        map.extend(map2);
    }
    /// Make a copy of the object map, add all property values of another object map
    /// (existing property values of the same names are replaced), then returning it.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a:1, b:2, c:3};
    /// let n = #{a: 42, d:0};
    ///
    /// print(m + n);       // prints "#{a:42, b:2, c:3, d:0}"
    ///
    /// print(m);           // prints "#{a:1, b:2, c:3}"
    /// ```
    #[rhai_fn(name = "+")]
    pub fn merge(map1: Map, map2: Map) -> Map {
        if map2.is_empty() {
            return map1;
        }
        if map1.is_empty() {
            return map2;
        }

        let mut map1 = map1;
        map1.extend(map2);
        map1
    }
    /// Add all property values of another object map into the object map.
    /// Only properties that do not originally exist in the object map are added.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a:1, b:2, c:3};
    /// let n = #{a: 42, d:0};
    ///
    /// m.fill_with(n);
    ///
    /// print(m);       // prints "#{a:1, b:2, c:3, d:0}"
    /// ```
    pub fn fill_with(map: &mut Map, map2: Map) {
        if map2.is_empty() {
            return;
        }
        if map.is_empty() {
            *map = map2;
            return;
        }

        for (key, value) in map2 {
            map.entry(key).or_insert(value);
        }
    }
    /// Return `true` if two object maps are equal (i.e. all property values are equal).
    ///
    /// The operator `==` is used to compare property values and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m1 = #{a:1, b:2, c:3};
    /// let m2 = #{a:1, b:2, c:3};
    /// let m3 = #{a:1, c:3};
    ///
    /// print(m1 == m2);        // prints true
    ///
    /// print(m1 == m3);        // prints false
    /// ```
    #[rhai_fn(name = "==", return_raw, pure)]
    pub fn equals(ctx: NativeCallContext, map1: &mut Map, map2: Map) -> RhaiResultOf<bool> {
        if map1.len() != map2.len() {
            return Ok(false);
        }

        if !map1.is_empty() {
            let mut map2 = map2;

            for (m1, v1) in map1 {
                match map2.get_mut(m1) {
                    Some(v2) => {
                        let equals = ctx
                            .call_native_fn_raw(OP_EQUALS, true, &mut [v1, v2])?
                            .as_bool()
                            .unwrap_or(false);

                        if !equals {
                            return Ok(false);
                        }
                    }
                    _ => return Ok(false),
                }
            }
        }

        Ok(true)
    }
    /// Return `true` if two object maps are not equal (i.e. at least one property value is not equal).
    ///
    /// The operator `==` is used to compare property values and must be defined,
    /// otherwise `false` is assumed.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m1 = #{a:1, b:2, c:3};
    /// let m2 = #{a:1, b:2, c:3};
    /// let m3 = #{a:1, c:3};
    ///
    /// print(m1 != m2);        // prints false
    ///
    /// print(m1 != m3);        // prints true
    /// ```
    #[rhai_fn(name = "!=", return_raw, pure)]
    pub fn not_equals(ctx: NativeCallContext, map1: &mut Map, map2: Map) -> RhaiResultOf<bool> {
        equals(ctx, map1, map2).map(|r| !r)
    }

    /// Return an array with all the property names in the object map.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a:1, b:2, c:3};
    ///
    /// print(m.keys());        // prints ["a", "b", "c"]
    /// ```
    #[cfg(not(feature = "no_index"))]
    #[rhai_fn(pure)]
    pub fn keys(map: &mut Map) -> Array {
        if map.is_empty() {
            return Array::new();
        }

        map.keys().cloned().map(Into::into).collect()
    }
    /// Return an array with all the property values in the object map.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a:1, b:2, c:3};
    ///
    /// print(m.values());      // prints "[1, 2, 3]""
    /// ```
    #[cfg(not(feature = "no_index"))]
    #[rhai_fn(pure)]
    pub fn values(map: &mut Map) -> Array {
        if map.is_empty() {
            return Array::new();
        }

        map.values().cloned().collect()
    }
    /// Iterate through all the elements in the object map, applying a `mapper` function to each
    /// element in turn, and return the results as a new object map.
    ///
    /// # Function Parameters
    ///
    /// * `key`: current key
    /// * `value` _(optional)_: copy of element (bound to `this` if omitted)
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = #{a:1, b:2, c:3, d:4, e:5};
    ///
    /// let y = x.map(|k| this * 2);
    ///
    /// print(y);       // prints #{"a":2, "b":4, "c":6, "d":8, "e":10}
    ///
    /// let y = x.filter(|k, v| v + k.len());
    ///
    /// print(y);       // prints #{"a":2, "b":3, "c":5, "d":6, "e":7}
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn map(ctx: NativeCallContext, map: &mut Map, mapper: FnPtr) -> RhaiResultOf<Map> {
        if map.is_empty() {
            return Ok(Map::new());
        }

        let mut result = Map::new();

        for (key, item) in map.iter_mut() {
            result.insert(
                key.clone(),
                mapper.call_raw_with_extra_args(
                    "filter",
                    &ctx,
                    Some(item),
                    [key.into()],
                    [],
                    Some(1),
                )?,
            );
        }

        Ok(result)
    }
    /// Iterate through all the elements in the object map, applying a `filter` function to each
    /// and return a new collection of all elements that return `true` as a new object map.
    ///
    /// # Function Parameters
    ///
    /// * `key`: current key
    /// * `value` _(optional)_: copy of element (bound to `this` if omitted)
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = #{a:1, b:2, c:3, d:4, e:5};
    ///
    /// let y = x.filter(|k| this >= 3);
    ///
    /// print(y);       // prints #{"c":3, "d":4, "e":5}
    ///
    /// let y = x.filter(|k, v| k != "d" && v < 5);
    ///
    /// print(y);       // prints #{"a":1, "b":2, "c":3}
    /// ```
    #[rhai_fn(return_raw, pure)]
    pub fn filter(ctx: NativeCallContext, map: &mut Map, filter: FnPtr) -> RhaiResultOf<Map> {
        if map.is_empty() {
            return Ok(Map::new());
        }

        let mut result = Map::new();

        for (key, item) in map.iter_mut() {
            if filter
                .call_raw_with_extra_args("filter", &ctx, Some(item), [key.into()], [], Some(1))?
                .as_bool()
                .unwrap_or(false)
            {
                result.insert(key.clone(), item.clone());
            }
        }

        Ok(result)
    }
    /// Remove all elements in the object map that return `true` when applied the `filter` function and
    /// return them as a new object map.
    ///
    /// # Function Parameters
    ///
    /// * `key`: current key
    /// * `value` _(optional)_: copy of element (bound to `this` if omitted)
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = #{a:1, b:2, c:3, d:4, e:5};
    ///
    /// let y = x.drain(|k| this < 3);
    ///
    /// print(x);       // prints #{"c":3, "d":4, "e":5]
    ///
    /// print(y);       // prints #{"a":1, "b"2}
    ///
    /// let z = x.drain(|k, v| k == "c" || v >= 5);
    ///
    /// print(x);       // prints #{"d":4}
    ///
    /// print(z);       // prints #{"c":3, "e":5}
    /// ```
    #[rhai_fn(return_raw)]
    pub fn drain(ctx: NativeCallContext, map: &mut Map, filter: FnPtr) -> RhaiResultOf<Map> {
        if map.is_empty() {
            return Ok(Map::new());
        }

        let mut drained = Map::new();
        let mut retained = Map::new();
        let mut error = None;

        for (key, mut value) in mem::take(map) {
            if error.is_some() {
                retained.insert(key, value);
                continue;
            }

            match filter.call_raw_with_extra_args(
                "drain",
                &ctx,
                Some(&mut value),
                [key.clone().into()],
                [],
                Some(1),
            ) {
                Ok(r) => {
                    if r.as_bool().unwrap_or(false) {
                        drained.insert(key, value);
                    } else {
                        retained.insert(key, value);
                    }
                }
                Err(err) => error = Some(err),
            }
        }

        *map = retained;

        error.map_or_else(|| Ok(drained), Err)
    }
    /// Remove all elements in the object map that do not return `true` when applied the `filter` function and
    /// return them as a new object map.
    ///
    /// # Function Parameters
    ///
    /// * `key`: current key
    /// * `value` _(optional)_: copy of element (bound to `this` if omitted)
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = #{a:1, b:2, c:3, d:4, e:5};
    ///
    /// let y = x.retain(|k| this < 3);
    ///
    /// print(x);       // prints #{"a":1, "b"2}
    ///
    /// print(y);       // prints #{"c":3, "d":4, "e":5]
    ///
    /// let z = y.retain(|k, v| k == "c" || v >= 5);
    ///
    /// print(y);       // prints #{"c":3, "e":5}
    ///
    /// print(z);       // prints #{"d":4}
    /// ```
    #[rhai_fn(return_raw)]
    pub fn retain(ctx: NativeCallContext, map: &mut Map, filter: FnPtr) -> RhaiResultOf<Map> {
        if map.is_empty() {
            return Ok(Map::new());
        }

        let mut drained = Map::new();
        let mut retained = Map::new();
        let mut error = None;

        for (key, mut value) in mem::take(map) {
            if error.is_some() {
                retained.insert(key, value);
                continue;
            }

            match filter.call_raw_with_extra_args(
                "retain",
                &ctx,
                Some(&mut value),
                [key.clone().into()],
                [],
                Some(1),
            ) {
                Ok(r) => {
                    if r.as_bool().unwrap_or(false) {
                        retained.insert(key, value);
                    } else {
                        drained.insert(key, value);
                    }
                }
                Err(err) => error = Some(err),
            }
        }

        *map = retained;

        error.map_or_else(|| Ok(drained), Err)
    }

    /// Return the JSON representation of the object map.
    ///
    /// # Data types
    ///
    /// Only the following data types should be kept inside the object map:
    /// `INT`, `FLOAT`, `ImmutableString`, `char`, `bool`, `()`, `Array`, `Map`.
    ///
    /// # Errors
    ///
    /// Data types not supported by JSON serialize into formats that may
    /// invalidate the result.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = #{a:1, b:2, c:3};
    ///
    /// print(m.to_json());     // prints {"a":1, "b":2, "c":3}
    /// ```
    #[rhai_fn(pure)]
    pub fn to_json(map: &mut Map) -> String {
        #[cfg(feature = "metadata")]
        return serde_json::to_string(map).unwrap_or_else(|_| "ERROR".into());
        #[cfg(not(feature = "metadata"))]
        return crate::format_map_as_json(map);
    }
}
