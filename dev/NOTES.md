# diag-lang — Engineering Notes

Working notes for decisions that were parked for review, and the status at the
pre-1.0 stop point. Lives alongside `DIRECTIVES.md` and `ROADMAP.md` in `dev/`.

---

## Parked for review

### A `Render` trait for swappable output targets

**Context.** `dev/ROADMAP.md` for v0.4.0 lists "a renderer trait so output targets
are swappable (terminal, plain string, structured)". This is potentially a
cross-crate contract: the language crates in the family all render through
diag-lang, and if any of them want to dispatch over a `dyn` renderer (to switch
between, say, a terminal renderer and a JSON one at runtime), the trait's shape
becomes a shared API they depend on. Freezing the wrong shape at 1.0 would be hard
to undo.

**What shipped instead.** The output *sink* is already swappable through
`Renderer::render_to(&mut impl fmt::Write, …)`: it drives a terminal (via a `Write`
adapter), a `String`, a reused buffer, or a formatter. Colour is a configuration on
the one renderer (`with_color`), not a separate target. So "terminal" and "plain
string" — two of the three targets named — are covered without a trait.

**The open question — "structured" output.** A JSON/structured target is a
different *format*, not just a different sink, so `render_to` does not cover it. The
options:

1. **Defer the trait (recommended).** Ship 1.0 with `render_to` as the seam and no
   trait. There is exactly one renderer today, so a trait would be a one-implementor
   abstraction — speculative generality, which REPS calls out as an anti-pattern.
   When a structured renderer is actually built, introduce the trait then; adding a
   trait and a second impl is a non-breaking minor. Lowest risk, smallest frozen
   surface.
2. **Add a minimal object-safe trait now**, e.g.
   `trait RenderDiagnostic { fn render_to(&self, out: &mut dyn fmt::Write, diag: &Diagnostic, map: &SourceMap) -> fmt::Result; }`,
   and impl it for `Renderer`. Honours the roadmap wording literally, but freezes a
   shape (object-safe `&mut dyn Write`, no return value beyond `fmt::Result`, no
   structured payload) against a single implementor and no concrete second use case.
3. **Add a structured target too** (a JSON renderer + the trait) before 1.0. Largest
   scope; the payload schema would itself become a frozen contract, and no consumer
   has asked for it yet.

**Recommendation:** option 1. The `render_to` seam meets the swappable-sink need;
the trait is best designed against a real second implementor. If you want the trait
in 1.0 regardless, option 2 is the conservative shape. This is the one roadmap item
v0.4.0 did not implement literally, and the pre-1.0 stop is the right moment to
decide.

---

## STATUS — stopped at the pre-1.0 freeze point

The crate is built through the full 0.x roadmap and sits at the last green commit
before `1.0.0`. `1.0.0` (the freeze tag and release) is left for you, per the build
directive.

**Done — v0.2.0 → v0.4.0, each committed green:**

- v0.2.0 — `Severity`, `Label`, `Diagnostic`, and the single-line caret renderer.
  `span-lang`/`source-lang` wired and used. Caret alignment exact over multi-byte
  UTF-8 and tab expansion, proven by byte-exact snapshot tests and a property test.
- v0.3.0 — multi-line spans, secondary labels (`-`), notes/help, per-file frames in
  a stable position-driven order, and an out-of-range fallback note for any label.
- v0.4.0 — optional ANSI colour (`color` feature, `Renderer::with_color`) with the
  additivity invariant tested (strip escapes ⇒ plain output), and the surface
  documented as the 1.0 freeze candidate in `docs/API.md`.

**Quality gates, all green** (Windows local, via the rust-lld linker workaround;
CI runs the same on Linux/macOS/Windows, stable + 1.85): `fmt --check`, `clippy
-D warnings` on default / all-features / no-default-features, `test` on default and
all-features, `doc -D warnings`, no-default-features (`no_std`) build, `+1.85`
build, `deny check`. `#![forbid(unsafe_code)]`, no `unwrap`/`expect`/`todo!` in
shipping code. Counts (default features): 13 unit + 11 single-line snapshot + 7
frame snapshot + 4 colour + 4 property + 24 doctests.

**Decisions taken under the standing defaults (FYI, no action needed):**

- Dropped the scaffold's no-op `serde` feature rather than freeze it unused; a
  `serde` minor can be added post-1.0.
- Fixed `clippy.toml` MSRV from a stray `1.87` to `1.85`, matching the manifest and
  the sibling crates.
- Labels carry **global** spans (resolved through the `SourceMap`), matching
  source-lang's design, so a `Label` is just a span plus a message.
- Multi-line spans render as per-line underlines (carets on every covered line,
  message on the last), not rustc's connector bars — coherent, aligned, and far
  lower risk to get exactly right. Multi-label frames stack underlines per line
  rather than merging with vertical connectors. Both are documented layout rules,
  and the rendered byte format is not part of the frozen contract, so a fancier
  frame style remains a non-breaking output change later.

**Needs your decision before 1.0:**

- The `Render` trait question above. My recommendation is to ship 1.0 without it
  and add it when a second renderer exists. Everything else is ready to freeze.
