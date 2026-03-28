//! Internal core implementation for compressed string types.

use ::core::borrow::Borrow;
use ::core::cmp::Ordering;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::marker::PhantomData;
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use ::core::ptr;
use ::core::ptr::NonNull;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

const TAG_MASK: usize = 1usize << (usize::BITS - 1);
const LEN_MASK: usize = !TAG_MASK;

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

    /// Build the shared string handle from a `&str`.
    fn from_str(s: &str) -> Self::Shared;

    /// Build the shared string handle from a [`String`].
    fn from_string(s: String) -> Self::Shared;

    /// Build the shared string handle from a `Box<str>`
    fn from_boxed_str(s: Box<str>) -> Self::Shared;
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

    /// Build `Rc<str>` from a `&str`.
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

    /// Build `Arc<str>` from a `&str`.
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

/// Raw compact representation used to move encoded state through `const` APIs.
#[derive(Copy, Clone)]
pub(crate) struct RawParts {
    raw_ptr: NonNull<u8>,
    len_and_tag: usize,
}

impl RawParts {
    /// Build encoded parts from already-validated fields.
    pub(crate) const unsafe fn new(raw_ptr: NonNull<u8>, len: usize, tag: usize) -> Self {
        assert!(len <= LEN_MASK, "string too large to compress");
        Self {
            raw_ptr,
            len_and_tag: len | tag,
        }
    }

    /// Return the stored data pointer.
    pub(crate) const fn raw_ptr(self) -> NonNull<u8> {
        self.raw_ptr
    }

    /// Return the stored string length.
    pub(crate) const fn len(self) -> usize {
        self.len_and_tag & LEN_MASK
    }

    /// Return the stored tag bits.
    pub(crate) const fn tag(self) -> usize {
        self.len_and_tag & TAG_MASK
    }

    /// Return the packed `len | tag` word.
    pub(crate) const fn len_and_tag(self) -> usize {
        self.len_and_tag
    }

    /// Convert the parts into a raw `*const str`.
    pub(crate) const fn into_raw(self) -> *const str {
        ptr::slice_from_raw_parts(self.raw_ptr.as_ptr(), self.len()) as *const str
    }

    /// Expose the legacy `(ptr, len, tag)` tuple form.
    pub(crate) const fn into_tuple(self) -> (NonNull<u8>, usize, usize) {
        (self.raw_ptr(), self.len(), self.tag())
    }
}

/// Internal compact two-word string representation.
pub struct RefStrCore<'a, B: RefCountBackend> {
    /// Packed pointer to the string data.
    raw_ptr: NonNull<u8>,
    /// Packed length and tag bits.
    len_and_tag: usize,
    /// Carries the borrowed lifetime.
    _marker: PhantomData<&'a str>,
    /// Keeps the backend type in the type system.
    _backend: PhantomData<B>,
}

// Safety: `RefStrCore` only stores a pointer/len pair and defers shared-ownership
// synchronization guarantees to the backend handle type.
unsafe impl<'a, B> Send for RefStrCore<'a, B>
where
    B: RefCountBackend,
    B::Shared: Send,
{
}

// Safety: sharing references to `RefStrCore` is sound whenever the backend
// shared handle is itself `Sync`.
unsafe impl<'a, B> Sync for RefStrCore<'a, B>
where
    B: RefCountBackend,
    B::Shared: Sync,
{
}

impl<'a, B: RefCountBackend> RefStrCore<'a, B> {
    /// Bit used to distinguish shared and borrowed states.
    ///
    /// This tag is stored in `len_and_tag`, never in the pointer itself.
    const TAG_MASK: usize = TAG_MASK;
    /// Mask for the length payload.
    const LEN_MASK: usize = LEN_MASK;
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

    /// Build a shared value from string-like input.
    #[inline]
    pub fn from_owned_like<R: AsRef<str>>(s: R) -> Self {
        Self::from_shared(B::from_str(s.as_ref()))
    }

