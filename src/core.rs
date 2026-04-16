//! Internal core implementation for compressed string types.
//!
//! This module contains the backend-agnostic compact string engine. It is the
//! layer that decides when a value stays borrowed, becomes inline, or is stored
//! in shared heap-backed form.

pub use crate::backend::{LocalBackend, RefCountBackend, SharedBackend};

use ::core::borrow::Borrow;
use ::core::cmp::Ordering;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::marker::PhantomData;
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use ::core::ptr;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::RawParts;
use crate::arch::encode;
use crate::arch::layout;
use crate::arch::layout::StateTag;

/// Internal compact two-word string representation.
///
/// The payload itself is only two machine words wide. The type parameters keep
/// the borrowed lifetime and the backend choice visible to the compiler.
#[repr(C)]
pub struct RefStrCore<'a, B: RefCountBackend> {
    /// Compact raw representation.
    parts: RawParts,
    /// Carries the borrowed lifetime.
    _marker: PhantomData<&'a str>,
    /// Keeps the backend type in the type system.
    _backend: PhantomData<B>,
}

unsafe impl<'a, B> Send for RefStrCore<'a, B>
where
    B: RefCountBackend,
    B::Shared: Send,
{
}

unsafe impl<'a, B> Sync for RefStrCore<'a, B>
where
    B: RefCountBackend,
    B::Shared: Sync,
{
}

impl<'a, B: RefCountBackend> RefStrCore<'a, B> {
    /// Construct from a borrowed string.
    #[inline]
    pub const fn new(s: &'a str) -> Self {
        Self::from_str(s)
    }

    /// Construct from a borrowed string and cache state derived from it.
    #[inline]
    pub const fn from_str(s: &'a str) -> Self {
        let raw_ptr = s.as_ptr();
        let hash = encode::short_hash(s.as_bytes());
        let mut meta = encode::encode_len_tag_hash(s.len(), StateTag::Borrowed, hash);
        if s.is_ascii() {
            meta |= layout::IS_ASCII_MASK;
        }

        unsafe { Self::from_raw_parts_struct(RawParts::new(raw_ptr, meta)) }
    }

    /// Construct from any string-like input, preferring inline storage.
    #[inline]
    pub fn from_owned_like<R: AsRef<str>>(s: R) -> Self {
        let s = s.as_ref();

        if encode::supports_inline_len(s.len()) {
            Self::new_inline(s)
        } else {
            Self::from_shared(B::from_str(s))
        }
    }

    /// Rebuild a core value from raw parts.
    ///
    /// # Safety
    ///
    /// The `parts` value must originate from this crate and match the backend
    /// and lifetime contract of the target type.
    #[inline]
    pub const unsafe fn from_raw_parts(parts: RawParts) -> Self {
        unsafe { Self::from_raw_parts_struct(parts) }
    }

    #[inline]
    pub(crate) const unsafe fn from_raw_parts_struct(parts: RawParts) -> Self {
        Self {
            parts,
            _marker: PhantomData,
            _backend: PhantomData,
        }
    }

    /// Construct from backend-owned shared storage.
    pub fn from_shared(s: B::Shared) -> Self {
        let len = s.deref().len();
        let is_ascii = s.deref().is_ascii();
        let hash = encode::short_hash(s.deref().as_bytes());
        let raw = B::into_raw(s);
        let raw_ptr = raw as *const u8;
        let mut meta = encode::encode_len_tag_hash(len, StateTag::Shared, hash);
        if is_ascii {
            meta |= layout::IS_ASCII_MASK;
        }

        unsafe { Self::from_raw_parts_struct(RawParts::new(raw_ptr, meta)) }
    }

    /// Construct from an owned `String`, using inline storage when possible.
    fn from_owned_string(value: String) -> Self {
        if encode::supports_inline_len(value.len()) {
            Self::new_inline(value.as_str())
        } else {
            Self::from_shared(B::from_string(value))
        }
    }

    /// Construct from an owned `Box<str>`, using inline storage when possible.
    fn from_owned_boxed_str(value: Box<str>) -> Self {
        if encode::supports_inline_len(value.len()) {
            Self::new_inline(value.as_ref())
        } else {
            Self::from_shared(B::from_boxed_str(value))
        }
    }

