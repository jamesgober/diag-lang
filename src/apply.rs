//! Applying suggestion edits to source text: validated, ordered, and
//! conflict-checked, so a fix is applied exactly or not at all.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use span_lang::Span;

use crate::{Applicability, Diagnostic, Edit};

/// Why a set of edits could not be applied. Nothing is applied when any edit is
/// refused: the text is never left half-edited.
///
/// # Examples
///
/// ```
/// use diag_lang::{ApplyError, Edit, Span, apply_edits};
///
/// let text = "let x = 1;";
/// let err = apply_edits(text, 0, &[
///     Edit::new(Span::new(4, 5), "y"),
///     Edit::new(Span::new(4, 9), "z = 2"),
/// ])
/// .unwrap_err();
/// assert_eq!(err, ApplyError::Overlap { first: Span::new(4, 5), second: Span::new(4, 9) });
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// An edit's span is not inside the text: it starts before the text's first
    /// position or ends past its last. The edit was made for a different
    /// source, a stale version of this one, or with the wrong `base`; re-run
    /// the check that produced it against the current text.
    OutOfBounds {
        /// The offending edit's span, in global position space.
        span: Span,
    },
    /// An edit's span starts or ends inside a multi-byte UTF-8 character, so
    /// applying it would produce invalid text. The emitter computed the span
    /// in the wrong unit (characters, UTF-16) or against different text.
    NotCharBoundary {
        /// The offending edit's span, in global position space.
        span: Span,
    },
    /// Two edits overlap, or two different insertions target the same
    /// position, so no order of applying them is right. Edits that are exactly
    /// identical (same span, same text) are not a conflict: they are applied
    /// once. Apply the suggestions one at a time and re-check between them.
    Overlap {
        /// The earlier of the two edits, by position.
        first: Span,
        /// The later edit, which starts before `first` ends.
        second: Span,
    },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds { span } => {
                write!(f, "edit at {span} is outside the text being edited")
            }
            Self::NotCharBoundary { span } => {
                write!(f, "edit at {span} splits a UTF-8 character")
            }
            Self::Overlap { first, second } => {
                write!(f, "edits at {first} and {second} overlap")
            }
        }
    }
}

impl core::error::Error for ApplyError {}

/// One validated edit, in local byte offsets of the text it applies to.
#[derive(Clone, Copy)]
pub(crate) struct Planned<'a> {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) text: &'a str,
}

/// Validates `edits` against `text` (whose first byte is global position
/// `base`) and returns them in application order: sorted by position,
/// identical duplicates and no-op edits removed.
///
/// The cost is `O(n log n)` in the number of edits; nothing depends on the
/// length of the text beyond two boundary checks per edit.
pub(crate) fn plan<'a>(
    text: &str,
    base: u32,
    edits: impl IntoIterator<Item = &'a Edit>,
) -> Result<Vec<Planned<'a>>, ApplyError> {
    let mut planned: Vec<(Planned<'a>, Span)> = Vec::new();
    for edit in edits {
        let span = edit.span();
        let local = |pos: u32| {
            pos.checked_sub(base)
                .map(|at| at as usize)
                .filter(|&at| at <= text.len())
        };
        let (Some(start), Some(end)) = (local(span.start().to_u32()), local(span.end().to_u32()))
        else {
            return Err(ApplyError::OutOfBounds { span });
        };
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Err(ApplyError::NotCharBoundary { span });
        }
        // A no-op changes nothing and conflicts with nothing.
        if start == end && edit.replacement().is_empty() {
            continue;
        }
        planned.push((
            Planned {
                start,
                end,
                text: edit.replacement(),
            },
            span,
        ));
    }

    // Stable: equal positions keep their given order, so the outcome (or the
    // reported conflict) is deterministic. An insertion sorts before a
    // replacement starting at the same position and is applied first.
    planned.sort_by_key(|(p, _)| (p.start, p.end));

    let mut out: Vec<Planned<'a>> = Vec::with_capacity(planned.len());
    let mut last: Option<(Planned<'a>, Span)> = None;
    for (edit, span) in planned {
        if let Some((prev, prev_span)) = last {
            let same_place = edit.start == prev.start && edit.end == prev.end;
            if same_place && edit.text == prev.text {
                continue; // exactly the same edit, suggested twice
            }
            // Sorted by start, and the kept edits never overlap, so the previous
            // kept edit has the furthest end: one comparison finds any overlap.
            // Two insertions at one position overlap too: neither order is right.
            if edit.start < prev.end || same_place {
                return Err(ApplyError::Overlap {
                    first: prev_span,
                    second: span,
                });
            }
        }
        out.push(edit);
        last = Some((edit, span));
    }
    Ok(out)
}

