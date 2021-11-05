use crate::def_package;
use crate::dynamic::Tag;
use crate::plugin::*;
use crate::{Dynamic, EvalAltResult, INT};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

#[export_module]
mod core_functions {
    #[rhai_fn(name = "tag", get = "tag", pure)]
    pub fn get_tag(value: &mut Dynamic) -> INT {
        value.tag() as INT
    }
    #[rhai_fn(name = "set_tag", set = "tag", return_raw)]
    pub fn set_tag(value: &mut Dynamic, tag: INT) -> Result<(), Box<EvalAltResult>> {
        if tag < Tag::MIN as INT {
            EvalAltResult::ErrorArithmetic(
                format!(
                    "{} is too small to fit into a tag (must be between {} and {})",
                    tag,
                    Tag::MIN,
                    Tag::MAX
                ),
                Position::NONE,
            )
            .into()
        } else if tag > Tag::MAX as INT {
            EvalAltResult::ErrorArithmetic(
                format!(
                    "{} is too large to fit into a tag (must be between {} and {})",
                    tag,
                    Tag::MIN,
                    Tag::MAX
                ),
                Position::NONE,
            )
            .into()
        } else {
            value.set_tag(tag as Tag);
            Ok(())
        }
    }
}

def_package!(crate:LanguageCorePackage:"Language core functions.", lib, {
    lib.standard = true;

    combine_with_exported_module!(lib, "language_core", core_functions);
});
