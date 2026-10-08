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
    // Read the base from the map rather than computing it (contiguous: 15).
    let b_id = map.add("b.rs", b).unwrap();
    let b_base = map.source(b_id).unwrap().span().start().to_u32();
    assert_eq!(b_base, 15);

    let diag = Diagnostic::new(
        Severity::Error,
        "this function takes 0 arguments but 1 was supplied",
        Label::new(span_of(b, "foo", b_base), "unexpected argument"),
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

// ---------------------------------------------------------------------------
// 1.0.1 regressions: ISSUES M16 (elision, linear grouping)
// ---------------------------------------------------------------------------

/// Ten short lines, `l1` through `l10`, each newline-terminated.
fn ten_lines() -> String {
    let mut src = String::new();
    for i in 1..=10 {
        src.push('l');
        src.push_str(&i.to_string());
        src.push('\n');
    }
    src
}

#[test]
fn test_m16_span_over_five_lines_is_shown_whole() {
    let src = ten_lines();
    let mut map = SourceMap::new();
    map.add("m.rs", src.as_str()).unwrap();

    // l2 through l6: exactly five lines, the most shown without elision.
    let start = src.find("l2").unwrap() as u32;
    let end = src.find("l6").unwrap() as u32 + 2;
    let diag = Diagnostic::new(
        Severity::Error,
        "block",
        Label::new(Span::new(start, end), "five lines"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: block
 --> m.rs:2:1
  |
2 | l2
  | ^^
3 | l3
  | ^^
4 | l4
  | ^^
5 | l5
  | ^^
6 | l6
  | ^^ five lines
  |
"
    );
}

#[test]
fn test_m16_long_span_elides_its_middle() {
    let src = ten_lines();
    let mut map = SourceMap::new();
    map.add("m.rs", src.as_str()).unwrap();

    // l2 through l9: eight lines, so only the first two and last two show.
    let start = src.find("l2").unwrap() as u32;
    let end = src.find("l9").unwrap() as u32 + 2;
    let diag = Diagnostic::new(
        Severity::Error,
        "block",
        Label::new(Span::new(start, end), "whole block"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: block
 --> m.rs:2:1
  |
2 | l2
  | ^^
3 | l3
  | ^^
...
8 | l8
  | ^^
9 | l9
  | ^^ whole block
  |
"
    );
}

#[test]
fn test_m16_label_inside_an_elided_span_is_still_shown() {
    let src = ten_lines();
    let mut map = SourceMap::new();
    map.add("m.rs", src.as_str()).unwrap();

    let start = src.find("l2").unwrap() as u32;
    let end = src.find("l9").unwrap() as u32 + 2;
    let diag = Diagnostic::new(
        Severity::Error,
        "block",
        Label::new(Span::new(start, end), "outer"),
    )
    .with_secondary(Label::new(span_of(&src, "l5", 0), "inner"));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: block
 --> m.rs:2:1
  |
2 | l2
  | ^^
3 | l3
  | ^^
...
5 | l5
  | -- inner
...
8 | l8
  | ^^
9 | l9
  | ^^ outer
  |
"
    );
}

#[test]
fn test_m16_distance_between_unrelated_labels_is_not_an_elision() {
    let src = ten_lines();
    let mut map = SourceMap::new();
    map.add("m.rs", src.as_str()).unwrap();

    // Lines 1 and 7 are far apart, but no span covers the gap: no `...`.
    let diag = Diagnostic::new(
        Severity::Error,
        "apart",
        Label::new(span_of(&src, "l7", 0), "here"),
    )
    .with_secondary(Label::new(span_of(&src, "l1", 0), "and here"));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: apart
 --> m.rs:7:1
  |
1 | l1
  | -- and here
7 | l7
  | ^^ here
  |
"
    );
}

#[test]
fn test_m16_whole_file_label_on_a_large_file_prints_a_bounded_frame() {
    let mut src = String::new();
    for i in 0..50_000 {
        src.push_str(&format!("line {i}\n"));
    }
    let len = src.len() as u32;
    let mut map = SourceMap::new();
    map.add("big.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "everything",
        Label::new(Span::new(0, len), "all of it"),
    );
    let out = Renderer::new().render(&diag, &map);

    assert_eq!(
        out,
        "\
error: everything
     --> big.rs:1:1
      |
    1 | line 0
      | ^^^^^^
    2 | line 1
      | ^^^^^^
...
49999 | line 49998
      | ^^^^^^^^^^
50000 | line 49999
      | ^^^^^^^^^^ all of it
      |
"
    );
}

/// Many labels on many lines group in one pass and keep a stable order.
#[test]
fn test_m16_many_labels_render_in_line_order() {
    let mut src = String::new();
    for i in 0..200 {
        src.push_str(&format!("v{i:03}\n"));
    }
    let mut map = SourceMap::new();
    map.add("m.rs", src.as_str()).unwrap();

    let mut diag = Diagnostic::new(
        Severity::Error,
        "many",
        Label::new(span_of(&src, "v199", 0), "last"),
    );
    // Secondaries added in reverse line order.
    for i in (0..199).rev() {
        diag = diag.with_secondary(Label::new(span_of(&src, &format!("v{i:03}"), 0), "x"));
    }
    let out = Renderer::new().render(&diag, &map);

    let shown: Vec<u32> = out
        .lines()
        .filter_map(|l| l.split(" | ").next()?.trim().parse().ok())
        .collect();
    assert_eq!(shown, (1..=200).collect::<Vec<u32>>());
    assert!(!out.contains("..."));
}
