use super::arithmetic::ArithmeticPackage;
use super::fn_basic::BasicFnPackage;
use super::iter_basic::BasicIteratorPackage;
use super::lang_core::LanguageCorePackage;
use super::logic::LogicPackage;
use super::string_basic::BasicStringPackage;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::def_package;

def_package!(crate:CorePackage:"_Core_ package containing basic facilities.", lib, {
    lib.standard = true;

    LanguageCorePackage::init(lib);
    ArithmeticPackage::init(lib);
    LogicPackage::init(lib);
    BasicStringPackage::init(lib);
    BasicIteratorPackage::init(lib);
    BasicFnPackage::init(lib);
});