    /// Construct an inline string value.
    fn new_inline(s: &str) -> Self {
        unsafe { Self::from_raw_parts_struct(RawParts::pack_inline(s)) }
    }

    /// Decompose the value into its raw transport representation.
    ///
    /// # Safety
    ///
    /// Any shared payload's ownership responsibility moves to the caller.
    pub const unsafe fn into_raw_parts(self) -> RawParts {
        unsafe { self.into_raw_parts_struct() }
    }

    pub(crate) const unsafe fn into_raw_parts_struct(self) -> RawParts {
        #[repr(C)]
        union View<'a, B: RefCountBackend> {
            core: ManuallyDrop<RefStrCore<'a, B>>,
            parts: RawParts,
        }

        let view = View {
            core: ManuallyDrop::new(self),
        };

        unsafe { view.parts }
    }

    /// Convert into a raw `*const str`.
    ///
    /// Inline values are first promoted to shared storage.
    ///
    /// # Safety
    ///
    /// The result may point to borrowed data or shared backend storage.
    pub unsafe fn into_raw(self) -> *const str {
        let parts = self.parts;
        if parts.is_inline() {
            B::into_raw(B::from_str(self.as_str()))
        } else {
            unsafe { self.into_raw_parts_struct().into_raw_non_inline() }
        }
    }

    /// Convert into a raw pointer only if the payload is already shared.
    pub fn into_raw_shared(self) -> Option<*const str> {
        let parts = self.parts;
        if parts.is_shared() {
            Some(unsafe { self.into_raw_parts_struct().into_raw_non_inline() })
        } else {
            None
        }
    }

    /// Increment the strong count for a backend-owned shared pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must come from the same backend and still reference a live
    /// allocation.
    pub unsafe fn increment_strong_count(ptr: *const str) {
        unsafe {
            B::increment_strong_count(ptr);
        }
    }

    #[inline]
    const fn state_tag(&self) -> StateTag {
        self.parts.tag()
    }

    #[inline]
    pub const fn is_shared(&self) -> bool {
        self.parts.is_shared()
    }

    #[inline]
    pub const fn is_borrowed(&self) -> bool {
        self.parts.is_borrowed()
    }

    #[inline]
    pub const fn is_inline(&self) -> bool {
        self.parts.is_inline()
    }

    #[inline]
    pub const fn is_ascii(&self) -> bool {
        self.parts.is_ascii()
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.parts.len()
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    /// Borrow the contents as `&str`.
    pub fn as_str(&self) -> &str {
        unsafe {
            let parts = self.parts;
            let meta = parts.meta();
            let (raw_ptr, len) = if (meta & layout::INLINE_MASK) != 0 {
                (
                    self as *const Self as *const u8,
                    encode::inline_len_from_meta(meta),
                )
            } else {
                (parts.as_ptr(), encode::decode_borrowed_or_shared_len(meta))
            };
            let slice = ::core::slice::from_raw_parts(raw_ptr, len);
            ::core::str::from_utf8_unchecked(slice)
        }
    }

    /// Convert a possibly borrowed value into an owned `'static` core.
    pub fn to_static_core(&self) -> RefStrCore<'static, B> {
        match self.state_tag() {
            StateTag::Shared => {
                let cloned = self.clone();
                let parts = unsafe { cloned.into_raw_parts() };
                unsafe { RefStrCore::from_raw_parts(parts) }
            }
            StateTag::Inline => unsafe {
                RefStrCore::from_raw_parts(self.clone().into_raw_parts())
            },
            StateTag::Borrowed => {
                if encode::supports_inline_len(self.len()) {
                    RefStrCore::new_inline(self.as_str())
                } else {
                    RefStrCore::from_shared(B::from_str(self.as_str()))
                }
            }
        }
    }

    /// Convert this value into an owned `'static` core.
    pub fn into_static_core(self) -> RefStrCore<'static, B> {
        match self.state_tag() {
            StateTag::Shared | StateTag::Inline => {
                let parts = unsafe { self.into_raw_parts() };
                unsafe { RefStrCore::from_raw_parts(parts) }
            }
            StateTag::Borrowed => {
                if encode::supports_inline_len(self.len()) {
                    RefStrCore::new_inline(self.as_str())
                } else {
                    RefStrCore::from_shared(B::from_str(self.as_str()))
                }
            }
        }
    }

    /// Consume the value and return its UTF-8 bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        let parts = self.parts;
        let len = parts.len();
        let ptr = if parts.is_inline() {
            &self as *const Self as *const u8
        } else {
            parts.as_ptr()
        };

        unsafe { ::core::slice::from_raw_parts(ptr, len).to_vec() }
    }

    /// Consume the value and return `Box<str>`.
    pub fn into_boxed_str(self) -> Box<str> {
        Box::from(self.as_str())
    }

    /// Consume the value and return `String`.
    pub fn into_string(self) -> String {
        String::from(self.as_str())
    }

    /// Consume the value and return `Cow<str>`.
    pub fn into_cow(self) -> Cow<'a, str> {
        if self.is_borrowed() {
            unsafe { Cow::Borrowed(self.into_str_unchecked()) }
        } else {
            Cow::Owned(self.into_string())
        }
    }

    #[inline]
    const unsafe fn borrowed_str_unchecked(&self) -> &'a str {
        let parts = self.parts;
        let meta = parts.meta();
        let slice =
            ptr::slice_from_raw_parts(parts.as_ptr(), encode::decode_borrowed_or_shared_len(meta))
                as *const str;

        unsafe { &*slice }
    }

    /// Convert into `&'a str` without checking the state tag.
    ///
    /// # Safety
    ///
    /// The value must currently store a borrowed string.
    pub const unsafe fn into_str_unchecked(self) -> &'a str {
        debug_assert!(
            self.is_borrowed(),
            "into_str_unchecked requires a borrowed value"
        );

        let this = ManuallyDrop::new(self);
        let this_ptr = &this as *const ManuallyDrop<Self> as *const Self;

        unsafe { (&*this_ptr).borrowed_str_unchecked() }
    }

    /// Borrow as `Cow<str>`, avoiding allocation for borrowed values.
    pub fn as_cow(&self) -> Cow<'a, str> {
        if self.is_borrowed() {
            unsafe { Cow::Borrowed(self.borrowed_str_unchecked()) }
        } else {
            Cow::Owned(self.to_string())
        }
    }
}