/// Splices validated, sorted, non-overlapping edits into `text`.
pub(crate) fn splice(text: &str, edits: &[Planned<'_>]) -> String {
    let removed: usize = edits.iter().map(|e| e.end - e.start).sum();
    let inserted: usize = edits.iter().map(|e| e.text.len()).sum();
    let mut out =
        String::with_capacity(text.len().saturating_sub(removed).saturating_add(inserted));
    let mut at = 0;
    for edit in edits {
        out.push_str(text.get(at..edit.start).unwrap_or(""));
        out.push_str(edit.text);
        at = edit.end;
    }
    out.push_str(text.get(at..).unwrap_or(""));
    out
}

/// Applies `edits` to `text` and returns the edited text, or an error if any
/// edit is out of bounds, splits a character, or conflicts with another.
///
/// `base` is the global position of `text`'s first byte: `0` when the spans are
/// local offsets into `text` (a single-source map, or an editor document), or
/// `file.span().start().to_u32()` for a file in a multi-source
/// [`SourceMap`](crate::SourceMap). An edit may touch any position from `base`
/// to `base + text.len()` inclusive, so inserting at the very end is allowed.
///
/// The edits are applied as one atomic change against the original text, in
/// position order whatever order they are given in. Two edits that are exactly
/// identical are applied once; an edit that inserts nothing and deletes nothing
/// is ignored. An insertion at the position where a replacement starts goes
/// before the replacement's text. Everything else that touches the same bytes
/// is a conflict.
///
/// Runs in `O(n log n + len)` for `n` edits and an output of `len` bytes, and
/// never panics.
///
/// # Errors
///
/// - [`ApplyError::OutOfBounds`] if an edit's span is not inside
///   `base..=base + text.len()`.
/// - [`ApplyError::NotCharBoundary`] if a span starts or ends inside a
///   multi-byte character.
/// - [`ApplyError::Overlap`] if two edits overlap or two different insertions
///   share a position.
///
/// # Examples
///
/// ```
/// use diag_lang::{Edit, Span, apply_edits};
///
/// let text = "let x = foo()";
/// let fixed = apply_edits(text, 0, &[
///     Edit::insert(13, ";"),             // given out of order:
///     Edit::new(Span::new(8, 11), "bar"), // applied by position
/// ])?;
/// assert_eq!(fixed, "let x = bar();");
/// # Ok::<(), diag_lang::ApplyError>(())
/// ```
pub fn apply_edits<'a>(
    text: &str,
    base: u32,
    edits: impl IntoIterator<Item = &'a Edit>,
) -> Result<String, ApplyError> {
    let planned = plan(text, base, edits)?;
    Ok(splice(text, &planned))
}

