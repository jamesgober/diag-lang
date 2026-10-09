//! Diagnostic codes: the stable, machine-readable name of a kind of diagnostic.

use core::fmt;

/// The longest prefix a [`Code`] accepts, in letters.
const MAX_PREFIX: usize = 4;

/// The largest number a [`Code`] accepts: codes are written with exactly four
/// digits.
const MAX_NUMBER: u16 = 9999;

/// A diagnostic code such as `E0001`, `T0042`, or `KRA0013`: an uppercase
/// prefix naming the registry that owns it, and a four-digit number within that
/// registry.
///
/// A code is the stable identity of a *kind* of diagnostic. Messages can be
/// reworded between releases; the code does not change, so tools key on it: an
/// editor links it to documentation, a lint configuration silences it, a test
/// asserts on it, and a filter separates lexical errors from parse errors by
/// range without parsing message text. The renderer shows it in the header,
/// rustc-style (`error[E0001]: …`), and the JSON output carries it as a string.
///
/// # Format
///
/// One to four ASCII uppercase letters (`A`–`Z`) followed by a number from `0`
/// to `9999`, always written with exactly four digits: `Code::new("E", 1)`
/// displays as `E0001`. The fixed width means a code's text and its value
/// round-trip exactly ([`Code::parse`] accepts precisely what `Display`
/// writes), and codes of one prefix sort the same as text and as numbers.
///
/// # Registries and ranges
///
/// The prefix names the registry; the number range within it names the phase.
/// Codes in use across the `-lang` family:
///
/// | Prefix | Owner | Range |
/// |---|---|---|
/// | `E` | Runtime errors: OPS | `E0001`–`E0099` |
/// | `E` | Runtime errors: LSB | `E0100`–`E0199` |
/// | `T` | Type checking (typeck-lang) | `T0001`– |
///
/// A language front end takes its own prefix (Kraken uses `KRA`) and is
/// recommended to split its numbers by phase, so that "is this a lexical
/// error?" is a range check:
///
/// | Range | Phase |
/// |---|---|
/// | `0001`–`0999` | Lexical: bad characters, unterminated literals, malformed numbers |
/// | `1000`–`1999` | Parse: unexpected or missing tokens, unbalanced delimiters |
/// | `2000`–`7999` | Semantic: name resolution, types, borrow and flow checks |
/// | `8000`–`8999` | Lints and style warnings |
/// | `9000`–`9999` | Internal and tool errors (`9999`: internal compiler error) |
///
/// This is a convention, not an enforced rule: the crate accepts any
/// well-formed code. See `docs/API.md` for the full guidance.
///
/// # Examples
///
/// ```
/// use diag_lang::Code;
///
/// // A registry declares its codes as constants.
/// const UNTERMINATED_STRING: Code = Code::new("KRA", 1).unwrap();
/// const UNEXPECTED_TOKEN: Code = Code::new("KRA", 1000).unwrap();
///
/// assert_eq!(UNTERMINATED_STRING.to_string(), "KRA0001");
/// assert_eq!(UNEXPECTED_TOKEN.prefix(), "KRA");
/// assert_eq!(UNEXPECTED_TOKEN.number(), 1000);
///
/// // The phase is a range check on the number.
/// let is_lexical = |c: Code| c.prefix() == "KRA" && c.number() < 1000;
/// assert!(is_lexical(UNTERMINATED_STRING));
/// assert!(!is_lexical(UNEXPECTED_TOKEN));
///
/// // Text and value round-trip exactly.
/// assert_eq!(Code::parse("KRA0001"), Some(UNTERMINATED_STRING));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code {
    /// The prefix letters, left-aligned and padded with zero bytes. Zero sorts
    /// below every letter, so the derived order puts `E` before `EA` before `F`.
    prefix: [u8; MAX_PREFIX],
    number: u16,
}

impl Code {
    /// Builds the code `prefix` + `number`, or `None` if either is out of range.
    ///
    /// `prefix` must be one to four ASCII uppercase letters and `number` at
    /// most `9999`. The function is `const`, so a registry can declare its codes
    /// as constants with `Code::new("E", 1).unwrap()`, where a malformed code is
    /// a compile error rather than a runtime failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Code;
    ///
    /// const ARITH_OVERFLOW: Code = Code::new("E", 1).unwrap();
    /// assert_eq!(ARITH_OVERFLOW.to_string(), "E0001");
    ///
    /// assert_eq!(Code::new("e", 1), None); // lowercase
    /// assert_eq!(Code::new("", 1), None); // no prefix
    /// assert_eq!(Code::new("ABCDE", 1), None); // prefix too long
    /// assert_eq!(Code::new("E", 10_000), None); // more than four digits
    /// ```
    #[must_use]
    pub const fn new(prefix: &str, number: u16) -> Option<Self> {
        let bytes = prefix.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_PREFIX || number > MAX_NUMBER {
            return None;
        }
        let mut stored = [0u8; MAX_PREFIX];
        let mut i = 0;
        while i < bytes.len() {
            if !bytes[i].is_ascii_uppercase() {
                return None;
            }
            stored[i] = bytes[i];
            i += 1;
        }
        Some(Self {
            prefix: stored,
            number,
        })
    }

