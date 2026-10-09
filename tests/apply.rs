//! Applying edits: `apply_edits` and `apply_fixes`.
//!
//! The properties check the helper against simple reference implementations:
//! it never panics on any input; it succeeds exactly when a naive pairwise
//! check finds the edits valid; and when it succeeds, its output equals the
//! edits applied one at a time from the back with `String::replace_range`.

#![allow(clippy::unwrap_used)]

use diag_lang::{
    Applicability, ApplyError, Diagnostic, Edit, Label, Severity, Span, Suggestion, apply_edits,
    apply_fixes,
};
use proptest::prelude::*;

/// The naive validity check: every edit inside the text and on character
/// boundaries, and no two (after dropping no-ops and exact duplicates)
/// conflicting, compared pairwise.
fn naive_valid(text: &str, base: u32, edits: &[Edit]) -> bool {
    let local = |pos: u32| pos.checked_sub(base).map(|p| p as usize);
    let mut ranges = Vec::new();
    for edit in edits {
        let (Some(a), Some(b)) = (
            local(edit.span().start().to_u32()),
            local(edit.span().end().to_u32()),
        ) else {
            return false;
        };
        if b > text.len() || !text.is_char_boundary(a) || !text.is_char_boundary(b) {
            return false;
        }
        if a == b && edit.replacement().is_empty() {
            continue;
        }
        ranges.push((a, b, edit.replacement()));
    }
    for (i, x) in ranges.iter().enumerate() {
        for y in &ranges[i + 1..] {
            if x == y {
                continue;
            }
            let conflict = match (x.0 == x.1, y.0 == y.1) {
                (true, true) => x.0 == y.0,
                (true, false) => y.0 < x.0 && x.0 < y.1,
                (false, true) => x.0 < y.0 && y.0 < x.1,
                (false, false) => x.0 < y.1 && y.0 < x.1,
            };
            if conflict {
                return false;
            }
        }
    }
    true
}

