use ::core::borrow::Borrow;
use ::core::cmp::Ordering;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;

#[cfg(feature = "arbitrary")]
use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};

#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::local::{LocalRefStr, LocalStaticRefStr};

/// A compact string that is either borrowed for `'a` or shared via [`Arc<str>`].
///
/// Use this type when you want to preserve borrowing when possible, but still
/// accept owned strings and clone them cheaply through atomic reference
/// counting.
#[repr(transparent)]
pub struct RefStr<'a>(
    pub(crate) ManuallyDrop<crate::core::RefStrCore<'a, crate::core::SharedBackend>>,
);

/// A `'static` compact string that is either borrowed or shared via [`Arc<str>`].
///
/// Borrowed instances hold `&'static str`. Owned inputs are promoted into shared
/// storage so the resulting value remains `'static`.
#[repr(transparent)]
pub struct StaticRefStr(
    pub(crate) ManuallyDrop<crate::core::RefStrCore<'static, crate::core::SharedBackend>>,
);

impl_ref_str_common! {
    impl [<'a>] RefStr<'a> {
        lifetime = 'a;
        backend = crate::core::SharedBackend;
        shared = Arc<str>;

        /// Borrows the contents as [`Cow<str>`][Cow].
        ///
        /// Borrowed values stay borrowed. Inline and shared values allocate and
        /// clone into an owned string to satisfy the borrow semantics of `Cow`.
        #[inline]
        pub fn as_cow(&self) -> Cow<'a, str> {
            self.inner().as_cow()
        }

        /// Convert to a static RefStr.
        ///
        /// Shared values are cloned to increase the reference count.
        /// Borrowed values are copied into owned storage, preferring inline for
        /// short strings and shared storage for longer ones.
        #[inline]
        pub fn to_static_str(&self) -> StaticRefStr {
            StaticRefStr::from_inner(self.inner().to_static_core())
        }

        /// Convert to a static RefStr.
        ///
        /// Shared and inline values are transferred directly.
        /// Borrowed values are copied into owned storage, preferring inline for
        /// short strings and shared storage for longer ones.
        #[inline]
        pub fn into_static_str(self) -> StaticRefStr {
            StaticRefStr::from_inner(unsafe { self.into_inner() }.into_static_core())
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
            if let Some(value) = self.inner().borrowed_static_str() {
                Cow::Borrowed(value)
            } else {
                Cow::Owned(String::from(self.as_str()))
            }
        }

        /// Creates a borrowed value from `&'static str`.
        #[inline]
        pub const fn from_static(s: &'static str) -> Self {
            Self::from_str(s)
        }
    }
}

impl_ref_str_non_static!(RefStr<'a>, crate::core::SharedBackend, Arc<str>);
impl_ref_str_static!(StaticRefStr, crate::core::SharedBackend, Arc<str>);

impl_ref_str_partial_eqs! {
    for RefStr<'a> {
        ['a] RefStr<'a> => |lhs, other| lhs.inner() == other.inner();
        ['a] &str => |lhs, other| lhs.as_str() == *other;
        ['a] String => |lhs, other| lhs.as_str() == other.as_str();
        ['a, 'b] Cow<'b, str> => |lhs, other| lhs.as_str() == other.as_ref();
        ['a] Arc<str> => |lhs, other| lhs.as_str() == other.as_ref();
        ['a] Rc<str> => |lhs, other| lhs.as_str() == other.as_ref();
    }
}

impl_ref_str_partial_eqs! {
    for StaticRefStr {
        [] StaticRefStr => |lhs, other| lhs.inner() == other.inner();
        [] &str => |lhs, other| lhs.as_str() == *other;
        [] String => |lhs, other| lhs.as_str() == other.as_str();
        ['b] Cow<'b, str> => |lhs, other| lhs.as_str() == other.as_ref();
        [] Arc<str> => |lhs, other| lhs.as_str() == other.as_ref();
        [] Rc<str> => |lhs, other| lhs.as_str() == other.as_ref();
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
