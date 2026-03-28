use std::borrow::Cow;
use std::boxed::Box;
use std::string::String;

use ref_str::{LocalRefStr, LocalStaticRefStr, RefStr, StaticRefStr};

macro_rules! lifetime_wrapper_suite {
    (
        $module:ident,
        $ty:ident,
        $shared:ty,
        $make_shared:expr,
        $strong_count:path,
        $from_raw:path,
        $peer:ident
    ) => {
        mod $module {
            use super::*;

            type Sut<'a> = $ty<'a>;
            type Peer<'a> = $peer<'a>;

            #[test]
            fn borrowed_roundtrip() {
                let owned = String::from("borrowed");
                let value = Sut::from(&owned[..]);
                let borrowed_cow = Cow::Borrowed("borrowed");
                let borrowed_arc = std::sync::Arc::<str>::from("borrowed");
                let borrowed_rc = std::rc::Rc::<str>::from("borrowed");

                assert_eq!(value.as_str(), "borrowed");
                assert!(value.is_borrowed());
                assert_eq!(value.as_cow(), Cow::Borrowed("borrowed"));
                assert!(value == &borrowed_cow);
                assert!(value == &borrowed_arc);
                assert!(value == &borrowed_rc);
            }

            #[test]
            fn owned_roundtrip() {
                let from_string: Sut<'_> = String::from("hello").into();
                let from_box: Sut<'_> = Box::<str>::from("boxed").into();
                let from_cow: Sut<'_> = Cow::Borrowed("cow").into();

                assert_eq!(from_string.clone().into_string(), "hello");
                assert_eq!(from_box.into_boxed_str().as_ref(), "boxed");
                assert_eq!(from_cow.as_str(), "cow");
            }

            #[test]
            fn shared_roundtrip() {
                let original: $shared = ($make_shared)("hello");
                let value = Sut::from_shared(original.clone());
                let expected_cow = Cow::Owned(String::from("hello"));

                assert_eq!(value.as_str(), "hello");
                assert!(value.is_shared());
                assert_eq!($strong_count(&original), 2);
                assert!(value == &expected_cow);

                let cloned = value.clone();
                assert_eq!(cloned.as_str(), "hello");
                assert_eq!($strong_count(&original), 3);
            }

            #[test]
            fn reference_lhs_partial_eq_roundtrip() {
                let borrowed_owner = String::from("borrowed");
                let borrowed = Sut::from(&borrowed_owner[..]);
                let borrowed_string = String::from("borrowed");
                let borrowed_cow = Cow::Borrowed("borrowed");
                let borrowed_arc = std::sync::Arc::<str>::from("borrowed");
                let borrowed_rc = std::rc::Rc::<str>::from("borrowed");

                assert!((&borrowed) == borrowed.clone());
                assert!((&borrowed) == "borrowed");
                assert!((&borrowed) == &borrowed_string);
                assert!((&borrowed) == &borrowed_cow);
                assert!((&borrowed) == &borrowed_arc);
                assert!((&borrowed) == &borrowed_rc);

                let shared = Sut::from_shared(($make_shared)("shared"));
                let shared_string = String::from("shared");
                let shared_cow = Cow::Owned(String::from("shared"));
                let shared_arc = std::sync::Arc::<str>::from("shared");
                let shared_rc = std::rc::Rc::<str>::from("shared");

                assert!((&shared) == shared.clone());
                assert!((&shared) == "shared");
                assert!((&shared) == &shared_string);
                assert!((&shared) == &shared_cow);
                assert!((&shared) == &shared_arc);
                assert!((&shared) == &shared_rc);
            }

            #[test]
            fn rhs_reference_partial_eq_roundtrip() {
                let borrowed_owner = String::from("borrowed");
                let borrowed = Sut::from(&borrowed_owner[..]);
                let borrowed_cow = Cow::Borrowed("borrowed");
                let borrowed_arc = std::sync::Arc::<str>::from("borrowed");
                let borrowed_rc = std::rc::Rc::<str>::from("borrowed");

                assert!(borrowed == &borrowed_cow);
                assert!(borrowed == &borrowed_arc);
                assert!(borrowed == &borrowed_rc);

                let shared = Sut::from_shared(($make_shared)("shared"));
                let shared_cow = Cow::Owned(String::from("shared"));
                let shared_arc = std::sync::Arc::<str>::from("shared");
                let shared_rc = std::rc::Rc::<str>::from("shared");

                assert!(shared == &shared_cow);
                assert!(shared == &shared_arc);
                assert!(shared == &shared_rc);
            }

            #[test]
            fn into_raw_shared_disambiguates_state() {
                let borrowed_owner = String::from("borrowed");
                let borrowed = Sut::from(&borrowed_owner[..]);
                assert!(borrowed.into_raw_shared().is_none());

                let original: $shared = ($make_shared)("shared");
                let shared = Sut::from_shared(original.clone());
                let raw = shared
                    .into_raw_shared()
                    .expect("shared value should produce a raw pointer");

                assert_eq!($strong_count(&original), 2);

                unsafe {
                    Sut::increment_strong_count(raw);
                }
                assert_eq!($strong_count(&original), 3);

                unsafe {
                    drop($from_raw(raw));
                }
                assert_eq!($strong_count(&original), 2);
            }

            #[test]
            fn backend_conversion_roundtrip() {
                let shared = Sut::from_shared(($make_shared)("bridge"));
                let peer: Peer<'_> = shared.clone().into();
                let back: Sut<'_> = peer.into();

                assert_eq!(back.as_str(), "bridge");
                assert!(back.is_shared());

                let borrowed_owner = String::from("borrowed");
                let borrowed = Sut::from(&borrowed_owner[..]);
                let peer_borrowed: Peer<'_> = borrowed.into();

                assert_eq!(peer_borrowed.as_str(), "borrowed");
                assert!(peer_borrowed.is_borrowed());
            }

            #[test]
            fn raw_roundtrip() {
                let original: $shared = ($make_shared)("raw");
                let value = Sut::from_shared(original.clone());
                let (raw_ptr, len, tag) = unsafe { Sut::into_raw_parts(value) };

                assert_eq!(len, 3);
                assert_ne!(tag, 0);

                let value = unsafe { Sut::from_raw_parts(raw_ptr, len, tag) };
                let raw = unsafe { Sut::into_raw(value) };

                assert_eq!($strong_count(&original), 2);

                unsafe {
                    Sut::increment_strong_count(raw);
                }
                assert_eq!($strong_count(&original), 3);

                unsafe {
                    drop($from_raw(raw));
                }
                assert_eq!($strong_count(&original), 2);
            }

            #[cfg(feature = "serde")]
            #[test]
            fn serde_roundtrip() {
                use serde::Deserialize;
                use serde::de::value::{
                    BorrowedStrDeserializer, Error as DeError, StringDeserializer,
                };
                use serde_test::{Token, assert_de_tokens, assert_ser_tokens};

                let value = Sut::from("serde");
                assert_ser_tokens(&value, &[Token::Str("serde")]);

                let borrowed: Sut<'_> =
                    Deserialize::deserialize(BorrowedStrDeserializer::<DeError>::new("serde"))
                        .unwrap();
                let owned: Sut<'_> = Deserialize::deserialize(StringDeserializer::<DeError>::new(
                    String::from("serde"),
                ))
                .unwrap();

                assert!(borrowed.is_borrowed());
                assert!(owned.is_shared());
                assert_de_tokens(&borrowed, &[Token::BorrowedStr("serde")]);
            }

            #[cfg(feature = "arbitrary")]
            #[test]
            fn arbitrary_roundtrip() {
                use arbitrary::{Arbitrary, Unstructured};

                let mut shared = Unstructured::new(b"hello\x01\x05");
                let mut borrowed = Unstructured::new(b"hello\x00\x05");

                let shared_value = Sut::arbitrary(&mut shared).unwrap();
                let borrowed_value = Sut::arbitrary(&mut borrowed).unwrap();

                assert_eq!(shared_value.as_str(), "hello");
                assert_eq!(borrowed_value.as_str(), "hello");
                assert!(shared_value.is_shared());
                assert!(borrowed_value.is_borrowed());
            }
        }
    };
}

