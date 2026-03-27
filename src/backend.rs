//! Internal backend and shared implementation for compressed string types.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr;
use core::ptr::NonNull;

/// Backend behavior for shared string ownership.
pub trait RefCountBackend {
    /// The shared string handle for this backend.
    type Shared: Deref<Target = str>;

    /// Convert the shared string handle into a raw `*const str`.
    fn into_raw(shared: Self::Shared) -> *const str;

    /// Increment the strong count for a raw `*const str`.
    unsafe fn increment_strong_count(ptr: *const str);

    /// Rebuild the shared string handle from a raw `*const str`.
    unsafe fn from_raw(ptr: *const str) -> Self::Shared;
}

/// `Rc<str>` backend.
pub enum LocalBackend {}

impl RefCountBackend for LocalBackend {
    /// The shared string handle is `Rc<str>`.
    type Shared = Rc<str>;

    /// Convert `Rc<str>` into a raw `*const str`.
    fn into_raw(shared: Self::Shared) -> *const str {
        Rc::into_raw(shared)
    }

    /// Increment the `Rc<str>` strong count.
    unsafe fn increment_strong_count(ptr: *const str) {
        unsafe {
            Rc::increment_strong_count(ptr);
        }
    }

    /// Rebuild `Rc<str>` from a raw `*const str`.
    unsafe fn from_raw(ptr: *const str) -> Self::Shared {
        unsafe { Rc::from_raw(ptr) }
    }
}

/// `Arc<str>` backend.
pub enum SharedBackend {}

impl RefCountBackend for SharedBackend {
    /// The shared string handle is `Arc<str>`.
    type Shared = Arc<str>;

    /// Convert `Arc<str>` into a raw `*const str`.
    fn into_raw(shared: Self::Shared) -> *const str {
        Arc::into_raw(shared)
    }

    /// Increment the `Arc<str>` strong count.
    unsafe fn increment_strong_count(ptr: *const str) {
        unsafe {
            Arc::increment_strong_count(ptr);
        }
    }

    /// Rebuild `Arc<str>` from a raw `*const str`.
    unsafe fn from_raw(ptr: *const str) -> Self::Shared {
        unsafe { Arc::from_raw(ptr) }
    }
}

/// Internal 16-byte compressed string representation.
pub struct CompressedRefStr<'a, B: RefCountBackend> {
    /// Packed pointer to the string data.
    raw_ptr: NonNull<u8>,
    /// Packed length and tag bits.
    len_and_tag: usize,
    /// Carries the borrowed lifetime.
    _marker: PhantomData<&'a str>,
    /// Keeps the backend type in the type system.
    _backend: PhantomData<B>,
}

impl<'a, B: RefCountBackend> CompressedRefStr<'a, B> {
    /// Bit used to distinguish shared and static states.
    const TAG_MASK: usize = 1usize << (usize::BITS - 1);
    /// Mask for the length payload.
    const LEN_MASK: usize = !Self::TAG_MASK;
    /// Tag value used for shared strings.
    const SHARED_TAG: usize = Self::TAG_MASK;
    /// Tag value used for borrowed strings.
    const STATIC_TAG: usize = 0;

    /// Build from a borrowed `&str`.
    #[inline]
    pub const fn new(s: &'a str) -> Self {
        Self::from_str(s)
    }

    /// Build from a borrowed `&str`.
    #[inline]
    pub const fn from_str(s: &'a str) -> Self {
        unsafe {
            Self::from_raw_parts(
                NonNull::new_unchecked(s.as_ptr() as *mut u8),
                s.len(),
                Self::STATIC_TAG,
            )
        }
    }

    /// Build from raw parts.
    #[inline]
    pub const unsafe fn from_raw_parts(raw_ptr: NonNull<u8>, len: usize, tag: usize) -> Self {
        assert!(len <= Self::LEN_MASK, "string too large to compress");
        Self {
            raw_ptr,
            len_and_tag: len | tag,
            _marker: PhantomData,
            _backend: PhantomData,
        }
    }

    /// Build from a shared owned string.
    pub fn from_shared(s: B::Shared) -> Self {
        let len = s.deref().len();
        let raw = B::into_raw(s);
        unsafe {
            Self::from_raw_parts(
                NonNull::new_unchecked(raw as *mut u8),
                len,
                Self::SHARED_TAG,
            )
        }
    }

    /// Split the value into raw parts.
    ///
    /// The caller becomes responsible for reconstructing or freeing it.
    pub const unsafe fn into_raw_parts(self) -> (NonNull<u8>, usize, usize) {
        let this = ManuallyDrop::new(self);
        let this_ptr = &this as *const ManuallyDrop<Self> as *const Self;
        unsafe {
            // Read the packed fields without moving the value out.
            (
                (*this_ptr).raw_ptr,
                (*this_ptr).len_and_tag & Self::LEN_MASK,
                (*this_ptr).len_and_tag & Self::TAG_MASK,
            )
        }
    }