impl<B: RefCountBackend> RefStrCore<'static, B> {
    /// Construct a `'static` core from a borrowed `'static` string.
    pub fn from_static(s: &'static str) -> Self {
        Self::from_str(s)
    }

    /// Return the borrowed `'static` string if this value is borrowed.
    pub fn borrowed_static_str(&self) -> Option<&'static str> {
        let parts = self.parts;
        if parts.is_borrowed() {
            Some(unsafe { self.borrowed_str_unchecked() })
        } else {
            None
        }
    }
}

impl<'a, B: RefCountBackend> From<&'a str> for RefStrCore<'a, B> {
    fn from(value: &'a str) -> Self {
        Self::new(value)
    }
}

impl<'a, B: RefCountBackend> From<&'a String> for RefStrCore<'a, B> {
    fn from(value: &'a String) -> Self {
        Self::from(value.as_str())
    }
}

impl<'a> From<Rc<str>> for RefStrCore<'a, LocalBackend> {
    fn from(value: Rc<str>) -> Self {
        Self::from_shared(value)
    }
}

impl<'a> From<Arc<str>> for RefStrCore<'a, SharedBackend> {
    fn from(value: Arc<str>) -> Self {
        Self::from_shared(value)
    }
}

impl<'a> From<RefStrCore<'a, LocalBackend>> for RefStrCore<'a, SharedBackend> {
    fn from(value: RefStrCore<'a, LocalBackend>) -> Self {
        if value.is_shared() {
            Self::from_shared(Arc::from(value.as_str()))
        } else {
            let parts = unsafe { value.into_raw_parts() };
            unsafe { Self::from_raw_parts(parts) }
        }
    }
}

