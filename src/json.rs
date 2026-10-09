//! A dependency-free, deterministic JSON form of diagnostics, for language
//! servers, MCP tools, AI clients, and anything else that reads diagnostics
//! rather than displays them.
//!
//! The schema is versioned by [`JSON_SCHEMA_VERSION`] and documented in
//! `docs/API.md`; within one version it only ever gains keys.

use alloc::string::String;
use core::fmt;

use source_lang::{SourceFile, SourceMap};
use span_lang::Span;

use crate::render::{floor_boundary, line_col_at, resolve};
use crate::{Diagnostic, Diagnostics, Edit, Label, Suggestion};

/// The version of the JSON schema this crate writes, emitted as the `"version"`
/// key of every top-level object.
///
/// It increases only when a key is removed or changes meaning. Adding a key is
/// not a version change, so a reader should ignore keys it does not know.
///
/// *Since 1.1.0.*
///
/// # Examples
///
/// ```
/// use diag_lang::{Diagnostic, JSON_SCHEMA_VERSION, Label, Severity, SourceMap, Span};
///
/// let diag = Diagnostic::new(Severity::Error, "boom", Label::unlabelled(Span::empty(0)));
/// let json = diag.to_json(&SourceMap::new());
/// assert!(json.starts_with(&format!("{{\"version\":{JSON_SCHEMA_VERSION},")));
/// ```
pub const JSON_SCHEMA_VERSION: u32 = 1;

impl Diagnostic {
    /// Returns the diagnostic as one line of JSON, its spans resolved against
    /// `map`.
    ///
    /// The output is a single object with no whitespace and no trailing
    /// newline, so diagnostics can be streamed one per line (JSON Lines). Key
    /// order is fixed and the same diagnostic and map always produce the same
    /// bytes. Every string is escaped per RFC 8259, with U+2028 and U+2029
    /// escaped as well so the output is also a valid JavaScript literal.
    ///
    /// ```text
    /// {"version":1,"severity":"error","code":"E0425","message":"…",
    ///  "labels":[{"primary":true,"message":"…","span":{"start":24,"end":27},
    ///             "location":{"file":"main.rs","start":{"offset":24,"line":2,"column":13},
    ///                                          "end":{"offset":27,"line":2,"column":16}}}],
    ///  "notes":["…"],"help":["…"],
    ///  "suggestions":[{"message":"…","applicability":"machine_applicable",
    ///                  "edits":[{"span":{…},"location":{…},"replacement":"…"}]}]}
    /// ```
    ///
    /// `code` is `null` when no code is set. `labels` lists the primary label
    /// first, then the secondary labels in the order they were added. `span`
    /// is the label's global byte span as given; `location` resolves it: the
    /// file name, and for each end the byte `offset` into that file, the 1-based
    /// `line`, and the 1-based `column` counted in Unicode scalar values (the
    /// numbers the text renderer prints after `-->`). An end past the file is
    /// clamped to it and an offset inside a character is moved to that
    /// character's start. `location` is `null` for a span outside every loaded
    /// source. The full schema is in `docs/API.md`.
    ///
    /// *Since 1.1.0.*
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Code, Diagnostic, Label, Severity, SourceMap, Span};
    ///
    /// let mut map = SourceMap::new();
    /// map.add("a.txt", "let x = y;").expect("fits");
    /// let diag = Diagnostic::new(Severity::Error, "unknown `y`", Label::new(Span::new(8, 9), "here"))
    ///     .with_code(Code::new("E", 2001).unwrap());
    ///
    /// assert_eq!(
    ///     diag.to_json(&map),
    ///     concat!(
    ///         r#"{"version":1,"severity":"error","code":"E2001","message":"unknown `y`","#,
    ///         r#""labels":[{"primary":true,"message":"here","span":{"start":8,"end":9},"#,
    ///         r#""location":{"file":"a.txt","start":{"offset":8,"line":1,"column":9},"#,
    ///         r#""end":{"offset":9,"line":1,"column":10}}}],"#,
    ///         r#""notes":[],"help":[],"suggestions":[]}"#,
    ///     ),
    /// );
    /// ```
    #[must_use]
    pub fn to_json(&self, map: &SourceMap) -> String {
        let mut out = String::new();
        // Writing into a `String` cannot fail.
        let _ = self.write_json(&mut out, map);
        out
    }

    /// Writes the diagnostic as JSON into any [`fmt::Write`] sink; see
    /// [`to_json`](Diagnostic::to_json) for the format.
    ///
    /// *Since 1.1.0.*
    ///
    /// # Errors
    ///
    /// Returns the first [`fmt::Error`] the sink reports; writing into a
    /// `String` never fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Severity, SourceMap, Span};
    ///
    /// let diag = Diagnostic::new(Severity::Warning, "w", Label::unlabelled(Span::empty(0)));
    /// let mut line = String::new();
    /// diag.write_json(&mut line, &SourceMap::new()).expect("string write");
    /// line.push('\n'); // one diagnostic per line
    /// assert!(line.contains(r#""location":null"#));
    /// ```
    pub fn write_json(&self, out: &mut impl fmt::Write, map: &SourceMap) -> fmt::Result {
        write!(
            out,
            "{{\"version\":{JSON_SCHEMA_VERSION},\"severity\":\"{}\",\"code\":",
            self.severity().as_str()
        )?;
        match self.code() {
            Some(code) => write!(out, "\"{code}\"")?,
            None => out.write_str("null")?,
        }
        out.write_str(",\"message\":")?;
        write_string(out, self.message())?;

        out.write_str(",\"labels\":[")?;
        write_label(out, map, self.primary(), true)?;
        for label in self.secondary() {
            out.write_char(',')?;
            write_label(out, map, label, false)?;
        }

        out.write_str("],\"notes\":")?;
        write_strings(out, self.notes())?;
        out.write_str(",\"help\":")?;
        write_strings(out, self.help())?;

        out.write_str(",\"suggestions\":[")?;
        for (i, suggestion) in self.suggestions().iter().enumerate() {
            if i > 0 {
                out.write_char(',')?;
            }
            write_suggestion(out, map, suggestion)?;
        }
        out.write_str("]}")
    }
}