    /// Build from raw parts.
    #[inline]
    pub const unsafe fn from_raw_parts(raw_ptr: NonNull<u8>, len: usize, tag: usize) -> Self {
        unsafe { Self::from_raw_parts_struct(RawParts::new(raw_ptr, len, tag)) }
    }

    /// Build from encoded raw parts.
    #[inline]
    pub(crate) const unsafe fn from_raw_parts_struct(parts: RawParts) -> Self {
        Self {
            raw_ptr: parts.raw_ptr(),
            len_and_tag: parts.len_and_tag(),
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
        unsafe { self.into_raw_parts_struct().into_tuple() }
    }

    /// Split the value into encoded raw parts.
    pub(crate) const unsafe fn into_raw_parts_struct(self) -> RawParts {
        let this = ManuallyDrop::new(self);
        unsafe { Self::raw_parts_from_manuallydrop_ptr(&this as *const ManuallyDrop<Self>) }
    }

    /// Read encoded raw parts from a `ManuallyDrop<Self>` pointer.
    pub(crate) const unsafe fn raw_parts_from_manuallydrop_ptr(
        this: *const ManuallyDrop<Self>,
    ) -> RawParts {
        let this_ptr = this as *const Self;
        unsafe {
            RawParts {
                raw_ptr: ptr::read(ptr::addr_of!((*this_ptr).raw_ptr)),
                len_and_tag: ptr::read(ptr::addr_of!((*this_ptr).len_and_tag)),
            }
        }
    }

    /// Convert the value into a raw `*const str`.
    ///
    /// # Safety
    ///
    /// The returned pointer is ambiguous: it may represent either borrowed
    /// string data or backend-managed shared storage.
    ///
    /// Only pointers originating from a shared value may be used with backend
    /// strong-count operations or reconstructed as backend shared handles.
    ///
    /// If you need to branch on ownership state, prefer
    /// [`into_raw_parts`](Self::into_raw_parts) or
    /// [`into_raw_shared`](Self::into_raw_shared).
    pub const unsafe fn into_raw(self) -> *const str {
        unsafe { self.into_raw_parts_struct().into_raw() }
    }

    /// Convert into a raw pointer only when the value is shared.
    ///
    /// Returns `None` for borrowed values, avoiding ambiguous raw pointers in
    /// mixed borrowed/shared code paths.
    pub fn into_raw_shared(self) -> Option<*const str> {
        if self.is_shared() {
            Some(unsafe { self.into_raw() })
        } else {
            None
        }
    }

    /// Increment the strong count for a raw `*const str`.
    ///
    /// # Safety
    ///
    /// `ptr` must come from a shared value of this backend. Passing a pointer
    /// derived from a borrowed value is undefined behavior.
    pub unsafe fn increment_strong_count(ptr: *const str) {
        unsafe {
            B::increment_strong_count(ptr);
        }
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
            let slice = ::core::slice::from_raw_parts(self.data_ptr(), self.len());
            ::core::str::from_utf8_unchecked(slice)
        }
    }

    /// Convert the string into owned bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.as_str().as_bytes().to_vec()
    }

    /// Convert the string into a boxed string slice.
    pub fn into_boxed_str(self) -> Box<str> {
        Box::from(self.as_str())
    }

    /// Convert the string into `String`.
    pub fn into_string(self) -> String {
        String::from(self.as_str())
    }

    pub fn into_cow(self) -> Cow<'a, str> {
        if self.is_shared() {
            Cow::Owned(self.into_string())
        } else {
            unsafe { Cow::Borrowed(self.into_str_unchecked()) }
        }
    }

    /// Convert the string into `&str` without checking if it's borrowed.
    pub unsafe fn into_str_unchecked(self) -> &'a str {
        debug_assert!(
            self.is_borrowed(),
            "into_str_unchecked requires a borrowed value"
        );
        let (raw_ptr, len, _) = unsafe { self.into_raw_parts() };
        unsafe { &*(ptr::slice_from_raw_parts(raw_ptr.as_ptr(), len) as *const str) }
    }

    /// Convert the string into a `Cow<str>`.
    ///
    /// Borrowed values stay borrowed. Shared values allocate a new owned
    /// string because `Cow<'a, str>` cannot borrow from `Rc<str>` or `Arc<str>`.
    pub fn as_cow(&self) -> Cow<'a, str> {
        if self.is_shared() {
            Cow::Owned(self.to_string())
        } else {
            // Extend borrow to `'a` without tying it to `&self` lifetime.
            unsafe { Cow::Borrowed(self.clone().into_str_unchecked()) }
        }
    }
}

