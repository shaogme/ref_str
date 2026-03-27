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

`ref_str` provides compact string types for `no_std` Rust.

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

`LocalRefStr<'a>` and `RefStr<'a>` store either a borrowed `&'a str` or an owned shared string, while keeping the representation compact and clone-friendly.

## Why Two Types

- `LocalRefStr<'a>` is optimized for single-threaded code and uses `Rc<str>` when it needs shared ownership.
- `RefStr<'a>` is the thread-safe counterpart and uses `Arc<str>` when it needs shared ownership.
- Both types expose the same high-level API, so you can switch between them without changing your call sites much.

## API

| Item | Purpose |
| --- | --- |
| `LocalRefStr<'a>` | Compact string backed by `Rc<str>` when shared |
| `RefStr<'a>` | Compact string backed by `Arc<str>` when shared |
| `new(&str)` | Build a borrowed value |
| `from_str(&str)` | Alias of `new` |
| `from_shared(...)` | Build from `Rc<str>` or `Arc<str>` |
| `from_static(&'static str)` | Build from a static string |
| `into_raw_parts()` | Split into `(raw_ptr, len, tag)` |
| `into_raw()` | Convert into a raw `*const str` |
| `into_bytes()` | Convert into `Vec<u8>` |
| `into_boxed_str()` | Convert into `Box<str>` |
| `into_string()` | Convert into `String` |

## Conversion Map

```text
&str / String / Box<str> / Cow<str>
            │
            ▼
   LocalRefStr<'a>  <──────►  RefStr<'a>
            │                   │
            ├──── into_bytes ───┤
            ├─ into_boxed_str ──┤
            └── into_string  ───┘
```

## Safety Boundaries

- `into_raw_parts`, `into_raw`, and `increment_strong_count` are `unsafe` because they hand ownership or reference-count control to the caller.
- `from_raw_parts` is `unsafe` because the caller must provide a valid non-null pointer and a correct length/tag combination.
- Conversions between `LocalRefStr` and `RefStr` preserve borrowed strings without allocation, but shared strings are re-materialized into the target backend.

## Example

```rust
extern crate alloc;

use alloc::string::String;
use ref_str::{LocalRefStr, RefStr};

let local: LocalRefStr<'_> = String::from("hello").into();
let shared: RefStr<'_> = String::from("world").into();

assert_eq!(local.as_str(), "hello");
assert_eq!(shared.as_str(), "world");

let back: LocalRefStr<'_> = shared.into();
assert_eq!(back.as_str(), "world");
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
use alloc::rc::Rc;
use ref_str::LocalRefStr;

let value = LocalRefStr::from_shared(Rc::from("hello"));
assert!(value.is_shared());
assert_eq!(value.as_str(), "hello");
```

Raw:

```rust
use alloc::sync::Arc;
use ref_str::RefStr;

let value = RefStr::from_shared(Arc::from("hello"));
let (raw_ptr, len, tag) = unsafe { RefStr::into_raw_parts(value) };
let value = unsafe { RefStr::from_raw_parts(raw_ptr, len, tag) };
assert_eq!(value.as_str(), "hello");
```

Cow:

```rust
use alloc::borrow::Cow;
use ref_str::RefStr;

let value: RefStr<'_> = Cow::Borrowed("hello").into();
assert_eq!(value.as_str(), "hello");
```

## Notes

- This crate is `no_std` and depends on `alloc`.
- The `std` feature does not enable `serde` by itself; it only forwards `serde/std` when `serde` is already enabled.
- The `arbitrary` feature enables `Arbitrary` support for fuzzing and property testing.
- The raw-pointer APIs are intentionally `unsafe`.

## License

Dual licensed under [MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE).
