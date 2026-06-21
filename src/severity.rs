//! The level a diagnostic reports at.

use core::fmt;

/// The level a diagnostic reports at — how serious it is and how it is labelled.
///
/// A severity drives the word that heads the rendered diagnostic (`error`,
/// `warning`, …) and, once colour lands, the colour that word is printed in. It
/// carries no source position of its own; it is one field of a
/// [`Diagnostic`](crate::Diagnostic), paired with the message and the labelled
/// span.
///
/// The four levels are ordered from most to least serious, so the derived
/// ordering sorts a batch of diagnostics with the errors first:
/// `Error < Warning < Note < Help`.
///
/// # Examples
///
/// ```
/// use diag_lang::Severity;
///
/// assert_eq!(Severity::Error.as_str(), "error");
/// assert!(Severity::Error < Severity::Warning); // errors sort first
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// A hard failure: the input is wrong and cannot be accepted as-is.
    Error,
    /// A suspicious construct that is still accepted, but worth flagging.
    Warning,
    /// Neutral context attached to a diagnostic — a clarifying remark.
    Note,
    /// A suggestion that points the reader toward a fix.
    Help,
}

impl Severity {
    /// Returns the lowercase word that heads a rendered diagnostic of this level.
    ///
    /// The returned strings are `"error"`, `"warning"`, `"note"`, and `"help"` —
    /// the labels a renderer prints before the message, matching the convention
    /// developers expect from a modern toolchain.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Severity;
    ///
    /// assert_eq!(Severity::Warning.as_str(), "warning");
    /// assert_eq!(Severity::Help.as_str(), "help");
    /// ```
    #[inline]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Help => "help",
        }
    }
}

impl fmt::Display for Severity {
    /// Formats as the lowercase label from [`as_str`](Severity::as_str).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn test_as_str_matches_each_level() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Note.as_str(), "note");
        assert_eq!(Severity::Help.as_str(), "help");
    }

    #[test]
    fn test_display_equals_as_str() {
        assert_eq!(Severity::Error.to_string(), "error");
        assert_eq!(Severity::Note.to_string(), "note");
    }

    #[test]
    fn test_order_runs_most_to_least_serious() {
        assert!(Severity::Error < Severity::Warning);
        assert!(Severity::Warning < Severity::Note);
        assert!(Severity::Note < Severity::Help);
    }
}
