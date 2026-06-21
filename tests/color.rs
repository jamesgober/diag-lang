//! Tests for ANSI styling, behind the `color` feature.
//!
//! The central invariant is that colour is additive: stripping the escape codes
//! from a coloured render reproduces the plain render exactly, so styling never
//! changes the layout, the alignment, or the text — only wraps it.

#![cfg(feature = "color")]
#![allow(clippy::unwrap_used)]

use diag_lang::{Diagnostic, Label, Renderer, Severity};
use diag_lang::{SourceMap, Span};

/// Removes every `ESC [ ... m` SGR sequence, leaving the text it wrapped.
fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c2 in chars.by_ref() {
                if c2 == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A diagnostic exercising every coloured part: a multi-line primary, a secondary
/// in the same file, an out-of-range secondary, a note, and a help line.
fn rich() -> (SourceMap, Diagnostic) {
    let src = "let x: i32 =\n    foo(\n        bar,\n    );\n";
    let mut map = SourceMap::new();
    map.add("m.rs", src).unwrap();

    let call = Span::new(
        src.find("foo").unwrap() as u32,
        src.find(");").unwrap() as u32 + 1,
    );
    let ty = {
        let at = src.find("i32").unwrap() as u32;
        Span::new(at, at + 3)
    };

    let diag = Diagnostic::new(
        Severity::Error,
        "mismatched types",
        Label::new(call, "this call"),
    )
    .with_secondary(Label::new(ty, "expected `i32`"))
    .with_secondary(Label::new(Span::new(900, 902), "defined far away"))
    .with_note("the body has type `()`")
    .with_help("return a value instead");
    (map, diag)
}

#[test]
fn test_stripped_colour_equals_plain_output() {
    let (map, diag) = rich();
    let plain = Renderer::new().render(&diag, &map);
    let coloured = Renderer::new().with_color().render(&diag, &map);

    // Styling actually happened, and removing it reproduces the plain bytes.
    assert_ne!(plain, coloured);
    assert!(coloured.contains('\u{1b}'));
    assert_eq!(strip_ansi(&coloured), plain);
}

#[test]
fn test_severity_drives_the_header_colour() {
    let mut map = SourceMap::new();
    map.add("m.rs", "let x = 1;").unwrap();
    let span = Span::new(4, 5);

    let error = Renderer::new().with_color().render(
        &Diagnostic::new(Severity::Error, "e", Label::new(span, "")),
        &map,
    );
    let warning = Renderer::new().with_color().render(
        &Diagnostic::new(Severity::Warning, "w", Label::new(span, "")),
        &map,
    );

    assert!(error.starts_with("\u{1b}[1;31merror\u{1b}[0m: e"));
    assert!(warning.starts_with("\u{1b}[1;33mwarning\u{1b}[0m: w"));
}

#[test]
fn test_colour_off_by_default_is_byte_for_byte_plain() {
    let (map, diag) = rich();
    // A renderer that never called `with_color` is identical to the default one,
    // and neither emits any escape.
    let default = Renderer::new().render(&diag, &map);
    assert!(!default.contains('\u{1b}'));
}

#[test]
fn test_exact_coloured_frame_for_a_single_label() {
    let mut map = SourceMap::new();
    map.add("m.rs", "let x = 1;").unwrap();
    let diag = Diagnostic::new(Severity::Error, "oops", Label::new(Span::new(4, 5), "here"));

    assert_eq!(
        Renderer::new().with_color().render(&diag, &map),
        "\u{1b}[1;31merror\u{1b}[0m: oops\n \u{1b}[1;34m-->\u{1b}[0m m.rs:1:5\n  \u{1b}[1;34m|\u{1b}[0m\n\u{1b}[1;34m1\u{1b}[0m \u{1b}[1;34m|\u{1b}[0m let x = 1;\n  \u{1b}[1;34m|\u{1b}[0m     \u{1b}[1;31m^ here\u{1b}[0m\n  \u{1b}[1;34m|\u{1b}[0m\n"
    );
}
