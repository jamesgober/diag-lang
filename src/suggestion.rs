//! Fix-it suggestions: concrete edits that would resolve a diagnostic.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use span_lang::Span;

/// How confident a [`Suggestion`] is that applying its edits is correct.
///
/// The levels follow rustc's, so tooling built for one reads the other: an
/// editor offers every suggestion as a code action, but only a
/// [`MachineApplicable`](Applicability::MachineApplicable) one is applied
/// without a human looking at it (`--fix`, "fix all", an AI client applying
/// fixes unattended). [`apply_fixes`](crate::apply_fixes) applies exactly those.
///
/// The enum is `#[non_exhaustive]`: a finer level can be added in a minor
/// release, so a `match` on it needs a wildcard arm. Treat an unknown level as
/// [`Unspecified`](Applicability::Unspecified).
///
/// # Examples
///
/// ```
/// use diag_lang::Applicability;
///
/// assert_eq!(Applicability::MachineApplicable.as_str(), "machine_applicable");
/// assert_eq!(Applicability::default(), Applicability::Unspecified);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Applicability {
    /// The edits are definitely what the user intended and produce valid code:
    /// safe to apply automatically.
    MachineApplicable,
    /// The edits are probably right but may not be what the user meant, or may
    /// not compile: show them, let a human decide.
    MaybeIncorrect,
    /// The edits contain placeholders (for example `/* value */`) that a human
    /// has to fill in; applying them as-is does not produce valid code.
    HasPlaceholders,
    /// The emitter has not said how confident it is. Treated like
    /// [`MaybeIncorrect`](Applicability::MaybeIncorrect) by every consumer in
    /// this crate.
    #[default]
    Unspecified,
}

impl Applicability {
    /// Returns the level's name as written in the JSON output:
    /// `"machine_applicable"`, `"maybe_incorrect"`, `"has_placeholders"`, or
    /// `"unspecified"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Applicability;
    ///
    /// assert_eq!(Applicability::HasPlaceholders.as_str(), "has_placeholders");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MachineApplicable => "machine_applicable",
            Self::MaybeIncorrect => "maybe_incorrect",
            Self::HasPlaceholders => "has_placeholders",
            Self::Unspecified => "unspecified",
        }
    }
}

impl fmt::Display for Applicability {
    /// Formats as the name from [`as_str`](Applicability::as_str).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One text replacement: the bytes a [`Span`] covers are replaced with new text.
///
/// The span is in the same global position space as a
/// [`Label`](crate::Label)'s, so an edit names its file the same way a label
/// does. An empty span inserts at that position; an empty replacement deletes
/// the span; both empty is a no-op.
///
/// # Examples
///
/// ```
/// use diag_lang::{Edit, Span};
///
/// let fix = Edit::new(Span::new(8, 11), "bar");
/// assert_eq!(fix.span(), Span::new(8, 11));
/// assert_eq!(fix.replacement(), "bar");
///
/// let semi = Edit::insert(14, ";");
/// assert!(semi.span().is_empty());
///
/// let gone = Edit::delete(Span::new(3, 5));
/// assert_eq!(gone.replacement(), "");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    span: Span,
    replacement: Box<str>,
}

impl Edit {
    /// Builds an edit replacing the text `span` covers with `replacement`.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Edit, Span};
    ///
    /// let edit = Edit::new(Span::new(0, 3), "let");
    /// assert_eq!(edit.replacement(), "let");
    /// ```
    #[inline]
    #[must_use]
    pub fn new(span: Span, replacement: impl Into<Box<str>>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
        }
    }

    /// Builds an edit inserting `text` at global position `at`, touching
    /// nothing around it. Equivalent to `Edit::new(Span::empty(at), text)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Edit, Span};
    ///
    /// assert_eq!(Edit::insert(7, ";"), Edit::new(Span::empty(7), ";"));
    /// ```
    #[inline]
    #[must_use]
    pub fn insert(at: u32, text: impl Into<Box<str>>) -> Self {
        Self::new(Span::empty(at), text)
    }

    /// Builds an edit deleting the text `span` covers. Equivalent to
    /// `Edit::new(span, "")`.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Edit, Span};
    ///
    /// assert_eq!(Edit::delete(Span::new(2, 4)), Edit::new(Span::new(2, 4), ""));
    /// ```
    #[inline]
    #[must_use]
    pub fn delete(span: Span) -> Self {
        Self::new(span, "")
    }

    /// Returns the span the edit replaces, in global position space.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Edit, Span};
    ///
    /// assert_eq!(Edit::insert(4, "x").span(), Span::empty(4));
    /// ```
    #[inline]
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Returns the text that replaces the span; empty for a deletion.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Edit, Span};
    ///
    /// assert_eq!(Edit::new(Span::new(0, 1), "y").replacement(), "y");
    /// ```
    #[inline]
    #[must_use]
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

