//! Byte-exact snapshot tests for the single-line caret renderer.
//!
//! Each test renders a diagnostic against a known source and asserts the whole
//! output string byte for byte. The point is the section-4 invariant: the caret
//! aligns exactly under the characters the span covers, across multi-byte UTF-8
//! and tab expansion, and identical input renders identical output.

#![allow(clippy::unwrap_used)]

use diag_lang::{Diagnostic, Label, Renderer, Severity};
use diag_lang::{SourceMap, Span};

/// The byte span of the first occurrence of `needle` in `text`, offset by `base`
/// so it lands in the right file's slot in the global position space.
fn span_of(text: &str, needle: &str, base: u32) -> Span {
    let at = text.find(needle).expect("needle present in text") as u32;
    Span::new(base + at, base + at + needle.len() as u32)
}

#[test]
fn test_ascii_single_line_underlines_the_span() {
    let src = "let x = 1;";
    let mut map = SourceMap::new();
    map.add("a.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Warning,
        "unused variable",
        Label::new(span_of(src, "x", 0), "unused"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
warning: unused variable
 --> a.rs:1:5
  |
1 | let x = 1;
  |     ^ unused
  |
"
    );
}

#[test]
fn test_multibyte_prefix_keeps_carets_aligned() {
    // "x = αβ;" — the span is the two-character, four-byte Greek pair. The caret
    // run must start at column 5 (after "x = "), not byte 5.
    let src = "x = αβ;";
    let mut map = SourceMap::new();
    map.add("g.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "unexpected characters",
        Label::new(span_of(src, "αβ", 0), "here"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: unexpected characters
 --> g.rs:1:5
  |
1 | x = αβ;
  |     ^^ here
  |
"
    );
}

#[test]
fn test_multibyte_span_draws_one_caret_per_character() {
    // "héllo" is five characters but six bytes; the underline is five carets.
    let src = "héllo = 1;";
    let mut map = SourceMap::new();
    map.add("u.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "reserved name",
        Label::new(span_of(src, "héllo", 0), "not allowed"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: reserved name
 --> u.rs:1:1
  |
1 | héllo = 1;
  | ^^^^^ not allowed
  |
"
    );
}

#[test]
fn test_leading_tab_expands_and_aligns_the_caret() {
    // A leading tab expands to four columns; the caret sits under the expanded
    // position, and the displayed line shows spaces, not a tab.
    let src = "\tx = 1;";
    let mut map = SourceMap::new();
    map.add("t.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "tab indent",
        Label::new(span_of(src, "x", 0), "after tab"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: tab indent
 --> t.rs:1:2
  |
1 |     x = 1;
  |     ^ after tab
  |
"
    );
}

#[test]
fn test_zero_width_span_draws_a_single_caret() {
    let src = "abc";
    let mut map = SourceMap::new();
    map.add("z.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "missing separator",
        Label::new(Span::empty(1), "expected `;`"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: missing separator
 --> z.rs:1:2
  |
1 | abc
  |  ^ expected `;`
  |
"
    );
}

#[test]
fn test_unlabelled_label_renders_a_bare_caret() {
    let src = "abc";
    let mut map = SourceMap::new();
    map.add("b.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Note,
        "see here",
        Label::unlabelled(span_of(src, "a", 0)),
    );

    // No trailing message means no trailing space after the caret.
    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
note: see here
 --> b.rs:1:1
  |
1 | abc
  | ^
  |
"
    );
}

#[test]
fn test_multi_digit_line_number_widens_the_gutter() {
    let src = "1\n2\n3\n4\n5\n6\n7\n8\n9\nxy\n";
    let mut map = SourceMap::new();
    map.add("m.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "deep line",
        Label::new(span_of(src, "x", 0), "on line ten"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: deep line
  --> m.rs:10:1
   |
10 | xy
   | ^ on line ten
   |
"
    );
}

#[test]
fn test_span_in_second_file_names_that_file() {
    let mut map = SourceMap::new();
    map.add("a.rs", "first").unwrap(); // global 0..5
    let second = "second line";
    map.add("b.rs", second).unwrap(); // global 5..16

    let diag = Diagnostic::new(
        Severity::Error,
        "in the second file",
        Label::new(span_of(second, "line", 5), "here"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: in the second file
 --> b.rs:1:8
  |
1 | second line
  |        ^^^^ here
  |
"
    );
}

#[test]
fn test_span_outside_sources_renders_a_defined_note() {
    let map = SourceMap::new(); // empty: nothing to locate against

    let diag = Diagnostic::new(
        Severity::Error,
        "unexpected end of input",
        Label::unlabelled(Span::new(10, 12)),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: unexpected end of input
 = note: span 10..12 is outside the loaded sources
"
    );
}

#[test]
fn test_span_past_end_of_only_source_falls_back() {
    let mut map = SourceMap::new();
    map.add("a.rs", "abc").unwrap(); // global 0..3

    let diag = Diagnostic::new(
        Severity::Error,
        "ran off the end",
        Label::new(Span::new(5, 7), "out here"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: ran off the end
 = note: span 5..7 is outside the loaded sources
"
    );
}

#[test]
fn test_identical_input_renders_byte_identical_output() {
    let build = || {
        let mut map = SourceMap::new();
        map.add("x.rs", "fn f() {}").unwrap();
        let diag = Diagnostic::new(
            Severity::Error,
            "same every time",
            Label::new(span_of("fn f() {}", "f()", 0), "call"),
        );
        Renderer::new().render(&diag, &map)
    };

    // Two independently built maps and renderers produce the same bytes, and a
    // second render of the same inputs matches the first exactly.
    assert_eq!(build(), build());
}
