//! Compact borrowed-or-shared string types for `no_std` Rust.
//!
//! `ref_str` stores either:
//! - a borrowed `&str`, or
//! - a shared owned string backed by [`Arc<str>`] or [`Rc<str>`]
//!
//! The public wrappers sit on top of a compact two-word core representation, so
//! they are useful when you want to preserve borrowing when possible while still
//! supporting cheap clones of owned values.
//!
//! # Type Families
//!
//! [`RefStr<'a>`] and [`StaticRefStr`] use [`Arc<str>`] for shared ownership and
//! are appropriate when values may cross thread boundaries.
//!
//! [`LocalRefStr<'a>`] and [`LocalStaticRefStr`] use [`Rc<str>`] instead, which
//! avoids atomic reference counting in single-threaded contexts.
//!
//! The lifetime-parameterized wrappers can preserve borrowed data during
//! deserialization. The dedicated `'static` wrappers always deserialize into
//! shared owned storage, even when the input format exposes borrowed strings.
//!
//! # Borrowed vs Shared
//!
//! Each value is always in exactly one of two states:
//! - borrowed, exposed by [`is_borrowed`](RefStr::is_borrowed)
//! - shared, exposed by [`is_shared`](RefStr::is_shared)
//!
//! Borrowed values preserve the original lifetime and can be recovered as `&str`
//! or `Cow<'a, str>` without allocation. Shared values own their backing
//! allocation through `Arc<str>` or `Rc<str>`, and cloning them only bumps the
//! reference count.
//!
//! # Common Operations
//!
//! All four public wrappers expose the same core operations:
//! - constructors such as [`new`](RefStr::new), [`from_str`](RefStr::from_str),
//!   [`from_owned_like`](RefStr::from_owned_like), [`from_shared`](RefStr::from_shared),
//!   and [`from_static`](StaticRefStr::from_static) for the dedicated static types
//! - state inspection via [`is_borrowed`](RefStr::is_borrowed),
//!   [`is_shared`](RefStr::is_shared), [`len`](RefStr::len), and
//!   [`is_empty`](RefStr::is_empty)
//! - string access through [`as_str`](RefStr::as_str), [`as_cow`](RefStr::as_cow),
//!   [`into_cow`](RefStr::into_cow), [`into_string`](RefStr::into_string),
//!   [`into_boxed_str`](RefStr::into_boxed_str), and [`into_bytes`](RefStr::into_bytes)
//! - content-based comparisons with `&str`, [`String`], [`Cow<'_, str>`][Cow],
//!   [`Rc<str>`], and [`Arc<str>`] through [`PartialEq`]
//!
//! # Allocation Notes
//!
//! [`as_cow`](RefStr::as_cow) is allocation-free only for borrowed values. When
//! the value is shared, `as_cow` returns `Cow::Owned` and clones the string
//! contents, because a `Cow<'a, str>` cannot borrow directly from `Rc<str>` or
//! `Arc<str>`.
//!
//! Conversions between [`LocalRefStr<'a>`] and [`RefStr<'a>`] preserve borrowed
//! values without allocation. Shared values must be re-materialized into the
//! target backend, so converting between the `Rc` and `Arc` families allocates
//! and copies the string contents.
//!
//! # Advanced APIs
//!
//! Advanced raw-pointer escape hatches are available through
//! [`into_raw_parts`](RefStr::into_raw_parts),
//! [`from_raw_parts`](RefStr::from_raw_parts), [`into_raw`](RefStr::into_raw),
//! [`into_raw_shared`](RefStr::into_raw_shared), and
//! [`increment_strong_count`](RefStr::increment_strong_count).
//!
//! These APIs are `unsafe`: callers must preserve the original backend,
//! ownership rules, and encoded tag values.
//!
//! [`into_raw`](RefStr::into_raw) is intentionally low-level: its returned
//! `*const str` may point to either borrowed data or shared backend storage. If
//! you need a raw pointer that is guaranteed to come from shared storage,
//! prefer [`into_raw_shared`](RefStr::into_raw_shared). Passing a borrowed
//! pointer from `into_raw` into
//! [`increment_strong_count`](RefStr::increment_strong_count) is undefined
//! behavior.
//!
//! # Examples
//!
//! ```rust
//! use ref_str::{RefStr, StaticRefStr};
//!
//! let borrowed = RefStr::from("hello");
//! assert!(borrowed.is_borrowed());
//! assert_eq!(borrowed.as_str(), "hello");
//!
//! let shared = RefStr::from(String::from("world"));
//! assert!(shared.is_shared());
//! assert_eq!(shared.clone().into_string(), "world");
//!
//! let borrowed_cow = borrowed.as_cow();
//! assert_eq!(borrowed_cow.as_ref(), "hello");
//!
//! let static_value = StaticRefStr::from_static("fixed");
//! assert!(static_value.is_borrowed());
//! ```

