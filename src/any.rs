use std::any::{Any as StdAny, TypeId};
use std::fmt;

pub trait Any: StdAny {
    fn box_clone(&self) -> Box<Any>;

    /// This type may only be implemented by `rhai`.
    #[doc(hidden)]
    fn _closed(&self) -> _Private;
}

impl<T> Any for T
    where
        T: Clone + StdAny + ?Sized {
    #[inline]
    fn box_clone(&self) -> Box<Any> {
        Box::new(self.clone())
    }

    fn _closed(&self) -> _Private { _Private }
}

impl Any {
    #[inline]
    fn box_clone(&self) -> Box<Any> {
        Any::box_clone(self)
    }

    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        let t = TypeId::of::<T>();
        let boxed = self.type_id();

        t == boxed
    }

    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if self.is::<T>() {
            unsafe {
                Some(&*(self as *const Any as *const T))
            }
        } else {
            None
        }
    }

    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            unsafe {
                Some(&mut *(self as *mut Any as *mut T))
            }
        } else {
            None
        }
    }
}

impl Clone for Box<Any> {
    fn clone(&self) -> Self {
        Any::box_clone(self.as_ref() as &Any)
    }
}

impl fmt::Debug for Any {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("Any")
    }
}

pub trait AnyExt: Sized {
    fn downcast<T: Any + Clone>(self) -> Result<Box<T>, Self>;
}

impl AnyExt for Box<Any> {
    fn downcast<T: Any + Clone>(self) -> Result<Box<T>, Self> {
        if self.is::<T>() {
            unsafe {
                let raw: *mut Any = Box::into_raw(self);
                Ok(Box::from_raw(raw as *mut T))
            }
        } else {
            Err(self)
        }
    }
}

/// Private type which ensures that `rhai::Any` can only
/// be implemented by this crate.
#[doc(hidden)]
pub struct _Private;
