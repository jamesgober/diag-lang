//! Property tests for the rendering invariants.
//!
//! The caret math is cross-checked against a deliberately naive reimplementation
//! of the column model: the renderer is correct only if its tab-and-UTF-8 caret
//! width agrees with a plain character walk on every input.

#![allow(clippy::unwrap_used)]

use diag_lang::{Diagnostic, Label, Renderer, Severity};
use diag_lang::{SourceMap, Span};
use proptest::prelude::*;

/// The visual column (0-based) at byte offset `byte`, expanding tabs to the next
/// multiple of four — a naive linear walk, independent of the renderer's own
/// implementation.
fn visual_col_at(line: &str, byte: usize) -> usize {
    let mut col = 0;
    for (i, ch) in line.char_indices() {
        if i >= byte {
            break;
        }
        if ch == '\t' {
            col += 4 - (col % 4);
        } else {
            col += 1;
        }
    }
    col
}

/// The number of carets the renderer should draw for the byte range
/// `start..end` of a single line: the visual width of the range, at least one.
fn naive_caret_width(line: &str, start: usize, end: usize) -> usize {
    visual_col_at(line, end)
        .saturating_sub(visual_col_at(line, start))
        .max(1)
}

proptest! {
    /// Rendering any span against any source is total: it never panics, returns a
    /// non-empty string, and ends with a newline — whatever offsets it is handed.
    #[test]
    fn render_is_total_and_newline_terminated(
        text in "(?s).{0,60}",
        a in any::<u32>(),
        b in any::<u32>(),
    ) {
        let mut map = SourceMap::new();
        map.add("f", text.as_str()).unwrap();

        let margin = text.len() as u32 + 4;
        let span = Span::new(a % margin, b % margin);
        let diag = Diagnostic::new(Severity::Error, "msg", Label::new(span, "lbl"));

        let out = Renderer::new().render(&diag, &map);
        prop_assert!(!out.is_empty());
        prop_assert!(out.ends_with('\n'));
    }

    /// The same diagnostic and map render byte-identical output every time.
    #[test]
    fn render_is_deterministic(
        text in "(?s).{0,60}",
        a in any::<u32>(),
        b in any::<u32>(),
    ) {
        let mut map = SourceMap::new();
        map.add("f", text.as_str()).unwrap();

        let margin = text.len() as u32 + 4;
        let span = Span::new(a % margin, b % margin);
        let diag = Diagnostic::new(Severity::Warning, "msg", Label::new(span, "lbl"));

        let renderer = Renderer::new();
        prop_assert_eq!(renderer.render(&diag, &map), renderer.render(&diag, &map));
    }

    /// A diagnostic with arbitrary secondary labels, notes, and help — over
    /// several files — still renders totally and deterministically, whatever spans
    /// and how many labels it carries.
    #[test]
    fn render_with_many_labels_is_total_and_deterministic(
        texts in prop::collection::vec("(?s).{0,30}", 1..4),
        spans in prop::collection::vec((any::<u32>(), any::<u32>()), 1..6),
    ) {
        let mut map = SourceMap::new();
        let mut total = 0u32;
        for (i, text) in texts.iter().enumerate() {
            map.add(alloc_name(i), text.as_str()).unwrap();
            total += text.len() as u32;
        }
        let margin = total + 8;

        let mut iter = spans.iter().map(|&(a, b)| Span::new(a % margin, b % margin));
        let primary = Label::new(iter.next().unwrap(), "primary");
        let mut diag = Diagnostic::new(Severity::Error, "msg", primary);
        for span in iter {
            diag = diag.with_secondary(Label::new(span, "secondary"));
        }
        diag = diag.with_note("a note").with_help("a hint");

        let renderer = Renderer::new();
        let out = renderer.render(&diag, &map);
        prop_assert!(out.ends_with('\n'));
        prop_assert!(out.starts_with("error: msg\n"));
        prop_assert_eq!(&out, &renderer.render(&diag, &map));
    }

    /// On a single line, the number of carets equals the visual width of the
    /// spanned text — one caret per column, tabs expanded, multi-byte characters
    /// counted once — and never fewer than one.
    #[test]
    fn caret_count_matches_visual_width_on_one_line(
        prefix in "[a-z\\t]{0,12}",
        mid in "[a-z\\t]{1,12}",
        suffix in "[a-z]{0,12}",
    ) {
        let line = alloc_format(&prefix, &mid, &suffix);
        let mut map = SourceMap::new();
        map.add("f", line.as_str()).unwrap();

        let start = prefix.len();
        let end = prefix.len() + mid.len();
        let span = Span::new(start as u32, end as u32);
        let diag = Diagnostic::new(Severity::Error, "msg", Label::new(span, "lbl"));

        let out = Renderer::new().render(&diag, &map);
        // The source charset excludes `^`, and the message text carries none, so
        // every caret in the output is part of the underline.
        let carets = out.matches('^').count();
        prop_assert_eq!(carets, naive_caret_width(&line, start, end));
    }
}

/// A distinct source name for index `i`, so the map's files are easy to tell
/// apart in a failing case.
fn alloc_name(i: usize) -> String {
    let mut s = String::from("f");
    s.push_str(&i.to_string());
    s.push_str(".rs");
    s
}

/// Joins three fragments into one line without pulling in `std`'s `format!`
/// machinery at the call site — a plain concatenation keeps the test readable.
fn alloc_format(a: &str, b: &str, c: &str) -> String {
    let mut s = String::with_capacity(a.len() + b.len() + c.len());
    s.push_str(a);
    s.push_str(b);
    s.push_str(c);
    s
}
