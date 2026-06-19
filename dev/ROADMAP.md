# diag-lang — Roadmap

> Path from scaffold to a stable 1.0. Hard parts are front-loaded; each phase has hard exit criteria.
>
> **Anti-deferral rule:** no listed hard task moves to a later phase unless this file records the move and the reason.

---

## v0.1.0 — Scaffold (DONE)

Compiles, CI green, structure correct, no domain logic.

- [x] Manifest, README, CHANGELOG, REPS, dual license, CI, deny, clippy, rustfmt.
- [x] API surface sketched in `docs/API.md`.

---

## v0.2.0 — Diagnostic model & the renderer (THE HARD PART, NOT DEFERRED)

Wire `span-lang` and `source-lang`, then build the core: `Severity`,
`Diagnostic`, `Label`, and — the genuinely hard part, front-loaded — the renderer
that takes a single-line labelled span plus its source and prints it with a caret
underline aligned exactly under the offending bytes. Alignment over multi-byte
UTF-8 and tab expansion is where naive renderers break, so it is proven here with
snapshot tests before any of the easier multi-label work.

Exit criteria:
- [ ] `span-lang` and `source-lang` wired and used.
- [ ] Every public item has rustdoc + a runnable example.
- [ ] Caret alignment under UTF-8 and tabs verified by byte-exact snapshot tests; byte-identical output for identical input.

---

## v0.3.0 — Multi-line spans, secondary labels, notes

Render multi-line spans coherently, multiple primary/secondary labels in a stable
order, and trailing notes/help lines — the full rustc-style frame.

Exit criteria:
- [ ] Multi-line and multi-label layouts snapshot-tested, including label ordering.
- [ ] Out-of-range / unknown-source diagnostics render a defined fallback, not a panic.

---

## v0.4.0 — Styling, output seam, feature freeze

Optional colour/styling behind a feature with graceful degradation, a renderer
trait so output targets are swappable (terminal, plain string, structured), and a
declared frozen surface.

Exit criteria:
- [ ] Plain (no-colour) output is complete and correctly aligned, snapshot-tested.
- [ ] API surface documented as frozen in `docs/API.md`.

---

## v1.0.0 — API freeze

The diagnostic/rendering surface is stable and frozen until 2.0. No new public
API, only documentation, tests, and internal optimisation.

Exit criteria:
- [ ] `docs/API.md` marked stable; SemVer promise recorded.
- [ ] Full snapshot and benchmark suite green on all three platforms.
