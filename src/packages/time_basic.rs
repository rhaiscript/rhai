#![cfg(not(feature = "no_time"))]

use super::arithmetic::make_err as make_arithmetic_err;
use crate::module::ModuleFlags;
use crate::plugin::*;
use crate::{def_package, Dynamic, RhaiResult, RhaiResultOf, INT};

#[cfg(not(feature = "no_float"))]
use crate::FLOAT;

#[cfg(any(not(target_family = "wasm"), not(target_os = "unknown")))]
use std::time::{Duration, Instant};

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use instant::{Duration, Instant};

def_package! {
    /// Package of basic timing utilities.
    pub BasicTimePackage(lib) {
        lib.flags |= ModuleFlags::STANDARD_LIB;

        // Register date/time functions
        combine_with_exported_module!(lib, "time", time_functions);
    }
}

#[export_module]
mod time_functions {
    /// Create a timestamp containing the current system time.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let now = timestamp();
    ///
    /// sleep(10.0);            // sleep for 10 seconds
    ///
    /// print(now.elapsed);     // prints 10.???
    /// ```
    #[rhai_fn(volatile)]
    pub fn timestamp() -> Instant {
        Instant::now()
    }

    /// Return the number of seconds between the current system time and the timestamp.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let now = timestamp();
    ///
    /// sleep(10.0);            // sleep for 10 seconds
    ///
    /// print(now.elapsed);     // prints 10.???
    /// ```
    #[rhai_fn(name = "elapsed", get = "elapsed", return_raw)]
    pub fn elapsed(timestamp: Instant) -> RhaiResult {
        #[cfg(not(feature = "no_float"))]
        if timestamp > Instant::now() {
            Err(make_arithmetic_err("Time-stamp is later than now"))
        } else {
            Ok((timestamp.elapsed().as_secs_f64() as FLOAT).into())
        }

        #[cfg(feature = "no_float")]
        {
            let seconds = timestamp.elapsed().as_secs();

            if cfg!(not(feature = "unchecked")) && seconds > (INT::MAX as u64) {
                return Err(make_arithmetic_err(format!(
                    "Integer overflow for timestamp.elapsed: {seconds}"
                )));
            }
            if timestamp > Instant::now() {
                return Err(make_arithmetic_err("Time-stamp is later than now"));
            }

            Ok((seconds as INT).into())
        }
    }

    /// Return the number of seconds between two timestamps.
    #[rhai_fn(return_raw, name = "-")]
    #[allow(clippy::unnecessary_wraps)]
    pub fn time_diff(timestamp1: Instant, timestamp2: Instant) -> RhaiResult {
        #[cfg(not(feature = "no_float"))]
        return Ok(if timestamp2 > timestamp1 {
            -(timestamp2 - timestamp1).as_secs_f64() as FLOAT
        } else {
            (timestamp1 - timestamp2).as_secs_f64() as FLOAT
        }
        .into());

        #[cfg(feature = "no_float")]
        if timestamp2 > timestamp1 {
            let seconds = (timestamp2 - timestamp1).as_secs();

            if cfg!(not(feature = "unchecked")) && seconds > (INT::MAX as u64) {
                return Err(make_arithmetic_err(format!(
                    "Integer overflow for timestamp duration: -{seconds}"
                )));
            }

            Ok((-(seconds as INT)).into())
        } else {
            let seconds = (timestamp1 - timestamp2).as_secs();

            if cfg!(not(feature = "unchecked")) && seconds > (INT::MAX as u64) {
                return Err(make_arithmetic_err(format!(
                    "Integer overflow for timestamp duration: {seconds}"
                )));
            }

            Ok((seconds as INT).into())
        }
    }

    #[cfg(not(feature = "no_float"))]
    pub mod float_functions {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn add_impl(timestamp: Instant, seconds: FLOAT) -> RhaiResultOf<Instant> {
            if seconds < 0.0 {
                return subtract_impl(timestamp, -seconds);
            }
            if cfg!(not(feature = "unchecked")) {
                if seconds > (INT::MAX as FLOAT).min(u64::MAX as FLOAT) {
                    return Err(make_arithmetic_err(format!(
                        "Integer overflow for timestamp add: {seconds}"
                    )));
                }

                timestamp
                    .checked_add(Duration::from_millis((seconds * 1000.0) as u64))
                    .ok_or_else(|| {
                        make_arithmetic_err(format!(
                            "Timestamp overflow when adding {seconds} second(s)"
                        ))
                    })
            } else {
                Ok(timestamp + Duration::from_millis((seconds * 1000.0) as u64))
            }
        }
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        fn subtract_impl(timestamp: Instant, seconds: FLOAT) -> RhaiResultOf<Instant> {
            if seconds < 0.0 {
                return add_impl(timestamp, -seconds);
            }
            if cfg!(not(feature = "unchecked")) {
                if seconds > (INT::MAX as FLOAT).min(u64::MAX as FLOAT) {
                    return Err(make_arithmetic_err(format!(
                        "Integer overflow for timestamp subtract: {seconds}"
                    )));
                }

                timestamp
                    .checked_sub(Duration::from_millis((seconds * 1000.0) as u64))
                    .ok_or_else(|| {
                        make_arithmetic_err(format!(
                            "Timestamp overflow when subtracting {seconds} second(s)"
                        ))
                    })
            } else {
                Ok(timestamp
                    .checked_sub(Duration::from_millis((seconds * 1000.0) as u64))
                    .unwrap())
            }
        }

