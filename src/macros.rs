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
            pub(crate) const fn from_inner(inner: crate::core::RefStrCore<$lt, $backend>) -> Self {
                Self(ManuallyDrop::new(inner))
            }

            /// Borrow the internal core representation.
            #[inline]
            pub(crate) const fn inner(&self) -> &crate::core::RefStrCore<$lt, $backend> {
                let ptr = &self.0
                    as *const ManuallyDrop<crate::core::RefStrCore<$lt, $backend>>
                    as *const crate::core::RefStrCore<$lt, $backend>;
                unsafe { &*ptr }
            }

            #[inline]
            pub(crate) const unsafe fn into_raw_parts_struct(self) -> crate::RawParts {
                #[repr(C)]
                union View<T> {
                    outer: ManuallyDrop<T>,
                    parts: crate::RawParts,
                }

                let view = View {
                    outer: ManuallyDrop::new(self),
                };

                unsafe { view.parts }
            }

            #[inline]
            pub(crate) unsafe fn into_inner(self) -> crate::core::RefStrCore<$lt, $backend> {
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

            /// Create an owned value from any string-like input.
            ///
            /// Unlike [`from_str`](Self::from_str), this copies the input into
            /// owned storage. Short strings are stored inline, while longer
            /// strings use the backend's shared representation.
            #[inline]
            pub fn from_owned_like<R: AsRef<str>>(s: R) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from_owned_like(s))
            }

            /// Rebuild a value from the raw parts produced by [`into_raw_parts`](Self::into_raw_parts).
            ///
            /// # Safety
            ///
            /// `parts` must come from a compatible `$ty` value created by this
            /// crate. The pointer/metadata pair must reference valid UTF-8 and a
            /// valid encoding state for this representation.
            #[inline]
            pub const unsafe fn from_raw_parts(parts: crate::RawParts) -> Self {
                Self::from_inner(unsafe {
                    <crate::core::RefStrCore<$lt, $backend>>::from_raw_parts(parts)
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
            pub const unsafe fn into_raw_parts(self) -> crate::RawParts {
                unsafe { self.into_raw_parts_struct() }
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
            pub unsafe fn into_raw(self) -> *const str {
                unsafe { self.into_inner().into_raw() }
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

            /// Returns `true` when this value stores an inline short string.
            #[inline]
            pub const fn is_inline(&self) -> bool {
                self.inner().is_inline()
            }

            /// Returns `true` when the cached contents are ASCII-only.
            #[inline]
            pub const fn is_ascii(&self) -> bool {
                self.inner().is_ascii()
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
            /// Borrowed values stay borrowed. Inline and shared values become
            /// owned strings.
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

        impl $($impl_generics)* Eq for $ty {}

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
                    } else if self.is_inline() {
                        "Inline"
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
            /// Creates an owned value from `Box<str>`.
            fn from(value: Box<str>) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<String> for $name<$lt> {
            /// Creates an owned value from [`String`].
            fn from(value: String) -> Self {
                Self::from_inner(<crate::core::RefStrCore<$lt, $backend>>::from(value))
            }
        }

        impl<$lt> From<Cow<$lt, str>> for $name<$lt> {
            /// Creates a value from [`Cow<str>`][Cow].
            ///
            /// Borrowed `Cow` values stay borrowed, while owned values become
            /// owned storage.
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

macro_rules! impl_ref_str_partial_eqs {
    (for $lhs:ty { $([$($impl_generics:tt)*] $rhs:ty => |$lhs_ref:ident, $other:ident| $compare:expr;)+ }) => {
        $(
            impl_ref_str_partial_eqs!(@single [$($impl_generics)*] $lhs => $rhs, |$lhs_ref, $other| $compare);
        )+
    };
    (@single [] $lhs:ty => $rhs:ty, |$lhs_ref:ident, $other:ident| $compare:expr) => {
        impl PartialEq<$rhs> for $lhs {
            fn eq(&self, other: &$rhs) -> bool {
                let $lhs_ref = self;
                let $other = other;
                $compare
            }
        }

        impl PartialEq<$rhs> for &$lhs {
            fn eq(&self, other: &$rhs) -> bool {
                let $lhs_ref = *self;
                let $other = other;
                $compare
            }
        }

        impl PartialEq<&$rhs> for $lhs {
            fn eq(&self, other: &&$rhs) -> bool {
                let $lhs_ref = self;
                let $other = *other;
                $compare
            }
        }

    };
    (@single [$($impl_generics:tt)+] $lhs:ty => $rhs:ty, |$lhs_ref:ident, $other:ident| $compare:expr) => {
        impl<$($impl_generics)+> PartialEq<$rhs> for $lhs {
            fn eq(&self, other: &$rhs) -> bool {
                let $lhs_ref = self;
                let $other = other;
                $compare
            }
        }

        impl<$($impl_generics)+> PartialEq<$rhs> for &$lhs {
            fn eq(&self, other: &$rhs) -> bool {
                let $lhs_ref = *self;
                let $other = other;
                $compare
            }
        }

        impl<$($impl_generics)+> PartialEq<&$rhs> for $lhs {
            fn eq(&self, other: &&$rhs) -> bool {
                let $lhs_ref = self;
                let $other = *other;
                $compare
            }
        }

    };
}