    /// Convert the value into a raw `*const str`.
    ///
    /// The caller takes over ownership and must free it correctly.
    pub const unsafe fn into_raw(self) -> *const str {
        let (raw_ptr, len, _) = unsafe { self.into_raw_parts() };
        ptr::slice_from_raw_parts(raw_ptr.as_ptr(), len) as *const str
    }

    /// Increment the strong count for a raw `*const str`.
    pub unsafe fn increment_strong_count(ptr: *const str) {
        unsafe {
            B::increment_strong_count(ptr);
        }
    }

    /// Build from a `&'static str`.
    pub fn from_static(s: &'static str) -> CompressedRefStr<'static, B> {
        CompressedRefStr::from_str(s)
    }

    /// Return `true` if the value stores a shared string.
    #[inline]
    pub const fn is_shared(&self) -> bool {
        (self.len_and_tag & Self::TAG_MASK) != 0
    }

    /// Return `true` if the value stores a borrowed string.
    #[inline]
    pub const fn is_borrowed(&self) -> bool {
        !self.is_shared()
    }

    /// Return the string length.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len_and_tag & Self::LEN_MASK
    }

    /// Return `true` if the string is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the underlying data pointer.
    #[inline]
    fn data_ptr(&self) -> *const u8 {
        self.raw_ptr.as_ptr()
    }

    /// View the value as `&str`.
    pub fn as_str(&self) -> &str {
        unsafe {
            let slice = core::slice::from_raw_parts(self.data_ptr(), self.len());
            core::str::from_utf8_unchecked(slice)
        }
    }

    /// Convert the string into owned bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.as_str().as_bytes().to_vec()
    }

    /// Convert the string into a boxed string slice.
    pub fn into_boxed_str(self) -> Box<str> {
        self.into_string().into_boxed_str()
    }

    /// Convert the string into `String`.
    pub fn into_string(self) -> String {
        String::from(self.as_str())
    }
}

impl<'a, B: RefCountBackend> From<&'a str> for CompressedRefStr<'a, B> {
    /// Build from a borrowed `&str`.
    fn from(value: &'a str) -> Self {
        Self::new(value)
    }
}

impl<'a, B: RefCountBackend> From<&'a alloc::string::String> for CompressedRefStr<'a, B> {
    /// Build from a borrowed `String`.
    fn from(value: &'a alloc::string::String) -> Self {
        Self::from(value.as_str())
    }
}

impl<'a> From<Rc<str>> for CompressedRefStr<'a, LocalBackend> {
    /// Build a local string from `Rc<str>`.
    fn from(value: Rc<str>) -> Self {
        Self::from_shared(value)
    }
}

impl<'a> From<Arc<str>> for CompressedRefStr<'a, SharedBackend> {
    /// Build a shared string from `Arc<str>`.
    fn from(value: Arc<str>) -> Self {
        Self::from_shared(value)
    }
}

impl<'a> From<CompressedRefStr<'a, LocalBackend>> for CompressedRefStr<'a, SharedBackend> {
    /// Convert a local string into a shared string.
    fn from(value: CompressedRefStr<'a, LocalBackend>) -> Self {
        if value.is_shared() {
            Self::from_shared(Arc::from(value.as_str()))
        } else {
            let (raw_ptr, len, tag) = unsafe { value.into_raw_parts() };
            unsafe { Self::from_raw_parts(raw_ptr, len, tag) }
        }
    }
}

impl<'a> From<CompressedRefStr<'a, SharedBackend>> for CompressedRefStr<'a, LocalBackend> {
    /// Convert a shared string into a local string.
    fn from(value: CompressedRefStr<'a, SharedBackend>) -> Self {
        if value.is_shared() {
            Self::from_shared(Rc::from(value.as_str()))
        } else {
            let (raw_ptr, len, tag) = unsafe { value.into_raw_parts() };
            unsafe { Self::from_raw_parts(raw_ptr, len, tag) }
        }
    }
}

impl<'a> From<Box<str>> for CompressedRefStr<'a, LocalBackend> {
    /// Build a local string from `Box<str>`.
    fn from(value: Box<str>) -> Self {
        Self::from_shared(Rc::from(value))
    }
}

impl<'a> From<Box<str>> for CompressedRefStr<'a, SharedBackend> {
    /// Build a shared string from `Box<str>`.
    fn from(value: Box<str>) -> Self {
        Self::from_shared(Arc::from(value))
    }
}

