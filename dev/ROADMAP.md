# diag-lang — Roadmap

> Path from scaffold to a stable 1.0. Hard parts are front-loaded; each phase has hard exit criteria.
>
> **Anti-deferral rule:** no listed hard task moves to a later phase unless this file records the move and the reason.
>
> **Status: complete — all phases shipped, API frozen at v1.0.0; patch v1.0.1 done.**

---

## v0.1.0 — Scaffold (DONE)

Compiles, CI green, structure correct, no domain logic.

- [x] Manifest, README, CHANGELOG, REPS, dual license, CI, deny, clippy, rustfmt.
- [x] API surface sketched in `docs/API.md`.

---

## v0.2.0 — Diagnostic model & the renderer (DONE)

Wire `span-lang` and `source-lang`, then build the core: `Severity`,
`Diagnostic`, `Label`, and — the genuinely hard part, front-loaded — the renderer
that takes a single-line labelled span plus its source and prints it with a caret
underline aligned exactly under the offending bytes. Alignment over multi-byte
UTF-8 and tab expansion is where naive renderers break, so it is proven here with
snapshot tests before any of the easier multi-label work.

Exit criteria:
- [x] `span-lang` and `source-lang` wired and used.
- [x] Every public item has rustdoc + a runnable example.
- [x] Caret alignment under UTF-8 and tabs verified by byte-exact snapshot tests; byte-identical output for identical input.

---

## v0.3.0 — Multi-line spans, secondary labels, notes (DONE)

Render multi-line spans coherently, multiple primary/secondary labels in a stable
order, and trailing notes/help lines — the full rustc-style frame.

Exit criteria:
- [x] Multi-line and multi-label layouts snapshot-tested, including label ordering.
- [x] Out-of-range / unknown-source diagnostics render a defined fallback, not a panic.

---

## v0.4.0 — Styling, output seam, feature freeze (DONE)

Optional colour/styling behind a feature with graceful degradation, an output seam
so targets are swappable, and a declared frozen surface.

Exit criteria:
- [x] Plain (no-colour) output is complete and correctly aligned, snapshot-tested.
- [x] API surface documented as frozen in `docs/API.md`.

Note: the output seam shipped as `Renderer::render_to(&mut impl fmt::Write)`
(swappable sink) rather than a `Render` trait. A trait abstracting the renderer was
deferred to avoid freezing a single-implementor abstraction; see `dev/NOTES.md`. It
can be added as a non-breaking minor when a second renderer exists.

---

## v1.0.0 — API freeze (DONE)

The diagnostic/rendering surface is stable and frozen until 2.0. No new public
API, only documentation, tests, and internal optimisation.

Exit criteria:
- [x] `docs/API.md` marked stable; SemVer promise recorded.
- [x] Full snapshot and benchmark suite green (Windows local + MSRV; CI runs Linux/macOS/Windows on push).

---

## v1.0.1 — Patch: EOF frames, sanitising, elision, display width (DONE)

No public API change. Fixes from the LexerSketch issue ledger
(`_lexersketch/ISSUES.md`): H04, M15, M16, M17, M18.

Delivered:
- [x] **H04** a zero-width label at a source's end renders in that source's frame
  wherever source-lang 1.0.1's `locate` attributes it (single source; most recently
  added source), with the caret just after the last visible character (trailing
  newline, `\r\n`, no newline, and empty sources covered), and the header's
  `line:col` equals the caret's position (snapshot test over eight input shapes
  plus a property). The label's start position alone decides the source. Regression
  tests `test_h04_*`, including the lang-forge case reproduced textually, the
  last-of-several-files case, and a test pinning the known limitation below.
- [x] Consumer check: repl-lang and lang-forge (the diag-lang dependents) pass
  unchanged against the patched pair.
- [x] **M15** every rendered string escapes C0/C1, `DEL`, CR, U+2028/9, and bidi
  controls as `\u{..}`; snapshot test plus a property over arbitrary text, names,
  and messages that the plain output carries no raw control character.
- [x] **M16** marks sorted and grouped by line once; frames grouped by file once;
  spans over five lines elided to first two + `...` + last two; bench
  `render_whole_file/50k_lines`.
- [x] **M17** no `LineIndex` built per frame: line numbers from source-lang's
  cached table, line bounds by local scans.
- [x] **M18** display width via `unicode-lang::char_width`; one `advance` function
  is both the drawing and the measuring model; property against a naive walk.

Dependency wiring: `source-lang = "1.0.1"` (was `"1"`), new
`unicode-lang = { version = "1", default-features = false }` (first-party). Until
source-lang 1.0.1 is on crates.io this crate cannot resolve standalone: publish
source-lang first. Local testing used
`--config 'patch.crates-io.source-lang.path="../source-lang"'` (never committed).

Moved to 2.0 of source-lang (anti-deferral record):
- **End of input in a source that is not the last one** renders at the next
  source's `1:1`. Reason: in source-lang's contiguous 1.x layout that position *is*
  the next source's first byte, so diag-lang cannot tell them apart. The fix is
  source-lang's reserved end position per source, which is semver-major (it breaks
  repl-lang's base prediction); it is built and parked as Wave M
  (`_lexersketch/waves/M-source-lang.md`). diag-lang needs no change for it.

Not done here, and why:
- Width is per character, not per grapheme cluster (ZWJ sequences, flags,
  regional indicators over-count). Recorded under "deliberately left out" in
  `docs/API.md`; additive later.
