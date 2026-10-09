<h1 align="center">
    <img width="90px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <br><b>CHANGELOG</b>
</h1>
<p>
  All notable changes to <code>diag-lang</code> will be documented in this file. The format is based on <a href="https://keepachangelog.com/en/1.1.0/">Keep a Changelog</a>,
  and this project adheres to <a href="https://semver.org/spec/v2.0.0.html/">Semantic Versioning</a>.
</p>

---

## [Unreleased]

### Added

### Changed

### Fixed

### Security

---

## [1.1.0] - 2026-10-08

An additive minor release: error codes, fix-it suggestions, JSON output, and a
diagnostics collector (ISSUES M19, and the diag-lang side of P04 and P19). Code
that builds against 1.0 builds unchanged, and a diagnostic without a code or
suggestions renders byte for byte as in 1.0.1.

### Added

- **Error codes.** `Code` (one to four uppercase letters plus exactly four
  digits: `E0001`, `T0042`, `KRA0013`) with `const fn new` and `parse`,
  `prefix`, `number`, `Display`, and ordering by prefix then number.
  `Diagnostic::with_code` / `code`. The renderer shows the code in the header,
  rustc-style: `error[E0425]: ...`. `docs/API.md` documents the family's code
  registries and a recommended split by phase: lexical `0001`-`0999`, parse
  `1000`-`1999`, semantic `2000`-`7999`, lints `8000`-`8999`, internal
  `9000`-`9999` (P19: lexical errors are a range check, not a message prefix).
- **Fix-it suggestions.** `Suggestion { message, edits, applicability }`,
  `Edit { span, replacement }` (`new`, `insert`, `delete`), and
  `Applicability` (`MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders`,
  `Unspecified`; `#[non_exhaustive]`). `Diagnostic::with_suggestion` /
  `suggestions`. The renderer shows each suggestion as a `help:` block with the
  fixed lines: `+` under insertions and `~` under replacements, or a `-`/`+`
  diff when an edit only deletes; a block in another source is headed
  `--> file:line:col`; long blocks are elided like long spans; a suggestion
  that cannot be placed shows its message alone.
- **Applying edits.** `apply_edits(text, base, edits)` and
  `apply_fixes(text, base, diagnostics)` (every machine-applicable suggestion
  that targets the text) return the edited text or an `ApplyError`
  (`OutOfBounds`, `NotCharBoundary`, `Overlap`; `#[non_exhaustive]`). Edits are
  validated against the text's bounds and character boundaries, sorted, and
  conflict-checked; identical edits are applied once; nothing is applied on
  error. `O(n log n + len)`.
- **JSON output.** `Diagnostic::to_json` / `write_json` (one line per
  diagnostic, for JSON Lines) and `Diagnostics::to_json` / `write_json` (one
  document with counts), plus `JSON_SCHEMA_VERSION` (`1`). Dependency-free,
  compact, deterministic key order, RFC 8259 escaping (and U+2028/U+2029).
  Labels and edits carry the global byte span and a resolved location: file,
  and byte offset, line, and column for each end. The schema is documented in
  `docs/API.md` and only gains keys within a version.
- **Collector.** `Diagnostics`: `new`, `with_limit`, `push` (identical
  diagnostics are kept once, via a SipHash-2-4 fingerprint and an ordered
  index, `O(log n)`), `sort` (stable, by primary span then severity),
  `clear`, `has_errors`, `error_count`, `warning_count`, `count`, `omitted`,
  `limit`, `len`, `is_empty`, `as_slice`, `iter`, `into_vec`, and the
  `Extend`/`FromIterator`/`IntoIterator` impls. Past the limit, diagnostics are
  counted but not stored. `Renderer::render_all` / `render_all_to` render a
  collector with blank lines between diagnostics and a final
  `note: N more diagnostics not shown` line when the cap dropped some.
- `examples/fixes.rs`: a check, rendering, JSON, and `--fix` end to end.
- Tests: snapshot tests for codes and every suggestion layout
  (`tests/suggestions.rs`); a strict JSON parser and round-trip properties
  (`tests/json.rs`); apply properties against a reference applier and a naive
  conflict check (`tests/apply.rs`); collector properties against a naive model
  and code-format properties (`tests/collect.rs`); a property that a suggestion
  block shows exactly what `apply_edits` produces. The README and
  `docs/API.md` examples now run as doctests.
- Benchmarks: `construct_with_code_and_fix`, `render_with_code_and_fix`,
  `to_json`, `collect/push_100k_twice`, `collect/sort_100k`,
  `apply_edits/100k_edits`.

