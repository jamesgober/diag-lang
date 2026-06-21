//! The diagnostic: a severity, a message, the span it points at, and the related
//! labels and notes that round it out.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::{Label, Severity};

/// One thing a compiler stage has to say about the source: a [`Severity`], a
/// headline message, a primary [`Label`] pointing at the span it concerns, and
/// optionally secondary labels for related locations and trailing note/help lines.
///
/// A `Diagnostic` is the unit a lexer, parser, or type checker emits and a
/// [`Renderer`](crate::Renderer) turns into caret-annotated output. Constructing
/// one is cheap — it allocates only what the message, labels, and notes require —
/// because it is on the front-end's hot path. The primary label is supplied at
/// construction, so a diagnostic always knows where it points; secondary labels
/// and notes are added with the chainable `with_*` builders.
///
/// # Examples
///
/// ```
/// use diag_lang::{Diagnostic, Label, Severity};
/// use span_lang::Span;
///
/// let diag = Diagnostic::new(
///     Severity::Error,
///     "mismatched types",
///     Label::new(Span::new(20, 27), "expected `i32`, found `&str`"),
/// )
/// .with_secondary(Label::new(Span::new(8, 11), "expected due to this"))
/// .with_note("string literals have type `&str`")
/// .with_help("convert with `.parse()`");
///
/// assert_eq!(diag.secondary().len(), 1);
/// assert_eq!(diag.notes().count(), 1);
/// assert_eq!(diag.help().count(), 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    severity: Severity,
    message: Box<str>,
    primary: Label,
    secondary: Vec<Label>,
    notes: Vec<Box<str>>,
    help: Vec<Box<str>>,
}

impl Diagnostic {
    /// Builds a diagnostic at `severity`, headed by `message`, pointing at
    /// `primary`.
    ///
    /// The message is taken by value (a `&str` or an owned `String`), so the
    /// diagnostic owns its text. The primary label is required: a diagnostic
    /// always has a span to render a caret under, which keeps the type total and
    /// the renderer free of a "no location" branch. Secondary labels and notes
    /// start empty; add them with [`with_secondary`](Diagnostic::with_secondary),
    /// [`with_note`](Diagnostic::with_note), and [`with_help`](Diagnostic::with_help).
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(
    ///     Severity::Warning,
    ///     "unused variable `x`",
    ///     Label::new(Span::new(4, 5), "first defined here"),
    /// );
    /// assert_eq!(diag.severity(), Severity::Warning);
    /// assert!(diag.secondary().is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn new(severity: Severity, message: impl Into<Box<str>>, primary: Label) -> Self {
        Self {
            severity,
            message: message.into(),
            primary,
            secondary: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    /// Adds a secondary label, returning the diagnostic for chaining.
    ///
    /// Secondary labels mark locations related to the primary one — "expected
    /// because of this", "first defined here". A renderer draws them with a `-`
    /// underline to distinguish them from the primary `^`. Any number may be
    /// added; they render in a stable order driven by where they point, not by the
    /// order they were added.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(
    ///     Severity::Error,
    ///     "duplicate definition",
    ///     Label::new(Span::new(30, 33), "redefined here"),
    /// )
    /// .with_secondary(Label::new(Span::new(4, 7), "first defined here"));
    /// assert_eq!(diag.secondary().len(), 1);
    /// ```
    #[must_use]
    pub fn with_secondary(mut self, label: Label) -> Self {
        self.secondary.push(label);
        self
    }

    /// Adds a trailing note, returning the diagnostic for chaining.
    ///
    /// A note is neutral context with no span of its own; a renderer prints it on
    /// its own line below the frame, marked `note:`. Notes render in the order they
    /// are added, after any help lines' siblings — see
    /// [`with_help`](Diagnostic::with_help).
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)))
    ///     .with_note("the value was moved on the previous line");
    /// assert_eq!(diag.notes().next(), Some("the value was moved on the previous line"));
    /// ```
    #[must_use]
    pub fn with_note(mut self, note: impl Into<Box<str>>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Adds a trailing help line, returning the diagnostic for chaining.
    ///
    /// A help line is a suggestion toward a fix; a renderer prints it below the
    /// notes, marked `help:`. Help lines render in the order they are added.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)))
    ///     .with_help("add a semicolon");
    /// assert_eq!(diag.help().next(), Some("add a semicolon"));
    /// ```
    #[must_use]
    pub fn with_help(mut self, help: impl Into<Box<str>>) -> Self {
        self.help.push(help.into());
        self
    }

    /// Returns the level this diagnostic reports at.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)));
    /// assert_eq!(diag.severity(), Severity::Error);
    /// ```
    #[inline]
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// Returns the headline message.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)));
    /// assert_eq!(diag.message(), "boom");
    /// ```
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the primary label — the span the caret underlines.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(
    ///     Severity::Error,
    ///     "boom",
    ///     Label::new(Span::new(2, 5), "right here"),
    /// );
    /// assert_eq!(diag.primary().message(), "right here");
    /// ```
    #[inline]
    #[must_use]
    pub const fn primary(&self) -> &Label {
        &self.primary
    }

    /// Returns the secondary labels, in the order they were added.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)))
    ///     .with_secondary(Label::new(Span::new(1, 2), "related"));
    /// assert_eq!(diag.secondary()[0].message(), "related");
    /// ```
    #[inline]
    #[must_use]
    pub fn secondary(&self) -> &[Label] {
        &self.secondary
    }

    /// Iterates over the note lines, in the order they were added.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)))
    ///     .with_note("one")
    ///     .with_note("two");
    /// assert_eq!(diag.notes().collect::<Vec<_>>(), ["one", "two"]);
    /// ```
    pub fn notes(&self) -> impl ExactSizeIterator<Item = &str> {
        self.notes.iter().map(AsRef::as_ref)
    }

    /// Iterates over the help lines, in the order they were added.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity};
    /// use span_lang::Span;
    ///
    /// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)))
    ///     .with_help("try this");
    /// assert_eq!(diag.help().collect::<Vec<_>>(), ["try this"]);
    /// ```
    pub fn help(&self) -> impl ExactSizeIterator<Item = &str> {
        self.help.iter().map(AsRef::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use span_lang::Span;

    use super::*;

    #[test]
    fn test_new_starts_with_empty_extras() {
        let diag = Diagnostic::new(
            Severity::Error,
            "type mismatch",
            Label::new(Span::new(8, 11), "here"),
        );
        assert_eq!(diag.severity(), Severity::Error);
        assert_eq!(diag.message(), "type mismatch");
        assert_eq!(diag.primary().span(), Span::new(8, 11));
        assert!(diag.secondary().is_empty());
        assert_eq!(diag.notes().count(), 0);
        assert_eq!(diag.help().count(), 0);
    }

    #[test]
    fn test_builders_accumulate_in_order() {
        let diag = Diagnostic::new(Severity::Error, "m", Label::unlabelled(Span::empty(0)))
            .with_secondary(Label::new(Span::new(1, 2), "a"))
            .with_secondary(Label::new(Span::new(3, 4), "b"))
            .with_note("n1")
            .with_note("n2")
            .with_help("h1");

        let secondary: Vec<_> = diag.secondary().iter().map(Label::message).collect();
        assert_eq!(secondary, ["a", "b"]);
        assert_eq!(diag.notes().collect::<Vec<_>>(), ["n1", "n2"]);
        assert_eq!(diag.help().collect::<Vec<_>>(), ["h1"]);
    }
}