impl<'a> From<alloc::string::String> for CompressedRefStr<'a, LocalBackend> {
    /// Build a local string from `String`.
    fn from(value: alloc::string::String) -> Self {
        Self::from_shared(Rc::from(value))
    }
}

impl<'a> From<alloc::string::String> for CompressedRefStr<'a, SharedBackend> {
    /// Build a shared string from `String`.
    fn from(value: alloc::string::String) -> Self {
        Self::from_shared(Arc::from(value))
    }
}

impl<'a, B: RefCountBackend> From<CompressedRefStr<'a, B>> for Cow<'a, str> {
    /// Convert into a borrowed-or-owned `Cow<str>`.
    fn from(value: CompressedRefStr<'a, B>) -> Self {
        Cow::Owned(value.into_string())
    }
}

impl<'a> From<Cow<'a, str>> for CompressedRefStr<'a, LocalBackend> {
    /// Build a local string from `Cow<str>`.
    fn from(value: Cow<'a, str>) -> Self {
        match value {
            Cow::Borrowed(s) => Self::from(s),
            Cow::Owned(s) => Self::from(s),
        }
    }
}

impl<'a> From<Cow<'a, str>> for CompressedRefStr<'a, SharedBackend> {
    /// Build a shared string from `Cow<str>`.
    fn from(value: Cow<'a, str>) -> Self {
        match value {
            Cow::Borrowed(s) => Self::from(s),
            Cow::Owned(s) => Self::from(s),
        }
    }
}

impl<'a, B: RefCountBackend> Default for CompressedRefStr<'a, B> {
    /// Create an empty borrowed string.
    fn default() -> Self {
        Self::from_static("")
    }
}

