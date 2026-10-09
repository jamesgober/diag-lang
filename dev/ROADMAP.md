# diag-lang — Roadmap

> Path from scaffold to a stable 1.0. Hard parts are front-loaded; each phase has hard exit criteria.
>
> **Anti-deferral rule:** no listed hard task moves to a later phase unless this file records the move and the reason.
>
> **Status: API frozen at v1.0.0; patch v1.0.1 done; additive minor v1.1.0 done. Next: 2.0 candidates below (Wave M).**

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

---

## v1.1.0 — Codes, fix-its, JSON, collector (DONE)

Additive minor. ISSUES M19, and the diag-lang side of P04 (codes for the forge's
registry) and P19 (lexical errors get a distinct code range). LSP code actions,
lint autofix, and AI/MCP clients need machine-readable diagnostics
(`_lexersketch/ARCHITECTURE.md` §11, §14).

Exit criteria:
- [x] `Diagnostic::with_code`/`code` with a code type that fits family-wide
      registries; the renderer shows `error[E0001]: …`; code conventions
      documented, including the lexical/parse/semantic split (P19).
- [x] `Suggestion { message, edits, applicability }` with `Edit { span,
      replacement }` and rustc's four applicability levels; rendered as `help:`
      blocks; a helper applies machine-applicable edits safely (sorted,
      non-overlapping, char-boundary-checked, error on conflict).
- [x] A dependency-free, deterministic JSON emitter with a versioned,
      documented schema.
- [x] A `Diagnostics` collector: counts, `has_errors`, sort by position,
      dedup, cap with an "N more" summary.
- [x] No breaking change; benches unchanged within noise.

Delivered 2026-10-08:

- **`Code`**: 1–4 ASCII uppercase letters + exactly four digits, 6 bytes,
  `Copy`/`Ord`/`Hash`. `const fn new(prefix, number) -> Option<Code>` (a
  registry writes `const X: Code = Code::new("E", 1).unwrap();`, checked at
  compile time), `const fn parse` (accepts exactly what `Display` writes),
  `prefix`, `number`. Chosen over a free-form string so codes are cheap to carry,
  compare and range-check, and over a bare number so the registry prefix (`E`,
  `T`, `KRA`) is part of the identity. `Diagnostic::with_code` / `code`; the
  header renders `severity[CODE]: message`. The registry guide in
  `docs/API.md` records the family's ranges (OPS `E0001`–`E0099`, LSB
  `E0100`–`E0199`, typeck `T0001`–) and the recommended per-prefix split
  (lexical 0001–0999, parse 1000–1999, semantic 2000–7999, lints 8000–8999,
  internal 9000–9999).
- **`Suggestion`, `Edit`, `Applicability`** (`#[non_exhaustive]`, chosen now
  because it is free only at introduction). `Diagnostic::with_suggestion` /
  `suggestions`. Rendering: underline style (`+` insert, `~` replace) or diff
  style (`-`/`+`, common leading/trailing lines trimmed) when an edit only
  deletes; per-source blocks with `-->` for sources other than the primary's;
  five-line elision; unplaceable suggestions degrade to the `help:` line.
- **`apply_edits` / `apply_fixes` / `ApplyError`** (`#[non_exhaustive]`):
  atomic, `O(n log n + len)`, one validation routine shared with the renderer
  so the two cannot disagree (property-tested).
- **JSON**: `Diagnostic::to_json`/`write_json`, `Diagnostics::to_json`/
  `write_json`, `JSON_SCHEMA_VERSION = 1`; schema and evolution rule in
  `docs/API.md#json-output`; round-trip tested through a strict parser.
- **`Diagnostics`**: push-time dedup via a crate-local SipHash-2-4 fingerprint
  in a `BTreeSet<(u64, index)>` (O(log n), collision-resistant against crafted
  messages), stable sort, cap that counts but does not store,
  `Renderer::render_all`/`render_all_to` with the "N more" line.
- `examples/fixes.rs`; tests `suggestions.rs`, `json.rs`, `apply.rs`,
  `collect.rs`, two new properties in `properties.rs`, colour tests; README and
  `docs/API.md` run as doctests; new benches.
- Internal: suggestions stored behind one lazily allocated pointer to keep
  construction cost flat (interleaved A/B vs 1.0.1: construct +0.4–1.7%,
  render ≤ +1.0%, whole-file ±0.5%); stricter crate-level lint set (without
  `deny(warnings)`, per F03).

Dependency wiring: unchanged — `span-lang = "0.4"`, `source-lang = "1.0.1"`,
`unicode-lang = "1"`, all first-party from crates.io. No new dependency of any
kind.

Not done here, and why:
- **`Severity` `#[non_exhaustive]` (F13)** — breaking for exhaustive matchers
  (lsp-lang's severity mapping is one); moved to 2.0, below.
- **The forge's code registry (P04, P19 forge side)** — lang-forge's job: assign
  its lexical errors `0001`–`0999` and parse errors `1000`–`1999` under its
  prefix. diag-lang provides the type, the rendering, the JSON, and the
  convention.
- **lsp-lang does not yet publish codes or code actions** — lsp-lang's job
  (`code` from `Diagnostic::code`, `CodeAction`s from suggestions); the mapping
  is written down in `docs/API.md#json-output`.
- **Multi-source `apply_fixes`** — a suggestion spanning sources is skipped by
  the per-source helper (documented); a `SourceMap`-wide applier is additive
  later.
- **Interior-boundary edits** inherit source-lang 1.x's limitation: a position
  at the end of a non-last source is the next source's first byte. The renderer
  attributes an edit there to the next source (as it does labels); `apply_fixes`,
  which selects by the inclusive range `base..=base + len` so end-of-file
  insertions work, selects it for both sources. Documented and pinned by
  `test_apply_fixes_boundary_edit_is_selected_for_both_sources`; fixed by
  source-lang 2.0's reserved end positions (no diag-lang change needed then
  beyond tightening the selection).

---

## 2.0 candidates (recorded, not scheduled)

Breaking changes deliberately kept out of 1.x, to be designed once with the rest
of Wave M:

- **`Severity` and `#[non_exhaustive]` (ISSUES F13).** Today exhaustive; making
  it `non_exhaustive` (or adding a level) breaks downstream exhaustive matches.
  Decide with the family-wide policy.
- **span-lang 1.x migration (ISSUES F01).** `span-lang = "0.4"` is shared with
  the other dependents; moving alone would split `Span` into two types.
- **source-lang 2.0** reserved end positions remove the interior-boundary
  limitation for labels and edits; diag-lang needs no code change for it.

