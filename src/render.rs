//! Turning a [`Diagnostic`] into caret-annotated text.

use alloc::string::String;
use core::fmt;

use source_lang::SourceMap;
use span_lang::Span;

use crate::Diagnostic;

/// The number of columns a tab advances to. A tab moves the cursor to the next
/// multiple of this width, the convention source viewers use, so the caret line
/// can be aligned without knowing the reader's terminal tab settings.
const TAB_WIDTH: usize = 4;

/// Renders a [`Diagnostic`] against a [`SourceMap`] into aligned, caret-annotated
/// text in the style of a modern toolchain.
///
/// The renderer reads the span a diagnostic points at, finds the source and line
/// it falls on through the map, and prints that line with a caret underline placed
/// exactly under the offending characters. Alignment is computed in display
/// columns — a multi-byte UTF-8 character counts as one column, and a tab expands
/// to the next four-column tab stop — so the carets line up under the source
/// whatever it contains.
///
/// Output is deterministic: the same diagnostic and map always render
/// byte-identical text. Producing it is allocation-light and never panics; a span
/// that falls outside the loaded sources renders a defined note rather than
/// failing (see [`render`](Renderer::render)).
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
    /// If the diagnostic's span cannot be located in `map` — it is past the end of
    /// the loaded sources, or references an offset no source covers — the renderer
    /// does not panic. It prints the header followed by a note that the span is
    /// outside the sources, so a stale or hostile span degrades to a readable
    /// message instead of a crash.
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
        // error here; the panic path is unreachable in practice and `render_to`
        // exposes the fallible form for sinks that can fail.
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

        match Frame::resolve(map, diag.primary().span()) {
            Some(frame) => frame.write(out, diag.primary().message()),
            None => writeln!(
                out,
                " = note: span {} is outside the loaded sources",
                diag.primary().span()
            ),
        }
    }
}

/// Everything needed to draw one single-line caret frame, resolved from the map.
struct Frame<'a> {
    /// The source's display name.
    name: &'a str,
    /// 1-based line number of the span's start.
    line_no: u32,
    /// 1-based character column of the span's start, for the `file:line:col`
    /// header. Counted in characters, as editors report it.
    col: u32,
    /// The offending line's text, terminator excluded.
    line_text: &'a str,
    /// Byte offset within `line_text` where the caret run begins, floored to a
    /// character boundary.
    start_byte: usize,
    /// Byte offset within `line_text` where the caret run ends, floored to a
    /// character boundary and never before `start_byte`.
    end_byte: usize,
}

impl<'a> Frame<'a> {
    /// Resolves a global span to a single-line frame, or `None` if no source
    /// covers the span's start.
    fn resolve(map: &'a SourceMap, span: Span) -> Option<Self> {
        let (id, local_start) = map.locate(span.start())?;
        let file = map.source(id)?;
        let text = file.text();

        let index = file.line_index();
        let lc = index.line_col(local_start);
        let line_span = index.line_span(lc.line)?;

        let line_lo = line_span.start().to_usize();
        let line_hi = line_span.end().to_usize();
        let line_text = text.get(line_lo..line_hi)?;

        // The span's end in this source's local coordinates. The global end minus
        // the file's global base; saturating because a span may reach past the
        // file, in which case the carets stop at the line's end.
        let local_end = (span
            .end()
            .to_u32()
            .saturating_sub(file.span().start().to_u32())) as usize;

        // Offsets relative to the line, clamped into it. A multi-line span is
        // clamped to this first line — full multi-line frames arrive in v0.3.0.
        let start_byte = floor_boundary(line_text, local_start.to_usize().saturating_sub(line_lo));
        let end_byte = floor_boundary(line_text, local_end.saturating_sub(line_lo)).max(start_byte);

        Some(Self {
            name: file.name(),
            line_no: lc.line,
            col: lc.col,
            line_text,
            start_byte,
            end_byte,
        })
    }

    /// Writes the location line, the source line, and the caret line.
    fn write(&self, out: &mut impl fmt::Write, label: &str) -> fmt::Result {
        let gutter = decimal_width(self.line_no);

        // Location: `<pad>--> name:line:col`.
        writeln!(
            out,
            "{:gutter$}--> {}:{}:{}",
            "", self.name, self.line_no, self.col
        )?;

        // Blank rib, then the numbered source line with tabs expanded so the
        // carets below line up under it.
        writeln!(out, "{:gutter$} |", "")?;
        let mut expanded = String::new();
        expand_tabs(self.line_text, &mut expanded);
        writeln!(out, "{:>gutter$} | {}", self.line_no, expanded)?;

        // Caret line: lead in to the span's column, then a run of carets, then the
        // label. An empty or zero-width span still draws one caret as a pointer.
        let lead = visual_width(&self.line_text[..self.start_byte]);
        let span_width = visual_width(&self.line_text[..self.end_byte]).saturating_sub(lead);
        let carets = span_width.max(1);

        write!(out, "{:gutter$} | {:lead$}", "", "")?;
        for _ in 0..carets {
            out.write_char('^')?;
        }
        if label.is_empty() {
            out.write_char('\n')?;
        } else {
            writeln!(out, " {label}")?;
        }

        // Closing rib.
        writeln!(out, "{:gutter$} |", "")
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
/// model the caret line is aligned in.
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
        // Two characters, four bytes — two columns.
        assert_eq!(visual_width("αβ"), 2);
        assert_eq!(visual_width("abc"), 3);
    }

    #[test]
    fn test_visual_width_expands_tabs_to_stops() {
        assert_eq!(visual_width("\t"), 4);
        assert_eq!(visual_width("a\t"), 4); // one char then a tab to column 4
        assert_eq!(visual_width("abcd\t"), 8); // tab from column 4 to 8
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
        // 'α' is bytes 0..2; index 1 floors to 0.
        assert_eq!(floor_boundary("αβ", 1), 0);
        assert_eq!(floor_boundary("αβ", 2), 2);
        // Past the end clamps to the length.
        assert_eq!(floor_boundary("ab", 9), 2);
    }
}