/// A proposed fix for a diagnostic: a message, the edits that make the fix, and
/// how confident the emitter is that they are right.
///
/// Attach one to a [`Diagnostic`](crate::Diagnostic) with
/// [`with_suggestion`](crate::Diagnostic::with_suggestion). The
/// [`Renderer`](crate::Renderer) shows it as a `help:` block with the source as
/// it would read after the fix; the JSON output carries every edit; and
/// [`apply_fixes`](crate::apply_fixes) applies the
/// [machine-applicable](Applicability::MachineApplicable) ones to a source.
///
/// The edits of one suggestion are applied together, as a unit, and must not
/// overlap; they may be added in any order. A suggestion with no edits is
/// allowed and renders as its message alone.
///
/// # Examples
///
/// ```
/// use diag_lang::{Applicability, Edit, Span, Suggestion};
///
/// let fix = Suggestion::new("use the declared name", Applicability::MachineApplicable)
///     .with_edit(Edit::new(Span::new(20, 23), "count"));
///
/// assert_eq!(fix.message(), "use the declared name");
/// assert_eq!(fix.edits().len(), 1);
/// assert_eq!(fix.applicability(), Applicability::MachineApplicable);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    message: Box<str>,
    edits: Vec<Edit>,
    applicability: Applicability,
}

impl Suggestion {
    /// Builds a suggestion with `message` at `applicability` and no edits yet;
    /// add them with [`with_edit`](Suggestion::with_edit).
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Applicability, Suggestion};
    ///
    /// let s = Suggestion::new("add a semicolon", Applicability::MachineApplicable);
    /// assert!(s.edits().is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn new(message: impl Into<Box<str>>, applicability: Applicability) -> Self {
        Self {
            message: message.into(),
            edits: Vec::new(),
            applicability,
        }
    }

    /// Adds an edit, returning the suggestion for chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Applicability, Edit, Span, Suggestion};
    ///
    /// // Wrap `x` in parentheses: two insertions, applied together.
    /// let s = Suggestion::new("add parentheses", Applicability::MaybeIncorrect)
    ///     .with_edit(Edit::insert(4, "("))
    ///     .with_edit(Edit::insert(5, ")"));
    /// assert_eq!(s.edits().len(), 2);
    /// ```
    #[must_use]
    pub fn with_edit(mut self, edit: Edit) -> Self {
        self.edits.push(edit);
        self
    }

    /// Returns the message that says what the fix does.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Applicability, Suggestion};
    ///
    /// let s = Suggestion::new("remove this", Applicability::Unspecified);
    /// assert_eq!(s.message(), "remove this");
    /// ```
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the edits, in the order they were added.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Applicability, Edit, Suggestion};
    ///
    /// let s = Suggestion::new("m", Applicability::Unspecified).with_edit(Edit::insert(0, "a"));
    /// assert_eq!(s.edits()[0].replacement(), "a");
    /// ```
    #[inline]
    #[must_use]
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// Returns how confident the emitter is in the edits.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Applicability, Suggestion};
    ///
    /// let s = Suggestion::new("m", Applicability::HasPlaceholders);
    /// assert_eq!(s.applicability(), Applicability::HasPlaceholders);
    /// ```
    #[inline]
    #[must_use]
    pub const fn applicability(&self) -> Applicability {
        self.applicability
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_constructors_agree() {
        assert_eq!(Edit::insert(3, "x"), Edit::new(Span::new(3, 3), "x"));
        assert_eq!(
            Edit::delete(Span::new(1, 2)),
            Edit::new(Span::new(1, 2), "")
        );
    }

    #[test]
    fn test_suggestion_keeps_edits_in_order() {
        let s = Suggestion::new("m", Applicability::MachineApplicable)
            .with_edit(Edit::insert(9, "b"))
            .with_edit(Edit::insert(1, "a"));
        let order: Vec<_> = s.edits().iter().map(Edit::replacement).collect();
        assert_eq!(order, ["b", "a"]);
    }

    #[test]
    fn test_applicability_names_are_distinct() {
        let all = [
            Applicability::MachineApplicable,
            Applicability::MaybeIncorrect,
            Applicability::HasPlaceholders,
            Applicability::Unspecified,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.as_str(), b.as_str());
            }
        }
    }
}
