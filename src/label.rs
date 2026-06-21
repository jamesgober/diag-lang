//! A span paired with the message that explains it.

use alloc::boxed::Box;

use span_lang::Span;

/// A [`Span`] plus the text that explains what is at that span.
///
/// A label is the thing a diagnostic points *with*: "this region of the source is
/// what I am talking about, and here is why". A [`Diagnostic`](crate::Diagnostic)
/// always carries one primary label — the span the caret underlines — and, from
/// `v0.3.0`, any number of secondary labels for related locations.
///
/// The span is a position in a [`SourceMap`](source_lang::SourceMap)'s **global**
/// position space, not an offset into a single file. That is what lets a renderer
/// take a label and a map and work out, on its own, which file the label falls in
/// and where on the line to draw the caret — see
/// [`SourceMap::locate`](source_lang::SourceMap::locate). A label built from a
/// local, single-file offset is still valid; it simply resolves against a map that
/// holds exactly that one source.
///
/// The message may be empty: a label with no text marks a location without
/// annotating it, which a renderer draws as a bare caret.
///
/// # Examples
///
/// ```
/// use diag_lang::Label;
/// use span_lang::Span;
///
/// let label = Label::new(Span::new(8, 11), "expected `Int`, found `Str`");
/// assert_eq!(label.span(), Span::new(8, 11));
/// assert_eq!(label.message(), "expected `Int`, found `Str`");
///
/// // A label can point without annotating.
/// let bare = Label::unlabelled(Span::new(8, 11));
/// assert_eq!(bare.message(), "");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    span: Span,
    message: Box<str>,
}

impl Label {
    /// Builds a label covering `span` and explained by `message`.
    ///
    /// The message is taken by value — anything that converts into a `Box<str>`,
    /// such as a `&str` or an owned `String` — so the label owns its text and a
    /// caller need not keep the original alive.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Label;
    /// use span_lang::Span;
    ///
    /// let label = Label::new(Span::new(0, 4), "unknown keyword");
    /// assert_eq!(label.message(), "unknown keyword");
    /// ```
    #[inline]
    #[must_use]
    pub fn new(span: Span, message: impl Into<Box<str>>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    /// Builds a label that marks `span` without any explanatory text.
    ///
    /// Equivalent to [`Label::new(span, "")`](Label::new). A renderer draws it as a
    /// bare caret with no trailing message — useful when the diagnostic's main
    /// message already says everything and the label only needs to point.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Label;
    /// use span_lang::Span;
    ///
    /// let label = Label::unlabelled(Span::empty(5));
    /// assert!(label.message().is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn unlabelled(span: Span) -> Self {
        Self {
            span,
            message: Box::from(""),
        }
    }

    /// Returns the span this label points at, in global position space.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Label;
    /// use span_lang::Span;
    ///
    /// assert_eq!(Label::new(Span::new(3, 9), "here").span(), Span::new(3, 9));
    /// ```
    #[inline]
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Returns the label's message, which is empty for an
    /// [`unlabelled`](Label::unlabelled) label.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Label;
    /// use span_lang::Span;
    ///
    /// assert_eq!(Label::new(Span::new(0, 1), "oops").message(), "oops");
    /// ```
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_stores_span_and_message() {
        let label = Label::new(Span::new(2, 6), "msg");
        assert_eq!(label.span(), Span::new(2, 6));
        assert_eq!(label.message(), "msg");
    }

    #[test]
    fn test_unlabelled_has_empty_message() {
        let label = Label::unlabelled(Span::new(4, 4));
        assert_eq!(label.span(), Span::new(4, 4));
        assert_eq!(label.message(), "");
    }

    #[test]
    fn test_message_accepts_owned_string() {
        extern crate alloc;
        use alloc::string::String;
        let label = Label::new(Span::new(0, 1), String::from("owned"));
        assert_eq!(label.message(), "owned");
    }
}