#![no_std]

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme_doctests {}

#[cfg(doctest)]
#[doc = include_str!("../README_CN.md")]
mod readme_cn_doctests {}

extern crate alloc;

use ::core::borrow::Borrow;
use ::core::cmp::Ordering;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use ::core::ptr;
use ::core::ptr::NonNull;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;

#[cfg(feature = "arbitrary")]
use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};

#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod core;

/// A compact string that is either borrowed for `'a` or shared via [`Arc<str>`].
///
/// Use this type when you want to preserve borrowing when possible, but still
/// accept owned strings and clone them cheaply through atomic reference
/// counting.
#[repr(transparent)]
pub struct RefStr<'a>(ManuallyDrop<crate::core::RefStrCore<'a, crate::core::SharedBackend>>);

/// A compact string that is either borrowed for `'a` or shared via [`Rc<str>`].
///
/// This is the single-threaded counterpart to [`RefStr`]. It has the same API
/// shape, but shared ownership uses non-atomic reference counting.
#[repr(transparent)]
pub struct LocalRefStr<'a>(ManuallyDrop<crate::core::RefStrCore<'a, crate::core::LocalBackend>>);

/// A `'static` compact string that is either borrowed or shared via [`Arc<str>`].
///
/// Borrowed instances hold `&'static str`. Owned inputs are promoted into shared
/// storage so the resulting value remains `'static`.
#[repr(transparent)]
pub struct StaticRefStr(ManuallyDrop<crate::core::RefStrCore<'static, crate::core::SharedBackend>>);

/// A `'static` compact string that is either borrowed or shared via [`Rc<str>`].
///
/// This is the single-threaded counterpart to [`StaticRefStr`].
#[repr(transparent)]
pub struct LocalStaticRefStr(
    ManuallyDrop<crate::core::RefStrCore<'static, crate::core::LocalBackend>>,
);

macro_rules! impl_ref_str_common {
    (
        impl [$($impl_generics:tt)*] $ty:ty {
            lifetime = $lt:lifetime;
            backend = $backend:ty;
            shared = $shared:ty;
            $($methods:tt)*
        }
    ) => {
        impl $($impl_generics)* $ty {
            /// Wrap an internal core value.
            #[inline]
            const fn from_inner(inner: crate::core::RefStrCore<$lt, $backend>) -> Self {
                Self(ManuallyDrop::new(inner))
            }

            /// Borrow the internal core representation.
            #[inline]
            const fn inner(&self) -> &crate::core::RefStrCore<$lt, $backend> {
                let ptr = &self.0
                    as *const ManuallyDrop<crate::core::RefStrCore<$lt, $backend>>
                    as *const crate::core::RefStrCore<$lt, $backend>;
                unsafe { &*ptr }
            }

            #[inline]
            const unsafe fn into_raw_parts_struct(self) -> crate::core::RawParts {
                let this = ManuallyDrop::new(self);
                let this_ptr = &this as *const ManuallyDrop<Self> as *const Self;
                let inner_ptr = unsafe {
                    ptr::addr_of!((*this_ptr).0)
                        as *const ManuallyDrop<crate::core::RefStrCore<$lt, $backend>>
                };

                unsafe {
                    <crate::core::RefStrCore<$lt, $backend>>::raw_parts_from_manuallydrop_ptr(
                        inner_ptr,
                    )
                }
            }

            #[inline]
            const unsafe fn into_inner(self) -> crate::core::RefStrCore<$lt, $backend> {
                unsafe {
                    <crate::core::RefStrCore<$lt, $backend>>::from_raw_parts_struct(
                        self.into_raw_parts_struct(),
                    )
                }
            }

            /// Create a borrowed value from `&str`.
            ///
            /// This is equivalent to [`from_str`](Self::from_str).
            #[inline]
            pub const fn new(s: &$lt str) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::new(s))
            }

            /// Create a borrowed value from `&str`.
            ///
            /// The resulting value does not allocate and remains in the borrowed
            /// state.
            #[inline]
            pub const fn from_str(s: &$lt str) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from_str(s))
            }

            /// Create a shared value from any string-like input.
            ///
            /// Unlike [`from_str`](Self::from_str), this always goes through the
            /// backend's shared representation and therefore allocates and
            /// produces a shared value, even if the input is already `&str`.
            #[inline]
            pub fn from_owned_like<R: AsRef<str>>(s: R) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from_owned_like(s))
            }

            /// Rebuild a value from the raw parts produced by [`into_raw_parts`](Self::into_raw_parts).
            ///
            /// # Safety
            ///
            /// `raw_ptr`, `len`, and `tag` must come from a compatible `$ty` value created by this
            /// crate. The pointer must reference valid UTF-8 for `len` bytes, and `tag` must be a
            /// valid encoding tag for this representation.
            #[inline]
            pub const unsafe fn from_raw_parts(
                raw_ptr: NonNull<u8>,
                len: usize,
                tag: usize,
            ) -> Self {
                Self::from_inner(unsafe {
                    <crate::core::RefStrCore<$lt, $backend>>::from_raw_parts(raw_ptr, len, tag)
                })
            }

            /// Create a value directly from the backend's shared string type.
            ///
            /// This keeps the allocation shared without copying string contents.
            #[inline]
            pub fn from_shared(s: $shared) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from_shared(s))
            }

            /// Decompose this value into raw parts for FFI or manual ownership transfer.
            ///
            /// # Safety
            ///
            /// The returned parts transfer ownership responsibilities to the caller. You must later
            /// reconstruct the value with [`from_raw_parts`](Self::from_raw_parts) or otherwise
            /// release the underlying shared allocation exactly once.
            #[inline]
            pub const unsafe fn into_raw_parts(self) -> (NonNull<u8>, usize, usize) {
                unsafe { self.into_raw_parts_struct().into_tuple() }
            }

            /// Convert this value into a raw `*const str`.
            ///
            /// # Safety
            ///
            /// The returned pointer is ambiguous: it may represent either borrowed data or shared
            /// backend storage.
            ///
            /// Only pointers originating from shared values may be used with
            /// [`increment_strong_count`](Self::increment_strong_count) or reconstructed into backend
            /// shared handles.
            ///
            /// If you need to branch on ownership state, prefer
            /// [`into_raw_parts`](Self::into_raw_parts) or
            /// [`into_raw_shared`](Self::into_raw_shared).
            #[inline]
            pub const unsafe fn into_raw(self) -> *const str {
                unsafe { self.into_raw_parts_struct().into_raw() }
            }

            /// Convert into a raw pointer only when this value is shared.
            ///
            /// Returns `None` for borrowed values, avoiding ambiguous raw pointers in mixed
            /// borrowed/shared code paths.
            #[inline]
            pub fn into_raw_shared(self) -> Option<*const str> {
                unsafe { self.into_inner() }.into_raw_shared()
            }

            /// Increment the strong count for a raw pointer produced by [`into_raw`](Self::into_raw).
            ///
            /// # Safety
            ///
            /// `ptr` must have been produced by this crate for the same backend and must still point
            /// to a live shared allocation. Calling this on a borrowed string pointer or an invalid
            /// pointer is undefined behavior.
            #[inline]
            pub unsafe fn increment_strong_count(ptr: *const str) {
                unsafe {
                    <crate::core::RefStrCore<$lt, $backend>>::increment_strong_count(ptr);
                }
            }

            /// Returns `true` when this value owns shared storage.
            #[inline]
            pub const fn is_shared(&self) -> bool {
                self.inner().is_shared()
            }

            /// Returns `true` when this value is a borrowed string.
            #[inline]
            pub const fn is_borrowed(&self) -> bool {
                self.inner().is_borrowed()
            }

            /// Returns the string length in bytes.
            #[inline]
            pub const fn len(&self) -> usize {
                self.inner().len()
            }

            /// Returns `true` when the string is empty.
            #[inline]
            pub const fn is_empty(&self) -> bool {
                self.inner().is_empty()
            }

            /// Borrows the contents as `&str`.
            #[inline]
            pub fn as_str(&self) -> &str {
                self.inner().as_str()
            }

            /// Converts this value into owned UTF-8 bytes.
            #[inline]
            pub fn into_bytes(self) -> alloc::vec::Vec<u8> {
                unsafe { self.into_inner() }.into_bytes()
            }

            /// Converts this value into `Box<str>`.
            #[inline]
            pub fn into_boxed_str(self) -> Box<str> {
                unsafe { self.into_inner() }.into_boxed_str()
            }

            /// Converts this value into [`String`].
            #[inline]
            pub fn into_string(self) -> String {
                unsafe { self.into_inner() }.into_string()
            }

            /// Converts this value into [`Cow<str>`][Cow].
            ///
            /// Borrowed values stay borrowed. Shared values become owned strings.
            #[inline]
            pub fn into_cow(self) -> Cow<$lt, str> {
                unsafe { self.into_inner() }.into_cow()
            }

            /// Convert into `&str` without checking whether the value is borrowed.
            ///
            /// # Safety
            ///
            /// This is only sound when the value currently stores a borrowed string whose lifetime
            /// is valid for `$lt`. Calling this on a shared value can produce a dangling reference.
            #[inline]
            pub unsafe fn into_str_unchecked(self) -> &$lt str {
                unsafe { self.into_inner().into_str_unchecked() }
            }

            $($methods)*
        }

        impl $($impl_generics)* Drop for $ty {
            /// Drops the underlying string representation.
            fn drop(&mut self) {
                unsafe {
                    ManuallyDrop::drop(&mut self.0);
                }
            }
        }

        impl $($impl_generics)* Default for $ty {
            /// Creates an empty borrowed value.
            fn default() -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::default())
            }
        }

        impl $($impl_generics)* AsRef<str> for $ty {
            /// Borrows the contents as `&str`.
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl $($impl_generics)* Borrow<str> for $ty {
            /// Borrows the contents as `&str`.
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl $($impl_generics)* Clone for $ty {
            /// Clones the string, incrementing the shared reference count when needed.
            fn clone(&self) -> Self {
                Self::from_inner(self.inner().clone())
            }
        }

        impl $($impl_generics)* PartialEq for $ty {
            /// Compares by string contents.
            fn eq(&self, other: &Self) -> bool {
                self.as_str() == other.as_str()
            }
        }

        impl $($impl_generics)* Eq for $ty {}

        impl $($impl_generics)* PartialEq<&str> for $ty {
            /// Compares against a string slice.
            fn eq(&self, other: &&str) -> bool {
                self.as_str() == *other
            }
        }

        impl $($impl_generics)* PartialEq<String> for $ty {
            /// Compares against an owned string.
            fn eq(&self, other: &String) -> bool {
                self.as_str() == other.as_str()
            }
        }

        impl $($impl_generics)* PartialOrd for $ty {
            /// Performs lexicographic comparison.
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl $($impl_generics)* Ord for $ty {
            /// Performs lexicographic comparison.
            fn cmp(&self, other: &Self) -> Ordering {
                self.as_str().cmp(other.as_str())
            }
        }

        impl $($impl_generics)* Hash for $ty {
            /// Hashes the string contents.
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.as_str().hash(state)
            }
        }

        impl $($impl_generics)* Deref for $ty {
            type Target = str;

            /// Borrows the contents as `str`.
            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }

        impl $($impl_generics)* fmt::Debug for $ty {
            /// Formats the type and its string contents for debugging.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if f.alternate() {
                    let state = if self.is_shared() {
                        "Shared"
                    } else {
                        "Borrowed"
                    };
                    f.debug_struct(stringify!($ty))
                        .field("state", &state)
                        .field("len", &self.len())
                        .field("value", &self.as_str())
                        .finish()
                } else {
                    f.debug_tuple(stringify!($ty))
                        .field(&self.as_str())
                        .finish()
                }
            }
        }

        impl $($impl_generics)* fmt::Display for $ty {
            /// Formats the string contents.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl $($impl_generics)* Serialize for $ty {
            /// Serializes as a plain string.
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
    };
}