impl<B: RefCountBackend> RefStrCore<'static, B> {
    /// Build from a static `&str`.
    pub fn from_static(s: &'static str) -> Self {
        Self::from_str(s)
    }

    /// Return the borrowed `'static` string when this value is not shared.
    pub fn borrowed_static_str(&self) -> Option<&'static str> {
        if self.is_shared() {
            None
        } else {
            let slice = ptr::slice_from_raw_parts(self.data_ptr(), self.len()) as *const str;
            Some(unsafe { &*slice })
        }
    }
}

impl<'a, B: RefCountBackend> From<&'a str> for RefStrCore<'a, B> {
    /// Build from a borrowed `&str`.
    fn from(value: &'a str) -> Self {
        Self::new(value)
    }
}

impl<'a, B: RefCountBackend> From<&'a String> for RefStrCore<'a, B> {
    /// Build from a borrowed `String`.
    fn from(value: &'a String) -> Self {
        Self::from(value.as_str())
    }
}

impl<'a> From<Rc<str>> for RefStrCore<'a, LocalBackend> {
    /// Build a local string from `Rc<str>`.
    fn from(value: Rc<str>) -> Self {
        Self::from_shared(value)
    }
}

impl<'a> From<Arc<str>> for RefStrCore<'a, SharedBackend> {
    /// Build a shared string from `Arc<str>`.
    fn from(value: Arc<str>) -> Self {
        Self::from_shared(value)
    }
}

impl<'a> From<RefStrCore<'a, LocalBackend>> for RefStrCore<'a, SharedBackend> {
    /// Convert a local string into a shared string.
    fn from(value: RefStrCore<'a, LocalBackend>) -> Self {
        if value.is_shared() {
            Self::from_shared(Arc::from(value.as_str()))
        } else {
            let (raw_ptr, len, tag) = unsafe { value.into_raw_parts() };
            unsafe { Self::from_raw_parts(raw_ptr, len, tag) }
        }
    }
}

impl<'a> From<RefStrCore<'a, SharedBackend>> for RefStrCore<'a, LocalBackend> {
    /// Convert a shared string into a local string.
    fn from(value: RefStrCore<'a, SharedBackend>) -> Self {
        if value.is_shared() {
            Self::from_shared(Rc::from(value.as_str()))
        } else {
            let (raw_ptr, len, tag) = unsafe { value.into_raw_parts() };
            unsafe { Self::from_raw_parts(raw_ptr, len, tag) }
        }
    }
}

impl<'a, B: RefCountBackend> From<Box<str>> for RefStrCore<'a, B> {
    /// Build a local string from `Box<str>`.
    fn from(value: Box<str>) -> Self {
        Self::from_shared(B::from_boxed_str(value))
    }
}

impl<'a, B: RefCountBackend> From<String> for RefStrCore<'a, B> {
    /// Build a local string from `String`.
    fn from(value: String) -> Self {
        Self::from_shared(B::from_string(value))
    }
}

impl<'a, B: RefCountBackend> From<RefStrCore<'a, B>> for Cow<'a, str> {
    /// Convert into a borrowed-or-owned `Cow<str>`.
    fn from(value: RefStrCore<'a, B>) -> Self {
        value.into_cow()
    }
}

impl<'a, B: RefCountBackend> From<Cow<'a, str>> for RefStrCore<'a, B> {
    /// Build a local string from `Cow<str>`.
    fn from(value: Cow<'a, str>) -> Self {
        match value {
            Cow::Borrowed(s) => Self::from(s),
            Cow::Owned(s) => Self::from(s),
        }
    }
}

