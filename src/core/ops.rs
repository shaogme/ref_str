use ::core::borrow::Borrow;
use ::core::cmp::Ordering;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::ops::Deref;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;

use super::{LocalBackend, RefCountBackend, RefStrCore, SharedBackend};
use crate::arch;

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
                arch::StateTag::Borrowed => "Borrowed",
                arch::StateTag::Shared => "Shared",
                arch::StateTag::Inline => "Inline",
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
        if (meta & arch::NEEDS_DROP_MASK) != 0 {
            let len = arch::decode_borrowed_or_shared_len(meta);
            let fat_ptr = ::core::ptr::slice_from_raw_parts(parts.as_ptr(), len) as *const str;
            unsafe {
                B::increment_strong_count(fat_ptr);
            }
        }

        Self {
            parts,
            _marker: super::PhantomData,
            _backend: super::PhantomData,
        }
    }
}

impl<'a, B: RefCountBackend> Drop for RefStrCore<'a, B> {
    fn drop(&mut self) {
        let parts = self.parts;
        let meta = parts.meta();
        if (meta & arch::NEEDS_DROP_MASK) != 0 {
            let len = arch::decode_borrowed_or_shared_len(meta);
            let fat_ptr = ::core::ptr::slice_from_raw_parts(parts.as_ptr(), len) as *const str;
            unsafe {
                drop(B::from_raw(fat_ptr));
            }
        }
    }
}