        /// Add the specified number of `seconds` to the timestamp and return it as a new timestamp.
        #[rhai_fn(return_raw, name = "+")]
        pub fn add(timestamp: Instant, seconds: FLOAT) -> RhaiResultOf<Instant> {
            add_impl(timestamp, seconds)
        }
        /// Add the specified number of `seconds` to the timestamp.
        #[rhai_fn(return_raw, name = "+=")]
        pub fn add_assign(timestamp: &mut Instant, seconds: FLOAT) -> RhaiResultOf<()> {
            *timestamp = add_impl(*timestamp, seconds)?;
            Ok(())
        }
        /// Subtract the specified number of `seconds` from the timestamp and return it as a new timestamp.
        #[rhai_fn(return_raw, name = "-")]
        pub fn subtract(timestamp: Instant, seconds: FLOAT) -> RhaiResultOf<Instant> {
            subtract_impl(timestamp, seconds)
        }
        /// Subtract the specified number of `seconds` from the timestamp.
        #[rhai_fn(return_raw, name = "-=")]
        pub fn subtract_assign(timestamp: &mut Instant, seconds: FLOAT) -> RhaiResultOf<()> {
            *timestamp = subtract_impl(*timestamp, seconds)?;
            Ok(())
        }
    }

    #[inline(always)]
    fn add_inner(timestamp: Instant, seconds: u64) -> Option<Instant> {
        if cfg!(not(feature = "unchecked")) {
            timestamp.checked_add(Duration::from_secs(seconds))
        } else {
            Some(timestamp + Duration::from_secs(seconds))
        }
    }
    #[inline]
    fn add_impl(timestamp: Instant, seconds: INT) -> RhaiResultOf<Instant> {
        if seconds < 0 {
            subtract_inner(timestamp, seconds.unsigned_abs() as u64)
        } else {
            #[allow(clippy::cast_sign_loss)]
            add_inner(timestamp, seconds as u64)
        }
        .ok_or_else(|| {
            make_arithmetic_err(format!(
                "Timestamp overflow when adding {seconds} second(s)"
            ))
        })
    }
    #[inline(always)]
    fn subtract_inner(timestamp: Instant, seconds: u64) -> Option<Instant> {
        if cfg!(not(feature = "unchecked")) {
            timestamp.checked_sub(Duration::from_secs(seconds))
        } else {
            Some(timestamp - Duration::from_secs(seconds))
        }
    }
    #[inline]
    fn subtract_impl(timestamp: Instant, seconds: INT) -> RhaiResultOf<Instant> {
        if seconds < 0 {
            add_inner(timestamp, seconds.unsigned_abs() as u64)
        } else {
            #[allow(clippy::cast_sign_loss)]
            subtract_inner(timestamp, seconds as u64)
        }
        .ok_or_else(|| {
            make_arithmetic_err(format!(
                "Timestamp overflow when subtracting {seconds} second(s)"
            ))
        })
    }

    /// Add the specified number of `seconds` to the timestamp and return it as a new timestamp.
    #[rhai_fn(return_raw, name = "+")]
    pub fn add(timestamp: Instant, seconds: INT) -> RhaiResultOf<Instant> {
        add_impl(timestamp, seconds)
    }
    /// Add the specified number of `seconds` to the timestamp.
    #[rhai_fn(return_raw, name = "+=")]
    pub fn add_assign(timestamp: &mut Instant, seconds: INT) -> RhaiResultOf<()> {
        *timestamp = add_impl(*timestamp, seconds)?;
        Ok(())
    }
    /// Subtract the specified number of `seconds` from the timestamp and return it as a new timestamp.
    #[rhai_fn(return_raw, name = "-")]
    pub fn subtract(timestamp: Instant, seconds: INT) -> RhaiResultOf<Instant> {
        subtract_impl(timestamp, seconds)
    }
    /// Subtract the specified number of `seconds` from the timestamp.
    #[rhai_fn(return_raw, name = "-=")]
    pub fn subtract_assign(timestamp: &mut Instant, seconds: INT) -> RhaiResultOf<()> {
        *timestamp = subtract_impl(*timestamp, seconds)?;
        Ok(())
    }

    /// Return `true` if two timestamps are equal.
    #[rhai_fn(name = "==")]
    pub fn eq(timestamp1: Instant, timestamp2: Instant) -> bool {
        timestamp1 == timestamp2
    }
    /// Return `true` if two timestamps are not equal.
    #[rhai_fn(name = "!=")]
    pub fn ne(timestamp1: Instant, timestamp2: Instant) -> bool {
        timestamp1 != timestamp2
    }
    /// Return `true` if the first timestamp is earlier than the second.
    #[rhai_fn(name = "<")]
    pub fn lt(timestamp1: Instant, timestamp2: Instant) -> bool {
        timestamp1 < timestamp2
    }
    /// Return `true` if the first timestamp is earlier than or equals to the second.
    #[rhai_fn(name = "<=")]
    pub fn lte(timestamp1: Instant, timestamp2: Instant) -> bool {
        timestamp1 <= timestamp2
    }
    /// Return `true` if the first timestamp is later than the second.
    #[rhai_fn(name = ">")]
    pub fn gt(timestamp1: Instant, timestamp2: Instant) -> bool {
        timestamp1 > timestamp2
    }
    /// Return `true` if the first timestamp is later than or equals to the second.
    #[rhai_fn(name = ">=")]
    pub fn gte(timestamp1: Instant, timestamp2: Instant) -> bool {
        timestamp1 >= timestamp2
    }
}
