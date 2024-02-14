#![cfg(feature = "debugging")]

use crate::def_package;
use crate::module::ModuleFlags;
use crate::plugin::*;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_index"))]
use crate::{Array, Dynamic, NativeCallContext};

#[cfg(not(feature = "no_function"))]
#[cfg(not(feature = "no_index"))]
#[cfg(not(feature = "no_object"))]
use crate::Map;

def_package! {
    /// Package of basic debugging utilities.
    pub DebuggingPackage(lib) {
        lib.flags |= ModuleFlags::STANDARD_LIB;

        combine_with_exported_module!(lib, "debugging", debugging_functions);
    }
}

#[export_module]
mod debugging_functions {
    /// Get an array of object maps containing the function calls stack.
    ///
    /// If there is no debugging interface registered, an empty array is returned.
    ///
    /// An array of strings is returned under `no_object`.
    #[cfg(not(feature = "no_function"))]
    #[cfg(not(feature = "no_index"))]
    pub fn back_trace(ctx: NativeCallContext) -> Array {
        use crate::debugger::CallStackFrame;

        ctx.global_runtime_state()
            .debugger
            .as_ref()
            .map_or_else(Array::new, |debugger| {
                debugger
                    .call_stack()
                    .iter()
                    .rev()
                    .filter(|CallStackFrame { fn_name, args, .. }| {
                        fn_name != "back_trace" || !args.is_empty()
                    })
                    .map(
                        |frame @ CallStackFrame {
                             fn_name: _fn_name,
                             args: _args,
                             source: _source,
                             pos: _pos,
                         }| {
                            let display = frame.to_string();

                            #[cfg(not(feature = "no_object"))]
                            {
                                use crate::INT;
                                use std::iter::FromIterator;

                                let mut map = Map::new();
                                map.insert("display".into(), display.into());
                                map.insert("fn_name".into(), _fn_name.into());
                                if !_args.is_empty() {
                                    map.insert(
                                        "args".into(),
                                        Dynamic::from_iter(_args.iter().cloned()),
                                    );
                                }
                                if let Some(source) = _source {
                                    map.insert("source".into(), source.into());
                                }
                                if !_pos.is_none() {
                                    map.insert("line".into(), (_pos.line().unwrap() as INT).into());
                                    map.insert(
                                        "position".into(),
                                        (_pos.position().unwrap_or(0) as INT).into(),
                                    );
                                }
                                Dynamic::from_map(map)
                            }
                            #[cfg(feature = "no_object")]
                            display.into()
                        },
                    )
                    .collect()
            })
    }
}
