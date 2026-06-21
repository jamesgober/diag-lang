//! The diagnostic: a severity, a message, and the span it points at.

use alloc::boxed::Box;

use crate::{Label, Severity};

/// One thing a compiler stage has to say about the source: a [`Severity`], a
/// headline message, and a primary [`Label`] pointing at the span it concerns.
///
/// A `Diagnostic` is the unit a lexer, parser, or type checker emits and a
/// [`Renderer`](crate::Renderer) turns into caret-annotated output. Constructing
/// one is cheap and allocates only what the message and label text require — it is
/// on the front-end's hot path, so it holds owned `Box<str>` text and a single
/// `Copy` span rather than anything heavier.
///
/// Every diagnostic has a primary label, supplied at construction, so it always
/// knows where it points; there is no half-built state to guard against. Secondary
/// labels and trailing notes arrive in a later release as additive builder
/// methods.
///
/// # Examples
///
/// ```
/// use diag_lang::{Diagnostic, Label, Severity};
/// use span_lang::Span;
///
/// let diag = Diagnostic::new(
///     Severity::Error,
///     "type mismatch",
///     Label::new(Span::new(8, 11), "expected `Int`, found `Str`"),
/// );
///
/// assert_eq!(diag.severity(), Severity::Error);
/// assert_eq!(diag.message(), "type mismatch");
/// assert_eq!(diag.primary().span(), Span::new(8, 11));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    severity: Severity,
    message: Box<str>,
    primary: Label,
}

impl Diagnostic {
    /// Builds a diagnostic at `severity`, headed by `message`, pointing at
    /// `primary`.
    ///
    /// The message is taken by value (a `&str` or an owned `String`), so the
    /// diagnostic owns its text. The primary label is required: a diagnostic
    /// always has a span to render a caret under, which keeps the type total and
    /// the renderer free of a "no location" branch.
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
    /// ```
    #[inline]
    #[must_use]
    pub fn new(severity: Severity, message: impl Into<Box<str>>, primary: Label) -> Self {
        Self {
            severity,
            message: message.into(),
            primary,
        }
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
}

#[cfg(test)]
mod tests {
    use span_lang::Span;

    use super::*;

    #[test]
    fn test_new_exposes_all_three_fields() {
        let diag = Diagnostic::new(
            Severity::Error,
            "type mismatch",
            Label::new(Span::new(8, 11), "here"),
        );
        assert_eq!(diag.severity(), Severity::Error);
        assert_eq!(diag.message(), "type mismatch");
        assert_eq!(diag.primary().span(), Span::new(8, 11));
        assert_eq!(diag.primary().message(), "here");
    }

    #[test]
    fn test_message_accepts_owned_string() {
        extern crate alloc;
        use alloc::string::String;
        let diag = Diagnostic::new(
            Severity::Note,
            String::from("owned message"),
            Label::unlabelled(Span::empty(0)),
        );
        assert_eq!(diag.message(), "owned message");
    }
}