### Changed

- `Diagnostic`'s `Debug` output lists `code` and `suggestions` after the 1.0
  fields; equality also compares them. (Neither the `Debug` text nor the field
  layout is part of the contract.)
- Suggestions are stored behind one lazily allocated pointer and the code
  inline, so a diagnostic is 16 bytes larger than in 1.0; construction and
  rendering are unchanged within measurement noise (interleaved A/B against
  1.0.1, see the release notes).
- The crate root now also denies `unused_results`, `clippy::unwrap_used`,
  `expect_used`, `panic`, `todo`, `unimplemented`, `unreachable`, `dbg_macro`,
  and `print_stdout`/`print_stderr` in library code (not `warnings`, so new
  toolchain lints never break a downstream build).

### Notes

- `Severity` stays exhaustive. Adding `#[non_exhaustive]` (ISSUES F13) would
  break downstream exhaustive matches (lsp-lang has one), so it is recorded for
  2.0 in `dev/ROADMAP.md`, not done in 1.x.
- No new dependencies.
- Known limitation (source-lang 1.x layout): in a multi-source map the end of
  one source is the start of the next, so `apply_fixes` selects an edit at
  exactly that position for both sources. Documented and pinned by a test;
  fixed by source-lang 2.0.

---

## [1.0.1] - 2026-10-08

A patch release: no public API changes. It fixes end-of-input rendering (ISSUES
H04), terminal injection (M15), quadratic multi-line rendering (M16), the per-frame
line-index rebuild (M17), and caret alignment under wide characters (M18).

### Fixed

- **End-of-input errors render with their source frame (H04).** A label at
  `Span::empty(len)` (where lang-forge and any parser report "found end of input")
  used to render as `span N..N is outside the loaded sources` with no frame. It now
  renders in the source's frame wherever source-lang 1.0.1 attributes the position
  to that source (a single source, or the most recently added one), with the caret
  just after the last character of the last line, including sources without a
  trailing newline (`a = b +` → caret after `+`), `\r\n` endings, and empty
  sources (`1:1`). The frame's `--> file:line:col` header always names exactly the
  caret's line and column. Known limitation, from source-lang's contiguous 1.x
  layout: the end of a source that is not the last one is the same position as the
  next source's first byte, so an end-of-input label there renders at the next
  source's `1:1` (fixed by source-lang 2.0).
