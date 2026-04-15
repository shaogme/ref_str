# ref_str

![Crates.io](https://img.shields.io/crates/v/ref_str)
![Docs.rs](https://img.shields.io/docsrs/ref_str)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

<p>
  <a href="https://crates.io/crates/ref_str">crates.io</a> |
  <a href="https://docs.rs/ref_str">docs.rs</a> |
  <a href="./README_CN.md">中文文档</a> |
  <a href="./LICENSE-MIT">MIT License</a> |
  <a href="./LICENSE-APACHE">Apache-2.0 License</a>
</p>

`ref_str` provides compact borrowed-or-shared string types for `no_std` Rust.

## Install

```toml
[dependencies]
ref_str = "0.1"
```

With serde support:

```toml
[dependencies]
ref_str = { version = "0.1", features = ["serde"] }
```

With serde + std support:

```toml
[dependencies]
ref_str = { version = "0.1", features = ["serde", "std"] }
```

With arbitrary support:

```toml
[dependencies]
ref_str = { version = "0.1", features = ["arbitrary"] }
```

## Overview

`LocalRefStr<'a>` and `RefStr<'a>` store either a borrowed `&'a str` or a shared owned string while keeping the representation compact and clone-friendly. `LocalStaticRefStr` and `StaticRefStr` provide dedicated `'static` wrappers with the same layout and API shape, but with explicit static-only serde/arbitrary semantics.

All four public types share the same core model:

- Values are always either borrowed or shared.
- Borrowed values keep the original lifetime and avoid allocation.
- Shared values clone cheaply by bumping an `Rc<str>` or `Arc<str>` strong count.
- The internal representation is compact: a data pointer plus packed length/tag metadata.

## Why Four Types

- `LocalRefStr<'a>` is optimized for single-threaded code and uses `Rc<str>` when it needs shared ownership.
- `RefStr<'a>` is the thread-safe counterpart and uses `Arc<str>` when it needs shared ownership.
- `LocalStaticRefStr` and `StaticRefStr` mirror those two backends for `'static` strings, so static-only behavior is expressed by a real wrapper type instead of by aliasing `RefStr<'static>`.
- The `'a` wrappers can deserialize borrowed strings directly, while the static wrappers always materialize shared owned strings when deserializing or generating `Arbitrary` values.

## API

| Item | Purpose |
| --- | --- |
| `LocalRefStr<'a>` | Compact string backed by `Rc<str>` when shared |
| `RefStr<'a>` | Compact string backed by `Arc<str>` when shared |
| `LocalStaticRefStr` | Static compact string backed by `Rc<str>` when shared |
| `StaticRefStr` | Static compact string backed by `Arc<str>` when shared |
| `new(&str)` | Build a borrowed value |
| `from_str(&str)` | Alias of `new` |
| `from_owned_like(impl AsRef<str>)` | Always allocate and build a shared value from string-like input |
| `from_shared(...)` | Build from `Rc<str>` or `Arc<str>` |
| `from_static(&'static str)` | Build a borrowed static wrapper |
| `to_static_str()` | Promote to `'static` variant; clones shared or allocates borrowed |
| `into_static_str()` | Consume and promote to `'static`; transfers shared or allocates borrowed |
| `is_borrowed()` / `is_shared()` | Inspect the current storage mode |
| `len()` / `is_empty()` | Inspect string length |
| `as_str()` / `as_cow()` | Borrow as `&str` or convert to `Cow<str>`; `as_cow()` clones when shared |
| `into_cow()` | Convert into borrowed-or-owned `Cow<str>` |
| `into_bytes()` | Convert into `Vec<u8>` |
| `into_boxed_str()` | Convert into `Box<str>` |
| `into_string()` | Convert into `String` |
| `into_str_unchecked()` | Extract `&str` without verifying borrowed state |
| `==` / `PartialEq` | Compare directly with `&str`, `String`, `Cow<str>`, `Rc<str>`, and `Arc<str>` |

## Conversion Map

```text
                borrowed/shared input
                         │
          ┌──────────────┴──────────────┐
          ▼                             ▼
   LocalRefStr<'a>  <──────►  RefStr<'a>
          │                             │
          ▼                             ▼
 LocalStaticRefStr <──────►  StaticRefStr
```

## Allocation Notes

- `as_cow()` is allocation-free only for borrowed values. Shared values are converted into `Cow::Owned`, so the string contents are cloned.
- `into_cow()` follows the same rule: borrowed values stay borrowed, while shared values become owned strings.
- Conversions between `LocalRefStr` and `RefStr` preserve borrowed values without allocation.
- Conversions between `LocalRefStr` and `RefStr` allocate and copy when the source is already shared, because `Rc<str>` and `Arc<str>` use different backends.
- `to_static_str()` and `into_static_str()` only allocate when the source is in the borrowed state. If the value is already shared, they perform a cheap reference count increment or an ownership transfer.

## Safety Boundaries

- `from_raw_parts` is `unsafe` because the caller must provide a valid non-null pointer and a correct length/tag combination.
- `into_str_unchecked` is `unsafe` because it is only sound for values that are currently borrowed.
- `LocalStaticRefStr` and `StaticRefStr` never deserialize into borrowed values; non-`'static` input is always converted into shared owned storage.
- `from_owned_like` always constructs the shared representation, even if the input starts as `&str`.

## Example

```rust
extern crate alloc;

use alloc::string::String;
use ref_str::{LocalRefStr, RefStr, StaticRefStr};

let local: LocalRefStr<'_> = String::from("hello").into();
let shared: RefStr<'_> = String::from("world").into();

assert_eq!(local.as_str(), "hello");
assert_eq!(shared.as_str(), "world");

let back: LocalRefStr<'_> = shared.into();
assert_eq!(back.as_str(), "world");

let static_value = StaticRefStr::from_static("literal");
assert!(static_value.is_borrowed());

let forced_shared = RefStr::from_owned_like("shared");
assert!(forced_shared.is_shared());
```

## Examples

Borrowed:

```rust
use ref_str::LocalRefStr;

let value = LocalRefStr::from("hello");
assert!(value.is_borrowed());
assert_eq!(value.as_str(), "hello");
```

Shared:

```rust
# extern crate alloc;
use alloc::rc::Rc;
use ref_str::LocalRefStr;

let value = LocalRefStr::from_shared(Rc::from("hello"));
assert!(value.is_shared());
assert_eq!(value.as_str(), "hello");
```

## Advanced Raw Pointer APIs

These APIs are intended for FFI or other low-level ownership transfer cases:

- `into_raw_parts()`
- `from_raw_parts()`
- `into_raw()`
- `into_raw_shared()`
- `increment_strong_count()`

`into_raw()` is intentionally low-level: its returned `*const str` is ambiguous and may point to either borrowed data or shared backend storage. If you need a pointer that is guaranteed to come from shared storage, prefer `into_raw_shared()`. Passing a borrowed pointer from `into_raw()` into `increment_strong_count()` is undefined behavior.

The `unsafe` APIs here expose the packed representation or raw shared-pointer ownership rules directly.

Raw:

```rust
# extern crate alloc;
use alloc::sync::Arc;
use ref_str::RefStr;

let value = RefStr::from_shared(Arc::from("hello"));
let (raw_ptr, len, tag) = unsafe { RefStr::into_raw_parts(value) };
let value = unsafe { RefStr::from_raw_parts(raw_ptr, len, tag) };
assert_eq!(value.as_str(), "hello");
```

Cow:

```rust
# extern crate alloc;
use alloc::borrow::Cow;
use ref_str::RefStr;

let value: RefStr<'_> = Cow::Borrowed("hello").into();
assert_eq!(value.as_str(), "hello");
```

Comparisons:

```rust
# extern crate alloc;
use alloc::borrow::Cow;
use alloc::rc::Rc;
use alloc::sync::Arc;
use ref_str::RefStr;

let value = RefStr::from("hello");
assert!(value == Cow::Borrowed("hello"));
assert!(value == Arc::<str>::from("hello"));
assert!(value == Rc::<str>::from("hello"));
```

Static:

```rust
use ref_str::LocalStaticRefStr;

let value = LocalStaticRefStr::from_static("hello");
assert!(value.is_borrowed());
assert_eq!(value.as_str(), "hello");
```

Forced shared:

```rust
use ref_str::RefStr;

let value = RefStr::from_owned_like("hello");
assert!(value.is_shared());
assert_eq!(value.as_str(), "hello");
```

Lifetime Promotion:

```rust
use ref_str::RefStr;

let s = String::from("hello");
let borrowed = RefStr::from(s.as_str()); 

// Promote to StaticRefStr (allocates because it was borrowed)
let static_val = borrowed.to_static_str();
assert!(static_val.is_shared());
```

## Notes

- This crate is `no_std` and depends on `alloc`.
- The `std` feature does not enable `serde` by itself; it only forwards `serde/std` when `serde` is already enabled.
- The `arbitrary` feature enables `Arbitrary` support for fuzzing and property testing.
- `RefStr<'a>` / `LocalRefStr<'a>` may deserialize or generate borrowed values, while `StaticRefStr` / `LocalStaticRefStr` always materialize shared owned strings in those paths.
- `from_owned_like`, `String`, `Box<str>`, `Rc<str>`, and `Arc<str>` constructors all create shared values.
- `Default::default()` creates an empty borrowed value for all four wrappers.

## License

Dual licensed under [MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE).
