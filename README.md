<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>diag-lang</b>
    <br>
    <sub><sup>DIAGNOSTICS & ERROR RENDERING</sup></sub>
</h1>

<div align="center">
    <a href="https://crates.io/crates/diag-lang"><img alt="Crates.io" src="https://img.shields.io/crates/v/diag-lang"></a>
    <a href="https://crates.io/crates/diag-lang"><img alt="Downloads" src="https://img.shields.io/crates/d/diag-lang?color=%230099ff"></a>
    <a href="https://docs.rs/diag-lang"><img alt="docs.rs" src="https://img.shields.io/docsrs/diag-lang"></a>
    <a href="https://github.com/jamesgober/diag-lang/actions"><img alt="CI" src="https://github.com/jamesgober/diag-lang/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        diag-lang collects compiler diagnostics and renders them the way a modern toolchain does: a severity, a message, and the offending source with a caret underlining exactly where the problem is. It turns a span and a source into the kind of error output developers actually want to read, and it is reused unchanged across every language built on the -lang family.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.85+</strong> (Rust 2024 edition).
    </p>
    <blockquote>
        <strong>Status: pre-1.0, in active development.</strong> The public API is being designed across the 0.x series and frozen at <code>1.0.0</code>. See <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

## Installation

```toml
[dependencies]
diag-lang = "0.4"
```

<br>

## Example

```rust
use diag_lang::{Diagnostic, Label, Renderer, Severity, SourceMap, Span};

let mut map = SourceMap::new();
map.add("main.rs", "fn main() {\n    let x = foo();\n}\n").unwrap();

let diag = Diagnostic::new(
    Severity::Error,
    "cannot find value `foo` in this scope",
    Label::new(Span::new(24, 27), "not found in this scope"),
);

print!("{}", Renderer::new().render(&diag, &map));
```

```text
error: cannot find value `foo` in this scope
 --> main.rs:2:13
  |
2 |     let x = foo();
  |             ^^^ not found in this scope
  |
```

<br>

## Status

This is <code>v0.4.0</code>: the full diagnostic model and renderer — multi-line spans, secondary labels, notes/help, and optional ANSI colour (the `color` feature) with plain output staying complete. The surface is feature-complete and documented as the freeze candidate in <a href="./docs/API.md"><code>docs/API.md</code></a>; <code>1.0.0</code> freezes it under SemVer. See the <a href="./dev/ROADMAP.md"><code>ROADMAP</code></a>.

<hr>
<br>

## Contributing

See <a href="./dev/DIRECTIVES.md"><code>dev/DIRECTIVES.md</code></a> for engineering standards and the definition of done. Before a PR: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must be clean.

<br>

<div id="license">
    <h2>License</h2>
    <p>Licensed under either of</p>
    <ul>
        <li><b>Apache License, Version 2.0</b> &mdash; <a href="./LICENSE-APACHE">LICENSE-APACHE</a></li>
        <li><b>MIT License</b> &mdash; <a href="./LICENSE-MIT">LICENSE-MIT</a></li>
    </ul>
    <p>at your option.</p>
</div>

<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>James Gober <me@jamesgober.com>.</strong></sup>
</div>
