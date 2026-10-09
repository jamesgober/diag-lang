# diag-lang &mdash; API Reference

> Complete reference for every public item in `diag-lang`, with examples.
> **Status: stable since `1.0.0`; this is `1.1.0`.** The surface below follows
> Semantic Versioning — no breaking changes before `2.0`. Items added in 1.1.0
> are marked *since 1.1.0*. The contract is the [Stability](#stability) section.

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [Quick start](#quick-start)
- [`Severity`](#severity)
- [`Label`](#label)
- [`Diagnostic`](#diagnostic)
- [`Code`](#code) *(since 1.1.0)*
- [`Suggestion`, `Edit`, `Applicability`](#suggestion-edit-applicability) *(since 1.1.0)*
- [`apply_edits`, `apply_fixes`, `ApplyError`](#apply_edits-apply_fixes-applyerror) *(since 1.1.0)*
- [`Diagnostics`](#diagnostics) *(since 1.1.0)*
- [`Renderer`](#renderer)
- [JSON output](#json-output) *(since 1.1.0)*
- [Guide: code registries and ranges](#guide-code-registries-and-ranges)
- [Re-exports](#re-exports)
- [Feature flags](#feature-flags)
- [Stability](#stability)

---

## Overview

diag-lang collects compiler diagnostics and renders them with source context: a
severity, a message, and the offending source with a caret underlining exactly
where the problem is. It consumes positions from `span-lang` and source text from
`source-lang`, and produces the kind of error output developers expect from a
modern toolchain.

It owns collection and presentation only. It does not decide *what* is wrong — a
lexer, parser, or type checker builds the `Diagnostic`; diag-lang decides how it
looks, both to a person (the [`Renderer`](#renderer)) and to a tool
([JSON output](#json-output)).

A [`Label`](#label) points with a [`Span`](#re-exports) into a
[`SourceMap`](#re-exports)'s **global** position space; the renderer resolves that
span back to a file, a line, and a column on its own. Fix-it [`Edit`](#suggestion-edit-applicability)s
point the same way.

---

## Installation

```toml
[dependencies]
diag-lang = "1.1"
```

---

## Quick start

The one-call paths, most common first:

```rust
use diag_lang::{Applicability, Code, Diagnostic, Diagnostics, Edit, Label, Renderer};
use diag_lang::{Severity, SourceMap, Span, Suggestion, apply_fixes};

let text = "let x = 1\n";
let mut map = SourceMap::new();
map.add("main.ls", text).unwrap();

let diag = Diagnostic::new(Severity::Error, "expected `;`", Label::unlabelled(Span::empty(9)))
    .with_code(Code::new("E", 1001).unwrap())
    .with_suggestion(
        Suggestion::new("add `;`", Applicability::MachineApplicable).with_edit(Edit::insert(9, ";")),
    );

// For a person:
let shown = Renderer::new().render(&diag, &map);
assert!(shown.starts_with("error[E1001]: expected `;`\n"));

// For a tool (one line of JSON):
assert!(diag.to_json(&map).contains(r#""code":"E1001""#));

// `--fix`:
assert_eq!(apply_fixes(text, 0, [&diag]).unwrap(), "let x = 1;\n");

// For a whole compilation:
let mut diags = Diagnostics::with_limit(100);
diags.push(diag);
assert!(diags.has_errors());
let _all = Renderer::new().render_all(&diags, &map);
```

---

## `Severity`

The level a diagnostic reports at: `Error`, `Warning`, `Note`, or `Help`. The
variants are ordered most-to-least serious, so a slice of severities sorts with
the errors first.

```rust
use diag_lang::Severity;

assert_eq!(Severity::Error.as_str(), "error");
assert!(Severity::Error < Severity::Warning);
```

| Method | Description |
|--------|-------------|
| `as_str(self) -> &'static str` | The lowercase label (`"error"`, `"warning"`, `"note"`, `"help"`). |

`Severity` is `Copy` and implements `Display` (equal to `as_str`). It is
exhaustive: a `match` on it needs no wildcard arm, and that stays true for all
of 1.x (see [Stability](#stability)).

---

## `Label`

A `Span` paired with the message explaining it — the thing a diagnostic points
*with*. The span is in the source map's global position space, so the renderer can
resolve which file and line it lands on.

```rust
use diag_lang::{Label, Span};

let label = Label::new(Span::new(8, 11), "expected `Int`, found `Str`");
assert_eq!(label.span(), Span::new(8, 11));

let bare = Label::unlabelled(Span::new(8, 11)); // points without annotating
assert!(bare.message().is_empty());
```

| Method | Description |
|--------|-------------|
| `new(span: Span, message: impl Into<Box<str>>) -> Label` | A label covering `span`, explained by `message`. |
| `unlabelled(span: Span) -> Label` | A label that marks `span` with no text — a bare caret. |
| `span(&self) -> Span` | The span this label points at. |
| `message(&self) -> &str` | The message, empty for an `unlabelled` label. |

---

## `Diagnostic`

One thing a compiler stage has to say: a severity, a headline message, a required
primary label, and optionally secondary labels for related locations, trailing
note/help lines, a [`Code`](#code), and fix-it
[`Suggestion`s](#suggestion-edit-applicability). The primary label is required
at construction, so a diagnostic always knows where it points; the rest are
added with chainable `with_*` builders.

```rust
use diag_lang::{Applicability, Code, Diagnostic, Edit, Label, Severity, Span, Suggestion};

let diag = Diagnostic::new(
    Severity::Error,
    "mismatched types",
    Label::new(Span::new(20, 27), "expected `i32`, found `&str`"),
)
.with_secondary(Label::new(Span::new(8, 11), "expected due to this"))
.with_note("string literals have type `&str`")
.with_help("convert with `.parse()`")
.with_code(Code::new("T", 1).unwrap())
.with_suggestion(
    Suggestion::new("parse the string", Applicability::MaybeIncorrect)
        .with_edit(Edit::insert(27, ".parse()?")),
);

assert_eq!(diag.secondary().len(), 1);
assert_eq!(diag.notes().count(), 1);
assert_eq!(diag.code().map(|c| c.to_string()), Some("T0001".to_string()));
assert_eq!(diag.suggestions().len(), 1);
```

| Method | Description |
|--------|-------------|
| `new(severity: Severity, message: impl Into<Box<str>>, primary: Label) -> Diagnostic` | Builds a diagnostic pointing at `primary`. |
| `with_secondary(self, label: Label) -> Diagnostic` | Adds a secondary label (rendered with a `-` underline). |
| `with_note(self, note: impl Into<Box<str>>) -> Diagnostic` | Adds a trailing `note:` line. |
| `with_help(self, help: impl Into<Box<str>>) -> Diagnostic` | Adds a trailing `help:` line. |
| `with_code(self, code: Code) -> Diagnostic` | *Since 1.1.0.* Sets the code; setting it again replaces it. |
| `with_suggestion(self, suggestion: Suggestion) -> Diagnostic` | *Since 1.1.0.* Adds a fix-it suggestion. |
| `severity(&self) -> Severity` | The level it reports at. |
| `message(&self) -> &str` | The headline message. |
| `primary(&self) -> &Label` | The primary label — the span the caret underlines. |
| `secondary(&self) -> &[Label]` | The secondary labels, in the order added. |
| `notes(&self) -> impl ExactSizeIterator<Item = &str>` | The note lines, in order. |
| `help(&self) -> impl ExactSizeIterator<Item = &str>` | The help lines, in order. |
| `code(&self) -> Option<Code>` | *Since 1.1.0.* The code, if set. |
| `suggestions(&self) -> &[Suggestion]` | *Since 1.1.0.* The suggestions, in the order added. |
| `to_json(&self, map: &SourceMap) -> String` | *Since 1.1.0.* One line of [JSON](#json-output). |
| `write_json(&self, out: &mut impl fmt::Write, map: &SourceMap) -> fmt::Result` | *Since 1.1.0.* The same, into a sink. |

`Diagnostic` is `Clone`, `Debug`, `PartialEq`, and `Eq`. Two diagnostics are
equal when every part above is equal, in order.

Constructing a diagnostic is on the front-end's hot path: it allocates only what
the message, labels, notes, and suggestions require. The suggestion list is
allocated on the first suggestion, so a diagnostic without one is the same size
to build and move as in 1.0 plus 16 bytes (the code is stored inline).

---

## `Code`

*Since 1.1.0.* The stable, machine-readable name of a *kind* of diagnostic:
`E0001`, `T0042`, `KRA0013`. One to four ASCII uppercase letters (the registry
that owns the code) followed by a number from `0` to `9999`, always written with
exactly four digits. Messages can be reworded between releases; codes do not
change, so tools key on them: an editor links them to documentation, a lint
configuration silences them, a test asserts on them, and a filter separates
lexical from parse errors by range (see the
[guide](#guide-code-registries-and-ranges)).

```rust
use diag_lang::Code;

const UNTERMINATED_STRING: Code = Code::new("KRA", 1).unwrap(); // checked at compile time

assert_eq!(UNTERMINATED_STRING.to_string(), "KRA0001");
assert_eq!((UNTERMINATED_STRING.prefix(), UNTERMINATED_STRING.number()), ("KRA", 1));
assert_eq!(Code::parse("KRA0001"), Some(UNTERMINATED_STRING));
assert_eq!(Code::parse("KRA1"), None); // four digits, always
assert_eq!(Code::new("kra", 1), None); // uppercase prefix
```

| Method | Description |
|--------|-------------|
| `const fn new(prefix: &str, number: u16) -> Option<Code>` | The code `prefix` + `number`, or `None` unless the prefix is 1–4 ASCII uppercase letters and the number is at most `9999`. `const`, so `Code::new("E", 1).unwrap()` in a `const` is checked at compile time. |
| `const fn parse(text: &str) -> Option<Code>` | Parses exactly what `Display` writes: 1–4 uppercase letters then exactly four digits. Nothing else parses (no whitespace, no other digit counts). |
| `prefix(&self) -> &str` | The registry prefix. |
| `const fn number(&self) -> u16` | The number within the registry, `0`–`9999`. |

`Code` is `Copy` (6 bytes), `Eq`, `Ord` (by prefix, then number — the same order
as the text), and `Hash`. `Display` writes the code (`E0001`); `Debug` writes
`Code(E0001)`. `parse(&code.to_string()) == Some(code)` for every code.

The renderer prints the code in the header, rustc-style:

```text
error[E0425]: cannot find value `foo` in this scope
```

---

## `Suggestion`, `Edit`, `Applicability`

*Since 1.1.0.* A **suggestion** is a proposed fix: a message, the **edits** that
make it, and an **applicability** saying how sure the emitter is. An edit
replaces the text a span covers; an empty span inserts, an empty replacement
deletes. Edit spans are global positions, exactly like label spans.

```rust
use diag_lang::{Applicability, Edit, Span, Suggestion};

// Wrap `a + b` (bytes 6..11) in parentheses: two insertions, applied together.
let fix = Suggestion::new("add parentheses", Applicability::MachineApplicable)
    .with_edit(Edit::insert(6, "("))
    .with_edit(Edit::insert(11, ")"));
assert_eq!(fix.edits().len(), 2);

let rename = Edit::new(Span::new(0, 3), "let");
let remove = Edit::delete(Span::new(10, 11));
assert_eq!((rename.replacement(), remove.replacement()), ("let", ""));
```

### `Suggestion`

| Method | Description |
|--------|-------------|
| `new(message: impl Into<Box<str>>, applicability: Applicability) -> Suggestion` | A suggestion with no edits yet. |
| `with_edit(self, edit: Edit) -> Suggestion` | Adds an edit. Edits of one suggestion are applied together, as a unit, and must not overlap; they may be added in any order. |
| `message(&self) -> &str` | What the fix does. |
| `edits(&self) -> &[Edit]` | The edits, in the order added. |
| `applicability(&self) -> Applicability` | How confident the emitter is. |

A suggestion with no edits is allowed: it renders as its message alone, like a
`help:` line. `Suggestion` is `Clone`, `Debug`, `PartialEq`, `Eq`.

### `Edit`

| Method | Description |
|--------|-------------|
| `new(span: Span, replacement: impl Into<Box<str>>) -> Edit` | Replace the text `span` covers. |
| `insert(at: u32, text: impl Into<Box<str>>) -> Edit` | Insert at global position `at` (`Edit::new(Span::empty(at), text)`). |
| `delete(span: Span) -> Edit` | Delete the text `span` covers (`Edit::new(span, "")`). |
| `span(&self) -> Span` | The span replaced. |
| `replacement(&self) -> &str` | The new text; empty for a deletion. |

`Edit` is `Clone`, `Debug`, `PartialEq`, `Eq`.

### `Applicability`

The levels follow rustc's, so tooling built for one reads the other.

| Variant | Meaning | `as_str()` |
|---|---|---|
| `MachineApplicable` | Definitely right and produces valid code: safe to apply unattended. [`apply_fixes`](#apply_edits-apply_fixes-applyerror) applies exactly these. | `"machine_applicable"` |
| `MaybeIncorrect` | Probably right; a person should look. | `"maybe_incorrect"` |
| `HasPlaceholders` | Contains placeholders (`/* value */`) a person must fill in. | `"has_placeholders"` |
| `Unspecified` (default) | The emitter did not say; treated like `MaybeIncorrect`. | `"unspecified"` |

`Applicability` is `Copy`, `Default`, `Display` (equal to `as_str`), `Eq`, `Ord`,
`Hash`, and **`#[non_exhaustive]`**: a finer level may be added in a minor
release, so a `match` needs a wildcard arm — treat an unknown level as
`Unspecified`.

### How the renderer shows a suggestion

After the notes and help lines, each suggestion is a `help:` block showing the
lines its edits touch as they would read after the fix. Two styles, as rustc:

- **Underline** — no edit only deletes: the fixed lines, with `+` under inserted
  text and `~` under replacement text.
- **Diff** — some edit only deletes (non-empty span, empty replacement): the
  lines before the fix marked `-`, then the lines after it marked `+`; lines the
  two share at the start or end are left out, so removing a whole line shows just
  that line.

```text
help: add `;`
  |
1 | let x = 1;
  |          +
  |
help: a value with a similar name exists
  |
2 | let y = count + 1;
  |         ~~~~~
  |
help: remove the binding
  |
3 - let unused = 2;
  |
```

A block over more than five lines shows its first two and last two, with `...`
between. Edits in a source other than the primary label's get their own block
headed `--> file:line:col` (the primary's source first, then map order). If any
edit cannot be placed — outside the loaded sources, ending past its source,
splitting a UTF-8 character, or overlapping another edit of the same suggestion
— the block is left out and only the `help: message` line is written: the
renderer never guesses at a fix. Replacement text and messages are escaped like
all other text (see [Renderer](#renderer)).

---

## `apply_edits`, `apply_fixes`, `ApplyError`

*Since 1.1.0.* Apply edits to source text: validated, ordered, atomic.

```rust
use diag_lang::{Edit, Span, apply_edits};

let fixed = apply_edits("let x = foo()", 0, &[
    Edit::insert(13, ";"),              // any order:
    Edit::new(Span::new(8, 11), "bar"), // applied by position
]).unwrap();
assert_eq!(fixed, "let x = bar();");
```

### `apply_edits`

```text
pub fn apply_edits<'a>(
    text: &str,
    base: u32,
    edits: impl IntoIterator<Item = &'a Edit>,
) -> Result<String, ApplyError>
```

Applies `edits` to `text` as one change against the original text and returns
the result. `base` is the global position of `text`'s first byte: `0` for local
offsets (a single-source map, an editor document), or
`file.span().start().to_u32()` for a file in a multi-source map. Edits may touch
positions `base..=base + text.len()` (inserting at the very end is allowed).

- Edits are applied in position order, whatever order they are given in.
- Two exactly identical edits are applied once; an edit that inserts nothing and
  deletes nothing is ignored.
- An insertion at the position where a replacement starts goes before the
  replacement's text; edits that only touch (one ends where the next starts) are
  fine.
- Anything else that touches the same bytes is a conflict, including two
  different insertions at one position.

Nothing is applied if any edit is refused. `O(n log n + len)` for `n` edits and an
output of `len` bytes; never panics.

### `apply_fixes`

```text
pub fn apply_fixes<'a>(
    text: &str,
    base: u32,
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
) -> Result<String, ApplyError>
```

The one-call "fix all" for one source: applies every `MachineApplicable`
suggestion of `diagnostics` whose edits all start inside
`base..=base + text.len()`, together, exactly as `apply_edits` would. Skipped:
suggestions that are not machine-applicable, that have no edits, that edit only
other sources, and that edit this source *and* another (a suggestion is a unit;
applying half of it would be wrong — apply such a suggestion's per-source edits
with `apply_edits` if you have all its sources). Accepts `&Diagnostics`, a slice,
a `Vec`, or any iterator of `&Diagnostic`.

**Source boundaries (1.x limitation).** source-lang 1.x lays sources out back to
back, so in a multi-source map the end of one source and the start of the next are
the same position. An edit starting exactly there is selected for both texts (the
end of the first, so end-of-file insertions work; the start of the second). A
single-source map has no such position. Until source-lang 2.0 separates them,
apply a suggestion at a boundary with `apply_edits` to the source you mean.

```rust
use diag_lang::{Applicability, Diagnostic, Edit, Label, Severity, Span, Suggestion, apply_fixes};

let missing_semi = |at: u32| {
    Diagnostic::new(Severity::Error, "expected `;`", Label::unlabelled(Span::empty(at)))
        .with_suggestion(
            Suggestion::new("add `;`", Applicability::MachineApplicable).with_edit(Edit::insert(at, ";")),
        )
};
let text = "let x = 1\nlet y = 2\n";
assert_eq!(
    apply_fixes(text, 0, &[missing_semi(9), missing_semi(19)]).unwrap(),
    "let x = 1;\nlet y = 2;\n",
);
```

### `ApplyError`

`#[non_exhaustive]`; `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `Display`,
`core::error::Error`.

| Variant | Meaning | What to do |
|---|---|---|
| `OutOfBounds { span }` | An edit is not inside `base..=base + text.len()`. | The edit was made for another source, a stale text, or with the wrong `base`; re-run the check against the current text. |
| `NotCharBoundary { span }` | An edit starts or ends inside a multi-byte character. | The emitter computed the span in the wrong unit (chars, UTF-16) or on different text. |
| `Overlap { first, second }` | Two edits overlap, or two different insertions share a position. `first` is the earlier by position. | Apply the suggestions one at a time and re-check between them. |

---

## `Diagnostics`

*Since 1.1.0.* A collector for a compilation's diagnostics: counts by severity,
duplicate suppression, ordering by position, and an optional cap.

```rust
use diag_lang::{Diagnostic, Diagnostics, Label, Renderer, Severity, SourceMap, Span};

let mut map = SourceMap::new();
map.add("a.txt", "abc").unwrap();
let bad = |at: u32| Diagnostic::new(Severity::Error, "bad", Label::unlabelled(Span::new(at, at + 1)));

let mut diags = Diagnostics::with_limit(2);
diags.push(bad(2));
diags.push(bad(0));
diags.push(bad(0)); // identical: ignored
diags.push(bad(1)); // over the cap: counted, not stored

assert_eq!((diags.error_count(), diags.len(), diags.omitted()), (3, 2, 1));
diags.sort();
let out = Renderer::new().render_all(&diags, &map);
assert!(out.ends_with("note: 1 more diagnostic not shown\n"));
```

| Method | Description |
|--------|-------------|
| `new() -> Diagnostics` | An empty, uncapped collector (also `Default`). |
| `with_limit(limit: usize) -> Diagnostics` | Stores at most `limit` diagnostics; the rest are counted. Use a limit for untrusted input. `0` stores nothing and only counts. |
| `push(&mut self, diag: Diagnostic) -> bool` | Adds `diag`; `false` (and no change) if an identical diagnostic is already stored. |
| `sort(&mut self)` | Stable sort by the primary label's start, then end, then severity (errors first). |
| `clear(&mut self)` | Removes everything and resets the counts; keeps the limit and storage. |
| `has_errors(&self) -> bool` | Whether any error was pushed (stored or omitted). |
| `error_count(&self) -> usize` | Distinct errors pushed, omitted ones included. |
| `warning_count(&self) -> usize` | Distinct warnings pushed, omitted ones included. |
| `count(&self, severity: Severity) -> usize` | Distinct diagnostics pushed at `severity`, omitted ones included. |
| `omitted(&self) -> usize` | New diagnostics pushed past the limit. |
| `limit(&self) -> Option<usize>` | The cap, `None` if uncapped. |
| `len(&self) -> usize` / `is_empty(&self) -> bool` | Stored diagnostics. |
| `as_slice(&self) -> &[Diagnostic]` / `iter(&self)` | The stored diagnostics, in push order (or sorted order after `sort`). |
| `into_vec(self) -> Vec<Diagnostic>` | The stored diagnostics. |
| `to_json(&self, map) -> String` / `write_json(&self, out, map)` | The [JSON document](#json-output). |

Also `Clone`, `Debug`, `Default`, `Extend<Diagnostic>`, `FromIterator<Diagnostic>`,
`IntoIterator` (by value and by reference).

**Duplicates.** A diagnostic equal to a stored one (every part, in order) is not
stored or counted again. The check is a keyed 64-bit fingerprint (SipHash-2-4,
written in the crate) looked up in an ordered index, `O(log n)` plus one full
comparison per stored diagnostic with the same fingerprint, so crafted messages
cannot make it quadratic.

**Cap.** Past the limit, new diagnostics are counted by severity and in `omitted`
but not kept, so memory is bounded whatever the input. Since they are not kept,
omitted diagnostics are recognised as duplicates of stored ones only, not of each
other.

---

## `Renderer`

Turns a `Diagnostic` plus a `SourceMap` into aligned, caret-annotated text — `^`
under the primary label, `-` under secondaries. Alignment is computed in terminal
display columns — each character takes its Unicode display width (UAX #11 via
`unicode-lang`: one column for most text, two for CJK ideographs and wide emoji,
zero for combining marks), and a tab expands to the next four-column tab stop — so
the underlines line up under the source whatever it contains. The output is
deterministic and always ends with a newline.

The header is `severity: message`, or `severity[CODE]: message` when the
diagnostic has a [code](#code) (*since 1.1.0*). A span covering several lines
underlines each line it touches, with the message on the last. A span covering
**more than five lines** shows only its first two and last two lines, with a `...`
line in place of the elided middle (a label pointing into the elided region is
still shown, with `...` on either side). Labels are grouped into one frame per
source file — the primary's file first, the rest in map order — and within a
frame each labelled line is shown once with its underlines stacked beneath.
Trailing `note:` and `help:` lines follow, then (*since 1.1.0*) one `help:` block
per [suggestion](#how-the-renderer-shows-a-suggestion). The frame order depends on
where labels point, not on the order they were added. A diagnostic with no code
and no suggestions renders exactly as in 1.0.

**End of input.** A zero-width label at the very end of a source — where a "found
end of input" error points, `Span::empty(len)` — renders in that source's frame
wherever source-lang (`>= 1.0.1`) attributes the position to it: in a single-source
map, and for the most recently added source. The caret sits just after the last
character of the last line (for a text ending in a line break, after the last
character before that break; for an empty source, at `1:1`), and the frame's
`--> file:line:col` header names exactly that line and column, so the header and the
caret never disagree. With one exception, from source-lang's contiguous 1.x layout: the end of a source that
is not the last one is the same position as the next source's first byte, so an
end-of-input label there renders at the next source's `1:1` (fixed by
source-lang 2.0's reserved end positions).

**Untrusted text.** Every caller-supplied string — source lines, the message,
label messages, notes, help, suggestion messages and replacement text, and file
names — is written with its control characters escaped visibly as `\u{..}`: C0
and C1 controls (so `ESC`, and with it any ANSI/OSC sequence), `DEL`, a carriage
return, U+2028/U+2029, and the bidi embedding/override/isolate/mark controls.
Tabs are kept (expanded in source lines); newlines are kept in the message,
labels, notes, and help, and escaped in file names. The only escape sequences in
the output are the renderer's own colours.

**Cost.** Rendering is bounded by what is printed, plus `O(log lines)` per label:
no line index is built per frame. Line numbers come from source-lang's cached line
table (`SourceMap::line_col`, source-lang `>= 1.0.1`), and line boundaries are
found by scanning outward from each label. A label covering a whole 50,000-line file
prints and costs the same as one covering six lines. A suggestion block costs
time linear in the lines its edits rewrite plus the replacement text, and prints
at most five lines per source.

```text
error: mismatched types
 --> main.rs:2:5
  |
1 | let x: i32 =
  |        --- expected due to this
2 |     "hi";
  |     ^^^^ expected `i32`, found `&str`
  |
```

```rust
use diag_lang::{Diagnostic, Label, Renderer, Severity, SourceMap, Span};

let mut map = SourceMap::new();
map.add("main.rs", "fn main() {\n    let x = foo();\n}\n").unwrap();

let diag = Diagnostic::new(
    Severity::Error,
    "cannot find value `foo` in this scope",
    Label::new(Span::new(24, 27), "not found in this scope"),
);

assert_eq!(Renderer::new().render(&diag, &map), "\
error: cannot find value `foo` in this scope
 --> main.rs:2:13
  |
2 |     let x = foo();
  |             ^^^ not found in this scope
  |
");
```

| Method | Description |
|--------|-------------|
| `new() -> Renderer` | A renderer with the default layout, producing plain output. |
| `with_color(self) -> Renderer` | Enables ANSI styling. Requires the `color` feature. |
| `render(&self, diag: &Diagnostic, map: &SourceMap) -> String` | Renders into a fresh `String`. |
| `render_to(&self, out: &mut impl fmt::Write, diag: &Diagnostic, map: &SourceMap) -> fmt::Result` | Renders into a sink you own — reuse a buffer to avoid a per-diagnostic allocation. |
| `render_all(&self, diags: &Diagnostics, map: &SourceMap) -> String` | *Since 1.1.0.* Renders a whole collector. |
| `render_all_to(&self, out: &mut impl fmt::Write, diags: &Diagnostics, map: &SourceMap) -> fmt::Result` | *Since 1.1.0.* The same, into a sink. |

`render_to` is the output seam: it writes into any `core::fmt::Write`, so the same
renderer drives a terminal, a string, a reused buffer, or a formatter. `with_color`
(with the `color` feature) wraps the output in ANSI escapes coloured by severity;
stripping those escapes reproduces the plain output byte for byte, so colour only
ever adds.

`render_all` renders each stored diagnostic exactly as `render` does, in the
collector's current order, with a blank line between consecutive diagnostics.
If the cap omitted any, a final `note: N more diagnostics not shown` line (or
`1 more diagnostic`) follows after a blank line. An empty collector with nothing
omitted renders as the empty string.

A span that cannot be located in the map — past the end of the sources, or on an
empty map — renders the header plus a defined note rather than panicking:

```text
error: unexpected end of input
 = note: span 10..12 is outside the loaded sources
```

`Renderer` is `#[non_exhaustive]`; build it with `new()` or `Default`.

---

## JSON output

*Since 1.1.0.* A dependency-free, deterministic JSON form of diagnostics for
language servers, MCP tools, AI clients, and anything that reads diagnostics
rather than displays them.

| Function | Output |
|---|---|
| `Diagnostic::to_json(&map) -> String` / `write_json(out, &map)` | One diagnostic object, on one line (no trailing newline). Write one per line for JSON Lines. |
| `Diagnostics::to_json(&map) -> String` / `write_json(out, &map)` | One document holding the collection. |
| `JSON_SCHEMA_VERSION: u32` | `1`: the value of every top-level `"version"` key. |

**Format.** No whitespace outside strings. Keys always appear in the order below.
The same input always produces the same bytes. Strings are escaped per RFC 8259:
`"` and `\` as `\"` and `\\`, control characters below U+0020 as `\b \f \n \r \t`
or `\u00XX`, and U+2028 and U+2029 as `\u2028` and `\u2029` (so the output is
also a valid JavaScript literal). Everything else, including non-ASCII, is raw
UTF-8. Numbers are non-negative integers.

### Schema, version 1

A **diagnostic**:

```text
{
  "version": 1,
  "severity": "error" | "warning" | "note" | "help",
  "code": "E0001" | null,
  "message": string,
  "labels": [label, ...],          // the primary first, then the secondaries in order added
  "notes": [string, ...],
  "help": [string, ...],
  "suggestions": [suggestion, ...]
}
```

A **label**: `{"primary": bool, "message": string, "span": span, "location": location | null}`

A **suggestion**: `{"message": string, "applicability": "machine_applicable" | "maybe_incorrect" | "has_placeholders" | "unspecified", "edits": [edit, ...]}`

An **edit**: `{"span": span, "location": location | null, "replacement": string}`

A **span** is the global byte span as given: `{"start": u32, "end": u32}`.

A **location** resolves it against the map:
`{"file": string, "start": point, "end": point}`, where a **point** is
`{"offset": bytes into the file, "line": 1-based, "column": 1-based}`. `column`
counts Unicode scalar values (the numbers the text renderer prints after `-->`);
`offset` lets a client compute any other unit (UTF-16 for LSP). The file is the
one the span's start falls in; an end past that file is clamped to its end, and an
offset inside a multi-byte character is moved to that character's start. Unlike
the text renderer, which draws an end-of-input caret before a final line break,
JSON reports positions exactly: the end of `"ab\n"` is line 2, column 1.
`location` is `null` for a span outside every loaded source.

A **document** (from `Diagnostics`):

```text
{"version": 1, "errors": usize, "warnings": usize, "omitted": usize, "diagnostics": [diagnostic, ...]}
```

`errors` and `warnings` count every distinct diagnostic pushed, omitted ones
included; `diagnostics` holds the stored ones in their current order, each exactly
as `Diagnostic::to_json` writes it.

```rust
use diag_lang::{Diagnostic, Label, Severity, SourceMap, Span};

let mut map = SourceMap::new();
map.add("a.txt", "let x = y;").unwrap();
let diag = Diagnostic::new(Severity::Error, "unknown `y`", Label::new(Span::new(8, 9), "here"));
assert_eq!(
    diag.to_json(&map),
    concat!(
        r#"{"version":1,"severity":"error","code":null,"message":"unknown `y`","#,
        r#""labels":[{"primary":true,"message":"here","span":{"start":8,"end":9},"#,
        r#""location":{"file":"a.txt","start":{"offset":8,"line":1,"column":9},"#,
        r#""end":{"offset":9,"line":1,"column":10}}}],"#,
        r#""notes":[],"help":[],"suggestions":[]}"#,
    ),
);
```

**Evolution.** Within version 1, keys are only ever *added*, never removed,
renamed, reordered relative to each other, or changed in meaning. A reader should
ignore keys it does not know. A change that removes a key or changes a meaning
raises `JSON_SCHEMA_VERSION` (and is a major release of this crate).

**Mapping to LSP.** For `textDocument/publishDiagnostics`: `severity` 1–4 from
error/warning/note/help, `code` from `code`, `range` from the primary label's
location (converting `offset` to the session's encoding), `relatedInformation`
from the secondary labels. Each suggestion maps to a `CodeAction` with a
`WorkspaceEdit` built from its edits; `MachineApplicable` maps to
`isPreferred: true`.

---

## Guide: code registries and ranges

A code's prefix names the registry that owns it; the number range within a
registry names the phase. Codes in use across the `-lang` family:

| Prefix | Owner | Range |
|---|---|---|
| `E` | Runtime errors, OPS (`specs/OPS.md`) | `E0001`–`E0099` |
| `E` | Runtime errors, LSB (`specs/LSB.md`) | `E0100`–`E0199` |
| `T` | Type checking (typeck-lang) | `T0001`– |

A language front end takes its own prefix — Kraken uses `KRA` — and declares its
codes as constants in one module, its registry:

```rust
pub mod codes {
    use diag_lang::Code;

    // Lexical: 0001–0999
    pub const UNTERMINATED_STRING: Code = Code::new("KRA", 1).unwrap();
    pub const INVALID_CHARACTER: Code = Code::new("KRA", 2).unwrap();
    // Parse: 1000–1999
    pub const UNEXPECTED_TOKEN: Code = Code::new("KRA", 1000).unwrap();
    pub const UNCLOSED_DELIMITER: Code = Code::new("KRA", 1001).unwrap();
    // Semantic: 2000–7999
    pub const UNKNOWN_NAME: Code = Code::new("KRA", 2000).unwrap();
    // Internal: 9999
    pub const INTERNAL: Code = Code::new("KRA", 9999).unwrap();
}

assert_eq!(codes::UNKNOWN_NAME.to_string(), "KRA2000");
```

The recommended split within a prefix:

| Range | Phase | Examples |
|---|---|---|
| `0001`–`0999` | **Lexical** | bad character, unterminated string or comment, malformed number |
| `1000`–`1999` | **Parse** | unexpected or missing token, unclosed delimiter, end of input |
| `2000`–`7999` | **Semantic** | name resolution (`2xxx`), types (`3xxx`), flow and borrow checks, ... |
| `8000`–`8999` | **Lints** | style and suspicious-construct warnings |
| `9000`–`9999` | **Internal / tool** | `9999`: internal compiler error |

With this split, "is this a lexical error?" is a range check on the code, not a
guess from the message text — what a formatter needs to keep regions it cannot
lex, and what an editor needs to group problems:

```rust
use diag_lang::{Code, Diagnostic};

fn is_lexical(diag: &Diagnostic, prefix: &str) -> bool {
    diag.code().is_some_and(|c: Code| c.prefix() == prefix && c.number() < 1000)
}
```

Rules that keep codes useful:

- **Never reuse a number.** A retired code stays retired; its documentation says
  so.
- **One code per kind, not per message.** `KRA1000` is "unexpected token" for
  every token; the token goes in the message.
- **Codes are API.** Adding one is a minor change for your language; changing
  what one means is a major one.
- The crate does not enforce ranges: any well-formed code is accepted, so a
  registry can choose differently and document it.

---

## Re-exports

For convenience, the position and source types that appear in this crate's API are
re-exported, so a downstream need not also name `span-lang` and `source-lang` to
spell them:

- `Span` — from [`span-lang`](https://docs.rs/span-lang); the byte range a `Label` or `Edit` points at.
- `SourceMap` — from [`source-lang`](https://docs.rs/source-lang); the sources a span is resolved against.

---

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | yes | Standard library. With it off, the crate is `no_std` (it always needs `alloc`). Forwards to `span-lang/std` and `source-lang/std`. Every 1.1 item is available without it. |
| `color` | yes | ANSI styling via `Renderer::with_color`. Disable it, or never call `with_color`, for plain output — which is complete and identically aligned. |

---

## Stability

`diag-lang` 1.0 is the API freeze; 1.1 only adds. Everything below is covered by
[Semantic Versioning](https://semver.org/spec/v2.0.0.html): it does not change in
a breaking way before `2.0`.

### The frozen surface

| Item | Frozen | Since |
|---|---|---|
| `Severity` | `Error`, `Warning`, `Note`, `Help` (exhaustive), `as_str`; `Clone`, `Copy`, `Debug`, `Display`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`. | 1.0 |
| `Label` | `new`, `unlabelled`, `span`, `message`; `Clone`, `Debug`, `PartialEq`, `Eq`. | 1.0 |
| `Diagnostic` | `new`, `with_secondary`, `with_note`, `with_help`, `severity`, `message`, `primary`, `secondary`, `notes`, `help`; `Clone`, `Debug`, `PartialEq`, `Eq`. | 1.0 |
| `Diagnostic` | `with_code`, `code`, `with_suggestion`, `suggestions`, `to_json`, `write_json`. | 1.1 |
| `Renderer` | `new`, `with_color` (`color`), `render`, `render_to`; `Clone`, `Debug`, `Default`; `#[non_exhaustive]`. | 1.0 |
| `Renderer` | `render_all`, `render_all_to`. | 1.1 |
| `Code` | `new`, `parse`, `prefix`, `number`; `Clone`, `Copy`, `Debug`, `Display`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`. | 1.1 |
| `Suggestion` | `new`, `with_edit`, `message`, `edits`, `applicability`; `Clone`, `Debug`, `PartialEq`, `Eq`. | 1.1 |
| `Edit` | `new`, `insert`, `delete`, `span`, `replacement`; `Clone`, `Debug`, `PartialEq`, `Eq`. | 1.1 |
| `Applicability` | `MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders`, `Unspecified`, `as_str`; `Clone`, `Copy`, `Debug`, `Default`, `Display`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`. `#[non_exhaustive]`: new levels may be added in minor releases. | 1.1 |
| `apply_edits`, `apply_fixes` | Signatures and the application rules in [their section](#apply_edits-apply_fixes-applyerror). | 1.1 |
| `ApplyError` | `OutOfBounds { span }`, `NotCharBoundary { span }`, `Overlap { first, second }`; `Clone`, `Copy`, `Debug`, `Display`, `Error`, `PartialEq`, `Eq`. `#[non_exhaustive]`: new variants may be added in minor releases. | 1.1 |
| `Diagnostics` | `new`, `with_limit`, `push`, `sort`, `clear`, `has_errors`, `error_count`, `warning_count`, `count`, `omitted`, `limit`, `len`, `is_empty`, `as_slice`, `iter`, `into_vec`, `to_json`, `write_json`; `Clone`, `Debug`, `Default`, `Extend`, `FromIterator`, `IntoIterator`. | 1.1 |
| `JSON_SCHEMA_VERSION` | The constant; its value changes only with a major release. | 1.1 |
| Re-exports | `Span` from `span-lang` 0.4, `SourceMap` from `source-lang` 1. | 1.0 |
| Features | `std`, `color`, both default-on and additive. | 1.0 |

### The behavioural contract

These are part of the API, not implementation details:

- **Layout.** The rendered layout rules documented on [`Renderer`](#renderer)
  and [How the renderer shows a suggestion](#how-the-renderer-shows-a-suggestion):
  the header (with `[CODE]` when set), one frame per source in the stated order,
  `^`/`-` underlines in display columns, the five-line elision rule, the order
  notes → help → suggestion blocks, the underline and diff styles with `+`, `~`,
  and `-`, the `render_all` separators and summary line. A diagnostic without a
  code or suggestions renders as in 1.0.
- **Safety of output.** Control characters in caller-supplied text are escaped;
  the only escape sequences in rendered text are the renderer's own colours.
  Stripping the colours reproduces the plain output.
- **Determinism.** The same diagnostic and map render, and serialise, to the same
  bytes.
- **Codes.** The format (1–4 uppercase letters, exactly four digits), `parse`
  accepting exactly what `Display` writes, and the ordering.
- **Applying edits.** Atomic; position order; identical edits once; no-ops
  ignored; an insertion before a replacement starting at the same position;
  everything else touching the same bytes a conflict. `apply_fixes` selects
  exactly as documented.
- **Collector.** Duplicates (full equality) are not stored or counted; counts
  include omitted diagnostics; the cap bounds what is stored; `sort` is stable by
  (start, end, severity).
- **JSON.** Schema version 1 as documented: key order, escaping, the meaning of
  every key, `null` for no code and no location. Keys may be added in a minor
  release; nothing is removed or changed in meaning without a new schema version.
- **No panics.** The crate is `#![forbid(unsafe_code)]` and does not panic on any
  diagnostic, map, edit, or text.

### Not promised

- The exact bytes of a rendered frame beyond the documented layout rules (for
  example which columns a marker covers under an unusual character), and `Debug`
  output.
- The exact wording of `ApplyError` messages; match on variants.
- Which conflict `ApplyError::Overlap` reports when there are several, beyond
  being deterministic.
- The fingerprint function behind duplicate suppression, and performance figures.

### Deliberately left out, addable later without a breaking change

- **A `Render` trait.** The output *sink* is swappable through
  `render_to(&mut impl fmt::Write)`; the JSON emitter is a second output form but
  has a different shape (it needs no layout options), so a common trait would
  still be speculative.
- **Serialisation via `serde`.** The [JSON output](#json-output) covers the
  machine-readable case without a dependency; a `serde` feature can still be
  added as a non-breaking minor.
- **Configurable tab width.** Fixed at four columns; a `with_tab_width` setter
  can be added without breaking the surface.
- **Grapheme clustering.** Display width is summed per character, so a ZWJ emoji
  sequence or a flag counts as the sum of its parts. Terminals disagree on these
  anyway; a cluster-aware width can be added without changing the API.
- **A multi-source fix applier.** `apply_fixes` works one source at a time and
  skips suggestions that span sources; a `SourceMap`-wide variant returning one
  text per source can be added.

### Recorded for 2.0

- **`Severity` and `#[non_exhaustive]` (ISSUES F13).** `Severity` is exhaustive,
  and code downstream matches on it without a wildcard arm (lsp-lang's severity
  mapping is one). Adding `#[non_exhaustive]` — or a new level — breaks those
  matches, so it is not done in 1.x. The family-wide `non_exhaustive` policy (F13)
  decides it for 2.0.

### Dependencies and MSRV

`span-lang` 0.4, `source-lang` `>= 1.0.1`, `unicode-lang` 1 — all first-party,
from crates.io. No third-party runtime dependency; 1.1 adds none (the JSON
writer, the fingerprint hash, and the edit applier are written in the crate).
MSRV is Rust 1.85; it rises only in a minor release.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>
