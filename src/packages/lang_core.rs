use crate::def_package;
use crate::plugin::*;
use crate::types::dynamic::Tag;
use crate::{Dynamic, RhaiResult, RhaiResultOf, ERR, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_float"))]
#[cfg(not(feature = "no_std"))]
use crate::FLOAT;

def_package! {
    /// Package of core language features.
    pub LanguageCorePackage(lib) {
        lib.set_standard_lib(true);

        combine_with_exported_module!(lib, "core", core_functions);

        #[cfg(not(feature = "no_function"))]
        #[cfg(not(feature = "no_index"))]
        #[cfg(not(feature = "no_object"))]
        combine_with_exported_module!(lib, "reflection", reflection_functions);
    }
}

#[export_module]
mod core_functions {
    /// Exit the script evaluation immediately with a value.
    ///
    /// # Example
    /// ```rhai
    /// exit(42);
    /// ```
    #[rhai_fn(name = "exit", volatile, return_raw)]
    pub fn exit_with_value(value: Dynamic) -> RhaiResult {
        Err(ERR::Exit(value, Position::NONE).into())
    }
    /// Exit the script evaluation immediately with `()` as exit value.
    ///
    /// # Example
    /// ```rhai
    /// exit();
    /// ```
    #[rhai_fn(volatile, return_raw)]
    pub fn exit() -> RhaiResult {
        Err(ERR::Exit(Dynamic::UNIT, Position::NONE).into())
    }
    /// Take ownership of the data in a `Dynamic` value and return it.
    /// The data is _NOT_ cloned.
    ///
    /// The original value is replaced with `()`.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = 42;
    ///
    /// print(take(x));         // prints 42
    ///
    /// print(x);               // prints ()
    /// ```
    #[rhai_fn(return_raw)]
    pub fn take(value: &mut Dynamic) -> RhaiResult {
        if value.is_read_only() {
            return Err(
                ERR::ErrorNonPureMethodCallOnConstant("take".to_string(), Position::NONE).into(),
            );
        }

        Ok(std::mem::take(value))
    }
    /// Return the _tag_ of a `Dynamic` value.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = "hello, world!";
    ///
    /// x.tag = 42;
    ///
    /// print(x.tag);           // prints 42
    /// ```
    #[rhai_fn(name = "tag", get = "tag", pure)]
    pub fn get_tag(value: &mut Dynamic) -> INT {
        value.tag() as INT
    }
    /// Set the _tag_ of a `Dynamic` value.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let x = "hello, world!";
    ///
    /// x.tag = 42;
    ///
    /// print(x.tag);           // prints 42
    /// ```
    #[rhai_fn(name = "set_tag", set = "tag", return_raw)]
    pub fn set_tag(value: &mut Dynamic, tag: INT) -> RhaiResultOf<()> {
        const TAG_MIN: Tag = Tag::MIN;
        const TAG_MAX: Tag = Tag::MAX;

        if tag < TAG_MIN as INT {
            return Err(ERR::ErrorArithmetic(
                format!(
                    "{tag} is too small to fit into a tag (must be between {TAG_MIN} and {TAG_MAX})"
                ),
                Position::NONE,
            )
            .into());
        }
        if tag > TAG_MAX as INT {
            return Err(ERR::ErrorArithmetic(
                format!(
                    "{tag} is too large to fit into a tag (must be between {TAG_MIN} and {TAG_MAX})"
                ),
                Position::NONE,
            )
            .into());
        }

        value.set_tag(tag as Tag);
        Ok(())
    }

    /// Block the current thread for a particular number of `seconds`.
    ///
    /// # Example
    ///
    /// ```rhai
    /// // Do nothing for 10 seconds!
    /// sleep(10.0);
    /// ```
    #[cfg(not(feature = "no_float"))]
    #[cfg(not(feature = "no_std"))]
    #[rhai_fn(name = "sleep", volatile)]
    pub fn sleep_float(seconds: FLOAT) {
        if !seconds.is_normal() || seconds.is_sign_negative() {
            return;
        }

        #[cfg(not(feature = "f32_float"))]
        std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
        #[cfg(feature = "f32_float")]
        std::thread::sleep(std::time::Duration::from_secs_f32(seconds));
    }
    /// Block the current thread for a particular number of `seconds`.
    ///
    /// # Example
    ///
    /// ```rhai
    /// // Do nothing for 10 seconds!
    /// sleep(10);
    /// ```
    #[cfg(not(feature = "no_std"))]
    #[rhai_fn(volatile)]
    pub fn sleep(seconds: INT) {
        if seconds <= 0 {
            return;
        }

        #[allow(clippy::cast_sign_loss)]
        std::thread::sleep(std::time::Duration::from_secs(seconds as u64));
    }

    /// Parse a JSON string into a value.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let m = parse_json(`{"a":1, "b":2, "c":3}`);
    ///
    /// print(m);       // prints #{"a":1, "b":2, "c":3}
    /// ```
    #[cfg(not(feature = "no_object"))]
    #[rhai_fn(return_raw)]
    pub fn parse_json(_ctx: NativeCallContext, json: &str) -> RhaiResultOf<Dynamic> {
        #[cfg(feature = "metadata")]
        let out = serde_json::from_str(json).map_err(|err| err.to_string().into());

        #[cfg(not(feature = "metadata"))]
        let out = _ctx.engine().parse_json(json, true).map(Dynamic::from);

        out
    }
}

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
#[export_module]
mod reflection_functions {
    use crate::module::FuncInfo;
    use crate::{Array, Map, ScriptFnMetadata};

    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_index"))]
    #[cfg(not(feature = "no_object"))]
    fn collect(
        ctx: NativeCallContext,
        filter: impl Fn(FnNamespace, FnAccess, &str, usize, &ScriptFnMetadata) -> bool,
    ) -> Array {
        let engine = ctx.engine();

        engine.collect_fn_metadata_impl(
            Some(&ctx),
            |FuncInfo {
                 metadata,
                 #[cfg(not(feature = "no_module"))]
                 namespace,
                 script,
             }|
             -> Option<Dynamic> {
                let func = script.as_ref()?;

                if !filter(
                    metadata.namespace,
                    func.access,
                    func.name,
                    func.params.len(),
                    func,
                ) {
                    return None;
                }

                let mut map = Map::new();

                #[cfg(not(feature = "no_module"))]
                if !namespace.is_empty() {
                    map.insert(
                        "namespace".into(),
                        engine.get_interned_string(namespace).into(),
                    );
                }
                map.insert("name".into(), engine.get_interned_string(func.name).into());
                map.insert(
                    "access".into(),
                    engine
                        .get_interned_string(match func.access {
                            FnAccess::Public => "public",
                            FnAccess::Private => "private",
                        })
                        .into(),
                );
                map.insert(
                    "is_anonymous".into(),
                    func.name.starts_with(crate::engine::FN_ANONYMOUS).into(),
                );
                if let Some(this_type) = func.this_type {
                    map.insert("this_type".into(), this_type.into());
                }
                map.insert(
                    "params".into(),
                    func.params
                        .iter()
                        .map(|&p| engine.get_interned_string(p).into())
                        .collect::<Array>()
                        .into(),
                );
                #[cfg(feature = "metadata")]
                if !func.comments.is_empty() {
                    map.insert(
                        "comments".into(),
                        func.comments
                            .iter()
                            .map(|&s| engine.get_interned_string(s).into())
                            .collect::<Array>()
                            .into(),
                    );
                }

                Some(Dynamic::from_map(map))
            },
            false,
        )
    }

    /// Return an array of object maps containing metadata of all script-defined functions.
    #[rhai_fn(name = "get_fn_metadata_list", volatile)]
    pub fn get_fn_metadata_list(ctx: NativeCallContext) -> Array {
        collect(ctx, |_, _, _, _, _| true)
    }
    /// Return an array of object maps containing metadata of all script-defined functions
    /// matching the specified name.
    #[rhai_fn(name = "get_fn_metadata_list", volatile)]
    pub fn get_fn_metadata(ctx: NativeCallContext, name: &str) -> Array {
        collect(ctx, |_, _, n, _, _| n == name)
    }
    /// Return an array of object maps containing metadata of all script-defined functions
    /// matching the specified name and arity (number of parameters).
    #[rhai_fn(name = "get_fn_metadata_list", volatile)]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn get_fn_metadata2(ctx: NativeCallContext, name: &str, params: INT) -> Array {
        if !(0..=crate::MAX_USIZE_INT).contains(&params) {
            return Array::new();
        }

        collect(ctx, |_, _, n, p, _| p == (params as usize) && n == name)
    }
}