impl<'a, B: RefCountBackend> AsRef<str> for CompressedRefStr<'a, B> {
    /// Borrow as `&str`.
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> Borrow<str> for CompressedRefStr<'a, B> {
    /// Borrow as `&str`.
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> PartialEq for CompressedRefStr<'a, B> {
    /// Compare by string contents.
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<'a, B: RefCountBackend> Eq for CompressedRefStr<'a, B> {}

impl<'a, B: RefCountBackend> PartialEq<&str> for CompressedRefStr<'a, B> {
    /// Compare against `&str`.
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<'a, B: RefCountBackend> PartialEq<alloc::string::String> for CompressedRefStr<'a, B> {
    /// Compare against `String`.
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<'a, B: RefCountBackend> PartialOrd for CompressedRefStr<'a, B> {
    /// Compare lexicographically.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a, B: RefCountBackend> Ord for CompressedRefStr<'a, B> {
    /// Compare lexicographically.
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<'a, B: RefCountBackend> Hash for CompressedRefStr<'a, B> {
    /// Hash the string contents.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl<'a, B: RefCountBackend> Deref for CompressedRefStr<'a, B> {
    type Target = str;

    /// Dereference to `str`.
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> fmt::Debug for CompressedRefStr<'a, B> {
    /// Format as a debug tuple.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CompressedRefStr")
            .field(&self.as_str())
            .finish()
    }
}

impl<'a, B: RefCountBackend> fmt::Display for CompressedRefStr<'a, B> {
    /// Format the string contents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'a, B: RefCountBackend> Clone for CompressedRefStr<'a, B> {
    /// Clone the value and bump the shared count when needed.
    fn clone(&self) -> Self {
        if self.is_shared() {
            let data_ptr = self.data_ptr();
            let fat_ptr = ptr::slice_from_raw_parts(data_ptr, self.len()) as *const str;
            unsafe {
                B::increment_strong_count(fat_ptr);
            }
        }

        Self {
            raw_ptr: self.raw_ptr,
            len_and_tag: self.len_and_tag,
            _marker: PhantomData,
            _backend: PhantomData,
        }
    }
}

impl<'a, B: RefCountBackend> Drop for CompressedRefStr<'a, B> {
    /// Release the shared reference when dropping a shared value.
    fn drop(&mut self) {
        if self.is_shared() {
            let data_ptr = self.data_ptr();
            let fat_ptr = ptr::slice_from_raw_parts(data_ptr, self.len()) as *const str;
            unsafe {
                drop(B::from_raw(fat_ptr));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{LocalRefStr, RefStr};
    use alloc::borrow::Cow;
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::string::String;
    use alloc::string::ToString;
    use alloc::sync::Arc;

    #[test]
    fn local_shared_roundtrip() {
        let original: Rc<str> = Rc::from("hello");
        let value = LocalRefStr::from_shared(original.clone());

        assert_eq!(value.as_str(), "hello");
        assert!(value.is_shared());
        assert_eq!(Rc::strong_count(&original), 2);

        let cloned = value.clone();
        assert_eq!(cloned.as_str(), "hello");
        assert_eq!(Rc::strong_count(&original), 3);
    }

    #[test]
    fn shared_roundtrip() {
        let original: Arc<str> = Arc::from("hello");
        let value = RefStr::from_shared(original.clone());

        assert_eq!(value.as_str(), "hello");
        assert!(value.is_shared());
        assert_eq!(Arc::strong_count(&original), 2);

        let cloned = value.clone();
        assert_eq!(cloned.as_str(), "hello");
        assert_eq!(Arc::strong_count(&original), 3);
    }

    #[test]
    fn backend_conversion_roundtrip() {
        let local = LocalRefStr::from_shared(Rc::from("hello"));
        let shared: RefStr<'_> = local.into();

        assert_eq!(shared.as_str(), "hello");
        assert!(shared.is_shared());

        let local_again: LocalRefStr<'_> = shared.into();
        assert_eq!(local_again.as_str(), "hello");
        assert!(local_again.is_shared());
    }

    #[test]
    fn backend_conversion_borrowed() {
        let owned = String::from("borrowed");
        let local = LocalRefStr::from(&owned[..]);
        let shared: RefStr<'_> = local.into();

        assert_eq!(shared.as_str(), "borrowed");
        assert!(shared.is_borrowed());
    }

    #[test]
    fn borrowed_roundtrip() {
        let owned = String::from("borrowed");
        let local = LocalRefStr::from(&owned[..]);
        let shared = RefStr::from(&owned[..]);

        assert_eq!(local.as_str(), "borrowed");
        assert_eq!(shared.as_str(), "borrowed");
        assert!(!local.is_shared());
        assert!(!shared.is_shared());
        assert!(local.is_borrowed());
        assert!(shared.is_borrowed());
    }

    #[test]
    fn string_roundtrip() {
        let local: LocalRefStr<'_> = String::from("hello").into();
        let shared: RefStr<'_> = String::from("world").into();
        let via_ref: LocalRefStr<'_> = String::from("borrow").into();
        let from_box: RefStr<'_> = Box::<str>::from("boxed").into();
        let from_cow: LocalRefStr<'_> = Cow::Borrowed("cow").into();

        assert_eq!(local.to_string(), "hello");
        assert_eq!(shared.to_string(), "world");
        assert_eq!(via_ref.as_str(), "borrow");
        assert_eq!(from_box.as_str(), "boxed");
        assert_eq!(from_cow.as_str(), "cow");
    }

    #[test]
    fn owned_roundtrip() {
        let local: LocalRefStr<'_> = String::from("bytes").into();
        let shared: RefStr<'_> = String::from("boxed").into();

        assert_eq!(local.clone().into_bytes(), b"bytes");
        let boxed = shared.clone().into_boxed_str();
        assert_eq!(&*boxed, "boxed");
        assert_eq!(local.into_string(), "bytes");
        assert_eq!(shared.into_string(), "boxed");
    }

    #[test]
    fn cow_roundtrip() {
        let local: LocalRefStr<'_> = String::from("hello").into();
        let cow: Cow<'_, str> = local.into();

        assert_eq!(cow.as_ref(), "hello");
    }

    #[test]
    fn raw_roundtrip() {
        let original: Arc<str> = Arc::from("hello");
        let value = RefStr::from_shared(original.clone());
        let (raw_ptr, len, tag) = unsafe { RefStr::into_raw_parts(value) };

        assert_ne!(raw_ptr.as_ptr() as usize, 0);
        assert_eq!(len, 5);
        assert_eq!(tag, 1usize << (usize::BITS - 1));

        let value = unsafe { RefStr::from_raw_parts(raw_ptr, len, tag) };
        let raw = unsafe { RefStr::into_raw(value) };

        assert_eq!(Arc::strong_count(&original), 2);

        unsafe {
            RefStr::increment_strong_count(raw);
        }
        assert_eq!(Arc::strong_count(&original), 3);

        unsafe {
            drop(Arc::from_raw(raw));
        }
        assert_eq!(Arc::strong_count(&original), 2);
    }

    #[test]
    fn static_roundtrip() {
        let local = LocalRefStr::from_static("world");
        let shared = RefStr::from_static("world");
        let default_local: LocalRefStr<'_> = Default::default();
        let default_shared: RefStr<'_> = Default::default();

        assert_eq!(local.as_str(), "world");
        assert_eq!(shared.as_str(), "world");
        assert!(!local.is_shared());
        assert!(!shared.is_shared());
        assert!(default_local.is_borrowed());
        assert!(default_shared.is_borrowed());
        assert_eq!(default_local.as_str(), "");
        assert_eq!(default_shared.as_str(), "");
    }
}
