//! Turning a [`Diagnostic`] into caret-annotated text.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use source_lang::{SourceId, SourceMap};
use span_lang::BytePos;

use crate::{Diagnostic, Label};

/// The number of columns a tab advances to. A tab moves the cursor to the next
/// multiple of this width, the convention source viewers use, so the caret line
/// can be aligned without knowing the reader's terminal tab settings.
const TAB_WIDTH: usize = 4;

/// Renders a [`Diagnostic`] against a [`SourceMap`] into aligned, caret-annotated
/// text in the style of a modern toolchain.
///
/// The renderer reads the spans a diagnostic points at, finds the sources and
/// lines they fall on through the map, and prints those lines with underlines
/// placed exactly under the offending characters — `^` for the primary label,
/// `-` for secondary labels. Alignment is computed in display columns: a
/// multi-byte UTF-8 character counts as one column, and a tab expands to the next
/// four-column tab stop, so the underlines line up under the source whatever it
/// contains.
///
/// # Layout
///
/// Output begins with the `severity: message` header. Then, for each source file
/// any label points into — the file holding the primary label first, the rest in
/// the order they were added to the map — a frame is drawn: a `--> file:line:col`
/// location line and the labelled lines of that file, each shown once with every
/// label's underline stacked beneath it. A span covering several lines underlines
/// each line it touches, with the message on the last. Trailing `note:` and
/// `help:` lines, if any, close the output.
///
/// A label whose span falls outside the loaded sources is not dropped and does not
/// panic: it renders as a `note:` line reporting the offending span. Output is
/// deterministic — the same diagnostic and map always render byte-identical text —
/// and always ends with a newline.
///
/// # Examples
///
/// ```
/// use diag_lang::{Diagnostic, Label, Renderer, Severity};
/// use source_lang::SourceMap;
/// use span_lang::Span;
///
/// let mut map = SourceMap::new();
/// map.add("main.rs", "fn main() {\n    let x = foo();\n}\n").expect("fits");
///
/// // `foo` sits at global bytes 24..27.
/// let diag = Diagnostic::new(
///     Severity::Error,
///     "cannot find value `foo` in this scope",
///     Label::new(Span::new(24, 27), "not found in this scope"),
/// );
///
/// let out = Renderer::new().render(&diag, &map);
/// assert_eq!(out, "\
/// error: cannot find value `foo` in this scope
///  --> main.rs:2:13
///   |
/// 2 |     let x = foo();
///   |             ^^^ not found in this scope
///   |
/// ");
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct Renderer {}

