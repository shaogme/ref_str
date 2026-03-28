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

`ref_str` 为 `no_std` Rust 提供“借用或共享拥有”的紧凑字符串类型。

## 安装

```toml
[dependencies]
ref_str = "0.1"
```

启用 serde：

```toml
[dependencies]
ref_str = { version = "0.1", features = ["serde"] }
```

启用 serde + std：

```toml
[dependencies]
ref_str = { version = "0.1", features = ["serde", "std"] }
```

启用 arbitrary：

```toml
[dependencies]
ref_str = { version = "0.1", features = ["arbitrary"] }
```

## 概览

`LocalRefStr<'a>` 和 `RefStr<'a>` 可以在借用的 `&'a str` 与共享拥有的字符串之间切换，同时保持紧凑布局。`LocalStaticRefStr` 和 `StaticRefStr` 则提供了专门的 `'static` wrapper，在保持相同布局与 API 形状的同时，显式区分 static-only 的 serde/arbitrary 语义。

四个公开类型共享同一套核心语义：

- 值始终处于“借用”或“共享”两种状态之一。
- 借用态保留原始生命周期，不分配。
- 共享态通过 `Rc<str>` 或 `Arc<str>` 持有数据，克隆时只增加引用计数。
- 内部表示保持紧凑：数据指针加上打包后的长度/标签元数据。

## 为什么有四个类型

- `LocalRefStr<'a>` 面向单线程场景，在需要共享拥有时使用 `Rc<str>`。
- `RefStr<'a>` 是线程安全版本，在需要共享拥有时使用 `Arc<str>`。
- `LocalStaticRefStr` 和 `StaticRefStr` 分别对应上述两种后端，但语义固定为 `'static`，不再依赖 `RefStr<'static>` 这类别名来表达。
- 普通 `'a` wrapper 可以直接反序列化借用字符串，而 static wrapper 在 `Deserialize` 和 `Arbitrary` 路径上都会物化成 shared owned 字符串。

## API

| 项目 | 作用 |
| --- | --- |
| `LocalRefStr<'a>` | 共享态下使用 `Rc<str>` 的紧凑字符串 |
| `RefStr<'a>` | 共享态下使用 `Arc<str>` 的紧凑字符串 |
| `LocalStaticRefStr` | 共享态下使用 `Rc<str>` 的静态紧凑字符串 |
| `StaticRefStr` | 共享态下使用 `Arc<str>` 的静态紧凑字符串 |
| `new(&str)` | 从借用字符串构造 |
| `from_str(&str)` | `new` 的别名 |
| `from_owned_like(impl AsRef<str>)` | 从字符串类输入分配并强制构造共享态 |
| `from_shared(...)` | 从 `Rc<str>` 或 `Arc<str>` 构造 |
| `from_static(&'static str)` | 构造借用态的 static wrapper |
| `is_borrowed()` / `is_shared()` | 检查当前存储状态 |
| `len()` / `is_empty()` | 查询字符串长度 |
| `as_str()` / `as_cow()` | 借用为 `&str` 或转成 `Cow<str>`；shared 态下 `as_cow()` 会复制 |
| `into_cow()` | 转成借用或拥有的 `Cow<str>` |
| `into_bytes()` | 转成 `Vec<u8>` |
| `into_boxed_str()` | 转成 `Box<str>` |
| `into_string()` | 转成 `String` |
| `into_str_unchecked()` | 不检查状态直接取出 `&str` |
| `==` / `PartialEq` | 直接与 `&str`、`String`、`Cow<str>`、`Rc<str>`、`Arc<str>` 做内容比较 |

## 转换图

```text
                借用/共享输入
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
   LocalRefStr<'a>  <──────►  RefStr<'a>
          │                       │
          ▼                       ▼
 LocalStaticRefStr <──────►  StaticRefStr
```

## 分配语义

