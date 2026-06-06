//! Compact borrowed-or-shared string types for `no_std` Rust.
//!
//! `ref_str` stores either:
//! - a borrowed `&str`,
//! - an inline short string stored inside the value itself, or
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
//! owned storage, even when the input format exposes borrowed strings.
//!
//! # Borrowed vs Shared
//!
//! Each value is always in exactly one of three states:
//! - borrowed, exposed by [`is_borrowed`](RefStr::is_borrowed)
//! - inline, exposed by [`is_inline`](RefStr::is_inline)
//! - shared, exposed by [`is_shared`](RefStr::is_shared)
//!
//! Borrowed values preserve the original lifetime and can be recovered as `&str`
//! or `Cow<'a, str>` without allocation. Inline values own short strings inside
//! the compact two-word payload. Shared values own their backing allocation
//! through `Arc<str>` or `Rc<str>`, and cloning them only bumps the reference
//! count.
//!
//! # Common Operations
//!
//! All four public wrappers expose the same core operations:
//! - constructors such as [`new`](RefStr::new), [`from_str`](RefStr::from_str),
//!   [`from_owned_like`](RefStr::from_owned_like), [`from_shared`](RefStr::from_shared),
//!   and [`from_static`](StaticRefStr::from_static) for the dedicated static types
//! - state inspection via [`is_borrowed`](RefStr::is_borrowed),
//!   [`is_inline`](RefStr::is_inline), [`is_shared`](RefStr::is_shared),
//!   [`is_ascii`](RefStr::is_ascii), [`len`](RefStr::len), and
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
//! Most of these APIs are `unsafe`: callers must preserve the original backend,
//! ownership rules, and encoded tag values.
//!
//! [`into_raw`](RefStr::into_raw) is intentionally low-level: its returned
//! `*const str` may point to borrowed data or shared backend storage. Inline
//! values materialize shared storage before producing a raw pointer. If you
//! need a raw pointer that is guaranteed to come from shared storage, prefer
//! [`into_raw_shared`](RefStr::into_raw_shared). Passing a borrowed pointer from
//! `into_raw` into
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
//! let inline = RefStr::from(String::from("world"));
//! assert!(inline.is_inline());
//! assert_eq!(inline.clone().into_string(), "world");
//!
//! let shared = RefStr::from(String::from("this string is definitely shared"));
//! assert!(shared.is_shared());
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

#[macro_use]
mod macros;

mod arch;
mod core;
mod local;
mod raw;
mod shared;

pub use crate::local::{LocalRefStr, LocalStaticRefStr};
pub use crate::raw::RawParts;
pub use crate::shared::{RefStr, StaticRefStr};
