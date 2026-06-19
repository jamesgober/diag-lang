# diag-lang &mdash; Engineering Directives

> Engineering standards and the definition of done for this project. Read alongside `REPS.md` (root, authoritative) and `dev/ROADMAP.md` (current phase). If anything here conflicts with `REPS.md`, `REPS.md` wins.

---

## 0. Philosophy

This library is built and maintained to a production standard and treated as a flagship piece of work. Plan the full path, then build one verified step at a time. "Good enough" is treated as a defect. diag-lang is a quality differentiator: a language is judged by the errors its compiler prints, and every language in the family renders through here, so the output must be precise, readable, and correct on the first try.

---

## 1. What this is

diag-lang collects compiler diagnostics and renders them with source context. A diagnostic carries a severity, a message, a primary labelled span, optional secondary labels, and notes; the renderer turns that plus the source into caret-underlined, aligned, human-readable output in the style developers expect from a modern toolchain. It consumes positions from `span-lang` and source text from `source-lang`. It owns collection and rendering only — it does not lex, parse, or decide *what* is wrong, only how to present it.

---

## 2. Engineering law (non-negotiable)

- **Performance** — rendering is not on the compile hot path, but building a diagnostic is: constructing one allocates only what the labels require, and the common single-label case is cheap; no "faster" claim without `criterion` numbers.
- **Correctness** — caret alignment is exact, including across multi-byte UTF-8 and tab expansion; the invariants in section 4 are covered by snapshot tests against fixed expected output.
- **Security** — rendering hostile input (enormous spans, spans past end-of-source, invalid UTF-8 boundaries) is a defined, non-panicking outcome.
- **Architecture** — SOLID, KISS, YAGNI; one responsibility; the renderer sits behind a trait so output targets (terminal, plain string, future structured/JSON) are swappable.
- **Cross-platform** — Linux/macOS/Windows first-class; colour and width detection degrade gracefully when absent.
- **Error handling** — a diagnostic that references an unknown source or an out-of-range span renders a defined fallback, never a panic.
- **Production-ready** — `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` from the first commit; no stray `println!`/`dbg!`; every public item has rustdoc with a runnable example.

---

## 3. Definition of done

1. Compiles clean on Linux/macOS/Windows, stable and MSRV 1.85.
2. `fmt`, `clippy -D warnings`, `test --all-features`, `cargo doc -D warnings` clean.
3. `cargo audit` + `cargo deny check` pass.
4. No `unwrap`/`expect`/`todo!`/`dbg!` in shipping code.
5. A Tier-1 API exists and headlines the docs.
6. Snapshot tests cover the section-4 rendering invariants.
7. Hot-path (diagnostic construction) changes carry benchmarks; no regression over 5%.
8. Docs and `CHANGELOG.md` updated; the matching `docs/release/vX.Y.Z.md` written before the tag.

---

## 4. Project-specific invariants

- The caret/underline aligns exactly under the bytes the span covers, accounting for multi-byte UTF-8 characters and tab expansion — verified by snapshot.
- A primary label and any number of secondary labels render in a stable, documented order; the same diagnostic always produces byte-identical output.
- A multi-line span renders coherently (every covered line shown or summarised per the documented rule), never a broken or misaligned frame.
- A span that exceeds its source's bounds, or references an unknown source, renders a defined fallback message rather than panicking.
- Colour is additive: with styling disabled, the plain output is still complete and correctly aligned.