impl<'a, B: RefCountBackend> Default for RefStrCore<'a, B> {
    /// Create an empty borrowed string.
    fn default() -> Self {
        Self::from_str("")
    }
}

impl<'a, B: RefCountBackend> AsRef<str> for RefStrCore<'a, B> {
    /// Borrow as `&str`.
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> Borrow<str> for RefStrCore<'a, B> {
    /// Borrow as `&str`.
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> PartialEq for RefStrCore<'a, B> {
    /// Compare by string contents.
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<'a, B: RefCountBackend> Eq for RefStrCore<'a, B> {}

impl<'a, B: RefCountBackend> PartialEq<&str> for RefStrCore<'a, B> {
    /// Compare against `&str`.
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<'a, B: RefCountBackend> PartialEq<String> for RefStrCore<'a, B> {
    /// Compare against `String`.
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<'a, B: RefCountBackend> PartialOrd for RefStrCore<'a, B> {
    /// Compare lexicographically.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a, B: RefCountBackend> Ord for RefStrCore<'a, B> {
    /// Compare lexicographically.
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<'a, B: RefCountBackend> Hash for RefStrCore<'a, B> {
    /// Hash the string contents.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl<'a, B: RefCountBackend> Deref for RefStrCore<'a, B> {
    type Target = str;

    /// Dereference to `str`.
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> fmt::Debug for RefStrCore<'a, B> {
    /// Format as a debug tuple.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let state = if self.is_shared() {
                "Shared"
            } else {
                "Borrowed"
            };
            f.debug_struct("RefStrCore")
                .field("state", &state)
                .field("len", &self.len())
                .field("value", &self.as_str())
                .finish()
        } else {
            f.debug_tuple("RefStrCore").field(&self.as_str()).finish()
        }
    }
}

impl<'a, B: RefCountBackend> fmt::Display for RefStrCore<'a, B> {
    /// Format the string contents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'a, B: RefCountBackend> Clone for RefStrCore<'a, B> {
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

impl<'a, B: RefCountBackend> Drop for RefStrCore<'a, B> {
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
    use crate::{LocalRefStr, LocalStaticRefStr, RefStr, StaticRefStr};
    use alloc::borrow::Cow;
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::string::{String, ToString};
    use alloc::sync::Arc;

    #[cfg(feature = "arbitrary")]
    use arbitrary::{Arbitrary, Unstructured};

    #[cfg(feature = "serde")]
    use serde::Deserialize;
    #[cfg(feature = "serde")]
    use serde::de::value::{BorrowedStrDeserializer, Error as DeError, StringDeserializer};
    #[cfg(feature = "serde")]
    use serde_test::{Token, assert_de_tokens, assert_ser_tokens};

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
        let local = LocalRefStr::from_str("world");
        let shared = RefStr::from_str("world");
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

    #[test]
    fn dedicated_static_wrapper_roundtrip() {
        let local = LocalStaticRefStr::from_static("world");
        let shared = StaticRefStr::from_static("world");
        let default_local: LocalStaticRefStr = Default::default();
        let default_shared: StaticRefStr = Default::default();

        assert_eq!(local.as_str(), "world");
        assert_eq!(shared.as_str(), "world");
        assert!(!local.is_shared());
        assert!(!shared.is_shared());
        assert!(default_local.is_borrowed());
        assert!(default_shared.is_borrowed());
        assert_eq!(default_local.as_str(), "");
        assert_eq!(default_shared.as_str(), "");
    }

