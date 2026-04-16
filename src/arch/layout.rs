use ::core::mem::size_of;

/// Encoded state discriminator stored inside the metadata word.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum StateTag {
    /// Borrowed UTF-8 data, backed by a caller-owned lifetime.
    Borrowed,
    /// Shared heap-backed string storage.
    Shared,
    /// Inline short string stored inside the two-word payload.
    Inline,
}

/// Number of bits reserved for the state tag.
pub(crate) const TAG_BITS: u32 = 4;

#[cfg(target_pointer_width = "64")]
/// Number of bits available for the short hash cache on 64-bit targets.
pub(crate) const HASH_BITS: u32 = 24;
#[cfg(not(target_pointer_width = "64"))]
/// No hash cache is stored on narrow targets.
pub(crate) const HASH_BITS: u32 = 0;

/// Size of one machine word in bytes.
pub(crate) const WORD_BYTES: usize = size_of::<usize>();
/// Total inline payload size in bytes, matching two machine words.
pub(crate) const INLINE_TOTAL_BYTES: usize = WORD_BYTES * 2;
/// Maximum number of UTF-8 bytes that can be stored inline.
pub(crate) const INLINE_CAPACITY: usize = INLINE_TOTAL_BYTES - 1;
/// Byte index that holds the inline tag and inline length.
pub(crate) const BUFFER_TAG_BYTE_INDEX: usize = INLINE_TOTAL_BYTES - 1;
/// Byte index inside the metadata word that carries the state tag.
pub(crate) const META_TAG_BYTE_INDEX: usize = WORD_BYTES - 1;

#[cfg(target_endian = "little")]
pub(crate) const TAG_SHIFT: u32 = usize::BITS - TAG_BITS;
#[cfg(target_endian = "big")]
pub(crate) const TAG_SHIFT: u32 = 0;

#[cfg(target_endian = "little")]
pub(crate) const LEN_SHIFT: u32 = 0;
#[cfg(target_endian = "big")]
pub(crate) const LEN_SHIFT: u32 = TAG_BITS + HASH_BITS + 1;

#[cfg(target_endian = "little")]
pub(crate) const HASH_SHIFT: u32 = usize::BITS - TAG_BITS - HASH_BITS - 1;
#[cfg(target_endian = "big")]
pub(crate) const HASH_SHIFT: u32 = TAG_BITS;

pub(crate) const STATE_MASK: usize = 0b0111usize << TAG_SHIFT;
/// Mask that identifies inline payloads.
pub(crate) const INLINE_MASK: usize = 0b0010usize << TAG_SHIFT;
/// Mask that identifies payloads requiring a destructor.
pub(crate) const NEEDS_DROP_MASK: usize = 0b0100usize << TAG_SHIFT;
/// Mask that identifies the cached ASCII flag.
pub(crate) const IS_ASCII_MASK: usize = 0b1000usize << TAG_SHIFT;
/// Mask for the cached hash bits.
pub(crate) const HASH_MASK: usize = if HASH_BITS == 0 {
    0
} else {
    ((1usize << HASH_BITS) - 1) << HASH_SHIFT
};
pub(crate) const HASH_VALUE_MASK: usize = if HASH_BITS == 0 {
    0
} else {
    (1usize << HASH_BITS) - 1
};
/// Number of bits used for the encoded length field.
pub(crate) const LEN_BITS: u32 = usize::BITS - TAG_BITS - HASH_BITS - 1;
/// Mask for the encoded length field.
pub(crate) const LEN_MASK: usize = ((1usize << LEN_BITS) - 1) << LEN_SHIFT;
/// Maximum length representable in the borrowed/shared encoding.
pub(crate) const MAX_BORROWED_OR_SHARED_LEN: usize = (1usize << LEN_BITS) - 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_pointer_width() {
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(HASH_BITS, 24);
            assert_eq!(LEN_BITS, 35);
            assert_eq!(MAX_BORROWED_OR_SHARED_LEN, (1usize << 35) - 1);
            assert_ne!(HASH_MASK, 0);
        }

        #[cfg(not(target_pointer_width = "64"))]
        {
            assert_eq!(HASH_BITS, 0);
            assert_eq!(LEN_BITS, 27);
            assert_eq!(MAX_BORROWED_OR_SHARED_LEN, (1usize << 27) - 1);
            assert_eq!(HASH_MASK, 0);
        }
    }
}