impl Diagnostics {
    /// Returns the whole collection as one JSON document, spans resolved
    /// against `map`.
    ///
    /// ```text
    /// {"version":1,"errors":2,"warnings":1,"omitted":0,"diagnostics":[{…},{…},{…}]}
    /// ```
    ///
    /// `errors` and `warnings` count every distinct diagnostic pushed,
    /// including ones the cap omitted; `omitted` is
    /// [`Diagnostics::omitted`]; `diagnostics` holds the stored diagnostics in
    /// their current order, each exactly as [`Diagnostic::to_json`] writes it.
    /// No whitespace, no trailing newline, deterministic.
    ///
    /// *Since 1.1.0.*
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, SourceMap, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Error, "e", Label::unlabelled(Span::empty(0))));
    /// let json = diags.to_json(&SourceMap::new());
    /// assert!(json.starts_with(r#"{"version":1,"errors":1,"warnings":0,"omitted":0,"diagnostics":[{"#));
    /// ```
    #[must_use]
    pub fn to_json(&self, map: &SourceMap) -> String {
        let mut out = String::new();
        // Writing into a `String` cannot fail.
        let _ = self.write_json(&mut out, map);
        out
    }

    /// Writes the collection as one JSON document into any [`fmt::Write`]
    /// sink; see [`to_json`](Diagnostics::to_json) for the format.
    ///
    /// *Since 1.1.0.*
    ///
    /// # Errors
    ///
    /// Returns the first [`fmt::Error`] the sink reports; writing into a
    /// `String` never fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostics, SourceMap};
    ///
    /// let mut out = String::new();
    /// Diagnostics::new().write_json(&mut out, &SourceMap::new()).expect("string write");
    /// assert_eq!(out, r#"{"version":1,"errors":0,"warnings":0,"omitted":0,"diagnostics":[]}"#);
    /// ```
    pub fn write_json(&self, out: &mut impl fmt::Write, map: &SourceMap) -> fmt::Result {
        write!(
            out,
            "{{\"version\":{JSON_SCHEMA_VERSION},\"errors\":{},\"warnings\":{},\"omitted\":{},\"diagnostics\":[",
            self.error_count(),
            self.warning_count(),
            self.omitted()
        )?;
        for (i, diag) in self.iter().enumerate() {
            if i > 0 {
                out.write_char(',')?;
            }
            diag.write_json(out, map)?;
        }
        out.write_str("]}")
    }
}

/// `{"primary":…,"message":…,"span":…,"location":…}`
fn write_label(
    out: &mut impl fmt::Write,
    map: &SourceMap,
    label: &Label,
    primary: bool,
) -> fmt::Result {
    write!(out, "{{\"primary\":{primary},\"message\":")?;
    write_string(out, label.message())?;
    out.write_char(',')?;
    write_span_and_location(out, map, label.span())?;
    out.write_char('}')
}

/// `{"message":…,"applicability":…,"edits":[…]}`
fn write_suggestion(
    out: &mut impl fmt::Write,
    map: &SourceMap,
    suggestion: &Suggestion,
) -> fmt::Result {
    out.write_str("{\"message\":")?;
    write_string(out, suggestion.message())?;
    write!(
        out,
        ",\"applicability\":\"{}\",\"edits\":[",
        suggestion.applicability().as_str()
    )?;
    for (i, edit) in suggestion.edits().iter().enumerate() {
        if i > 0 {
            out.write_char(',')?;
        }
        write_edit(out, map, edit)?;
    }
    out.write_str("]}")
}

