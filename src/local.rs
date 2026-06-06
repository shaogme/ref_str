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

use crate::shared::{RefStr, StaticRefStr};

/// A compact string that is either borrowed for `'a` or shared via [`Rc<str>`].
///
/// This is the single-threaded counterpart to [`RefStr`]. It has the same API
/// shape, but shared ownership uses non-atomic reference counting.
#[repr(transparent)]
pub struct LocalRefStr<'a>(
    pub(crate) ManuallyDrop<crate::core::RefStrCore<'a, crate::core::LocalBackend>>,
);

/// A `'static` compact string that is either borrowed or shared via [`Rc<str>`].
///
/// This is the single-threaded counterpart to [`StaticRefStr`].
#[repr(transparent)]
pub struct LocalStaticRefStr(
    pub(crate) ManuallyDrop<crate::core::RefStrCore<'static, crate::core::LocalBackend>>,
);

impl_ref_str_common! {
    impl [<'a>] LocalRefStr<'a> {
        lifetime = 'a;
        backend = crate::core::LocalBackend;
        shared = Rc<str>;

        /// Borrows the contents as [`Cow<str>`][Cow].
        ///
        /// Borrowed values stay borrowed. Inline and shared values allocate and
        /// clone into an owned string to satisfy the borrow semantics of `Cow`.
        #[inline]
        pub fn as_cow(&self) -> Cow<'a, str> {
            self.inner().as_cow()
        }

        /// Convert to a static LocalRefStr.
        ///
        /// Shared values are cloned to increase the reference count.
        /// Borrowed values are copied into owned storage, preferring inline for
        /// short strings and shared storage for longer ones.
        #[inline]
        pub fn to_static_str(&self) -> LocalStaticRefStr {
            LocalStaticRefStr::from_inner(self.inner().to_static_core())
        }

        /// Convert to a static LocalRefStr.
        ///
        /// Shared and inline values are transferred directly.
        /// Borrowed values are copied into owned storage, preferring inline for
        /// short strings and shared storage for longer ones.
        #[inline]
        pub fn into_static_str(self) -> LocalStaticRefStr {
            LocalStaticRefStr::from_inner(unsafe { self.into_inner() }.into_static_core())
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

impl_ref_str_non_static!(LocalRefStr<'a>, crate::core::LocalBackend, Rc<str>);
impl_ref_str_static!(LocalStaticRefStr, crate::core::LocalBackend, Rc<str>);

impl_ref_str_partial_eqs! {
    for LocalRefStr<'a> {
        ['a] LocalRefStr<'a> => |lhs, other| lhs.inner() == other.inner();
        ['a] &str => |lhs, other| lhs.as_str() == *other;
        ['a] String => |lhs, other| lhs.as_str() == other.as_str();
        ['a, 'b] Cow<'b, str> => |lhs, other| lhs.as_str() == other.as_ref();
        ['a] Arc<str> => |lhs, other| lhs.as_str() == other.as_ref();
        ['a] Rc<str> => |lhs, other| lhs.as_str() == other.as_ref();
    }
}

impl_ref_str_partial_eqs! {
    for LocalStaticRefStr {
        [] LocalStaticRefStr => |lhs, other| lhs.inner() == other.inner();
        [] &str => |lhs, other| lhs.as_str() == *other;
        [] String => |lhs, other| lhs.as_str() == other.as_str();
        ['b] Cow<'b, str> => |lhs, other| lhs.as_str() == other.as_ref();
        [] Arc<str> => |lhs, other| lhs.as_str() == other.as_ref();
        [] Rc<str> => |lhs, other| lhs.as_str() == other.as_ref();
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