/// The reference application: drop no-ops and duplicates, sort by position
/// with insertions first, and splice from the back so earlier offsets stay put.
fn naive_apply(text: &str, base: u32, edits: &[Edit]) -> String {
    let mut ranges: Vec<(usize, usize, &str)> = Vec::new();
    for edit in edits {
        let a = (edit.span().start().to_u32() - base) as usize;
        let b = (edit.span().end().to_u32() - base) as usize;
        if (a == b && edit.replacement().is_empty()) || ranges.contains(&(a, b, edit.replacement()))
        {
            continue;
        }
        ranges.push((a, b, edit.replacement()));
    }
    ranges.sort_by_key(|&(a, b, _)| (a, b));
    let mut out = text.to_string();
    for &(a, b, r) in ranges.iter().rev() {
        out.replace_range(a..b, r);
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Any text, any base, any edits: no panic, and success exactly when the
    /// naive pairwise check says the edits are valid, with the reference
    /// output.
    #[test]
    fn apply_never_panics_and_agrees_with_the_reference(
        text in "(?s).{0,24}",
        base in prop_oneof![Just(0u32), 0u32..64, Just(u32::MAX - 8)],
        raw in proptest::collection::vec((0u32..40, 0u32..40, "[a-zé;]{0,3}"), 0..6),
    ) {
        let edits: Vec<Edit> = raw
            .into_iter()
            .map(|(a, b, r)| Edit::new(Span::new(base.saturating_add(a), base.saturating_add(b)), r))
            .collect();
        let result = apply_edits(&text, base, &edits);
        prop_assert_eq!(result.is_ok(), naive_valid(&text, base, &edits), "{:?}", result);
        if let Ok(out) = result {
            prop_assert_eq!(out, naive_apply(&text, base, &edits));
        }
    }

    /// Non-overlapping edits at character boundaries, in any order, always
    /// apply, and exactly: the output is the text with each range replaced.
    #[test]
    fn non_overlapping_edits_apply_exactly(
        text in "(?s).{0,40}",
        cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..8),
        replacements in proptest::collection::vec("(?s).{0,4}", 8),
        order in any::<prop::sample::Index>(),
    ) {
        // Sorted, distinct char-boundary cut points pair up into ranges.
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        let mut points: Vec<usize> = cuts.iter().map(|ix| bounds[ix.index(bounds.len())]).collect();
        points.sort_unstable();
        points.dedup();
        let ranges: Vec<(usize, usize)> = points.chunks(2).map(|c| (c[0], *c.last().unwrap())).collect();

        // The expected output, built forward and independently.
        let mut expected = String::new();
        let mut at = 0;
        let mut edits = Vec::new();
        for (k, &(a, b)) in ranges.iter().enumerate() {
            let r = &replacements[k];
            expected.push_str(&text[at..a]);
            expected.push_str(r);
            at = b;
            edits.push(Edit::new(Span::new(a as u32, b as u32), r.as_str()));
        }
        expected.push_str(&text[at..]);

        // Any rotation of the edit list gives the same answer.
        if !edits.is_empty() {
            let k = order.index(edits.len());
            edits.rotate_left(k);
        }
        prop_assert_eq!(apply_edits(&text, 0, &edits).unwrap(), expected);
    }

    /// `apply_fixes` applies exactly the machine-applicable suggestions whose
    /// edits all fall in the text, as `apply_edits` would.
    #[test]
    fn apply_fixes_is_apply_edits_over_the_selected_suggestions(
        text in "[a-z\n]{0,20}",
        raw in proptest::collection::vec(
            (0u8..4, proptest::collection::vec((0u32..30, 0u32..30, "[xy]{0,2}"), 0..3)),
            0..5,
        ),
    ) {
        let len = text.len() as u32;
        let applicabilities = [
            Applicability::MachineApplicable,
            Applicability::MaybeIncorrect,
            Applicability::HasPlaceholders,
            Applicability::Unspecified,
        ];
        let mut diags = Vec::new();
        let mut selected = Vec::new();
        for (kind, edits) in raw {
            let applicability = applicabilities[kind as usize];
            let mut s = Suggestion::new("s", applicability);
            let edits: Vec<Edit> = edits.into_iter().map(|(a, b, r)| Edit::new(Span::new(a, b), r)).collect();
            for e in &edits {
                s = s.with_edit(e.clone());
            }
            let inside = !edits.is_empty() && edits.iter().all(|e| e.span().start().to_u32() <= len);
            if applicability == Applicability::MachineApplicable && inside {
                selected.extend(edits);
            }
            diags.push(Diagnostic::new(Severity::Error, "d", Label::unlabelled(Span::empty(0))).with_suggestion(s));
        }
        prop_assert_eq!(apply_fixes(&text, 0, &diags), apply_edits(&text, 0, &selected));
    }
}

fn diag_with(suggestions: Vec<Suggestion>) -> Diagnostic {
    suggestions.into_iter().fold(
        Diagnostic::new(Severity::Error, "d", Label::unlabelled(Span::empty(0))),
        Diagnostic::with_suggestion,
    )
}

#[test]
fn test_apply_fixes_skips_what_is_not_machine_applicable() {
    let text = "ab";
    let diags = [diag_with(vec![
        Suggestion::new("maybe", Applicability::MaybeIncorrect).with_edit(Edit::insert(0, "1")),
        Suggestion::new("holes", Applicability::HasPlaceholders).with_edit(Edit::insert(0, "2")),
        Suggestion::new("unsure", Applicability::Unspecified).with_edit(Edit::insert(0, "3")),
        Suggestion::new("sure", Applicability::MachineApplicable).with_edit(Edit::insert(2, "!")),
    ])];
    assert_eq!(apply_fixes(text, 0, &diags).unwrap(), "ab!");
}