macro_rules! static_wrapper_suite {
    (
        $module:ident,
        $ty:ident,
        $shared:ty,
        $make_shared:expr,
        $strong_count:path,
        $from_raw:path,
        $peer:ident,
        $generic:ident
    ) => {
        mod $module {
            use super::*;

            type Sut = $ty;
            type Peer = $peer;

            #[test]
            fn borrowed_roundtrip() {
                let value = Sut::from_static("borrowed");
                let default_value: Sut = Default::default();
                let borrowed_cow = Cow::Borrowed("borrowed");
                let borrowed_arc = std::sync::Arc::<str>::from("borrowed");
                let borrowed_rc = std::rc::Rc::<str>::from("borrowed");

                assert_eq!(value.as_str(), "borrowed");
                assert!(value.is_borrowed());
                assert!(default_value.is_borrowed());
                assert_eq!(default_value.as_str(), "");
                assert!(value == &borrowed_cow);
                assert!(value == &borrowed_arc);
                assert!(value == &borrowed_rc);
            }

            #[test]
            fn owned_roundtrip() {
                let from_string: Sut = String::from("hello").into();
                let from_box: Sut = Box::<str>::from("boxed").into();
                let from_cow: Sut = Cow::Borrowed("cow").into();

                assert_eq!(from_string.clone().into_string(), "hello");
                assert_eq!(from_box.into_boxed_str().as_ref(), "boxed");
                assert_eq!(from_cow.as_str(), "cow");
            }

            #[test]
            fn shared_roundtrip() {
                let original: $shared = ($make_shared)("hello");
                let value = Sut::from_shared(original.clone());
                let expected_cow = Cow::Owned(String::from("hello"));

                assert_eq!(value.as_str(), "hello");
                assert!(value.is_shared());
                assert_eq!($strong_count(&original), 2);
                assert!(value == &expected_cow);

                let cloned = value.clone();
                assert_eq!(cloned.as_str(), "hello");
                assert_eq!($strong_count(&original), 3);
            }

            #[test]
            fn reference_lhs_partial_eq_roundtrip() {
                let borrowed = Sut::from_static("borrowed");
                let borrowed_string = String::from("borrowed");
                let borrowed_cow = Cow::Borrowed("borrowed");
                let borrowed_arc = std::sync::Arc::<str>::from("borrowed");
                let borrowed_rc = std::rc::Rc::<str>::from("borrowed");

                assert!((&borrowed) == borrowed.clone());
                assert!((&borrowed) == "borrowed");
                assert!((&borrowed) == &borrowed_string);
                assert!((&borrowed) == &borrowed_cow);
                assert!((&borrowed) == &borrowed_arc);
                assert!((&borrowed) == &borrowed_rc);

                let shared = Sut::from_shared(($make_shared)("shared"));
                let shared_string = String::from("shared");
                let shared_cow = Cow::Owned(String::from("shared"));
                let shared_arc = std::sync::Arc::<str>::from("shared");
                let shared_rc = std::rc::Rc::<str>::from("shared");

                assert!((&shared) == shared.clone());
                assert!((&shared) == "shared");
                assert!((&shared) == &shared_string);
                assert!((&shared) == &shared_cow);
                assert!((&shared) == &shared_arc);
                assert!((&shared) == &shared_rc);
            }

            #[test]
            fn rhs_reference_partial_eq_roundtrip() {
                let borrowed = Sut::from_static("borrowed");
                let borrowed_cow = Cow::Borrowed("borrowed");
                let borrowed_arc = std::sync::Arc::<str>::from("borrowed");
                let borrowed_rc = std::rc::Rc::<str>::from("borrowed");

                assert!(borrowed == &borrowed_cow);
                assert!(borrowed == &borrowed_arc);
                assert!(borrowed == &borrowed_rc);

                let shared = Sut::from_shared(($make_shared)("shared"));
                let shared_cow = Cow::Owned(String::from("shared"));
                let shared_arc = std::sync::Arc::<str>::from("shared");
                let shared_rc = std::rc::Rc::<str>::from("shared");

                assert!(shared == &shared_cow);
                assert!(shared == &shared_arc);
                assert!(shared == &shared_rc);
            }

            #[test]
            fn into_raw_shared_disambiguates_state() {
                let borrowed = Sut::from_static("borrowed");
                assert!(borrowed.into_raw_shared().is_none());

                let original: $shared = ($make_shared)("shared");
                let shared = Sut::from_shared(original.clone());
                let raw = shared
                    .into_raw_shared()
                    .expect("shared value should produce a raw pointer");

                assert_eq!($strong_count(&original), 2);

                unsafe {
                    Sut::increment_strong_count(raw);
                }
                assert_eq!($strong_count(&original), 3);

                unsafe {
                    drop($from_raw(raw));
                }
                assert_eq!($strong_count(&original), 2);
            }

            #[test]
            fn backend_conversion_roundtrip() {
                let shared = Sut::from_shared(($make_shared)("bridge"));
                let peer: Peer = shared.clone().into();
                let back: Sut = peer.into();

                assert_eq!(back.as_str(), "bridge");
                assert!(back.is_shared());
            }

            #[test]
            fn static_generic_roundtrip() {
                let value = Sut::from_static("static");
                let generic: $generic<'static> = value.clone().into();

                assert_eq!(generic.as_str(), "static");
                let back: Sut = generic.into();
                assert_eq!(back.as_str(), "static");
                assert!(back.is_borrowed());
            }

            #[test]
            fn raw_roundtrip() {
                let original: $shared = ($make_shared)("raw");
                let value = Sut::from_shared(original.clone());
                let (raw_ptr, len, tag) = unsafe { Sut::into_raw_parts(value) };

                assert_eq!(len, 3);
                assert_ne!(tag, 0);

                let value = unsafe { Sut::from_raw_parts(raw_ptr, len, tag) };
                let raw = unsafe { Sut::into_raw(value) };

                assert_eq!($strong_count(&original), 2);

                unsafe {
                    Sut::increment_strong_count(raw);
                }
                assert_eq!($strong_count(&original), 3);

                unsafe {
                    drop($from_raw(raw));
                }
                assert_eq!($strong_count(&original), 2);
            }

            #[cfg(feature = "serde")]
            #[test]
            fn serde_roundtrip() {
                use serde::Deserialize;
                use serde::de::value::{
                    BorrowedStrDeserializer, Error as DeError, StringDeserializer,
                };
                use serde_test::{Token, assert_ser_tokens};

                let value = Sut::from_static("serde");
                assert_ser_tokens(&value, &[Token::Str("serde")]);

                let borrowed: Sut =
                    Deserialize::deserialize(BorrowedStrDeserializer::<DeError>::new("serde"))
                        .unwrap();
                let owned: Sut = Deserialize::deserialize(StringDeserializer::<DeError>::new(
                    String::from("serde"),
                ))
                .unwrap();

                assert!(borrowed.is_shared());
                assert!(owned.is_shared());
            }

            #[cfg(feature = "arbitrary")]
            #[test]
            fn arbitrary_roundtrip() {
                use arbitrary::{Arbitrary, Unstructured};

                let mut input = Unstructured::new(b"hello\x01\x05");
                let value = Sut::arbitrary(&mut input).unwrap();

                assert_eq!(value.as_str(), "hello");
                assert!(value.is_shared());
            }
        }
    };
}