impl<'a> From<RefStrCore<'a, SharedBackend>> for RefStrCore<'a, LocalBackend> {
    fn from(value: RefStrCore<'a, SharedBackend>) -> Self {
        if value.is_shared() {
            Self::from_shared(Rc::from(value.as_str()))
        } else {
            let parts = unsafe { value.into_raw_parts() };
            unsafe { Self::from_raw_parts(parts) }
        }
    }
}

impl<'a, B: RefCountBackend> From<Box<str>> for RefStrCore<'a, B> {
    fn from(value: Box<str>) -> Self {
        Self::from_owned_boxed_str(value)
    }
}

impl<'a, B: RefCountBackend> From<String> for RefStrCore<'a, B> {
    fn from(value: String) -> Self {
        Self::from_owned_string(value)
    }
}

impl<'a, B: RefCountBackend> From<RefStrCore<'a, B>> for Cow<'a, str> {
    fn from(value: RefStrCore<'a, B>) -> Self {
        value.into_cow()
    }
}

impl<'a, B: RefCountBackend> From<Cow<'a, str>> for RefStrCore<'a, B> {
    fn from(value: Cow<'a, str>) -> Self {
        match value {
            Cow::Borrowed(s) => Self::from(s),
            Cow::Owned(s) => Self::from(s),
        }
    }
}

impl<'a, B: RefCountBackend> Default for RefStrCore<'a, B> {
    fn default() -> Self {
        Self::from_str("")
    }
}

impl<'a, B: RefCountBackend> AsRef<str> for RefStrCore<'a, B> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> Borrow<str> for RefStrCore<'a, B> {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> PartialEq for RefStrCore<'a, B> {
    fn eq(&self, other: &Self) -> bool {
        if self.parts.raw_ptr() == other.parts.raw_ptr() && self.parts.meta() == other.parts.meta()
        {
            return true;
        }

        if self.is_inline() || other.is_inline() {
            return self.as_str() == other.as_str();
        }

        if self.parts.cached_hash() != other.parts.cached_hash() {
            return false;
        }

        if self.len() != other.len() {
            return false;
        }

        self.as_str() == other.as_str()
    }
}

impl<'a, B: RefCountBackend> Eq for RefStrCore<'a, B> {}

impl<'a, B: RefCountBackend> PartialEq<&str> for RefStrCore<'a, B> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<'a, B: RefCountBackend> PartialEq<String> for RefStrCore<'a, B> {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<'a, B: RefCountBackend> PartialOrd for RefStrCore<'a, B> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a, B: RefCountBackend> Ord for RefStrCore<'a, B> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<'a, B: RefCountBackend> Hash for RefStrCore<'a, B> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl<'a, B: RefCountBackend> Deref for RefStrCore<'a, B> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<'a, B: RefCountBackend> fmt::Debug for RefStrCore<'a, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let state = match self.state_tag() {
                StateTag::Borrowed => "Borrowed",
                StateTag::Shared => "Shared",
                StateTag::Inline => "Inline",
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'a, B: RefCountBackend> Clone for RefStrCore<'a, B> {
    fn clone(&self) -> Self {
        let parts = self.parts;
        let meta = parts.meta();
        if (meta & layout::NEEDS_DROP_MASK) != 0 {
            let len = encode::decode_borrowed_or_shared_len(meta);
            let fat_ptr = ptr::slice_from_raw_parts(parts.as_ptr(), len) as *const str;
            unsafe {
                B::increment_strong_count(fat_ptr);
            }
        }

        Self {
            parts,
            _marker: PhantomData,
            _backend: PhantomData,
        }
    }
}

impl<'a, B: RefCountBackend> Drop for RefStrCore<'a, B> {
    fn drop(&mut self) {
        let parts = self.parts;
        let meta = parts.meta();
        if (meta & layout::NEEDS_DROP_MASK) != 0 {
            let len = encode::decode_borrowed_or_shared_len(meta);
            let fat_ptr = ptr::slice_from_raw_parts(parts.as_ptr(), len) as *const str;
            unsafe {
                drop(B::from_raw(fat_ptr));
            }
        }
    }
}