impl Renderer {
    /// Creates a renderer with the default layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Renderer;
    ///
    /// let renderer = Renderer::new();
    /// let _ = renderer; // ready to render against a source map
    /// ```
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }

    /// Renders `diag` against `map` and returns the result as an owned `String`.
    ///
    /// This is the convenience form of [`render_to`](Renderer::render_to): it
    /// allocates a fresh string, writes the diagnostic into it, and returns it.
    /// The output always ends with a newline.
    ///
    /// A label whose span cannot be located in `map` — past the end of the loaded
    /// sources, or at an offset no source covers — does not panic; it renders as a
    /// `note:` line reporting the span, so a stale or hostile span degrades to a
    /// readable message instead of a crash.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Renderer, Severity};
    /// use source_lang::SourceMap;
    /// use span_lang::Span;
    ///
    /// let map = SourceMap::new(); // empty: nothing to locate against
    /// let diag = Diagnostic::new(
    ///     Severity::Error,
    ///     "unexpected end of input",
    ///     Label::unlabelled(Span::new(10, 12)),
    /// );
    ///
    /// let out = Renderer::new().render(&diag, &map);
    /// assert_eq!(out, "\
    /// error: unexpected end of input
    ///  = note: span 10..12 is outside the loaded sources
    /// ");
    /// ```
    #[must_use]
    pub fn render(&self, diag: &Diagnostic, map: &SourceMap) -> String {
        let mut out = String::new();
        // Writing into a `String` is infallible, so the `fmt::Result` cannot be an
        // error here; `render_to` exposes the fallible form for sinks that can fail.
        let _ = self.render_to(&mut out, diag, map);
        out
    }

    /// Renders `diag` against `map`, writing into any [`fmt::Write`] sink.
    ///
    /// This is the primitive [`render`](Renderer::render) is built on. Use it to
    /// render into a buffer you already own — reused across many diagnostics to
    /// avoid a per-diagnostic allocation — or straight into a formatter. The
    /// output always ends with a newline.
    ///
    /// # Errors
    ///
    /// Returns the first [`fmt::Error`] the sink reports. Writing into a `String`
    /// never fails; a custom sink may.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Renderer, Severity};
    /// use source_lang::SourceMap;
    /// use span_lang::Span;
    ///
    /// let mut map = SourceMap::new();
    /// map.add("u.rs", "let x = 1;").expect("fits");
    /// let diag = Diagnostic::new(
    ///     Severity::Warning,
    ///     "unused variable `x`",
    ///     Label::new(Span::new(4, 5), "never read"),
    /// );
    ///
    /// let mut buf = String::new();
    /// Renderer::new().render_to(&mut buf, &diag, &map).expect("string write");
    /// assert!(buf.starts_with("warning: unused variable `x`\n"));
    /// ```
    pub fn render_to(
        &self,
        out: &mut impl fmt::Write,
        diag: &Diagnostic,
        map: &SourceMap,
    ) -> fmt::Result {
        writeln!(out, "{}: {}", diag.severity().as_str(), diag.message())?;

        // Split every label into the source it resolves against, keeping any that
        // resolve nowhere for a trailing note. The primary leads, so its file
        // frames first and its position anchors the frame.
        let mut located: Vec<(SourceId, bool, &Label)> = Vec::new();
        let mut unlocated: Vec<&Label> = Vec::new();
        for (is_primary, label) in labels(diag) {
            match map.locate(label.span().start()) {
                Some((id, _)) => located.push((id, is_primary, label)),
                None => unlocated.push(label),
            }
        }

        for id in frame_order(&located) {
            write_frame(out, map, id, &located)?;
        }

        for label in &unlocated {
            writeln!(
                out,
                " = note: span {} is outside the loaded sources",
                label.span()
            )?;
        }
        for note in diag.notes() {
            writeln!(out, " = note: {note}")?;
        }
        for help in diag.help() {
            writeln!(out, " = help: {help}")?;
        }

        Ok(())
    }
}

/// The primary label paired with `true`, then every secondary paired with
/// `false`, in the order they were added.
fn labels(diag: &Diagnostic) -> impl Iterator<Item = (bool, &Label)> {
    core::iter::once((true, diag.primary())).chain(diag.secondary().iter().map(|l| (false, l)))
}

/// The source ids to draw a frame for, the primary's file first and the rest in
/// map order, each once.
fn frame_order(located: &[(SourceId, bool, &Label)]) -> Vec<SourceId> {
    let primary_file = located.iter().find(|(_, p, _)| *p).map(|(id, _, _)| *id);

    let mut ids: Vec<SourceId> = located.iter().map(|(id, _, _)| *id).collect();
    ids.sort_unstable_by_key(|id| id.to_u32());
    ids.dedup();

    if let Some(pf) = primary_file {
        ids.retain(|id| *id != pf);
        ids.insert(0, pf);
    }
    ids
}

/// One label's underline on one line: where the carets sit and what they say.
struct Mark<'a> {
    line: u32,
    lead: usize,
    width: usize,
    marker: char,
    /// The message, present only on the label's last covered line.
    message: Option<&'a str>,
    /// Primary labels sort ahead of secondary ones when stacked on a line.
    primary: bool,
}