    /// Parses a code written the way [`Display`](fmt::Display) writes it: one
    /// to four ASCII uppercase letters followed by exactly four digits.
    ///
    /// Returns `None` for anything else, including surrounding whitespace, a
    /// lowercase prefix, or a number with more or fewer than four digits, so
    /// `parse(&code.to_string()) == Some(code)` for every code and nothing else
    /// parses.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Code;
    ///
    /// let code = Code::parse("T0042").unwrap();
    /// assert_eq!((code.prefix(), code.number()), ("T", 42));
    ///
    /// assert_eq!(Code::parse("T42"), None); // four digits, always
    /// assert_eq!(Code::parse("T00042"), None);
    /// assert_eq!(Code::parse("0042"), None); // a prefix is required
    /// ```
    #[must_use]
    pub const fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        // Four digits plus a prefix of one to four letters.
        if bytes.len() < 5 || bytes.len() > MAX_PREFIX + 4 {
            return None;
        }
        let split = bytes.len() - 4;
        let mut number: u16 = 0;
        let mut i = split;
        while i < bytes.len() {
            let digit = bytes[i];
            if !digit.is_ascii_digit() {
                return None;
            }
            // At most four digits, so this stays within 9999.
            number = number * 10 + (digit - b'0') as u16;
            i += 1;
        }
        let (prefix, _) = bytes.split_at(split);
        match core::str::from_utf8(prefix) {
            Ok(prefix) => Self::new(prefix, number),
            Err(_) => None,
        }
    }

    /// Returns the prefix: the registry the code belongs to, such as `"E"` or
    /// `"KRA"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Code;
    ///
    /// assert_eq!(Code::new("KRA", 13).unwrap().prefix(), "KRA");
    /// ```
    #[must_use]
    pub fn prefix(&self) -> &str {
        let len = self
            .prefix
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(MAX_PREFIX);
        // The stored bytes are ASCII letters by construction, so this is always
        // `Ok`; the fallback keeps the accessor total without an `unwrap`.
        core::str::from_utf8(self.prefix.get(..len).unwrap_or(&[])).unwrap_or("")
    }

    /// Returns the number within the prefix's registry, `0`–`9999`.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Code;
    ///
    /// assert_eq!(Code::new("E", 102).unwrap().number(), 102);
    /// ```
    #[inline]
    #[must_use]
    pub const fn number(&self) -> u16 {
        self.number
    }
}

impl fmt::Display for Code {
    /// Writes the prefix followed by the number as exactly four digits:
    /// `E0001`, `KRA9999`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:04}", self.prefix(), self.number)
    }
}

impl fmt::Debug for Code {
    /// Writes `Code(E0001)`: the code's text is more useful than its storage.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Code({self})")
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn test_new_accepts_the_documented_shapes() {
        for (prefix, number, text) in [
            ("E", 1, "E0001"),
            ("T", 42, "T0042"),
            ("KRA", 9999, "KRA9999"),
            ("ABCD", 0, "ABCD0000"),
        ] {
            let code = Code::new(prefix, number).unwrap();
            assert_eq!(code.prefix(), prefix);
            assert_eq!(code.number(), number);
            assert_eq!(code.to_string(), text);
            assert_eq!(Code::parse(text), Some(code));
        }
    }

    #[test]
    fn test_new_rejects_malformed_codes() {
        assert_eq!(Code::new("", 1), None);
        assert_eq!(Code::new("ABCDE", 1), None);
        assert_eq!(Code::new("e", 1), None);
        assert_eq!(Code::new("E1", 1), None);
        assert_eq!(Code::new("É", 1), None);
        assert_eq!(Code::new("E", 10_000), None);
        assert_eq!(Code::new("E", u16::MAX), None);
    }

    #[test]
    fn test_parse_rejects_everything_display_cannot_write() {
        for bad in [
            "",
            "E",
            "E1",
            "E001",
            "E00001",
            "0001",
            "e0001",
            " E0001",
            "E0001 ",
            "E-001",
            "E+001",
            "ABCDE0001",
            "E000a",
            "É0001",
            "E０００１",
        ] {
            assert_eq!(Code::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn test_order_is_prefix_then_number() {
        let e1 = Code::new("E", 1).unwrap();
        let e2 = Code::new("E", 2).unwrap();
        let ea = Code::new("EA", 0).unwrap();
        let t1 = Code::new("T", 1).unwrap();
        assert!(e1 < e2 && e2 < ea && ea < t1);
    }

    #[test]
    fn test_debug_shows_the_text() {
        assert_eq!(
            alloc::format!("{:?}", Code::new("E", 7).unwrap()),
            "Code(E0007)"
        );
    }
}
