use super::layout;
use super::layout::StateTag;
use ::core::hint::unreachable_unchecked;

/// Return `true` when a string length fits inline storage.
#[inline]
pub(crate) const fn supports_inline_len(len: usize) -> bool {
    len <= layout::INLINE_CAPACITY
}

/// Compute the cached short hash used to speed up equality checks.
///
/// This is a small FNV-1a style hash over the UTF-8 byte sequence. It is not a
/// cryptographic hash and is only used as a fast negative filter.
#[inline]
pub(crate) const fn short_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut index = 0;

    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        index += 1;
    }

    hash
}

/// Encode the inline tag byte for the final byte of the inline payload.
///
/// The byte packs the inline state tag, ASCII flag, and the inline length.
#[inline]
pub(crate) const fn inline_tag_byte(len: usize, is_ascii: bool) -> u8 {
    debug_assert!(len <= layout::INLINE_CAPACITY);
    let tag = ((tag_bits(StateTag::Inline) >> layout::TAG_SHIFT) as u8)
        | if is_ascii { 0b1000 } else { 0 };

    #[cfg(target_endian = "little")]
    {
        (tag << 4) | (len as u8)
    }

    #[cfg(target_endian = "big")]
    {
        ((len as u8) << 4) | tag
    }
}

/// Decode the inline length from the inline tag byte.
#[inline]
pub(crate) const fn inline_len_from_tag_byte(tag_byte: u8) -> usize {
    #[cfg(target_endian = "little")]
    {
        (tag_byte & 0b0000_1111) as usize
    }

    #[cfg(target_endian = "big")]
    {
        ((tag_byte & 0b1111_0000) >> 4) as usize
    }
}

/// Decode the inline length from the metadata word.
#[inline]
pub(crate) const fn inline_len_from_meta(meta: usize) -> usize {
    inline_len_from_tag_byte(meta.to_ne_bytes()[layout::META_TAG_BYTE_INDEX])
}

/// Decode the length for borrowed or shared payloads from the metadata word.
#[inline]
pub(crate) const fn decode_borrowed_or_shared_len(meta: usize) -> usize {
    #[cfg(target_endian = "little")]
    {
        // On little-endian targets, the length already sits in the low bits.
        meta & layout::LEN_MASK
    }

    #[cfg(target_endian = "big")]
    {
        (meta & layout::LEN_MASK) >> layout::LEN_SHIFT
    }
}

/// Decode the cached hash from the metadata word.
#[inline]
pub(crate) const fn decode_cached_hash(meta: usize) -> usize {
    (meta & layout::HASH_MASK) >> layout::HASH_SHIFT
}

/// Encode length, state tag, and cached hash into a metadata word.
#[inline]
pub(crate) const fn encode_len_tag_hash(len: usize, tag: StateTag, hash: u64) -> usize {
    assert!(
        len <= layout::MAX_BORROWED_OR_SHARED_LEN,
        "string too large to compress"
    );
    let hash = (hash as usize & layout::HASH_VALUE_MASK) << layout::HASH_SHIFT;
    #[cfg(target_endian = "little")]
    {
        // Keep the low bits as the raw length for the LE fast path.
        len | tag_bits(tag) | hash
    }

    #[cfg(target_endian = "big")]
    {
        (len << layout::LEN_SHIFT) | tag_bits(tag) | hash
    }
}

/// Return the bit pattern associated with a state tag.
#[inline]
pub(crate) const fn tag_bits(tag: StateTag) -> usize {
    match tag {
        StateTag::Borrowed => 0b01usize << layout::TAG_SHIFT,
        StateTag::Shared => 0b0101usize << layout::TAG_SHIFT,
        StateTag::Inline => 0b11usize << layout::TAG_SHIFT,
    }
}

/// Decode a tag from a metadata word when the tag bits are known to be valid.
#[inline]
pub(crate) const fn from_meta_checked(meta: usize) -> Option<StateTag> {
    const BORROWED_BITS: usize = 0b0001usize << layout::TAG_SHIFT;
    const SHARED_BITS: usize = 0b0101usize << layout::TAG_SHIFT;
    const INLINE_BITS: usize = 0b0011usize << layout::TAG_SHIFT;

    match meta & layout::STATE_MASK {
        BORROWED_BITS => Some(StateTag::Borrowed),
        SHARED_BITS => Some(StateTag::Shared),
        INLINE_BITS => Some(StateTag::Inline),
        _ => unsafe { unreachable_unchecked() },
    }
}

/// Decode a tag from a metadata word.
///
/// Panics if the tag bits are invalid, which should only happen if the
/// metadata was corrupted or manually forged incorrectly.
#[inline]
pub(crate) const fn from_meta(meta: usize) -> StateTag {
    match from_meta_checked(meta) {
        Some(tag) => tag,
        None => panic!("invalid tag for compressed string"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_length_roundtrip() {
        let len = 123usize;
        let hash = short_hash(b"hash-cache");
        let meta = encode_len_tag_hash(len, StateTag::Borrowed, hash);

        assert_eq!(decode_borrowed_or_shared_len(meta), len);

        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(decode_cached_hash(meta), (hash as usize) & 0xFF_FF_FF);
        }

        #[cfg(not(target_pointer_width = "64"))]
        {
            assert_eq!(decode_cached_hash(meta), 0);
        }
    }

    #[test]
    fn inline_tag_roundtrip() {
        #[cfg(target_pointer_width = "64")]
        let len = 11usize;

        #[cfg(not(target_pointer_width = "64"))]
        let len = layout::INLINE_CAPACITY;

        let meta = inline_tag_byte(len, true);
        assert_eq!(inline_len_from_tag_byte(meta), len);
    }
}
