//! The `Diagnostics` collector, checked against a naive model: a `Vec` with a
//! linear `contains` for duplicates, a length check for the cap, and a stable
//! sort for ordering. Also the code format (`Code`), whose parser must accept
//! exactly what `Display` writes.

#![allow(clippy::unwrap_used)]

use diag_lang::{
    Applicability, Code, Diagnostic, Diagnostics, Edit, Label, Severity, Span, Suggestion,
};
use proptest::prelude::*;

fn at(severity: Severity, pos: u32, message: &str) -> Diagnostic {
    Diagnostic::new(severity, message, Label::unlabelled(Span::empty(pos)))
}

/// The documented semantics, written the slow and obvious way.
#[derive(Default)]
struct Model {
    stored: Vec<Diagnostic>,
    omitted: usize,
    counts: [usize; 4],
}

impl Model {
    fn push(&mut self, d: Diagnostic, limit: Option<usize>) -> bool {
        if self.stored.contains(&d) {
            return false;
        }
        let slot = match d.severity() {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Note => 2,
            Severity::Help => 3,
        };
        self.counts[slot] += 1;
        if limit.is_some_and(|l| self.stored.len() >= l) {
            self.omitted += 1;
        } else {
            self.stored.push(d);
        }
        true
    }
}

fn severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Error),
        Just(Severity::Warning),
        Just(Severity::Note),
        Just(Severity::Help),
    ]
}