/// Renders the frame for the source named by `id`: its location line and every
/// line any of its labels touches, with the underlines stacked beneath each.
fn write_frame(
    out: &mut impl fmt::Write,
    map: &SourceMap,
    id: SourceId,
    located: &[(SourceId, bool, &Label)],
) -> fmt::Result {
    let Some(file) = map.source(id) else {
        return Ok(());
    };
    let text = file.text();
    let base = file.span().start().to_u32();
    let index = file.line_index();

    // Resolve this file's labels into per-line marks, and remember the anchor —
    // the primary if it is here, else the earliest label — for the location line.
    let mut marks: Vec<Mark<'_>> = Vec::new();
    let mut anchor: Option<(u32, u32)> = None;
    let mut anchor_is_primary = false;
    for &(label_id, is_primary, label) in located {
        if label_id != id {
            continue;
        }
        let local_start = label.span().start().to_u32().saturating_sub(base);
        let local_end = (label.span().end().to_u32().saturating_sub(base) as usize).min(text.len());
        let lc = index.line_col(BytePos::new(local_start));

        // The anchor is the primary label, or failing that the earliest one.
        let here = (lc.line, lc.col);
        let better = match anchor {
            None => true,
            Some(_) if is_primary && !anchor_is_primary => true,
            Some(_) if !is_primary && anchor_is_primary => false,
            Some(a) => here < a,
        };
        if better {
            anchor = Some(here);
            anchor_is_primary = is_primary;
        }

        push_marks(
            &mut marks,
            &index,
            text,
            local_start as usize,
            local_end,
            lc.line,
            is_primary,
            label.message(),
        );
    }

    let Some((anchor_line, anchor_col)) = anchor else {
        return Ok(());
    };

    // The gutter is as wide as the largest line number the frame shows.
    let mut lines: Vec<u32> = marks.iter().map(|m| m.line).collect();
    lines.sort_unstable();
    lines.dedup();
    let max_line = lines.last().copied().unwrap_or(anchor_line);
    let gw = decimal_width(max_line);

    writeln!(
        out,
        "{:gw$}--> {}:{anchor_line}:{anchor_col}",
        "",
        file.name()
    )?;
    writeln!(out, "{:gw$} |", "")?;

    for &line in &lines {
        let line_text = index
            .line_span(line)
            .and_then(|span| text.get(span.start().to_usize()..span.end().to_usize()))
            .unwrap_or("");

        let mut expanded = String::new();
        expand_tabs(line_text, &mut expanded);
        writeln!(out, "{line:>gw$} | {expanded}")?;

        // Stack this line's underlines left to right, primary ahead of secondary.
        let mut row: Vec<&Mark<'_>> = marks.iter().filter(|m| m.line == line).collect();
        row.sort_by(|a, b| a.lead.cmp(&b.lead).then_with(|| b.primary.cmp(&a.primary)));
        for mark in row {
            write!(out, "{:gw$} | {:lead$}", "", "", lead = mark.lead)?;
            for _ in 0..mark.width {
                out.write_char(mark.marker)?;
            }
            match mark.message {
                Some(message) => writeln!(out, " {message}")?,
                None => out.write_char('\n')?,
            }
        }
    }

    writeln!(out, "{:gw$} |", "")
}