    #[cfg(feature = "arbitrary")]
    #[test]
    fn arbitrary_roundtrip() {
        fn assert_case(bytes: &[u8], expected_shared: bool) {
            let mut expected_local_u = Unstructured::new(bytes);
            let expected_value = <&str>::arbitrary(&mut expected_local_u).unwrap();
            let expected_flag = bool::arbitrary(&mut expected_local_u).unwrap();

            let mut actual_local = Unstructured::new(bytes);
            let local = LocalRefStr::arbitrary(&mut actual_local).unwrap();

            assert_eq!(local.as_str(), expected_value);
            assert_eq!(local.is_shared(), expected_flag);
            assert_eq!(local.is_shared(), expected_shared);
            assert_eq!(local.is_borrowed(), !expected_flag);

            let mut expected_shared_u = Unstructured::new(bytes);
            let expected_value = <&str>::arbitrary(&mut expected_shared_u).unwrap();
            let expected_flag = bool::arbitrary(&mut expected_shared_u).unwrap();

            let mut actual_shared = Unstructured::new(bytes);
            let shared = RefStr::arbitrary(&mut actual_shared).unwrap();

            assert_eq!(shared.as_str(), expected_value);
            assert_eq!(shared.is_shared(), expected_flag);
            assert_eq!(shared.is_shared(), expected_shared);
            assert_eq!(shared.is_borrowed(), !expected_flag);
        }

        assert_case(b"hello\x01\x05", true);
        assert_case(b"hello\x00\x05", false);
    }

    #[cfg(feature = "arbitrary")]
    #[test]
    fn static_arbitrary_is_always_shared() {
        let mut local = Unstructured::new(b"hello\x01\x05");
        let mut shared = Unstructured::new(b"world\x00\x05");

        let local_value = LocalStaticRefStr::arbitrary(&mut local).unwrap();
        let shared_value = StaticRefStr::arbitrary(&mut shared).unwrap();

        assert_eq!(local_value.as_str(), "hello");
        assert_eq!(shared_value.as_str(), "world");
        assert!(local_value.is_shared());
        assert!(shared_value.is_shared());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let local = LocalRefStr::from("serde");
        let shared = RefStr::from("serde");

        assert_ser_tokens(&local, &[Token::Str("serde")]);
        assert_ser_tokens(&shared, &[Token::Str("serde")]);

        let local_borrowed: LocalRefStr<'_> =
            Deserialize::deserialize(BorrowedStrDeserializer::<DeError>::new("serde")).unwrap();
        let shared_borrowed: RefStr<'_> =
            Deserialize::deserialize(BorrowedStrDeserializer::<DeError>::new("serde")).unwrap();
        let local_owned: LocalRefStr<'_> =
            Deserialize::deserialize(StringDeserializer::<DeError>::new(String::from("serde")))
                .unwrap();
        let shared_owned: RefStr<'_> =
            Deserialize::deserialize(StringDeserializer::<DeError>::new(String::from("serde")))
                .unwrap();

        assert!(local_borrowed.is_borrowed());
        assert!(shared_borrowed.is_borrowed());
        assert!(local_owned.is_shared());
        assert!(shared_owned.is_shared());

        assert_de_tokens(&local_borrowed, &[Token::BorrowedStr("serde")]);
        assert_de_tokens(&shared_borrowed, &[Token::BorrowedStr("serde")]);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn static_serde_roundtrip() {
        let local = LocalStaticRefStr::from("serde");
        let shared = StaticRefStr::from("serde");

        assert_ser_tokens(&local, &[Token::Str("serde")]);
        assert_ser_tokens(&shared, &[Token::Str("serde")]);

        let local_borrowed: LocalStaticRefStr =
            Deserialize::deserialize(BorrowedStrDeserializer::<DeError>::new("serde")).unwrap();
        let shared_borrowed: StaticRefStr =
            Deserialize::deserialize(BorrowedStrDeserializer::<DeError>::new("serde")).unwrap();
        let local_owned: LocalStaticRefStr =
            Deserialize::deserialize(StringDeserializer::<DeError>::new(String::from("serde")))
                .unwrap();
        let shared_owned: StaticRefStr =
            Deserialize::deserialize(StringDeserializer::<DeError>::new(String::from("serde")))
                .unwrap();

        assert!(local_borrowed.is_shared());
        assert!(shared_borrowed.is_shared());
        assert!(local_owned.is_shared());
        assert!(shared_owned.is_shared());
    }
}
