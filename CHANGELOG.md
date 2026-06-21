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

[Unreleased]: https://github.com/jamesgober/diag-lang/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/jamesgober/diag-lang/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/diag-lang/releases/tag/v0.1.0