macro_rules! impl_ref_str_non_static {
    ($name:ident<$lt:lifetime>, $backend:ty, $shared:ty) => {
        impl<$lt> From<&$lt str> for $name<$lt> {
            /// Creates a borrowed value from `&str`.
            fn from(value: &$lt str) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<&$lt String> for $name<$lt> {
            /// Creates a borrowed value from `&String`.
            fn from(value: &$lt String) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<$shared> for $name<$lt> {
            /// Creates a shared value from the backend's shared string type.
            fn from(value: $shared) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<Box<str>> for $name<$lt> {
            /// Creates a shared value from `Box<str>`.
            fn from(value: Box<str>) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<String> for $name<$lt> {
            /// Creates a shared value from [`String`].
            fn from(value: String) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<Cow<$lt, str>> for $name<$lt> {
            /// Creates a value from [`Cow<str>`][Cow].
            ///
            /// Borrowed `Cow` values stay borrowed, while owned values become shared.
            fn from(value: Cow<$lt, str>) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<$name<$lt>> for Cow<$lt, str> {
            /// Converts into [`Cow<str>`][Cow].
            fn from(value: $name<$lt>) -> Self {
                value.into_cow()
            }
        }

        #[cfg(feature = "arbitrary")]
        impl<$lt> Arbitrary<$lt> for $name<$lt> {
            /// Generates either a borrowed or shared arbitrary string.
            fn arbitrary(u: &mut Unstructured<$lt>) -> ArbitraryResult<Self> {
                let value = <&$lt str>::arbitrary(u)?;

                if u.arbitrary::<bool>()? {
                    Ok(Self::from(String::from(value)))
                } else {
                    Ok(Self::from(value))
                }
            }
        }

        #[cfg(feature = "serde")]
        impl<'de: $lt, $lt> Deserialize<'de> for $name<$lt> {
            /// Deserializes from a string, preserving borrowed data when available.
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = $name<'de>;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a string")
                    }

                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        v: &'de str,
                    ) -> Result<Self::Value, E> {
                        Ok($name::from(v))
                    }

                    fn visit_str<E: serde::de::Error>(
                        self,
                        v: &str,
                    ) -> Result<Self::Value, E> {
                        Ok($name::from(String::from(v)))
                    }

                    fn visit_string<E: serde::de::Error>(
                        self,
                        v: String,
                    ) -> Result<Self::Value, E> {
                        Ok($name::from(v))
                    }
                }

                deserializer.deserialize_str(Visitor)
            }
        }
    };
}

macro_rules! impl_ref_str_static {
    ($name:ident, $backend:ty, $shared:ty) => {
        impl From<&'static str> for $name {
            /// Creates a borrowed `'static` value from `&'static str`.
            fn from(value: &'static str) -> Self {
                Self::from_inner(<crate::core::RefStrCore<'static, $backend>>::from(value))
            }
        }

        impl From<$shared> for $name {
            /// Creates a shared `'static` value from the backend's shared string type.
            fn from(value: $shared) -> Self {
                Self::from_inner(<crate::core::RefStrCore<'static, $backend>>::from(value))
            }
        }

        impl From<Box<str>> for $name {
            /// Creates a shared `'static` value from `Box<str>`.
            fn from(value: Box<str>) -> Self {
                Self::from_inner(<crate::core::RefStrCore<'static, $backend>>::from(value))
            }
        }

        impl From<String> for $name {
            /// Creates a shared `'static` value from [`String`].
            fn from(value: String) -> Self {
                Self::from_inner(<crate::core::RefStrCore<'static, $backend>>::from(value))
            }
        }

        impl From<Cow<'static, str>> for $name {
            /// Creates a `'static` value from [`Cow<'static, str>`][Cow].
            fn from(value: Cow<'static, str>) -> Self {
                match value {
                    Cow::Borrowed(value) => Self::from(value),
                    Cow::Owned(value) => Self::from(value),
                }
            }
        }

        impl From<$name> for Cow<'static, str> {
            /// Converts into [`Cow<'static, str>`][Cow].
            fn from(value: $name) -> Self {
                value.into_cow()
            }
        }

        #[cfg(feature = "arbitrary")]
        impl<'a> Arbitrary<'a> for $name {
            /// Generates an arbitrary shared `'static` string value.
            fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
                let value = <&str>::arbitrary(u)?;
                Ok(Self::from(String::from(value)))
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> Deserialize<'de> for $name {
            /// Deserializes from a string into a `'static` value.
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = $name;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a string")
                    }

                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        v: &'de str,
                    ) -> Result<Self::Value, E> {
                        Ok($name::from(String::from(v)))
                    }

                    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                        Ok($name::from(String::from(v)))
                    }

                    fn visit_string<E: serde::de::Error>(
                        self,
                        v: String,
                    ) -> Result<Self::Value, E> {
                        Ok($name::from(v))
                    }
                }

                deserializer.deserialize_str(Visitor)
            }
        }
    };
}