/// `{"span":…,"location":…,"replacement":…}`
fn write_edit(out: &mut impl fmt::Write, map: &SourceMap, edit: &Edit) -> fmt::Result {
    out.write_char('{')?;
    write_span_and_location(out, map, edit.span())?;
    out.write_str(",\"replacement\":")?;
    write_string(out, edit.replacement())?;
    out.write_char('}')
}

/// `"span":{"start":…,"end":…},"location":{…}|null`
fn write_span_and_location(out: &mut impl fmt::Write, map: &SourceMap, span: Span) -> fmt::Result {
    write!(
        out,
        "\"span\":{{\"start\":{},\"end\":{}}},\"location\":",
        span.start().to_u32(),
        span.end().to_u32()
    )?;
    let Some((id, start, end)) = resolve(map, span) else {
        return out.write_str("null");
    };
    let Some(file) = map.source(id) else {
        return out.write_str("null");
    };
    out.write_str("{\"file\":")?;
    write_string(out, file.name())?;
    out.write_str(",\"start\":")?;
    write_point(out, map, file, start)?;
    out.write_str(",\"end\":")?;
    write_point(out, map, file, end.max(start))?;
    out.write_char('}')
}

/// `{"offset":…,"line":…,"column":…}` for local offset `at` in `file`.
fn write_point(
    out: &mut impl fmt::Write,
    map: &SourceMap,
    file: &SourceFile,
    at: usize,
) -> fmt::Result {
    let text = file.text();
    let at = floor_boundary(text, at);
    let (line, column) = if at == text.len() && at > 0 {
        // The end of the text, one past its last character. Computed from that
        // character so it is answered by this file's line table even when the
        // map would attribute the position to the next source.
        let prev = floor_boundary(text, at - 1);
        let lc = line_col_at(map, file, prev);
        if text.as_bytes().get(prev) == Some(&b'\n') {
            (lc.line.saturating_add(1), 1)
        } else {
            (lc.line, lc.col.saturating_add(1))
        }
    } else {
        let lc = line_col_at(map, file, at);
        (lc.line, lc.col)
    };
    write!(
        out,
        "{{\"offset\":{at},\"line\":{line},\"column\":{column}}}"
    )
}

/// `["…","…"]`
fn write_strings<'a>(
    out: &mut impl fmt::Write,
    items: impl Iterator<Item = &'a str>,
) -> fmt::Result {
    out.write_char('[')?;
    for (i, item) in items.enumerate() {
        if i > 0 {
            out.write_char(',')?;
        }
        write_string(out, item)?;
    }
    out.write_char(']')
}

/// Writes `s` as a JSON string literal, quotes included.
///
/// Escapes `"` and `\`, every control character below U+0020 (with the short
/// forms `\b \f \n \r \t` where JSON has them, `\u00XX` otherwise), and U+2028
/// and U+2029, which JSON allows raw but JavaScript does not. Everything else
/// is written as UTF-8, in runs, so plain text costs one `write_str`.
pub(crate) fn write_string(out: &mut impl fmt::Write, s: &str) -> fmt::Result {
    out.write_char('"')?;
    let mut run = 0;
    for (i, ch) in s.char_indices() {
        let escape: Option<&str> = match ch {
            '"' => Some("\\\""),
            '\\' => Some("\\\\"),
            '\n' => Some("\\n"),
            '\r' => Some("\\r"),
            '\t' => Some("\\t"),
            '\u{8}' => Some("\\b"),
            '\u{c}' => Some("\\f"),
            '\u{0}'..='\u{1f}' | '\u{2028}' | '\u{2029}' => None,
            _ => continue,
        };
        out.write_str(s.get(run..i).unwrap_or(""))?;
        match escape {
            Some(text) => out.write_str(text)?,
            None => write!(out, "\\u{:04x}", u32::from(ch))?,
        }
        run = i + ch.len_utf8();
    }
    out.write_str(s.get(run..).unwrap_or(""))?;
    out.write_char('"')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quoted(s: &str) -> String {
        let mut out = String::new();
        write_string(&mut out, s).unwrap();
        out
    }

    #[test]
    fn test_strings_are_escaped() {
        assert_eq!(quoted(""), r#""""#);
        assert_eq!(quoted("plain"), r#""plain""#);
        assert_eq!(quoted("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(quoted("\n\r\t\u{8}\u{c}"), r#""\n\r\t\b\f""#);
        assert_eq!(quoted("\u{0}\u{1b}\u{1f}"), r#""\u0000\u001b\u001f""#);
        assert_eq!(quoted("\u{2028}\u{2029}"), r#""\u2028\u2029""#);
        // Non-ASCII and DEL are valid raw in JSON.
        assert_eq!(quoted("é世😀\u{7f}"), "\"é世😀\u{7f}\"");
    }
}