/// Applies every [machine-applicable](Applicability::MachineApplicable)
/// suggestion of `diagnostics` that targets `text`, returning the fixed text.
///
/// This is the one-call "fix all" for a source: lint `--fix`, an editor's "fix
/// all in file", or an AI client applying the fixes a check reported. `base`
/// is the global position of `text`'s first byte, as for [`apply_edits`].
///
/// A suggestion targets `text` when every one of its edits starts inside
/// `base..=base + text.len()`. Suggestions that are not machine-applicable, have
/// no edits, or edit only other sources are skipped. A suggestion with edits
/// both inside and outside this text is skipped too: its edits are a unit, and
/// applying half of them would be wrong. All selected edits are then applied
/// together, exactly as by [`apply_edits`]: if two suggestions conflict,
/// nothing is applied and the conflict is reported.
///
/// **Source boundaries.** In a multi-source map, source-lang 1.x lays sources
/// out back to back, so the end of one source and the start of the next are the
/// same position. An edit starting there is selected for *both* texts (the end
/// of the first, so end-of-file insertions work; the start of the second). Only
/// edits at exactly that position are affected, and a single-source map has
/// none. Until source-lang 2.0 separates the positions, apply a suggestion at a
/// boundary with [`apply_edits`] to the source you mean.
///
/// # Errors
///
/// The same as [`apply_edits`], for the selected edits.
///
/// # Examples
///
/// ```
/// use diag_lang::{Applicability, Diagnostic, Edit, Label, Severity, Span, Suggestion, apply_fixes};
///
/// let text = "let x = 1\nlet y = 2\n";
/// let missing_semi = |at: u32| {
///     Diagnostic::new(Severity::Error, "expected `;`", Label::unlabelled(Span::empty(at)))
///         .with_suggestion(
///             Suggestion::new("add `;`", Applicability::MachineApplicable)
///                 .with_edit(Edit::insert(at, ";")),
///         )
/// };
///
/// let fixed = apply_fixes(text, 0, &[missing_semi(9), missing_semi(19)])?;
/// assert_eq!(fixed, "let x = 1;\nlet y = 2;\n");
/// # Ok::<(), diag_lang::ApplyError>(())
/// ```
pub fn apply_fixes<'a>(
    text: &str,
    base: u32,
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
) -> Result<String, ApplyError> {
    let lo = u64::from(base);
    let hi = lo + text.len() as u64;
    let inside = |edit: &Edit| (lo..=hi).contains(&u64::from(edit.span().start().to_u32()));

    let edits = diagnostics
        .into_iter()
        .flat_map(Diagnostic::suggestions)
        .filter(|s| s.applicability() == Applicability::MachineApplicable)
        .filter(|s| !s.edits().is_empty() && s.edits().iter().all(inside))
        .flat_map(|s| s.edits());
    apply_edits(text, base, edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edits: &[Edit]) -> Result<String, ApplyError> {
        apply_edits(text, 0, edits)
    }

    #[test]
    fn test_empty_edit_list_returns_the_text() {
        assert_eq!(apply("abc", &[]).unwrap(), "abc");
        assert_eq!(apply("", &[]).unwrap(), "");
    }

    #[test]
    fn test_insert_replace_delete() {
        assert_eq!(apply("ac", &[Edit::insert(1, "b")]).unwrap(), "abc");
        assert_eq!(
            apply("axc", &[Edit::new(Span::new(1, 2), "b")]).unwrap(),
            "abc"
        );
        assert_eq!(
            apply("abxc", &[Edit::delete(Span::new(2, 3))]).unwrap(),
            "abc"
        );
        // At both ends.
        assert_eq!(
            apply("b", &[Edit::insert(1, "c"), Edit::insert(0, "a")]).unwrap(),
            "abc"
        );
    }

    #[test]
    fn test_base_shifts_the_coordinates() {
        assert_eq!(
            apply_edits("ac", 100, &[Edit::insert(101, "b")]).unwrap(),
            "abc"
        );
        assert_eq!(
            apply_edits("ac", 100, &[Edit::insert(99, "b")]),
            Err(ApplyError::OutOfBounds {
                span: Span::empty(99)
            })
        );
        assert_eq!(
            apply_edits("ac", 100, &[Edit::insert(103, "b")]),
            Err(ApplyError::OutOfBounds {
                span: Span::empty(103)
            })
        );
    }

    #[test]
    fn test_char_boundaries_are_enforced() {
        // `é` is two bytes: 1..3.
        let text = "aéb";
        assert_eq!(
            apply(text, &[Edit::insert(2, "x")]),
            Err(ApplyError::NotCharBoundary {
                span: Span::empty(2)
            })
        );
        assert_eq!(
            apply(text, &[Edit::new(Span::new(1, 3), "e")]).unwrap(),
            "aeb"
        );
    }

    #[test]
    fn test_conflicts_are_reported_and_duplicates_merged() {
        let text = "abcdef";
        assert_eq!(
            apply(
                text,
                &[
                    Edit::new(Span::new(1, 4), "X"),
                    Edit::new(Span::new(3, 5), "Y")
                ]
            ),
            Err(ApplyError::Overlap {
                first: Span::new(1, 4),
                second: Span::new(3, 5)
            })
        );
        // Two different insertions at one place.
        assert_eq!(
            apply(text, &[Edit::insert(2, "x"), Edit::insert(2, "y")]),
            Err(ApplyError::Overlap {
                first: Span::empty(2),
                second: Span::empty(2)
            })
        );
        // An insertion strictly inside a replacement.
        assert!(
            apply(
                text,
                &[Edit::new(Span::new(1, 4), "X"), Edit::insert(2, "y")]
            )
            .is_err()
        );
        // The identical edit twice is applied once.
        assert_eq!(
            apply(text, &[Edit::insert(2, "x"), Edit::insert(2, "x")]).unwrap(),
            "abxcdef"
        );
        // Touching edits do not overlap; an insertion at a replacement's start
        // goes first.
        assert_eq!(
            apply(
                text,
                &[
                    Edit::new(Span::new(2, 4), "R"),
                    Edit::insert(2, "i"),
                    Edit::new(Span::new(0, 2), "L")
                ]
            )
            .unwrap(),
            "LiRef"
        );
        // A no-op never conflicts.
        assert_eq!(
            apply(text, &[Edit::insert(2, "x"), Edit::insert(2, "")]).unwrap(),
            "abxcdef"
        );
    }

    #[test]
    fn test_error_messages_name_the_spans() {
        let err = ApplyError::Overlap {
            first: Span::new(1, 4),
            second: Span::new(3, 5),
        };
        assert_eq!(alloc::format!("{err}"), "edits at 1..4 and 3..5 overlap");
    }
}
