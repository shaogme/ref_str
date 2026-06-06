//! Internal core implementation for compressed string types.
//!
//! This module contains the backend-agnostic compact string engine. It is the
//! layer that decides when a value stays borrowed, becomes inline, or is stored
//! in shared heap-backed form.

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
use crate::arch::{self, StateTag};

mod ops;

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
        let hash = arch::short_hash(s.as_bytes());
        let mut meta = arch::encode_len_tag_hash(s.len(), StateTag::Borrowed, hash);
        if s.is_ascii() {
            meta |= arch::IS_ASCII_MASK;
        }

        unsafe { Self::from_raw_parts_struct(RawParts::new(raw_ptr, meta)) }
    }

    /// Construct from any string-like input, preferring inline storage.
    #[inline]
    pub fn from_owned_like<R: AsRef<str>>(s: R) -> Self {
        let s = s.as_ref();

        if arch::supports_inline_len(s.len()) {
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
        let hash = arch::short_hash(s.deref().as_bytes());
        let raw = B::into_raw(s);
        let raw_ptr = raw as *const u8;
        let mut meta = arch::encode_len_tag_hash(len, StateTag::Shared, hash);
        if is_ascii {
            meta |= arch::IS_ASCII_MASK;
        }

        unsafe { Self::from_raw_parts_struct(RawParts::new(raw_ptr, meta)) }
    }

    /// Construct from an owned `String`, using inline storage when possible.
    fn from_owned_string(value: String) -> Self {
        if arch::supports_inline_len(value.len()) {
            Self::new_inline(value.as_str())
        } else {
            Self::from_shared(B::from_string(value))
        }
    }

    /// Construct from an owned `Box<str>`, using inline storage when possible.
    fn from_owned_boxed_str(value: Box<str>) -> Self {
        if arch::supports_inline_len(value.len()) {
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
            let (raw_ptr, len) = if (meta & arch::INLINE_MASK) != 0 {
                (
                    self as *const Self as *const u8,
                    arch::inline_len_from_meta(meta),
                )
            } else {
                (parts.as_ptr(), arch::decode_borrowed_or_shared_len(meta))
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
                if arch::supports_inline_len(self.len()) {
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
                if arch::supports_inline_len(self.len()) {
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
            ptr::slice_from_raw_parts(parts.as_ptr(), arch::decode_borrowed_or_shared_len(meta))
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