impl_ref_str_common! {
    impl [<'a>] RefStr<'a> {
        lifetime = 'a;
        backend = crate::core::SharedBackend;
        shared = Arc<str>;

        /// Borrows the contents as [`Cow<str>`][Cow].
        ///
        /// Borrowed values stay borrowed. Shared values allocate and clone into
        /// an owned string to satisfy the borrow semantics of `Cow`.
        #[inline]
        pub fn as_cow(&self) -> Cow<'a, str> {
            self.inner().as_cow()
        }
    }
}

impl_ref_str_common! {
    impl [<'a>] LocalRefStr<'a> {
        lifetime = 'a;
        backend = crate::core::LocalBackend;
        shared = Rc<str>;

        /// Borrows the contents as [`Cow<str>`][Cow].
        ///
        /// Borrowed values stay borrowed. Shared values allocate and clone into
        /// an owned string to satisfy the borrow semantics of `Cow`.
        #[inline]
        pub fn as_cow(&self) -> Cow<'a, str> {
            self.inner().as_cow()
        }
    }
}

impl_ref_str_common! {
    impl [] StaticRefStr {
        lifetime = 'static;
        backend = crate::core::SharedBackend;
        shared = Arc<str>;

        /// Borrows the contents as [`Cow<'static, str>`][Cow].
        #[inline]
        pub fn as_cow(&self) -> Cow<'static, str> {
            self.clone().into_cow()
        }

        /// Creates a borrowed value from `&'static str`.
        #[inline]
        pub const fn from_static(s: &'static str) -> Self {
            Self::from_str(s)
        }
    }
}