lifetime_wrapper_suite!(
    local_ref_str,
    LocalRefStr,
    std::rc::Rc<str>,
    std::rc::Rc::<str>::from,
    std::rc::Rc::strong_count,
    std::rc::Rc::<str>::from_raw,
    RefStr
);

lifetime_wrapper_suite!(
    shared_ref_str,
    RefStr,
    std::sync::Arc<str>,
    std::sync::Arc::<str>::from,
    std::sync::Arc::strong_count,
    std::sync::Arc::<str>::from_raw,
    LocalRefStr
);

static_wrapper_suite!(
    local_static_ref_str,
    LocalStaticRefStr,
    std::rc::Rc<str>,
    std::rc::Rc::<str>::from,
    std::rc::Rc::strong_count,
    std::rc::Rc::<str>::from_raw,
    StaticRefStr,
    LocalRefStr
);

static_wrapper_suite!(
    shared_static_ref_str,
    StaticRefStr,
    std::sync::Arc<str>,
    std::sync::Arc::<str>::from,
    std::sync::Arc::strong_count,
    std::sync::Arc::<str>::from_raw,
    LocalStaticRefStr,
    RefStr
);

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}

#[test]
fn shared_variants_are_send_and_sync() {
    assert_send::<RefStr<'static>>();
    assert_sync::<RefStr<'static>>();
    assert_send::<StaticRefStr>();
    assert_sync::<StaticRefStr>();
}

