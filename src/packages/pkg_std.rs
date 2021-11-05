#[cfg(not(feature = "no_index"))]
use super::array_basic::BasicArrayPackage;
#[cfg(not(feature = "no_object"))]
use super::map_basic::BasicMapPackage;
use super::math_basic::BasicMathPackage;
use super::pkg_core::CorePackage;
use super::string_more::MoreStringPackage;
#[cfg(not(feature = "no_std"))]
use super::time_basic::BasicTimePackage;
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use crate::def_package;

def_package!(crate:StandardPackage:"_Standard_ package containing all built-in features.", lib, {
    lib.standard = true;

    CorePackage::init(lib);
    BasicMathPackage::init(lib);
    #[cfg(not(feature = "no_index"))]
    BasicArrayPackage::init(lib);
    #[cfg(not(feature = "no_object"))]
    BasicMapPackage::init(lib);
    #[cfg(not(feature = "no_std"))]
    BasicTimePackage::init(lib);
    MoreStringPackage::init(lib);
});
