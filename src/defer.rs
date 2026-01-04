//! Facility to run state restoration logic at the end of scope.

use std::ops::{Deref, DerefMut};
#[cfg(feature = "no_std")]
use std::prelude::v1::*;

/// Automatically restore state at the end of the scope.
macro_rules! defer {
    (let $temp:ident = $var:ident . $prop:ident; $code:stmt) => {
        defer!(let $temp = $var.$prop; $code => move |v| v.$prop = $temp);
    };
    (let $temp:ident = $var:ident . $prop:ident; $code:stmt => $restore:expr) => {
        let $temp = $var.$prop;
        $code
        defer!($var => $restore);
    };
    ($var:ident => $restore:ident; let $temp:ident = $save:expr;) => {
        defer!($var => $restore; let $temp = $save; {});
    };
    ($var:ident if $guard:expr => $restore:ident; let $temp:ident = $save:expr;) => {
        defer!($var if $guard => $restore; let $temp = $save; {});
    };
    ($var:ident => $restore:ident; let $temp:ident = $save:expr; $code:stmt) => {
        let $temp = $save;
        $code
        defer!($var => move |v| { v.$restore($temp); });
    };
    ($var:ident if $guard:expr => $restore:ident; let $temp:ident = $save:expr; $code:stmt) => {
        let $temp = $save;
        $code
        defer!($var if $guard => move |v| { v.$restore($temp); });
    };
    ($var:ident => $restore:expr) => {
        defer!($var = $var => $restore);
    };
    ($var:ident = $value:expr => $restore:expr) => {
        let $var = &mut *crate::Deferred::lock($value, $restore);
    };
    ($var:ident if Some($guard:ident) => $restore:expr) => {
        defer!($var = ($var) if Some($guard) => $restore);
    };
    ($var:ident = ( $value:expr ) if Some($guard:ident) => $restore:expr) => {
        let mut __rx__;
        let $var = match $guard {
            Some($guard) => { __rx__ = crate::Deferred::lock($value, $restore); &mut *__rx__ }
            _ => &mut *$value
        };
    };
    ($var:ident if $guard:expr => $restore:expr) => {
        defer!($var = ($var) if $guard => $restore);
    };
    ($var:ident = ( $value:expr ) if $guard:expr => $restore:expr) => {
        let mut __rx__;
        let $var = if $guard {
            __rx__ = crate::Deferred::lock($value, $restore);
            &mut *__rx__
        } else {
            &mut *$value
        };
    };
}

/// Run custom restoration logic upon the end of scope.
#[must_use]
pub struct Deferred<'a, T: ?Sized, R: FnOnce(&mut T)> {
    lock: &'a mut T,
    defer: Option<R>,
}

impl<'a, T: ?Sized, R: FnOnce(&mut T)> Deferred<'a, T, R> {
    /// Create a new [`Deferred`] that locks a mutable reference and runs restoration logic at
    /// the end of scope.
    ///
    /// Beware that the end of scope means the end of its lifetime, not necessarily waiting until
    /// the current block scope is exited.
    #[inline(always)]
    pub fn lock(value: &'a mut T, restore: R) -> Self {
        Self {
            lock: value,
            defer: Some(restore),
        }
    }
}

impl<T: ?Sized, R: FnOnce(&mut T)> Drop for Deferred<'_, T, R> {
    #[inline(always)]
    fn drop(&mut self) {
        self.defer.take().unwrap()(self.lock);
    }
}

impl<T: ?Sized, R: FnOnce(&mut T)> Deref for Deferred<'_, T, R> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.lock
    }
}

impl<T: ?Sized, R: FnOnce(&mut T)> DerefMut for Deferred<'_, T, R> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.lock
    }
}
