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
    <p>
        Since 1.1 a diagnostic can also carry a stable error code (<code>error[E0425]</code>) and fix-it suggestions, shown rustc-style with the fixed source and applied to a file in one call. The same diagnostics come out as versioned, deterministic JSON for language servers, MCP tools, and AI clients, and a collector counts, de-duplicates, sorts, and caps a compilation's worth of them.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.85+</strong> (Rust 2024 edition). <code>no_std</code>-compatible (needs only <code>alloc</code>), <code>#![forbid(unsafe_code)]</code>, three dependencies, all from the family: <a href="https://crates.io/crates/span-lang"><code>span-lang</code></a>, <a href="https://crates.io/crates/source-lang"><code>source-lang</code></a>, and <a href="https://crates.io/crates/unicode-lang"><code>unicode-lang</code></a>.
    </p>
    <blockquote>
        <strong>Status: stable.</strong> The public API is frozen as of <code>1.0.0</code> and follows Semantic Versioning &mdash; no breaking changes before <code>2.0</code>. <code>1.1.0</code> only adds. See <a href="./docs/API.md#stability"><code>docs/API.md</code></a> for the frozen surface and the behavioural contract, and <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

## The model

A handful of types, one per job:

- A **[`Diagnostic`](./docs/API.md#diagnostic)** is one thing a compiler stage has to say: a [`Severity`](./docs/API.md#severity), a message, a primary [`Label`](./docs/API.md#label) (a span and its explanation), and optionally secondary labels, notes, help, a [`Code`](./docs/API.md#code), and [`Suggestion`s](./docs/API.md#suggestion-edit-applicability) made of [`Edit`s](./docs/API.md#suggestion-edit-applicability).
- A **[`Renderer`](./docs/API.md#renderer)** turns a diagnostic and a `SourceMap` into caret-annotated text for a person.
- **[JSON output](./docs/API.md#json-output)** turns the same diagnostic into one line of versioned JSON for a tool.
- **[`apply_fixes`](./docs/API.md#apply_edits-apply_fixes-applyerror)** applies the machine-applicable suggestions to a source, atomically, or says why it cannot.
- A **[`Diagnostics`](./docs/API.md#diagnostics)** collector gathers a compilation: error and warning counts, duplicates dropped, sorted by position, capped.

<br>

What it guarantees, and how each guarantee is checked:

| Guarantee | How it is held |
|---|---|
| Carets and fix markers line up under the right characters: multi-byte UTF-8, tabs, CJK, emoji, combining marks. | Byte-exact snapshot tests, and property tests checking caret position and width against a naive column walk. |
| Rendering is total and deterministic: any diagnostic, any spans, any edits, any source — no panic, same bytes every time. | Property tests over arbitrary sources, spans, and suggestions (valid or not), including the ones that cannot be placed. |
| Untrusted text cannot drive the terminal. Control characters in sources, messages, file names, and replacement text are escaped. | A property test that plain output never contains a raw control character, plus snapshots of hostile input. |
| A suggestion block shows exactly the fix `apply_edits` would make. | A property test comparing every rendered line of the block with the text the applier produces. |
| Applying edits is exact or refused: never half-applied, never a panic. | Property tests against a reference applier and a naive pairwise conflict check, plus a 100,000-edit scale test. |
| JSON output always parses, has exactly the schema's keys, and decodes to exactly the diagnostic it came from, whatever its text. | Round-trip property tests through a strict RFC 8259 parser written in the tests (rejects duplicate keys, raw controls, lone surrogates). |
| The collector counts, de-duplicates, caps, and sorts exactly as documented. | A property test against a naive model, and a 100,000-diagnostic scale test. |
| End-of-input errors keep their source frame. | Property and snapshot tests over every input-ending shape. |

<hr>
<br>

## Installation

```toml
[dependencies]
diag-lang = "1.1"
```

Without the standard library:

```toml
[dependencies]
diag-lang = { version = "1.1", default-features = false }
```

<hr>
<br>

## Quick start

Build a diagnostic, render it:

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

### Codes and fix-its

```rust
use diag_lang::{Applicability, Code, Diagnostic, Edit, Label, Renderer, Severity};
use diag_lang::{SourceMap, Span, Suggestion, apply_fixes};

const MISSING_SEMICOLON: Code = Code::new("E", 1001).unwrap();

let text = "let total = count + 1\n";
let mut map = SourceMap::new();
map.add("calc.ls", text).unwrap();

let diag = Diagnostic::new(
    Severity::Error,
    "expected `;`",
    Label::new(Span::empty(21), "statement ends here"),
)
.with_code(MISSING_SEMICOLON)
.with_suggestion(
    Suggestion::new("add `;`", Applicability::MachineApplicable).with_edit(Edit::insert(21, ";")),
);

assert_eq!(Renderer::new().render(&diag, &map), "\
error[E1001]: expected `;`
 --> calc.ls:1:22
  |
1 | let total = count + 1
  |                      ^ statement ends here
  |
help: add `;`
  |
1 | let total = count + 1;
  |                      +
  |
");

// `--fix`: apply every machine-applicable suggestion to the source.
assert_eq!(apply_fixes(text, 0, [&diag]).unwrap(), "let total = count + 1;\n");
```

### JSON for tools

```rust
use diag_lang::{Diagnostic, Label, Severity, SourceMap, Span};

let mut map = SourceMap::new();
map.add("a.txt", "let x = y;").unwrap();
let diag = Diagnostic::new(Severity::Error, "unknown `y`", Label::new(Span::new(8, 9), "here"));

let json = diag.to_json(&map); // one line, deterministic, schema version 1
assert!(json.starts_with(r#"{"version":1,"severity":"error","code":null,"message":"unknown `y`""#));
assert!(json.contains(r#""location":{"file":"a.txt","start":{"offset":8,"line":1,"column":9}"#));
```

### A whole compilation

```rust
use diag_lang::{Diagnostic, Diagnostics, Label, Renderer, Severity, SourceMap, Span};

let mut map = SourceMap::new();
map.add("a.txt", "abc").unwrap();

let mut diags = Diagnostics::with_limit(100); // bounded, whatever the input
for at in [2, 0, 0] {
    diags.push(Diagnostic::new(Severity::Error, "bad", Label::unlabelled(Span::new(at, at + 1))));
}
assert_eq!(diags.error_count(), 2); // the repeat was dropped
diags.sort();

let for_people = Renderer::new().render_all(&diags, &map);
let for_tools = diags.to_json(&map);
assert!(for_people.starts_with("error: bad\n --> a.txt:1:1\n"));
assert!(for_tools.starts_with(r#"{"version":1,"errors":2,"warnings":0,"omitted":0,"#));
```

A runnable end-to-end version is in [`examples/fixes.rs`](./examples/fixes.rs) (`cargo run --example fixes`).

The code conventions the family uses (prefixes per registry; lexical `0001`–`0999`, parse `1000`–`1999`, semantic `2000`–`7999`) are in the [code registries guide](./docs/API.md#guide-code-registries-and-ranges).

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
