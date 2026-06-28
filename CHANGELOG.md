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

[Unreleased]: https://github.com/jamesgober/diag-lang/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/jamesgober/diag-lang/compare/v0.4.0...v1.0.0
[0.4.0]: https://github.com/jamesgober/diag-lang/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/diag-lang/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/diag-lang/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/diag-lang/releases/tag/v0.1.0