- **Wide and combining characters align (M18).** Caret placement and width use
  Unicode display width (`unicode-lang`'s `char_width`): CJK and wide emoji take two
  columns, combining marks none. Tab stops are computed in the same display columns.

### Security

- **Control characters are escaped (M15).** Source lines, the message, label
  messages, notes, help, and file names are written with C0/C1 controls (including
  `ESC`, so no ANSI/OSC sequence from untrusted input reaches a terminal), `DEL`,
  carriage returns, U+2028/U+2029, and the bidi override/isolate/mark controls
  escaped visibly as `\u{..}`. Tabs are kept; newlines are kept in messages, notes,
  and help (layout) and escaped in file names.

### Changed

- **Long multi-line spans are elided (M16).** A span over more than five lines shows
  its first two and last two lines with a `...` line between; spans of up to five
  lines render exactly as before. Marks are grouped by line in one sorted pass and
  frames by file in one pass, so rendering is no longer quadratic in the number of
  covered lines or labels.
- **No line index per frame (M17).** Line numbers come from source-lang 1.0.1's
  cached line table and line boundaries from a scan outward from each label, so a
  frame costs what it prints plus `O(log lines)` per label. Rendering a label over a
  whole 50,000-line file: 1.2042 s before, 1.6903 µs after; a one-line label:
  963.76 ns before, 765.22 ns after.

### Dependencies

- `source-lang` requirement raised from `"1"` to `"1.0.1"`: the end-of-input
  resolution (the end of the last source resolves to it) and the cached line table
  this release relies on are source-lang 1.0.1 behaviour. Publish source-lang 1.0.1
  first.
- **New dependency: `unicode-lang = "1"`** (`default-features = false`, so `no_std`
  is kept), a first-party `-lang` crate from crates.io with zero dependencies of its
  own, for display width. Adding it in a patch release is a deliberate exception,
  limited to first-party crates.

---

## [1.0.0] - 2026-06-28

API freeze. The diagnostic and rendering surface is stable and will not change in a
breaking way before `2.0`. No functional API changes from `0.4.0`.

### Changed

- The public API is declared stable under Semantic Versioning; `docs/API.md`
  catalogues the frozen surface and the SemVer promise.

### Notes

- A `Render` trait was considered and deliberately left out of 1.0: the output sink
  is already swappable via `Renderer::render_to`, and a trait with a single
  implementor would be speculative. It can be added as a non-breaking minor when a
  second renderer exists.

---

## [0.4.0] - 2026-06-20

Optional ANSI styling and the declared freeze candidate for 1.0.

### Added

- `color` feature (default) and `Renderer::with_color`: ANSI styling coloured by severity, with the gutter, underlines, and markers wrapped in escapes. Plain output is unchanged and stripping the escapes from a coloured render reproduces it byte for byte.
- `docs/API.md` now documents the complete surface as the 1.0 freeze candidate, including what is deliberately left out and addable later without a breaking change.

### Changed

- `Renderer` carries a private colour flag; `Renderer::new()` still produces plain output, so default rendering is byte-identical to v0.3.0.

### Notes

- The output seam is `Renderer::render_to`, which writes into any `core::fmt::Write`. A `Render` trait abstracting the renderer itself is deliberately deferred (single implementor today); see `dev/NOTES.md`.

---

## [0.3.0] - 2026-06-20

Multi-line spans, secondary labels, and trailing notes — the full frame.

### Added

- `Diagnostic::with_secondary` for related-location labels, rendered with a `-` underline.
- `Diagnostic::with_note` and `with_help` for trailing `note:`/`help:` lines.
- `Diagnostic::secondary`, `notes`, and `help` accessors.
- Multi-line span rendering: a span underlines every line it covers, with the message on the last.
- Per-file frames: labels are grouped by source — the primary's file first, the rest in map order — and each labelled line is shown once with its underlines stacked beneath, in a stable order independent of insertion order.
- A label whose span falls outside the loaded sources renders a `note:` line rather than being dropped or panicking, including for secondary labels.
- Snapshot tests for multi-line, multi-label, cross-file, stacked, and note/help layouts; a property test holding many-label rendering total and deterministic.

### Changed

- `Renderer` output now draws a frame per source file and places secondary labels; single-primary single-line output is unchanged.

---

## [0.2.0] - 2026-06-20

The diagnostic model and the caret-aligned single-line renderer — the hard part of
the roadmap, front-loaded. `span-lang` and `source-lang` are now wired and used.

### Added

- `Severity` — `Error`, `Warning`, `Note`, `Help`, ordered most-to-least serious, with `as_str` and `Display`.
- `Label` — a `Span` plus a message, via `new` and `unlabelled`; the span lives in the source map's global position space.
- `Diagnostic` — a severity, a message, and a required primary label, with `severity`/`message`/`primary` accessors.
- `Renderer` — `render` (to a `String`) and `render_to` (to any `fmt::Write` sink), producing aligned, caret-annotated output.
- Caret alignment computed in display columns: one column per character over multi-byte UTF-8, tabs expanded to four-column stops.
- Defined, non-panicking fallback note for a span that falls outside the loaded sources.
- `Span` and `SourceMap` re-exported from the crate root.
- Byte-exact snapshot tests and property tests cross-checking the caret width against a naive column walk; benchmarks for diagnostic construction and rendering.

### Changed

- `std` now forwards to `span-lang/std` and `source-lang/std`.

### Removed

- The reserved no-op `serde` feature, dropped rather than carried unused toward the frozen 1.0 surface. Serialisation can return post-1.0 as a non-breaking minor if needed.

---

## [0.1.0] - 2026-06-18

Initial scaffold and repository bootstrap. No domain logic yet &mdash; this release establishes the structure, tooling, and quality gates the implementation will be built on.

### Added

- `Cargo.toml` with crate metadata, Rust 2024 edition, MSRV 1.85.
- Dual `Apache-2.0 OR MIT` license files.
- `README.md`, `CHANGELOG.md`, and a documentation skeleton.
- `REPS.md` compliance baseline.
- `.github/workflows/ci.yml` CI matrix; `deny.toml`, `clippy.toml`, `rustfmt.toml`.
- `dev/DIRECTIVES.md` and `dev/ROADMAP.md` (committed engineering standards + plan).

[Unreleased]: https://github.com/jamesgober/diag-lang/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/jamesgober/diag-lang/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/jamesgober/diag-lang/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/jamesgober/diag-lang/compare/v0.4.0...v1.0.0
[0.4.0]: https://github.com/jamesgober/diag-lang/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/diag-lang/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/diag-lang/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/diag-lang/releases/tag/v0.1.0
