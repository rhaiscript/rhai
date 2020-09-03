#![cfg(not(feature = "no_std"))]

#[cfg(feature = "no_float")]
use super::{arithmetic::make_err, math_basic::MAX_INT};

use crate::def_package;
use crate::plugin::*;
use crate::result::EvalAltResult;

#[cfg(not(feature = "no_float"))]
use crate::parser::FLOAT;

#[cfg(feature = "no_float")]
use crate::parser::INT;

#[cfg(not(target_arch = "wasm32"))]
use crate::stdlib::time::Instant;

#[cfg(target_arch = "wasm32")]
use instant::Instant;

def_package!(crate:BasicTimePackage:"Basic timing utilities.", lib, {
    // Register date/time functions
    lib.combine_flatten(exported_module!(time_functions));
});

#[export_module]
mod time_functions {
    #[inline(always)]
    pub fn timestamp() -> Instant {
        Instant::now()
    }
    #[rhai_fn(return_raw)]
    pub fn elapsed(timestamp: &mut Instant) -> Result<Dynamic, Box<EvalAltResult>> {
        #[cfg(not(feature = "no_float"))]
        {
            Ok((timestamp.elapsed().as_secs_f64() as FLOAT).into())
        }

        #[cfg(feature = "no_float")]
        {
            let seconds = timestamp.elapsed().as_secs();

            if cfg!(not(feature = "unchecked")) && seconds > (MAX_INT as u64) {
                Err(make_err(format!(
                    "Integer overflow for timestamp.elapsed: {}",
                    seconds
                )))
            } else {
                Ok((seconds as INT).into())
            }
        }
    }

    #[rhai_fn(get = "elapsed", return_raw)]
    #[inline(always)]
    pub fn elapsed_prop(timestamp: &mut Instant) -> Result<Dynamic, Box<EvalAltResult>> {
        elapsed(timestamp)
    }

    #[rhai_fn(return_raw, name = "-")]
    pub fn time_diff(ts1: Instant, ts2: Instant) -> Result<Dynamic, Box<EvalAltResult>> {
        #[cfg(not(feature = "no_float"))]
        {
            Ok(if ts2 > ts1 {
                -(ts2 - ts1).as_secs_f64() as FLOAT
            } else {
                (ts1 - ts2).as_secs_f64() as FLOAT
            }
            .into())
        }

        #[cfg(feature = "no_float")]
        if ts2 > ts1 {
            let seconds = (ts2 - ts1).as_secs();

            if cfg!(not(feature = "unchecked")) && seconds > (MAX_INT as u64) {
                Err(make_err(format!(
                    "Integer overflow for timestamp duration: -{}",
                    seconds
                )))
            } else {
                Ok(Dynamic::from(-(seconds as INT)))
            }
        } else {
            let seconds = (ts1 - ts2).as_secs();

            if cfg!(not(feature = "unchecked")) && seconds > (MAX_INT as u64) {
                Err(make_err(format!(
                    "Integer overflow for timestamp duration: {}",
                    seconds
                )))
            } else {
                Ok((seconds as INT).into())
            }
        }
    }

    #[rhai_fn(name = "==")]
    #[inline(always)]
    pub fn eq(x: Instant, y: Instant) -> bool {
        x == y
    }
    #[rhai_fn(name = "!=")]
    #[inline(always)]
    pub fn ne(x: Instant, y: Instant) -> bool {
        x != y
    }
    #[rhai_fn(name = "<")]
    #[inline(always)]
    pub fn lt(x: Instant, y: Instant) -> bool {
        x < y
    }
    #[rhai_fn(name = "<=")]
    #[inline(always)]
    pub fn lte(x: Instant, y: Instant) -> bool {
        x <= y
    }
    #[rhai_fn(name = ">")]
    #[inline(always)]
    pub fn gt(x: Instant, y: Instant) -> bool {
        x > y
    }
    #[rhai_fn(name = ">=")]
    #[inline(always)]
    pub fn gte(x: Instant, y: Instant) -> bool {
        x >= y
    }
}