#[test]
fn test_apply_fixes_per_source_of_a_multi_source_map() {
    // a.ls at 0..4, b.ls at 4..8.
    let (a, b) = ("aaa\n", "bbb\n");
    let only_a =
        Suggestion::new("a", Applicability::MachineApplicable).with_edit(Edit::insert(0, "A"));
    let only_b =
        Suggestion::new("b", Applicability::MachineApplicable).with_edit(Edit::insert(5, "B"));
    let both = Suggestion::new("ab", Applicability::MachineApplicable)
        .with_edit(Edit::insert(1, "x"))
        .with_edit(Edit::insert(6, "y"));
    let diags = [diag_with(vec![only_a, only_b, both])];

    // The straddling suggestion is applied to neither source.
    assert_eq!(apply_fixes(a, 0, &diags).unwrap(), "Aaaa\n");
    assert_eq!(apply_fixes(b, 4, &diags).unwrap(), "bBbb\n");
}

#[test]
fn test_apply_fixes_reports_conflicts_between_diagnostics() {
    let text = "let x = 1";
    let semi = |s: &str| {
        diag_with(vec![
            Suggestion::new("end", Applicability::MachineApplicable).with_edit(Edit::insert(9, s)),
        ])
    };
    // The same fix from two diagnostics is applied once.
    assert_eq!(
        apply_fixes(text, 0, &[semi(";"), semi(";")]).unwrap(),
        "let x = 1;"
    );
    // Different fixes at one place conflict; nothing is applied.
    assert_eq!(
        apply_fixes(text, 0, &[semi(";"), semi(",")]),
        Err(ApplyError::Overlap {
            first: Span::empty(9),
            second: Span::empty(9)
        })
    );
}

#[test]
fn test_apply_fixes_reports_a_malformed_selected_edit() {
    // Starts inside the text (so it is selected) but ends past it.
    let diags = [diag_with(vec![
        Suggestion::new("bad", Applicability::MachineApplicable)
            .with_edit(Edit::new(Span::new(1, 9), "x")),
    ])];
    assert_eq!(
        apply_fixes("abc", 0, &diags),
        Err(ApplyError::OutOfBounds {
            span: Span::new(1, 9)
        })
    );
}

#[test]
fn test_apply_fixes_with_nothing_to_do_returns_the_text() {
    assert_eq!(apply_fixes("abc", 0, &[]).unwrap(), "abc");
    let none = [diag_with(vec![Suggestion::new(
        "empty",
        Applicability::MachineApplicable,
    )])];
    assert_eq!(apply_fixes("abc", 0, &none).unwrap(), "abc");
}

#[test]
fn test_errors_display_and_are_std_errors() {
    let errors: [Box<dyn std::error::Error>; 3] = [
        Box::new(ApplyError::OutOfBounds {
            span: Span::new(1, 2),
        }),
        Box::new(ApplyError::NotCharBoundary {
            span: Span::new(1, 2),
        }),
        Box::new(ApplyError::Overlap {
            first: Span::new(0, 2),
            second: Span::new(1, 3),
        }),
    ];
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        [
            "edit at 1..2 is outside the text being edited",
            "edit at 1..2 splits a UTF-8 character",
            "edits at 0..2 and 1..3 overlap",
        ]
    );
}

#[test]
fn test_large_edit_sets_apply_in_one_pass() {
    // 100k insertions into a 100k-byte text: n log n, not n².
    let text = "a".repeat(100_000);
    let edits: Vec<Edit> = (0..100_000u32)
        .rev()
        .map(|i| Edit::insert(i, "b"))
        .collect();
    let out = apply_edits(&text, 0, &edits).unwrap();
    assert_eq!(out.len(), 200_000);
    assert!(out.starts_with("baba"));
}

#[test]
fn test_apply_fixes_boundary_edit_is_selected_for_both_sources() {
    // Known 1.x limitation, pinned: with a.ls at 0..4 and b.ls at 4..8, position
    // 4 is both the end of a.ls and the start of b.ls (source-lang 1.x lays
    // sources out back to back), so an insertion there is selected for both.
    let diags = [diag_with(vec![
        Suggestion::new("boundary", Applicability::MachineApplicable)
            .with_edit(Edit::insert(4, "!")),
    ])];
    assert_eq!(apply_fixes("aaa\n", 0, &diags).unwrap(), "aaa\n!");
    assert_eq!(apply_fixes("bbb\n", 4, &diags).unwrap(), "!bbb\n");
}
