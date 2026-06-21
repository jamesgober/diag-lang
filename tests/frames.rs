//! Byte-exact snapshot tests for multi-line spans, secondary labels, and notes.
//!
//! These hold the v0.3.0 frame layout: a span covering several lines underlines
//! each, secondary labels stack with `-` under the lines they point at, labels in
//! other files render their own frame, and notes/help close the output — all in a
//! stable order that does not depend on the order labels were added.

#![allow(clippy::unwrap_used)]

use diag_lang::{Diagnostic, Label, Renderer, Severity};
use diag_lang::{SourceMap, Span};

/// The byte span of the first occurrence of `needle` in `text`, offset by `base`.
fn span_of(text: &str, needle: &str, base: u32) -> Span {
    let at = text.find(needle).expect("needle present in text") as u32;
    Span::new(base + at, base + at + needle.len() as u32)
}

#[test]
fn test_multi_line_span_underlines_every_covered_line() {
    let src = "foo(\n  bar,\n)\n";
    let mut map = SourceMap::new();
    map.add("m.rs", src).unwrap();

    // The whole call, "foo(" through ")", spans lines 1–3.
    let diag = Diagnostic::new(
        Severity::Error,
        "unbalanced delimiter",
        Label::new(Span::new(0, 13), "this group is never closed"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: unbalanced delimiter
 --> m.rs:1:1
  |
1 | foo(
  | ^^^^
2 |   bar,
  | ^^^^^^
3 | )
  | ^ this group is never closed
  |
"
    );
}

#[test]
fn test_secondary_label_on_another_line_shares_the_frame() {
    let src = "let x: i32 =\n    \"hi\";\n";
    let mut map = SourceMap::new();
    map.add("m.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "mismatched types",
        Label::new(span_of(src, "\"hi\"", 0), "expected `i32`, found `&str`"),
    )
    .with_secondary(Label::new(span_of(src, "i32", 0), "expected due to this"));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: mismatched types
 --> m.rs:2:5
  |
1 | let x: i32 =
  |        --- expected due to this
2 |     \"hi\";
  |     ^^^^ expected `i32`, found `&str`
  |
"
    );
}

#[test]
fn test_two_labels_on_one_line_stack_left_to_right() {
    let src = "let x: i32 = \"hi\";\n";
    let mut map = SourceMap::new();
    map.add("m.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "mismatched types",
        Label::new(span_of(src, "\"hi\"", 0), "found `&str`"),
    )
    .with_secondary(Label::new(
        span_of(src, "i32", 0),
        "expected because of this",
    ));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: mismatched types
 --> m.rs:1:14
  |
1 | let x: i32 = \"hi\";
  |        --- expected because of this
  |              ^^^^ found `&str`
  |
"
    );
}

#[test]
fn test_secondary_label_in_another_file_gets_its_own_frame() {
    let mut map = SourceMap::new();
    let a = "pub fn foo() {}";
    map.add("a.rs", a).unwrap(); // global 0..15
    let b = "foo(1);";
    map.add("b.rs", b).unwrap(); // global 15..22

    let diag = Diagnostic::new(
        Severity::Error,
        "this function takes 0 arguments but 1 was supplied",
        Label::new(span_of(b, "foo", 15), "unexpected argument"),
    )
    .with_secondary(Label::new(span_of(a, "foo", 0), "defined here"));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: this function takes 0 arguments but 1 was supplied
 --> b.rs:1:1
  |
1 | foo(1);
  | ^^^ unexpected argument
  |
 --> a.rs:1:8
  |
1 | pub fn foo() {}
  |        --- defined here
  |
"
    );
}

#[test]
fn test_notes_and_help_close_the_output() {
    let src = "let x = 1;";
    let mut map = SourceMap::new();
    map.add("m.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Warning,
        "unused variable `x`",
        Label::new(span_of(src, "x", 0), "never read"),
    )
    .with_note("`x` is assigned but its value is never used")
    .with_help("prefix it with an underscore: `_x`");

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
warning: unused variable `x`
 --> m.rs:1:5
  |
1 | let x = 1;
  |     ^ never read
  |
 = note: `x` is assigned but its value is never used
 = help: prefix it with an underscore: `_x`
"
    );
}

#[test]
fn test_unlocatable_secondary_renders_a_note_after_the_primary_frame() {
    let src = "abc";
    let mut map = SourceMap::new();
    map.add("m.rs", src).unwrap(); // global 0..3

    let diag = Diagnostic::new(
        Severity::Error,
        "bad reference",
        Label::new(span_of(src, "a", 0), "here"),
    )
    .with_secondary(Label::new(Span::new(50, 52), "and over here"));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: bad reference
 --> m.rs:1:1
  |
1 | abc
  | ^ here
  |
 = note: span 50..52 is outside the loaded sources
"
    );
}

#[test]
fn test_label_order_does_not_depend_on_insertion_order() {
    let src = "aaa\nbbb\nccc\n";
    let build = |reverse: bool| {
        let mut map = SourceMap::new();
        map.add("m.rs", src).unwrap();
        let s1 = Label::new(span_of(src, "aaa", 0), "first");
        let s2 = Label::new(span_of(src, "ccc", 0), "third");
        let primary = Label::new(span_of(src, "bbb", 0), "second");
        let diag = Diagnostic::new(Severity::Error, "ordered", primary);
        if reverse {
            diag.with_secondary(s2).with_secondary(s1)
        } else {
            diag.with_secondary(s1).with_secondary(s2)
        }
    };

    // Adding the two secondaries in either order renders the same frame: lines are
    // shown top to bottom regardless.
    let map = {
        let mut m = SourceMap::new();
        m.add("m.rs", src).unwrap();
        m
    };
    let forward = Renderer::new().render(&build(false), &map);
    let reverse = Renderer::new().render(&build(true), &map);
    assert_eq!(forward, reverse);
    assert_eq!(
        forward,
        "\
error: ordered
 --> m.rs:2:1
  |
1 | aaa
  | --- first
2 | bbb
  | ^^^ second
3 | ccc
  | --- third
  |
"
    );
}