/// Computes the underline marks for one label across every line its span covers,
/// appending them to `marks`. Local offsets are into `text`; `start_line` is the
/// 1-based line the span starts on.
#[allow(clippy::too_many_arguments)]
fn push_marks<'a>(
    marks: &mut Vec<Mark<'a>>,
    index: &span_lang::LineIndex<'_>,
    text: &str,
    local_start: usize,
    local_end: usize,
    start_line: u32,
    primary: bool,
    message: &'a str,
) {
    let marker = if primary { '^' } else { '-' };

    // The last line the span touches: the line of its final covered byte, or the
    // start line for a zero-width span.
    let end_line = if local_end > local_start {
        index.line_col(BytePos::new((local_end - 1) as u32)).line
    } else {
        start_line
    };

    for line in start_line..=end_line {
        let Some(span) = index.line_span(line) else {
            continue;
        };
        let lo = span.start().to_usize();
        let hi = span.end().to_usize();
        let Some(line_text) = text.get(lo..hi) else {
            continue;
        };

        // Intersect the span with this line's content, then express it as offsets
        // into the line, floored to character boundaries.
        let seg_start = local_start.max(lo);
        let seg_end = local_end.min(hi).max(seg_start);
        let a = floor_boundary(line_text, seg_start - lo);
        let b = floor_boundary(line_text, seg_end - lo).max(a);

        let lead = visual_width(&line_text[..a]);
        let width = visual_width(&line_text[..b]).saturating_sub(lead).max(1);

        let is_last = line == end_line;
        marks.push(Mark {
            line,
            lead,
            width,
            marker,
            message: (is_last && !message.is_empty()).then_some(message),
            primary,
        });
    }
}

/// The number of decimal digits in `n` (at least one, for `0`).
const fn decimal_width(mut n: u32) -> usize {
    let mut width = 1;
    while n >= 10 {
        n /= 10;
        width += 1;
    }
    width
}

/// Floors `idx` down to the nearest character boundary of `s`, clamping into
/// range first. A span offset that lands inside a multi-byte character is moved to
/// the start of that character, so slicing at the result never splits a `char`.
fn floor_boundary(s: &str, idx: usize) -> usize {
    let mut at = idx.min(s.len());
    while at > 0 && !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The display width of `s` in columns: every character is one column except a
/// tab, which advances to the next multiple of [`TAB_WIDTH`]. This is the column
/// model the underlines are aligned in.
fn visual_width(s: &str) -> usize {
    let mut col = 0;
    for ch in s.chars() {
        if ch == '\t' {
            col += TAB_WIDTH - (col % TAB_WIDTH);
        } else {
            col += 1;
        }
    }
    col
}

/// Appends `s` to `out` with every tab expanded to spaces up to the next
/// [`TAB_WIDTH`] stop, so the rendered line occupies exactly the columns
/// [`visual_width`] counts.
fn expand_tabs(s: &str, out: &mut String) {
    let mut col = 0;
    for ch in s.chars() {
        if ch == '\t' {
            let advance = TAB_WIDTH - (col % TAB_WIDTH);
            for _ in 0..advance {
                out.push(' ');
            }
            col += advance;
        } else {
            out.push(ch);
            col += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimal_width_counts_digits() {
        assert_eq!(decimal_width(0), 1);
        assert_eq!(decimal_width(9), 1);
        assert_eq!(decimal_width(10), 2);
        assert_eq!(decimal_width(999), 3);
        assert_eq!(decimal_width(1000), 4);
    }

    #[test]
    fn test_visual_width_counts_chars_as_one() {
        assert_eq!(visual_width("αβ"), 2);
        assert_eq!(visual_width("abc"), 3);
    }

    #[test]
    fn test_visual_width_expands_tabs_to_stops() {
        assert_eq!(visual_width("\t"), 4);
        assert_eq!(visual_width("a\t"), 4);
        assert_eq!(visual_width("abcd\t"), 8);
        assert_eq!(visual_width("\t\t"), 8);
    }

    #[test]
    fn test_expand_tabs_matches_visual_width() {
        for s in ["", "a", "\t", "a\tb", "\tx\t", "abcd\te"] {
            let mut out = String::new();
            expand_tabs(s, &mut out);
            assert_eq!(out.chars().count(), visual_width(s), "for {s:?}");
            assert!(!out.contains('\t'));
        }
    }

    #[test]
    fn test_floor_boundary_moves_into_char_start() {
        assert_eq!(floor_boundary("αβ", 1), 0);
        assert_eq!(floor_boundary("αβ", 2), 2);
        assert_eq!(floor_boundary("ab", 9), 2);
    }
}
