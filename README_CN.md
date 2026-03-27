# ref_str

![Crates.io](https://img.shields.io/crates/v/ref_str)
![Docs.rs](https://img.shields.io/docsrs/ref_str)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)

<p>
  <a href="https://crates.io/crates/ref_str">crates.io</a> |
  <a href="https://docs.rs/ref_str">docs.rs</a> |
  <a href="./README.md">English</a> |
  <a href="./LICENSE-MIT">MIT License</a> |
  <a href="./LICENSE-APACHE">Apache-2.0 License</a>
</p>

`ref_str` 为 `no_std` Rust 提供紧凑的字符串类型。

## 安装

```toml
[dependencies]
ref_str = "0.1"
```

## 概览

`LocalRefStr<'a>` 和 `RefStr<'a>` 可以在借用的 `&'a str` 与共享拥有的字符串之间切换，同时保持紧凑布局。

## 为什么有两个类型

- `LocalRefStr<'a>` 面向单线程场景，在需要共享拥有时使用 `Rc<str>`。
- `RefStr<'a>` 是线程安全版本，在需要共享拥有时使用 `Arc<str>`。
- 两者提供相同的高层 API，便于在不同并发模型之间切换。

## API

| 项目 | 作用 |
| --- | --- |
| `LocalRefStr<'a>` | 共享态下使用 `Rc<str>` 的紧凑字符串 |
| `RefStr<'a>` | 共享态下使用 `Arc<str>` 的紧凑字符串 |
| `new(&str)` | 从借用字符串构造 |
| `from_str(&str)` | `new` 的别名 |
| `from_shared(...)` | 从 `Rc<str>` 或 `Arc<str>` 构造 |
| `from_static(&'static str)` | 从静态字符串构造 |
| `into_raw_parts()` | 拆成 `(raw_ptr, len, tag)` |
| `into_raw()` | 转成原始 `*const str` |
| `into_bytes()` | 转成 `Vec<u8>` |
| `into_boxed_str()` | 转成 `Box<str>` |
| `into_string()` | 转成 `String` |

## 转换图

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

## 安全边界

- `into_raw_parts`、`into_raw`、`increment_strong_count` 都是 `unsafe`，因为它们把所有权或引用计数控制权交给调用方。
- `from_raw_parts` 也是 `unsafe`，因为调用方必须提供合法、非空的指针和正确的长度/标签组合。
- `LocalRefStr` 和 `RefStr` 之间的互转会保留借用态而不分配；共享态则会重新物化为目标后端。

## 示例

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

借用态：

```rust
use ref_str::LocalRefStr;

let value = LocalRefStr::from("hello");
assert!(value.is_borrowed());
assert_eq!(value.as_str(), "hello");
```

共享态：

```rust
use alloc::rc::Rc;
use ref_str::LocalRefStr;

let value = LocalRefStr::from_shared(Rc::from("hello"));
assert!(value.is_shared());
assert_eq!(value.as_str(), "hello");
```

原始值：

```rust
use alloc::sync::Arc;
use ref_str::RefStr;

let value = RefStr::from_shared(Arc::from("hello"));
let (raw_ptr, len, tag) = unsafe { RefStr::into_raw_parts(value) };
let value = unsafe { RefStr::from_raw_parts(raw_ptr, len, tag) };
assert_eq!(value.as_str(), "hello");
```

Cow：

```rust
use alloc::borrow::Cow;
use ref_str::RefStr;

let value: RefStr<'_> = Cow::Borrowed("hello").into();
assert_eq!(value.as_str(), "hello");
```

## 说明

- 本 crate 是 `no_std`，依赖 `alloc`
- 原始指针相关接口是 `unsafe`

## 许可证

采用 [MIT](./LICENSE-MIT) 或 [Apache-2.0](./LICENSE-APACHE) 双许可证。
