# diag-lang &mdash; API Reference

> Complete reference for every public item in `diag-lang`, with examples.
> **Status: stable as of `1.0.0`.** The surface below is frozen under Semantic Versioning — no breaking changes before `2.0`.

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
diag-lang = "1"
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

One thing a compiler stage has to say: a severity, a headline message, a required
primary label, and optionally secondary labels for related locations plus trailing
note/help lines. The primary label is required at construction, so a diagnostic
always knows where it points; the rest are added with chainable `with_*` builders.

```rust
use diag_lang::{Diagnostic, Label, Severity, Span};

let diag = Diagnostic::new(
    Severity::Error,
    "mismatched types",
    Label::new(Span::new(20, 27), "expected `i32`, found `&str`"),
)
.with_secondary(Label::new(Span::new(8, 11), "expected due to this"))
.with_note("string literals have type `&str`")
.with_help("convert with `.parse()`");

assert_eq!(diag.secondary().len(), 1);
assert_eq!(diag.notes().count(), 1);
```

| Method | Description |
|--------|-------------|
| `new(severity: Severity, message: impl Into<Box<str>>, primary: Label) -> Diagnostic` | Builds a diagnostic pointing at `primary`. |
| `with_secondary(self, label: Label) -> Diagnostic` | Adds a secondary label (rendered with a `-` underline). |
| `with_note(self, note: impl Into<Box<str>>) -> Diagnostic` | Adds a trailing `note:` line. |
| `with_help(self, help: impl Into<Box<str>>) -> Diagnostic` | Adds a trailing `help:` line. |
| `severity(&self) -> Severity` | The level it reports at. |
| `message(&self) -> &str` | The headline message. |
| `primary(&self) -> &Label` | The primary label — the span the caret underlines. |
| `secondary(&self) -> &[Label]` | The secondary labels, in the order added. |
| `notes(&self) -> impl ExactSizeIterator<Item = &str>` | The note lines, in order. |
| `help(&self) -> impl ExactSizeIterator<Item = &str>` | The help lines, in order. |

Constructing a diagnostic is on the front-end's hot path: it allocates only what
the message, labels, and notes require.

---

## `Renderer`

Turns a `Diagnostic` plus a `SourceMap` into aligned, caret-annotated text — `^`
under the primary label, `-` under secondaries. Alignment is computed in terminal
display columns — each character takes its Unicode display width (UAX #11 via
`unicode-lang`: one column for most text, two for CJK ideographs and wide emoji,
zero for combining marks), and a tab expands to the next four-column tab stop — so
the underlines line up under the source whatever it contains. The output is
deterministic and always ends with a newline.

A span covering several lines underlines each line it touches, with the message on
the last. A span covering **more than five lines** shows only its first two and last
two lines, with a `...` line in place of the elided middle (a label pointing into
the elided region is still shown, with `...` on either side). Labels are grouped into
one frame per source file — the primary's file first, the rest in map order — and
within a frame each labelled line is shown once with its underlines stacked
beneath. Trailing `note:` and `help:` lines close the output. The frame order
depends on where labels point, not on the order they were added.

**End of input.** A zero-width label at the very end of a source — where a "found
end of input" error points, `Span::empty(len)` — renders in that source's frame
wherever source-lang (`>= 1.0.1`) attributes the position to it: in a single-source
map, and for the most recently added source. The caret sits just after the last
character of the last line (for a text ending in a line break, after the last
character before that break; for an empty source, at `1:1`), and the frame's
`--> file:line:col` header names exactly that line and column, so the header and the
caret never disagree. With one exception, from source-lang's contiguous 1.x layout: the end of a source that
is not the last one is the same position as the next source's first byte, so an
end-of-input label there renders at the next source's `1:1` (fixed by
source-lang 2.0's reserved end positions).

**Untrusted text.** Every caller-supplied string — source lines, the message,
label messages, notes, help, and file names — is written with its control
characters escaped visibly as `\u{..}`: C0 and C1 controls (so `ESC`, and with it
any ANSI/OSC sequence), `DEL`, a carriage return, U+2028/U+2029, and the bidi
embedding/override/isolate/mark controls. Tabs are kept (expanded in source lines);
newlines are kept in the message, labels, notes, and help, and escaped in file
names. The only escape sequences in the output are the renderer's own colours.

**Cost.** Rendering is bounded by what is printed, plus `O(log lines)` per label:
no line index is built per frame. Line numbers come from source-lang's cached line
table (`SourceMap::line_col`, source-lang `>= 1.0.1`), and line boundaries are
found by scanning outward from each label. A label covering a whole 50,000-line file
prints and costs the same as one covering six lines.

```text
error: mismatched types
 --> main.rs:2:5
  |
1 | let x: i32 =
  |        --- expected due to this
2 |     "hi";
  |     ^^^^ expected `i32`, found `&str`
  |
```

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
| `new() -> Renderer` | A renderer with the default layout, producing plain output. |
| `with_color(self) -> Renderer` | Enables ANSI styling. Requires the `color` feature. |
| `render(&self, diag: &Diagnostic, map: &SourceMap) -> String` | Renders into a fresh `String`. |
| `render_to(&self, out: &mut impl fmt::Write, diag: &Diagnostic, map: &SourceMap) -> fmt::Result` | Renders into a sink you own — reuse a buffer to avoid a per-diagnostic allocation. |

`render_to` is the output seam: it writes into any `core::fmt::Write`, so the same
renderer drives a terminal, a string, a reused buffer, or a formatter. `with_color`
(with the `color` feature) wraps the output in ANSI escapes coloured by severity;
stripping those escapes reproduces the plain output byte for byte, so colour only
ever adds.

A span that cannot be located in the map — past the end of the sources, or on an
empty map — renders the header plus a defined note rather than panicking:

```text
error: unexpected end of input
 = note: span 10..12 is outside the loaded sources
```

`Renderer` is `#[non_exhaustive]`; build it with `new()` or `Default`.

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
| `color` | yes | ANSI styling via `Renderer::with_color`. Disable it, or never call `with_color`, for plain output — which is complete and identically aligned. |

---

## Frozen surface

The surface above is the complete public API, frozen as of `1.0.0`. It follows
Semantic Versioning: no breaking change before `2.0`, additions arrive in minor
releases, and the MSRV (Rust 1.85) only rises in a minor. Items not listed here —
internal modules, the exact bytes of a rendered frame beyond the documented layout
rules — are not part of the contract.

Deliberately left out of the 1.0 surface, each addable later without a breaking
change:

- **A `Render` trait.** The output *sink* is already swappable through
  `render_to(&mut impl fmt::Write)`; a trait abstracting the *renderer* has only
  one implementor today, so freezing its shape now would be speculative. It can be
  introduced as a minor when a second renderer (for example a structured/JSON
  target) gives it a second implementor.
- **Serialisation.** `Diagnostic` and friends do not derive `serde`; a consumer
  that needs to serialise can map the public accessors. A `serde` feature can be
  added later as a non-breaking minor.
- **Configurable tab width.** The tab stop is fixed at four columns; a
  `with_tab_width` setter can be added without breaking the surface.
- **Grapheme clustering.** Display width is summed per character, so a ZWJ emoji
  sequence or a flag counts as the sum of its parts, not as one glyph. Terminals
  disagree on these anyway; a cluster-aware width can be added without changing
  the API.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>
