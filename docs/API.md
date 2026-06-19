# diag-lang &mdash; API Reference

> Complete reference for every public item in `diag-lang`, with examples.
> **Status: pre-1.0 — the surface below is the planned design and is being built across the 0.x series.** Items marked _(planned)_ are not yet implemented; see [`dev/ROADMAP.md`](../dev/ROADMAP.md).

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [`Severity`](#severity) _(planned, v0.2.0)_
- [`Label`](#label) _(planned, v0.2.0)_
- [`Diagnostic`](#diagnostic) _(planned, v0.2.0)_
- [`Renderer`](#renderer) _(planned, v0.2.0)_
- [Feature flags](#feature-flags)

---

## Overview

diag-lang collects compiler diagnostics and renders them with source context: a
severity, a message, and the offending source with a caret underlining exactly
where the problem is. It consumes positions from `span-lang` and source text from
`source-lang`, and produces the kind of error output developers actually want to
read.

It owns collection and rendering only. It does not decide *what* is wrong — a
lexer, parser, or type checker builds the `Diagnostic`; diag-lang decides how it
looks.

---

## Installation

```toml
[dependencies]
diag-lang = "0.1"
```

Colour/styling is an additive feature; the default output is plain and complete.

---

## `Severity`

_(planned, v0.2.0)_ `Error`, `Warning`, `Note`, `Help` — the level a diagnostic
reports at, driving its label and colour.

## `Label`

_(planned, v0.2.0)_ A span plus a message: "this is the part of the source the
diagnostic is pointing at, and here's why." One primary, any number of secondary.

## `Diagnostic`

_(planned, v0.2.0)_ A severity, a message, a primary label, optional secondary
labels, and trailing notes — the full unit a compiler stage emits.

```rust,ignore
use diag_lang::{Diagnostic, Severity};

let diag = Diagnostic::new(Severity::Error, "type mismatch")
    .primary(expr_span, "expected `Int`, found `Str`")
    .note("string literals have type `Str`");

renderer.render(&diag, &source_map); // caret-underlined, aligned output
```

## `Renderer`

_(planned, v0.2.0)_ Turns a `Diagnostic` plus a source map into aligned,
caret-underlined output. Behind a trait so the target (terminal, plain string,
future structured) is swappable.

---

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Standard library; terminal width/colour detection. |
| `color` | yes | ANSI styling. Disable for plain output (still complete and aligned). |
| `serde` | no | Serialise diagnostics for structured/JSON output. |

diag-lang depends on `span-lang` and `source-lang`.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>
