//! Compressed borrowed-or-shared string types.
//!
//! `LocalRefStr` uses `Rc<str>` for shared ownership, while `RefStr`
//! uses `Arc<str>`.

#![no_std]

extern crate alloc;

mod backend;

/// A compressed string backed by `Rc<str>` when shared.
pub type LocalRefStr<'a> = backend::CompressedRefStr<'a, backend::LocalBackend>;
/// A compressed string backed by `Arc<str>` when shared.
pub type RefStr<'a> = backend::CompressedRefStr<'a, backend::SharedBackend>;
