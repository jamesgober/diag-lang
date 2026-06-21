# diag-lang &mdash; API Reference

> Complete reference for every public item in `diag-lang`, with examples.
> **Status: pre-1.0 — the surface is built across the 0.x series and frozen at `1.0.0`.** Items marked _(planned)_ are not yet implemented; see [`dev/ROADMAP.md`](../dev/ROADMAP.md).

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [`Severity`](#severity)
- [`Label`](#label)
- [`Diagnostic`](#diagnostic)
- [`Renderer`](#renderer)
- [Re-exports](#re-exports)
- [Feature flags](#feature-flags)

---

## Overview

diag-lang collects compiler diagnostics and renders them with source context: a
severity, a message, and the offending source with a caret underlining exactly
where the problem is. It consumes positions from `span-lang` and source text from
`source-lang`, and produces the kind of error output developers expect from a
modern toolchain.

It owns collection and rendering only. It does not decide *what* is wrong — a
lexer, parser, or type checker builds the `Diagnostic`; diag-lang decides how it
looks.

A [`Label`](#label) points with a [`Span`](#re-exports) into a
[`SourceMap`](#re-exports)'s **global** position space; the renderer resolves that
span back to a file, a line, and a column on its own.

---

## Installation

```toml
[dependencies]
diag-lang = "0.2"
```

---

## `Severity`

The level a diagnostic reports at: `Error`, `Warning`, `Note`, or `Help`. The
variants are ordered most-to-least serious, so a slice of severities sorts with
the errors first.

```rust
use diag_lang::Severity;

assert_eq!(Severity::Error.as_str(), "error");
assert!(Severity::Error < Severity::Warning);
```

| Method | Description |
|--------|-------------|
| `as_str(self) -> &'static str` | The lowercase label (`"error"`, `"warning"`, `"note"`, `"help"`). |

`Severity` is `Copy` and implements `Display` (equal to `as_str`).

---

## `Label`

A `Span` paired with the message explaining it — the thing a diagnostic points
*with*. The span is in the source map's global position space, so the renderer can
resolve which file and line it lands on.

```rust
use diag_lang::{Label, Span};

let label = Label::new(Span::new(8, 11), "expected `Int`, found `Str`");
assert_eq!(label.span(), Span::new(8, 11));

let bare = Label::unlabelled(Span::new(8, 11)); // points without annotating
assert!(bare.message().is_empty());
```

| Method | Description |
|--------|-------------|
| `new(span: Span, message: impl Into<Box<str>>) -> Label` | A label covering `span`, explained by `message`. |
| `unlabelled(span: Span) -> Label` | A label that marks `span` with no text — a bare caret. |
| `span(&self) -> Span` | The span this label points at. |
| `message(&self) -> &str` | The message, empty for an `unlabelled` label. |

---

## `Diagnostic`

One thing a compiler stage has to say: a severity, a headline message, and a
primary label. The primary label is required at construction, so a diagnostic
always knows where it points. Secondary labels and trailing notes are _(planned,
v0.3.0)_.

```rust
use diag_lang::{Diagnostic, Label, Severity, Span};

let diag = Diagnostic::new(
    Severity::Error,
    "type mismatch",
    Label::new(Span::new(8, 11), "expected `Int`, found `Str`"),
);
assert_eq!(diag.severity(), Severity::Error);
assert_eq!(diag.message(), "type mismatch");
```

| Method | Description |
|--------|-------------|
| `new(severity: Severity, message: impl Into<Box<str>>, primary: Label) -> Diagnostic` | Builds a diagnostic pointing at `primary`. |
| `severity(&self) -> Severity` | The level it reports at. |
| `message(&self) -> &str` | The headline message. |
| `primary(&self) -> &Label` | The primary label — the span the caret underlines. |

Constructing a diagnostic is on the front-end's hot path: it allocates only what
the message and label text require.

---

## `Renderer`

Turns a `Diagnostic` plus a `SourceMap` into aligned, caret-annotated text.
Alignment is computed in display columns — a multi-byte UTF-8 character is one
column, and a tab expands to the next four-column tab stop — so the carets line up
under the source whatever it contains. The output is deterministic and always ends
with a newline.

```rust
use diag_lang::{Diagnostic, Label, Renderer, Severity, SourceMap, Span};

let mut map = SourceMap::new();
map.add("main.rs", "fn main() {\n    let x = foo();\n}\n").unwrap();

let diag = Diagnostic::new(
    Severity::Error,
    "cannot find value `foo` in this scope",
    Label::new(Span::new(24, 27), "not found in this scope"),
);

assert_eq!(Renderer::new().render(&diag, &map), "\
error: cannot find value `foo` in this scope
 --> main.rs:2:13
  |
2 |     let x = foo();
  |             ^^^ not found in this scope
  |
");
```

| Method | Description |
|--------|-------------|
| `new() -> Renderer` | A renderer with the default layout. |
| `render(&self, diag: &Diagnostic, map: &SourceMap) -> String` | Renders into a fresh `String`. |
| `render_to(&self, out: &mut impl fmt::Write, diag: &Diagnostic, map: &SourceMap) -> fmt::Result` | Renders into a sink you own — reuse a buffer to avoid a per-diagnostic allocation. |

A span that cannot be located in the map — past the end of the sources, or an
offset no source covers — renders the header plus a defined note rather than
panicking:

```text
error: unexpected end of input
 = note: span 10..12 is outside the loaded sources
```

`Renderer` is `#[non_exhaustive]`; build it with `new()` or `Default`. Styling and
a swappable output seam are _(planned, v0.4.0)_.

---

## Re-exports

For convenience, the position and source types that appear in this crate's API are
re-exported, so a downstream need not also name `span-lang` and `source-lang` to
spell them:

- `Span` — from [`span-lang`](https://docs.rs/span-lang); the byte range a `Label` points at.
- `SourceMap` — from [`source-lang`](https://docs.rs/source-lang); the sources a span is resolved against.

---

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Standard library. With it off, the crate is `no_std` (it always needs `alloc`). Forwards to `span-lang/std` and `source-lang/std`. |

Colour/styling behind a feature is _(planned, v0.4.0)_; plain output is complete
and correctly aligned with no feature enabled.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>
