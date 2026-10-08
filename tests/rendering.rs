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
    // Read the base from the map rather than computing it (contiguous: 5).
    let b_id = map.add("b.rs", second).unwrap();
    let b_base = map.source(b_id).unwrap().span().start().to_u32();
    assert_eq!(b_base, 5);

    let diag = Diagnostic::new(
        Severity::Error,
        "in the second file",
        Label::new(span_of(second, "line", b_base), "here"),
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

// ---------------------------------------------------------------------------
// 1.0.1 regressions
// ---------------------------------------------------------------------------

/// ISSUES H04, reproduced textually from lang-forge: its parser reports every
/// "found end of input" error at `Span::empty(src.len())`. That used to render
/// as "span N..N is outside the loaded sources" with no frame; it must render in
/// the source's frame with the caret just after the last character.
#[test]
fn test_h04_lang_forge_end_of_input_renders_a_framed_caret() {
    let src = "[rules]\nprogram = \"stmt*\"\nstmt = \"expr ';'\n";
    let mut map = SourceMap::new();
    map.add("broken.lsf", src).unwrap();

    let eof = Span::empty(src.len() as u32);
    let diag = Diagnostic::new(
        Severity::Error,
        "expected `\"`, found end of input",
        Label::new(eof, "expected `\"`"),
    );

    let out = Renderer::new().render(&diag, &map);
    assert!(!out.contains("outside the loaded sources"), "{out}");
    assert_eq!(
        out,
        "\
error: expected `\"`, found end of input
 --> broken.lsf:3:17
  |
3 | stmt = \"expr ';'
  |                 ^ expected `\"`
  |
"
    );
}

#[test]
fn test_h04_end_of_input_without_trailing_newline() {
    let src = "a = b +";
    let mut map = SourceMap::new();
    map.add("e.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "expected expression, found end of input",
        Label::new(Span::empty(src.len() as u32), "expected expression"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: expected expression, found end of input
 --> e.rs:1:8
  |
1 | a = b +
  |        ^ expected expression
  |
"
    );
}

#[test]
fn test_h04_end_of_input_after_crlf() {
    let src = "ab\r\n";
    let mut map = SourceMap::new();
    map.add("w.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "unexpected end",
        Label::unlabelled(Span::empty(src.len() as u32)),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: unexpected end
 --> w.rs:1:3
  |
1 | ab
  |   ^
  |
"
    );
}

#[test]
fn test_h04_end_of_input_in_an_empty_file() {
    let mut map = SourceMap::new();
    map.add("empty.lsf", "").unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "expected `[language]`, found end of input",
        Label::new(Span::empty(0), "expected `[language]`"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: expected `[language]`, found end of input
 --> empty.lsf:1:1
  |
1 | 
  | ^ expected `[language]`
  |
"
    );
}

/// In a multi-file map, end of input in the most recently added file renders in
/// that file's frame, alongside a label in an earlier file.
#[test]
fn test_h04_end_of_input_in_the_last_of_several_files() {
    let mut map = SourceMap::new();
    map.add("a.rs", "fn a() {}\n").unwrap(); // global 0..10
    map.add("b.rs", "x = (1 +\n").unwrap(); // global 10..19

    let diag = Diagnostic::new(
        Severity::Error,
        "expected expression, found end of input",
        Label::new(Span::empty(19), "expected expression"),
    )
    .with_secondary(Label::new(Span::new(3, 4), "see also"));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: expected expression, found end of input
 --> b.rs:1:9
  |
1 | x = (1 +
  |         ^ expected expression
  |
 --> a.rs:1:4
  |
1 | fn a() {}
  |    - see also
  |
"
    );
}

/// Known 1.x limitation (source-lang `locate`, fixed by its 2.0 layout): the end
/// of a file that is not the last one is the next file's first byte, so an
/// end-of-input label there renders at the next file's 1:1. Pinned so the
/// behaviour is deliberate, not accidental.
#[test]
fn test_h04_known_limitation_inner_end_of_input_renders_in_the_next_file() {
    let mut map = SourceMap::new();
    map.add("a.rs", "x = 1\n").unwrap(); // global 0..6
    map.add("b.rs", "y = 2").unwrap(); // global 6..11

    let diag = Diagnostic::new(
        Severity::Error,
        "unterminated",
        Label::new(Span::empty(6), "meant for a.rs"),
    );

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: unterminated
 --> b.rs:1:1
  |
1 | y = 2
  | ^ meant for a.rs
  |
"
    );
}

/// ISSUES M15: control characters from the source, the messages, and the file
/// name are escaped visibly, so untrusted input cannot drive the terminal.
#[test]
fn test_m15_control_characters_are_escaped_everywhere() {
    let src = "let \u{1b}[2Jx = 1;\r";
    let mut map = SourceMap::new();
    map.add("evil\nname.rs", src).unwrap();

    let x = src.find('x').unwrap() as u32;
    let diag = Diagnostic::new(
        Severity::Error,
        "bad \u{1b}]0;pwned\u{7} title",
        Label::new(Span::new(x, x + 1), "label \u{9b}31m"),
    )
    .with_note("note \u{202e}desrever")
    .with_help("help\u{2028}sep");

    let out = Renderer::new().render(&diag, &map);
    assert_eq!(
        out,
        r"error: bad \u{1b}]0;pwned\u{7} title
 --> evil\u{a}name.rs:1:9
  |
1 | let \u{1b}[2Jx = 1;\u{d}
  |              ^ label \u{9b}31m
  |
 = note: note \u{202e}desrever
 = help: help\u{2028}sep
"
    );
    for bad in ['\u{1b}', '\u{7}', '\u{9b}', '\r', '\u{202e}', '\u{2028}'] {
        assert!(!out.contains(bad), "raw {bad:?} in output");
    }
}

/// ISSUES M18: carets align in display columns, so wide CJK characters take two
/// columns, combining marks none, and emoji two.
#[test]
fn test_m18_wide_and_combining_characters_align_the_caret() {
    let src = "let 名前 = 値;\ne\u{301}x\n😀 bad\n";
    let mut map = SourceMap::new();
    map.add("w.rs", src).unwrap();

    let diag = Diagnostic::new(
        Severity::Error,
        "width",
        Label::new(span_of(src, "値", 0), "wide"),
    )
    .with_secondary(Label::new(span_of(src, "x", 0), "after combining"))
    .with_secondary(Label::new(span_of(src, "bad", 0), "after emoji"));

    assert_eq!(
        Renderer::new().render(&diag, &map),
        "\
error: width
 --> w.rs:1:10
  |
1 | let 名前 = 値;
  |            ^^ wide
2 | e\u{301}x
  |  - after combining
3 | 😀 bad
  |    --- after emoji
  |
"
    );
}

/// Coordinator condition on the end-of-input caret: in every shape of input the
/// frame header's `line:col` equals the caret's line and column, so the header
/// never disagrees with the caret in the same frame.
#[test]
fn test_h04_end_of_input_header_equals_caret_position() {
    let cases: &[(&str, &str)] = &[
        ("abc", "1:4"),
        ("abc\n", "1:4"),
        ("abc\r\n", "1:4"),
        ("abc\n\n", "2:1"),
        ("one\ntwo", "2:4"),
        ("one\ntwo\n", "2:4"),
        ("\n", "1:1"),
        ("", "1:1"),
    ];
    for &(src, at) in cases {
        let mut map = SourceMap::new();
        map.add("f", src).unwrap();
        let diag = Diagnostic::new(
            Severity::Error,
            "found end of input",
            Label::unlabelled(Span::empty(src.len() as u32)),
        );
        let out = Renderer::new().render(&diag, &map);

        let (line, col) = at.split_once(':').unwrap();
        let (line, col): (u32, usize) = (line.parse().unwrap(), col.parse().unwrap());
        assert!(out.contains(&format!("--> f:{at}\n")), "{src:?}:\n{out}");
        // The source row shown above the caret is the header's line...
        let rows: Vec<&str> = out.lines().collect();
        let caret = rows.iter().position(|r| r.contains('^')).unwrap();
        assert!(
            rows[caret - 1].starts_with(&format!("{line} | ")),
            "{src:?}:\n{out}"
        );
        // ...and the caret is at the header's column.
        let after_bar = &rows[caret][rows[caret].find("| ").unwrap() + 2..];
        assert_eq!(after_bar.find('^').unwrap() + 1, col, "{src:?}:\n{out}");
    }
}