- `as_cow()` 只在借用态下是零分配的；如果当前是 shared 态，它会返回 `Cow::Owned`，因此会复制字符串内容。
- `into_cow()` 也遵循同样规则：借用态保持借用，shared 态转成拥有型字符串。
- `LocalRefStr` 和 `RefStr` 之间的互转在借用态下不会分配。
- `LocalRefStr` 和 `RefStr` 之间的互转如果源值已经是 shared 态，则会重新分配并复制，因为 `Rc<str>` 和 `Arc<str>` 后端不同。

## 安全边界

- `from_raw_parts` 也是 `unsafe`，因为调用方必须提供合法、非空的指针和正确的长度/标签组合。
- `into_str_unchecked` 也是 `unsafe`，因为它只在当前值确实处于借用态时才是健全的。
- `LocalStaticRefStr` 和 `StaticRefStr` 在反序列化时不会产出借用态；非 `'static` 输入会统一转成 shared owned 存储。
- `from_owned_like` 总是构造共享态，即使输入本身是 `&str`。

## 示例

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

## 示例

借用态：

```rust
use ref_str::LocalRefStr;

let value = LocalRefStr::from("hello");
assert!(value.is_borrowed());
assert_eq!(value.as_str(), "hello");
```

共享态：

```rust
# extern crate alloc;
use alloc::rc::Rc;
use ref_str::LocalRefStr;

let value = LocalRefStr::from_shared(Rc::from("hello"));
assert!(value.is_shared());
assert_eq!(value.as_str(), "hello");
```

## 高级 Raw Pointer API

这些接口主要面向 FFI 或低层所有权移交场景：

- `into_raw_parts()`
- `from_raw_parts()`
- `into_raw()`
- `into_raw_shared()`
- `increment_strong_count()`

`into_raw()` 是刻意保持底层语义的接口：它返回的 `*const str` 可能指向 borrowed 数据，也可能指向 shared 后端存储。如果你需要“确定来自共享存储”的原始指针，应优先使用 `into_raw_shared()`。把一个来自 borrowed 值的 `into_raw()` 指针传给 `increment_strong_count()` 属于未定义行为。

这里的 `unsafe` API 会直接暴露内部打包表示或共享指针的所有权规则。

原始值：

```rust
# extern crate alloc;
use alloc::sync::Arc;
use ref_str::RefStr;

let value = RefStr::from_shared(Arc::from("hello"));
let (raw_ptr, len, tag) = unsafe { RefStr::into_raw_parts(value) };
let value = unsafe { RefStr::from_raw_parts(raw_ptr, len, tag) };
assert_eq!(value.as_str(), "hello");
```

Cow：

```rust
# extern crate alloc;
use alloc::borrow::Cow;
use ref_str::RefStr;

let value: RefStr<'_> = Cow::Borrowed("hello").into();
assert_eq!(value.as_str(), "hello");
```

比较：

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

Static：

```rust
use ref_str::LocalStaticRefStr;

let value = LocalStaticRefStr::from_static("hello");
assert!(value.is_borrowed());
assert_eq!(value.as_str(), "hello");
```

强制共享态：

```rust
use ref_str::RefStr;

let value = RefStr::from_owned_like("hello");
assert!(value.is_shared());
assert_eq!(value.as_str(), "hello");
```

## 说明

- 本 crate 是 `no_std`，依赖 `alloc`
- `std` feature 本身不会启用 serde，只会在 serde 已启用时透传 `serde/std`
- `arbitrary` feature 会为模糊测试和性质测试启用 `Arbitrary`
- `RefStr<'a>` / `LocalRefStr<'a>` 在 `Deserialize` 和 `Arbitrary` 路径上可以保留借用态，而 `StaticRefStr` / `LocalStaticRefStr` 会统一物化为 shared owned 字符串
- `from_owned_like`、`String`、`Box<str>`、`Rc<str>`、`Arc<str>` 这些构造路径都会产生共享态
- `Default::default()` 会为四个 wrapper 都创建空字符串的借用态值

## 许可证

采用 [MIT](./LICENSE-MIT) 或 [Apache-2.0](./LICENSE-APACHE) 双许可证。