#[test]
fn as_cow_is_not_tied_to_container_lifetime() {
    fn via_ref_str<'a>(s: &'a str) -> Cow<'a, str> {
        let value = RefStr::from(s);
        value.as_cow()
    }

    fn via_local_ref_str<'a>(s: &'a str) -> Cow<'a, str> {
        let value = LocalRefStr::from(s);
        value.as_cow()
    }

    let owned = String::from("borrowed");
    let cow_ref = via_ref_str(&owned);
    let cow_local = via_local_ref_str(&owned);

    assert_eq!(cow_ref, Cow::Borrowed("borrowed"));
    assert_eq!(cow_local, Cow::Borrowed("borrowed"));
}

#[test]
fn alternate_debug_exposes_state() {
    let borrowed = RefStr::from("hello");
    let shared = RefStr::from(String::from("world"));

    let borrowed_dbg = format!("{:#?}", borrowed);
    let shared_dbg = format!("{:#?}", shared);

    assert!(borrowed_dbg.contains("state: \"Borrowed\""));
    assert!(borrowed_dbg.contains("len: 5"));
    assert!(borrowed_dbg.contains("value: \"hello\""));

    assert!(shared_dbg.contains("state: \"Shared\""));
    assert!(shared_dbg.contains("len: 5"));
    assert!(shared_dbg.contains("value: \"world\""));
}
