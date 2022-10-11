#[cfg(feature = "no_std")]
use std::prelude::v1::*;

use super::*;
use crate::def_package;

def_package! {
    /// Standard package containing all built-in features.
    ///
    /// # Contents
    ///
    /// * [`CorePackage`][super::CorePackage]
    /// * [`BitFieldPackage`][super::BitFieldPackage]
    /// * [`LogicPackage`][super::LogicPackage]
    /// * [`BasicMathPackage`][super::BasicMathPackage]
    /// * [`BasicArrayPackage`][super::BasicArrayPackage]
    /// * [`BasicBlobPackage`][super::BasicBlobPackage]
    /// * [`BasicMapPackage`][super::BasicMapPackage]
    /// * [`BasicTimePackage`][super::BasicTimePackage]
    /// * [`MoreStringPackage`][super::MoreStringPackage]
    pub StandardPackage(lib) :
            CorePackage,
            BitFieldPackage,
            LogicPackage,
            BasicMathPackage,
            #[cfg(not(feature = "no_index"))] BasicArrayPackage,
            #[cfg(not(feature = "no_index"))] BasicBlobPackage,
            #[cfg(not(feature = "no_object"))] BasicMapPackage,
            #[cfg(not(feature = "no_time"))] BasicTimePackage,
            MoreStringPackage
    {
        lib.standard = true;
    }
}
