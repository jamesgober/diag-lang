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

/// Whether `c` is a character the renderer must never emit raw (ISSUES M15):
/// any C0 control but newline (tabs are expanded or kept only in messages), DEL,
/// C1, the line/paragraph separators, and the bidi controls.
fn is_forbidden_raw(c: char, allow_tab: bool) -> bool {
    match c {
        '\n' => false,
        '\t' => !allow_tab,
        '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => true,
        '\u{2028}' | '\u{2029}' | '\u{61c}' | '\u{200e}' | '\u{200f}' => true,
        '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => true,
        _ => false,
    }
}

/// Naive display column of byte offset `byte` in `line`: Unicode width per
/// character, tabs to the next stop of four. Independent of the renderer's code.
fn display_col_at(line: &str, byte: usize) -> usize {
    let mut col = 0;
    for (i, ch) in line.char_indices() {
        if i >= byte {
            break;
        }
        col = if ch == '\t' {
            col + 4 - (col % 4)
        } else {
            col + unicode_lang::char_width(ch)
        };
    }
    col
}

proptest! {
    /// ISSUES M15: whatever the source text, message, label, note, help, and file
    /// name contain, the plain output carries no raw control character, so no
    /// escape sequence from the input can reach a terminal.
    #[test]
    fn m15_plain_output_never_contains_raw_controls(
        text in "(?s).{0,60}",
        name in "(?s).{0,10}",
        message in "(?s).{0,20}",
        note in "(?s).{0,20}",
        a in any::<u32>(),
        b in any::<u32>(),
    ) {
        let mut map = SourceMap::new();
        map.add(name.as_str(), text.as_str()).unwrap();
        let margin = text.len() as u32 + 2;
        let span = Span::new(a % margin, b % margin);
        let diag = Diagnostic::new(Severity::Error, message.as_str(), Label::new(span, note.as_str()))
            .with_note(note.as_str())
            .with_help(message.as_str());

        let out = Renderer::new().render(&diag, &map);
        // Tabs survive only in messages, where they are harmless; the source text
        // is the only place a tab is expanded, so allow them.
        let bad: Vec<char> = out.chars().filter(|&c| is_forbidden_raw(c, true)).collect();
        prop_assert!(bad.is_empty(), "raw controls {:?} in {:?}", bad, out);
    }

    /// ISSUES H04: a zero-width label at the end of the last source (where
    /// source-lang 1.0.1 attributes end of input) renders inside that source's
    /// frame, never as the "outside the loaded sources" fallback, whatever the
    /// earlier sources are (empty ones included).
    #[test]
    fn h04_end_of_input_label_always_renders_a_frame(
        texts in prop::collection::vec("(?s)[^^]{0,30}", 1..4),
        pick in any::<usize>(),
    ) {
        let mut map = SourceMap::new();
        for (i, text) in texts.iter().enumerate() {
            map.add(alloc_name(i), text.as_str()).unwrap();
        }
        // An end-of-input position source-lang 1.0.1 attributes to its file: the
        // end of the last source. (`pick` is kept so old seeds still replay.)
        let _ = pick;
        let (_, last) = map.iter().last().unwrap();
        let at = last.span().end().to_u32();
        let last_name = alloc_name(texts.len() - 1);
        let diag = Diagnostic::new(
            Severity::Error,
            "found end of input",
            Label::new(Span::empty(at), "here"),
        );
        let out = Renderer::new().render(&diag, &map);
        prop_assert!(out.contains(&format!("--> {last_name}:")), "{}", out);
        prop_assert!(!out.contains("outside the loaded sources"), "{}", out);
        prop_assert_eq!(out.matches('^').count(), 1);
    }

    /// ISSUES M18: on one line of arbitrary printable text, the caret run starts at
    /// the naive display column of the span and is as wide as its display width
    /// (at least one).
    #[test]
    fn m18_caret_lead_and_width_follow_display_width(
        prefix in "[a-z\t世界😀é\u{301}]{0,8}",
        mid in "[a-z\t世界😀é\u{301}]{1,8}",
        suffix in "[a-z世]{0,8}",
    ) {
        let line = alloc_format(&prefix, &mid, &suffix);
        let mut map = SourceMap::new();
        map.add("f", line.as_str()).unwrap();

        let start = prefix.len();
        let end = prefix.len() + mid.len();
        let diag = Diagnostic::new(
            Severity::Error,
            "msg",
            Label::new(Span::new(start as u32, end as u32), "lbl"),
        );
        let out = Renderer::new().render(&diag, &map);

        let caret_row = out.lines().find(|l| l.contains('^')).unwrap();
        let after_bar = &caret_row[caret_row.find("| ").unwrap() + 2..];
        let lead = after_bar.chars().take_while(|&c| c == ' ').count();
        let width = after_bar.chars().filter(|&c| c == '^').count();
        let expect_lead = display_col_at(&line, start);
        let expect_width = display_col_at(&line, end).saturating_sub(expect_lead).max(1);
        prop_assert_eq!(lead, expect_lead);
        prop_assert_eq!(width, expect_width);
    }
}

/// Parses a single-frame render: the `--> name:line:col` header, the number of
/// the source row the caret sits under, and the caret's 0-based display column.
fn header_and_caret(out: &str) -> (u32, u32, u32, usize) {
    let lines: Vec<&str> = out.lines().collect();
    let header = lines
        .iter()
        .find(|l| l.trim_start().starts_with("-->"))
        .unwrap();
    let mut parts = header.rsplitn(3, ':');
    let col: u32 = parts.next().unwrap().parse().unwrap();
    let line: u32 = parts.next().unwrap().parse().unwrap();
    let caret_at = lines.iter().position(|l| l.contains('^')).unwrap();
    // The row above the caret row is the source row: `N | text`.
    let row_line: u32 = lines[caret_at - 1]
        .split(" | ")
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let caret_row = lines[caret_at];
    let after_bar = &caret_row[caret_row.find("| ").unwrap() + 2..];
    let lead = after_bar.chars().take_while(|&c| c == ' ').count();
    (line, col, row_line, lead)
}

proptest! {
    /// Coordinator condition on the end-of-input caret: the frame header's
    /// `line:col` is exactly where the caret is drawn, whether or not the text
    /// ends with a line break (`\n` or `\r\n`), for ASCII text where a column is
    /// one character. The caret sits on the header's line, at the header's column.
    #[test]
    fn h04_end_of_input_header_matches_the_caret(
        text in "(a|b| |\n|\r\n){0,30}",
    ) {
        let mut map = SourceMap::new();
        map.add("f", text.as_str()).unwrap();
        let diag = Diagnostic::new(
            Severity::Error,
            "found end of input",
            Label::unlabelled(Span::empty(text.len() as u32)),
        );
        let out = Renderer::new().render(&diag, &map);
        let (line, col, row_line, lead) = header_and_caret(&out);
        prop_assert_eq!(row_line, line, "{}", out);
        prop_assert_eq!(lead as u32 + 1, col, "{}", out);
    }
}