/// Diagnostics drawn from a small space, so duplicates are common and near
/// duplicates (one field different) are too.
fn small_diagnostic() -> impl Strategy<Value = Diagnostic> {
    (
        severity(),
        0u32..4,
        prop_oneof![Just("a"), Just("b")],
        proptest::option::of(0u16..2),
        proptest::option::of(0u32..2),
    )
        .prop_map(|(sev, pos, msg, code, fix)| {
            let mut d = at(sev, pos, msg);
            if let Some(n) = code {
                d = d.with_code(Code::new("E", n).unwrap());
            }
            if let Some(p) = fix {
                d = d.with_suggestion(
                    Suggestion::new("f", Applicability::MachineApplicable)
                        .with_edit(Edit::insert(p, ";")),
                );
            }
            d
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Push semantics, counts, cap, and duplicate suppression agree with the
    /// model on every sequence.
    #[test]
    fn collector_matches_the_model(
        items in proptest::collection::vec(small_diagnostic(), 0..40),
        limit in proptest::option::of(0usize..8),
    ) {
        let mut diags = limit.map_or_else(Diagnostics::new, Diagnostics::with_limit);
        let mut model = Model::default();
        for d in items {
            prop_assert_eq!(diags.push(d.clone()), model.push(d, limit));
        }
        prop_assert_eq!(diags.as_slice(), model.stored.as_slice());
        prop_assert_eq!(diags.omitted(), model.omitted);
        prop_assert_eq!(diags.error_count(), model.counts[0]);
        prop_assert_eq!(diags.warning_count(), model.counts[1]);
        prop_assert_eq!(diags.count(Severity::Note), model.counts[2]);
        prop_assert_eq!(diags.count(Severity::Help), model.counts[3]);
        prop_assert_eq!(diags.has_errors(), model.counts[0] > 0);
    }

    /// Sorting is a stable sort by (start, end, severity), and duplicate
    /// suppression still works afterwards (the index is rebuilt).
    #[test]
    fn sort_is_stable_by_position_and_keeps_dedup(
        items in proptest::collection::vec(small_diagnostic(), 0..30),
    ) {
        let mut diags: Diagnostics = items.iter().cloned().collect();
        let mut expected = diags.as_slice().to_vec();
        expected.sort_by_key(|d| (d.primary().span().start(), d.primary().span().end(), d.severity()));
        diags.sort();
        prop_assert_eq!(diags.as_slice(), expected.as_slice());
        for d in items {
            prop_assert!(!diags.push(d));
        }
        prop_assert_eq!(diags.len(), expected.len());
    }
}

#[test]
fn test_empty_collector() {
    let diags = Diagnostics::new();
    assert!(diags.is_empty() && !diags.has_errors());
    assert_eq!((diags.len(), diags.omitted(), diags.limit()), (0, 0, None));
    assert_eq!(diags.iter().count(), 0);
    assert!(diags.into_vec().is_empty());
}

#[test]
fn test_limit_zero_only_counts() {
    let mut diags = Diagnostics::with_limit(0);
    assert!(diags.push(at(Severity::Error, 0, "e")));
    assert!(diags.push(at(Severity::Error, 0, "e"))); // not stored, so not recognised
    assert_eq!(
        (diags.len(), diags.omitted(), diags.error_count()),
        (0, 2, 2)
    );
    assert!(diags.has_errors());
}

#[test]
fn test_duplicates_of_stored_diagnostics_are_ignored_past_the_cap() {
    let mut diags = Diagnostics::with_limit(1);
    assert!(diags.push(at(Severity::Error, 0, "e")));
    assert!(diags.push(at(Severity::Warning, 1, "w")));
    assert!(!diags.push(at(Severity::Error, 0, "e")));
    assert_eq!((diags.len(), diags.omitted()), (1, 1));
    assert_eq!((diags.error_count(), diags.warning_count()), (1, 1));
}

#[test]
fn test_near_duplicates_are_distinct() {
    let base = at(Severity::Error, 0, "e");
    let mut diags = Diagnostics::new();
    for d in [
        base.clone(),
        base.clone().with_note("n"),
        base.clone().with_help("n"),
        base.clone().with_code(Code::new("E", 1).unwrap()),
        base.clone()
            .with_secondary(Label::unlabelled(Span::empty(0))),
        base.clone()
            .with_suggestion(Suggestion::new("s", Applicability::Unspecified)),
        base.clone()
            .with_suggestion(Suggestion::new("s", Applicability::MaybeIncorrect)),
    ] {
        assert!(diags.push(d));
    }
    assert_eq!(diags.len(), 7);
}

#[test]
fn test_clear_resets_everything_but_the_limit() {
    let mut diags = Diagnostics::with_limit(1);
    diags.push(at(Severity::Error, 0, "a"));
    diags.push(at(Severity::Error, 1, "b"));
    diags.clear();
    assert!(diags.is_empty() && !diags.has_errors());
    assert_eq!((diags.omitted(), diags.limit()), (0, Some(1)));
    // The duplicate index was cleared too.
    assert!(diags.push(at(Severity::Error, 0, "a")));
}

#[test]
fn test_collection_traits() {
    let items = vec![
        at(Severity::Error, 0, "a"),
        at(Severity::Error, 0, "a"),
        at(Severity::Warning, 1, "b"),
    ];
    let diags: Diagnostics = items.into_iter().collect();
    assert_eq!(diags.len(), 2);
    let by_ref: Vec<&str> = (&diags).into_iter().map(Diagnostic::message).collect();
    assert_eq!(by_ref, ["a", "b"]);

    let mut more = diags.clone();
    more.extend([at(Severity::Note, 2, "c"), at(Severity::Error, 0, "a")]);
    assert_eq!(more.len(), 3);

    let owned: Vec<Diagnostic> = more.into_iter().collect();
    assert_eq!(owned.len(), 3);
}

#[test]
fn test_scale_hundred_thousand_with_shared_positions() {
    // 100k distinct diagnostics at only ten positions (the shape of a cascade),
    // then every one of them again: duplicate detection stays logarithmic.
    let mut diags = Diagnostics::new();
    for i in 0..100_000u32 {
        assert!(diags.push(at(
            Severity::Error,
            i % 10,
            &format!("unexpected token #{i}")
        )));
    }
    for i in 0..100_000u32 {
        assert!(!diags.push(at(
            Severity::Error,
            i % 10,
            &format!("unexpected token #{i}")
        )));
    }
    assert_eq!((diags.len(), diags.error_count()), (100_000, 100_000));
    diags.sort();
    let first = diags.as_slice().first().unwrap();
    assert_eq!(first.message(), "unexpected token #0");
}

// ---------------------------------------------------------------------------
// Code
// ---------------------------------------------------------------------------

/// The format, by hand: one to four ASCII uppercase letters, then exactly four
/// ASCII digits.
fn naive_is_code(s: &str) -> bool {
    let letters = s.bytes().take_while(u8::is_ascii_uppercase).count();
    let rest = &s.as_bytes()[letters..];
    (1..=4).contains(&letters) && rest.len() == 4 && rest.iter().all(u8::is_ascii_digit)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `Code::parse` never panics and accepts exactly the documented format.
    #[test]
    fn code_parse_accepts_exactly_the_format(s in prop_oneof![
        "[A-Za-z0-9 ]{0,10}",
        "[A-Z]{0,5}[0-9]{0,6}",
        "(?s).{0,10}",
    ]) {
        let parsed = Code::parse(&s);
        prop_assert_eq!(parsed.is_some(), naive_is_code(&s));
        if let Some(code) = parsed {
            prop_assert_eq!(code.to_string(), s);
        }
    }

    /// Every constructible code displays as its prefix and four digits, and
    /// parses back to itself; ordering is (prefix, number).
    #[test]
    fn code_round_trips(
        a in ("[A-Z]{1,4}", 0u16..=9999),
        b in ("[A-Z]{1,4}", 0u16..=9999),
    ) {
        let x = Code::new(&a.0, a.1).unwrap();
        let y = Code::new(&b.0, b.1).unwrap();
        prop_assert_eq!(x.to_string(), format!("{}{:04}", a.0, a.1));
        prop_assert_eq!(Code::parse(&x.to_string()), Some(x));
        prop_assert_eq!(x.cmp(&y), (a.0.as_str(), a.1).cmp(&(b.0.as_str(), b.1)));
    }
}

#[test]
fn test_debug_shows_contents_and_counts() {
    let mut diags = Diagnostics::with_limit(1);
    diags.push(at(Severity::Error, 0, "e"));
    diags.push(at(Severity::Warning, 1, "w"));
    let debug = format!("{diags:?}");
    assert!(debug.starts_with("Diagnostics { items: [Diagnostic { severity: Error"));
    assert!(
        debug.ends_with("errors: 1, warnings: 1, notes: 0, help: 0, omitted: 1, limit: Some(1) }")
    );
}
