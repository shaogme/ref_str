use ::core::ops::Deref;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;

/// Backend behavior for the shared ownership arm of the compact string type.
///
/// The core representation delegates all backend-specific operations through
/// this trait so the same string machinery can work with either `Rc<str>` or
/// `Arc<str>`.
pub trait RefCountBackend {
    /// The shared string handle for this backend.
    type Shared: Deref<Target = str>;

    /// Convert the shared string handle into a raw `*const str`.
    fn into_raw(shared: Self::Shared) -> *const str;

    /// Increment the strong count for a raw `*const str`.
    ///
    /// # Safety
    ///
    /// `ptr` must have been produced by this backend and must still point to a
    /// live allocation.
    unsafe fn increment_strong_count(ptr: *const str);

    /// Rebuild the shared string handle from a raw `*const str`.
    ///
    /// # Safety
    ///
    /// `ptr` must be a valid pointer previously produced by this backend.
    unsafe fn from_raw(ptr: *const str) -> Self::Shared;

    /// Build the shared string handle from a `&str`.
    fn from_str(s: &str) -> Self::Shared;

    /// Build the shared string handle from a [`String`].
    fn from_string(s: String) -> Self::Shared;

    /// Build the shared string handle from a `Box<str>`.
    fn from_boxed_str(s: Box<str>) -> Self::Shared;
}

/// `Rc<str>` backend.
pub enum LocalBackend {}

impl RefCountBackend for LocalBackend {
    type Shared = Rc<str>;

    fn into_raw(shared: Self::Shared) -> *const str {
        Rc::into_raw(shared)
    }

    unsafe fn increment_strong_count(ptr: *const str) {
        unsafe {
            Rc::increment_strong_count(ptr);
        }
    }

    unsafe fn from_raw(ptr: *const str) -> Self::Shared {
        unsafe { Rc::from_raw(ptr) }
    }

    fn from_str(s: &str) -> Self::Shared {
        Rc::from(s)
    }

    fn from_string(s: String) -> Self::Shared {
        Rc::from(s)
    }

    fn from_boxed_str(s: Box<str>) -> Self::Shared {
        Rc::from(s)
    }
}

/// `Arc<str>` backend.
pub enum SharedBackend {}

impl RefCountBackend for SharedBackend {
    type Shared = Arc<str>;

    fn into_raw(shared: Self::Shared) -> *const str {
        Arc::into_raw(shared)
    }

    unsafe fn increment_strong_count(ptr: *const str) {
        unsafe {
            Arc::increment_strong_count(ptr);
        }
    }

    unsafe fn from_raw(ptr: *const str) -> Self::Shared {
        unsafe { Arc::from_raw(ptr) }
    }

    fn from_str(s: &str) -> Self::Shared {
        Arc::from(s)
    }

    fn from_string(s: String) -> Self::Shared {
        Arc::from(s)
    }

    fn from_boxed_str(s: Box<str>) -> Self::Shared {
        Arc::from(s)
    }
}
