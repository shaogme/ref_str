use ::core::fmt;
use ::core::num::NonZeroUsize;
use ::core::ptr;

use crate::arch::encode;
use crate::arch::layout;
use crate::arch::layout::StateTag;

/// Low-level two-word payload for the compact string representation.
///
/// `RawParts` is the stable, FFI-friendly shape used by the crate to move a
/// string value across the boundary between the public wrappers and the
/// internal encoding logic.
///
/// The first word stores a raw data pointer. The second word stores the encoded
/// metadata in a `NonZeroUsize`, which gives `Option<RawParts>` the same size as
/// `RawParts` itself.
///
/// # State model
///
/// The metadata encodes exactly one of three states:
/// - `Borrowed`: the pointer refers to borrowed UTF-8 data
/// - `Shared`: the pointer refers to backend-owned shared storage
/// - `Inline`: the string bytes are stored directly inside the two-word payload
///
/// # Safety model
///
/// Constructors and conversions assume the caller preserves the contract
/// between pointer, length, tag bits, and payload contents. Most consumers
/// should treat this type as an internal transport format and use the higher
/// level wrappers instead.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct RawParts {
    /// Raw pointer to borrowed or shared string bytes.
    raw_ptr: *const u8,
    /// Encoded metadata word. Must never be zero.
    meta: NonZeroUsize,
}

#[repr(C)]
/// Union view used to reinterpret the same two words as either raw bytes or
/// structured fields.
union RawView {
    parts: RawParts,
    bytes: [u8; layout::INLINE_TOTAL_BYTES],
}

impl RawParts {
    /// Build encoded parts from already-validated fields.
    ///
    /// # Safety
    ///
    /// `raw_ptr` and `meta` must describe a valid `ref_str` payload.
    /// The tag bits must match one of the supported states, and the length must
    /// fit the corresponding encoding.
    pub const unsafe fn new(raw_ptr: *const u8, meta: usize) -> Self {
        let tag = match encode::from_meta_checked(meta) {
            Some(tag) => tag,
            None => panic!("invalid tag for compressed string"),
        };

        match tag {
            StateTag::Borrowed | StateTag::Shared => {
                let len = encode::decode_borrowed_or_shared_len(meta);
                assert!(
                    len <= layout::MAX_BORROWED_OR_SHARED_LEN,
                    "string too large to compress"
                );
            }
            StateTag::Inline => {
                let len = encode::inline_len_from_meta(meta);
                assert!(
                    len <= layout::INLINE_CAPACITY,
                    "inline string too large to compress"
                );
            }
        }

        Self {
            raw_ptr,
            meta: unsafe { NonZeroUsize::new_unchecked(meta) },
        }
    }

    /// Pack a short string into the inline representation.
    ///
    /// The resulting value stores the bytes directly inside the payload and
    /// encodes the length, state tag, and ASCII flag in the metadata bytes.
    pub fn pack_inline(s: &str) -> Self {
        let len = s.len();
        debug_assert!(encode::supports_inline_len(len));

        let mut view = RawView {
            bytes: [0u8; layout::INLINE_TOTAL_BYTES],
        };

        unsafe {
            view.bytes[0..len].copy_from_slice(s.as_bytes());
            view.bytes[layout::BUFFER_TAG_BYTE_INDEX] = encode::inline_tag_byte(len, s.is_ascii());

            view.parts
        }
    }

    /// Return the stored pointer word.
    pub const fn raw_ptr(self) -> *const u8 {
        self.raw_ptr
    }

    /// Return the stored metadata word.
    pub const fn meta(self) -> usize {
        self.meta.get()
    }

    /// Return `true` when the payload cached ASCII-only contents.
    pub const fn is_ascii(self) -> bool {
        let meta = self.meta();
        (meta & layout::IS_ASCII_MASK) != 0
    }

    /// Return the cached short hash for borrowed or shared payloads.
    pub(crate) const fn cached_hash(self) -> usize {
        encode::decode_cached_hash(self.meta())
    }

    /// Return `true` when the payload stores a borrowed string.
    pub const fn is_borrowed(self) -> bool {
        let meta = self.meta();
        (meta & (layout::INLINE_MASK | layout::NEEDS_DROP_MASK)) == 0
    }

    /// Return `true` when the payload stores a shared string.
    pub const fn is_shared(self) -> bool {
        let meta = self.meta();
        (meta & layout::NEEDS_DROP_MASK) != 0
    }

    /// Return `true` when the payload stores an inline string.
    pub const fn is_inline(self) -> bool {
        let meta = self.meta();
        (meta & layout::INLINE_MASK) != 0
    }

    /// Return the stored string length in bytes.
    pub const fn len(self) -> usize {
        let meta = self.meta();
        if (meta & layout::INLINE_MASK) != 0 {
            encode::inline_len_from_meta(meta)
        } else {
            encode::decode_borrowed_or_shared_len(meta)
        }
    }

    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Expose the raw fields as a tuple.
    pub const fn into_fields(self) -> (*const u8, usize) {
        (self.raw_ptr, self.meta())
    }

    /// Return the pointer word as a raw pointer.
    #[inline]
    pub const fn as_ptr(&self) -> *const u8 {
        self.raw_ptr
    }

    pub(crate) const fn tag(self) -> StateTag {
        let meta = self.meta();
        encode::from_meta(meta)
    }

    /// Convert a non-inline payload into a raw `*const str`.
    ///
    /// # Safety
    ///
    /// The payload must not be inline. The pointer/length pair must still
    /// refer to a live UTF-8 allocation or borrowed string for the duration of
    /// the returned raw pointer's use.
    pub(crate) const unsafe fn into_raw_non_inline(self) -> *const str {
        let meta = self.meta();
        let len = encode::decode_borrowed_or_shared_len(meta);
        ptr::slice_from_raw_parts(self.raw_ptr, len) as *const str
    }
}

impl fmt::Debug for RawParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawParts")
            .field("raw_ptr", &self.raw_ptr)
            .field("meta", &self.meta)
            .field("len", &self.len())
            .field(
                "state",
                &match self.tag() {
                    StateTag::Borrowed => "Borrowed",
                    StateTag::Shared => "Shared",
                    StateTag::Inline => "Inline",
                },
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::mem::size_of;

    #[test]
    fn verify_niche_optimization() {
        assert_eq!(size_of::<RawParts>(), 2 * size_of::<usize>());
        assert_eq!(size_of::<Option<RawParts>>(), size_of::<RawParts>());

        let parts = RawParts::pack_inline("hello");
        assert!(parts.meta() != 0);
    }
}