impl_ref_str_common! {
    impl [] LocalStaticRefStr {
        lifetime = 'static;
        backend = crate::core::LocalBackend;
        shared = Rc<str>;

        /// Borrows the contents as [`Cow<'static, str>`][Cow].
        #[inline]
        pub fn as_cow(&self) -> Cow<'static, str> {
            self.clone().into_cow()
        }

        /// Creates a borrowed value from `&'static str`.
        #[inline]
        pub const fn from_static(s: &'static str) -> Self {
            Self::from_str(s)
        }
    }
}

impl_ref_str_non_static!(RefStr<'a>, crate::core::SharedBackend, Arc<str>);
impl_ref_str_non_static!(LocalRefStr<'a>, crate::core::LocalBackend, Rc<str>);
impl_ref_str_static!(StaticRefStr, crate::core::SharedBackend, Arc<str>);
impl_ref_str_static!(LocalStaticRefStr, crate::core::LocalBackend, Rc<str>);

impl<'a, 'b> PartialEq<Cow<'b, str>> for RefStr<'a> {
    /// Compares against [`Cow<str>`][Cow] by string contents.
    fn eq(&self, other: &Cow<'b, str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'a, 'b> PartialEq<Cow<'b, str>> for LocalRefStr<'a> {
    /// Compares against [`Cow<str>`][Cow] by string contents.
    fn eq(&self, other: &Cow<'b, str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'b> PartialEq<Cow<'b, str>> for StaticRefStr {
    /// Compares against [`Cow<str>`][Cow] by string contents.
    fn eq(&self, other: &Cow<'b, str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'b> PartialEq<Cow<'b, str>> for LocalStaticRefStr {
    /// Compares against [`Cow<str>`][Cow] by string contents.
    fn eq(&self, other: &Cow<'b, str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'a> PartialEq<Arc<str>> for RefStr<'a> {
    /// Compares against [`Arc<str>`] by string contents.
    fn eq(&self, other: &Arc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'a> PartialEq<Rc<str>> for RefStr<'a> {
    /// Compares against [`Rc<str>`] by string contents.
    fn eq(&self, other: &Rc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'a> PartialEq<Arc<str>> for LocalRefStr<'a> {
    /// Compares against [`Arc<str>`] by string contents.
    fn eq(&self, other: &Arc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'a> PartialEq<Rc<str>> for LocalRefStr<'a> {
    /// Compares against [`Rc<str>`] by string contents.
    fn eq(&self, other: &Rc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl PartialEq<Arc<str>> for StaticRefStr {
    /// Compares against [`Arc<str>`] by string contents.
    fn eq(&self, other: &Arc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl PartialEq<Rc<str>> for StaticRefStr {
    /// Compares against [`Rc<str>`] by string contents.
    fn eq(&self, other: &Rc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl PartialEq<Arc<str>> for LocalStaticRefStr {
    /// Compares against [`Arc<str>`] by string contents.
    fn eq(&self, other: &Arc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl PartialEq<Rc<str>> for LocalStaticRefStr {
    /// Compares against [`Rc<str>`] by string contents.
    fn eq(&self, other: &Rc<str>) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl<'a> From<LocalRefStr<'a>> for RefStr<'a> {
    /// Converts the local `Rc`-backed variant into the thread-safe `Arc`-backed variant.
    ///
    /// Borrowed values remain borrowed. Shared values are re-materialized into
    /// `Arc<str>`, which allocates and copies the string contents.
    fn from(value: LocalRefStr<'a>) -> Self {
        Self::from_inner(
            <crate::core::RefStrCore<'a, crate::core::SharedBackend>>::from(unsafe {
                value.into_inner()
            }),
        )
    }
}

impl<'a> From<RefStr<'a>> for LocalRefStr<'a> {
    /// Converts the thread-safe `Arc`-backed variant into the local `Rc`-backed variant.
    ///
    /// Borrowed values remain borrowed. Shared values are re-materialized into
    /// `Rc<str>`, which allocates and copies the string contents.
    fn from(value: RefStr<'a>) -> Self {
        Self::from_inner(
            <crate::core::RefStrCore<'a, crate::core::LocalBackend>>::from(unsafe {
                value.into_inner()
            }),
        )
    }
}

impl From<LocalStaticRefStr> for StaticRefStr {
    /// Converts the local `'static` variant into the thread-safe `'static` variant.
    fn from(value: LocalStaticRefStr) -> Self {
        Self::from_inner(
            <crate::core::RefStrCore<'static, crate::core::SharedBackend>>::from(unsafe {
                value.into_inner()
            }),
        )
    }
}

impl From<StaticRefStr> for LocalStaticRefStr {
    /// Converts the thread-safe `'static` variant into the local `'static` variant.
    fn from(value: StaticRefStr) -> Self {
        Self::from_inner(
            <crate::core::RefStrCore<'static, crate::core::LocalBackend>>::from(unsafe {
                value.into_inner()
            }),
        )
    }
}

impl From<StaticRefStr> for RefStr<'static> {
    /// Converts the dedicated `'static` wrapper into the lifetime-parameterized form.
    fn from(value: StaticRefStr) -> Self {
        Self::from_inner(unsafe { value.into_inner() })
    }
}

impl From<RefStr<'static>> for StaticRefStr {
    /// Converts the lifetime-parameterized `'static` form into the dedicated wrapper.
    fn from(value: RefStr<'static>) -> Self {
        Self::from_inner(unsafe { value.into_inner() })
    }
}

impl From<LocalStaticRefStr> for LocalRefStr<'static> {
    /// Converts the dedicated local `'static` wrapper into the lifetime-parameterized form.
    fn from(value: LocalStaticRefStr) -> Self {
        Self::from_inner(unsafe { value.into_inner() })
    }
}

impl From<LocalRefStr<'static>> for LocalStaticRefStr {
    /// Converts the lifetime-parameterized local `'static` form into the dedicated wrapper.
    fn from(value: LocalRefStr<'static>) -> Self {
        Self::from_inner(unsafe { value.into_inner() })
    }
}
