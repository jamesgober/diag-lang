//! Turning a [`Diagnostic`] into caret-annotated text.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use source_lang::{SourceFile, SourceId, SourceMap};
use span_lang::{BytePos, LineCol, LineIndex, Span};

use crate::{Diagnostic, Label};

/// The number of columns a tab advances to. A tab moves the cursor to the next
/// multiple of this width, the convention source viewers use, so the caret line
/// can be aligned without knowing the reader's terminal tab settings.
const TAB_WIDTH: usize = 4;

/// The ANSI styling applied to each part of a frame.
///
/// Every field is an SGR escape prefix that pairs with [`reset`](Palette::reset);
/// the [`PLAIN`](Palette::PLAIN) palette leaves them all empty, so the writers
/// produce byte-identical uncoloured output without a separate code path.
struct Palette {
    /// The severity word in the header.
    header: &'static str,
    /// The `-->`, the `|` rib, and the line numbers.
    gutter: &'static str,
    /// The primary `^` underline and its message.
    primary: &'static str,
    /// The secondary `-` underline and its message.
    secondary: &'static str,
    /// The `note:` marker.
    note: &'static str,
    /// The `help:` marker.
    help: &'static str,
    /// Resets styling back to the terminal default.
    reset: &'static str,
}

impl Palette {
    /// No styling: every part is rendered as plain text.
    const PLAIN: Palette = Palette {
        header: "",
        gutter: "",
        primary: "",
        secondary: "",
        note: "",
        help: "",
        reset: "",
    };

    /// The palette for a coloured frame at `severity`.
    #[cfg(feature = "color")]
    fn coloured(severity: crate::Severity) -> Palette {
        use crate::Severity;
        // Bold colours: the error level drives the header and primary underline.
        let sev = match severity {
            Severity::Error => "\x1b[1;31m",   // red
            Severity::Warning => "\x1b[1;33m", // yellow
            Severity::Note => "\x1b[1;32m",    // green
            Severity::Help => "\x1b[1;36m",    // cyan
        };
        Palette {
            header: sev,
            gutter: "\x1b[1;34m", // blue
            primary: sev,
            secondary: "\x1b[1;34m", // blue
            note: "\x1b[1m",         // bold, default colour
            help: "\x1b[1;36m",      // cyan
            reset: "\x1b[0m",
        }
    }

    /// The styling for an underline of the given kind.
    #[inline]
    const fn marker(&self, primary: bool) -> &'static str {
        if primary {
            self.primary
        } else {
            self.secondary
        }
    }
}

/// Renders a [`Diagnostic`] against a [`SourceMap`] into aligned, caret-annotated
/// text in the style of a modern toolchain.
///
/// The renderer reads the spans a diagnostic points at, finds the sources and
/// lines they fall on through the map, and prints those lines with underlines
/// placed exactly under the offending characters — `^` for the primary label,
/// `-` for secondary labels. Alignment is computed in terminal display columns:
/// a character occupies its Unicode display width (one column for most text, two
/// for CJK ideographs and wide emoji, zero for combining marks), and a tab expands
/// to the next four-column tab stop, so the underlines line up under the source
/// whatever it contains.
///
/// # Layout
///
/// Output begins with the `severity: message` header. Then, for each source file
/// any label points into — the file holding the primary label first, the rest in
/// the order they were added to the map — a frame is drawn: a `--> file:line:col`
/// location line and the labelled lines of that file, each shown once with every
/// label's underline stacked beneath it. A span covering several lines underlines
/// each line it touches, with the message on the last; a span covering more than
/// five lines shows only its first two and last two, with a `...` line marking the
/// elided middle. Trailing `note:` and `help:` lines, if any, close the output.
///
/// A zero-width label at the very end of a source (where a "found end of input"
/// error points) renders in that source's frame wherever the map attributes the
/// position to it (a single source, or the most recently added one), with the
/// caret just after the last character of its last line. A label whose span falls outside the loaded
/// sources is not dropped and does not panic: it renders as a `note:` line
/// reporting the offending span. Output is deterministic — the same diagnostic
/// and map always render byte-identical text — and always ends with a newline.
///
/// # Untrusted text
///
/// Source text, messages, labels, notes, help, and file names are written with
/// every control character escaped visibly as `\u{..}`: C0 and C1 controls
/// (including `ESC`, so no ANSI/OSC sequence can reach the terminal), `DEL`, a
/// carriage return, the Unicode line and paragraph separators, and the bidi
/// override and isolate controls. Tabs are kept (expanded to spaces in source
/// lines), and newlines are kept in messages, notes, and help, where they are
/// layout. The only escape sequences in the output are the renderer's own colours.
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
pub struct Renderer {
    /// Whether to wrap output in ANSI styling. Only present, and only ever true,
    /// with the `color` feature; the default is plain.
    #[cfg(feature = "color")]
    color: bool,
}

impl Renderer {
    /// Creates a renderer with the default layout, producing plain (uncoloured)
    /// output.
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
        Self::default()
    }

    /// Enables ANSI styling: the severity word, the underlines, and the frame are
    /// coloured for a terminal.
    ///
    /// Colour is additive — the layout, alignment, and text are identical to the
    /// plain output, only wrapped in ANSI escape codes. Enable it when writing to a
    /// terminal; leave it off (the default) when writing to a file, a string, or
    /// any sink that does not interpret the codes. The error level drives the
    /// colours: red for an error, yellow for a warning, green for a note, cyan for
    /// help.
    ///
    /// Requires the `color` feature.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Label, Renderer, Severity, SourceMap, Span};
    ///
    /// let mut map = SourceMap::new();
    /// map.add("m.rs", "let x = 1;").expect("fits");
    /// let diag = Diagnostic::new(
    ///     Severity::Error,
    ///     "boom",
    ///     Label::new(Span::new(4, 5), "here"),
    /// );
    ///
    /// let coloured = Renderer::new().with_color().render(&diag, &map);
    /// assert!(coloured.contains('\u{1b}')); // contains ANSI escapes
    /// ```
    #[cfg(feature = "color")]
    #[cfg_attr(docsrs, doc(cfg(feature = "color")))]
    #[inline]
    #[must_use]
    pub fn with_color(mut self) -> Self {
        self.color = true;
        self
    }

    /// The palette to render with: coloured for `severity` when styling is on,
    /// plain otherwise.
    #[cfg(feature = "color")]
    fn palette(&self, severity: crate::Severity) -> Palette {
        if self.color {
            Palette::coloured(severity)
        } else {
            Palette::PLAIN
        }
    }

    /// The palette to render with. Without the `color` feature there is only the
    /// plain one.
    #[cfg(not(feature = "color"))]
    fn palette(&self, _severity: crate::Severity) -> Palette {
        Palette::PLAIN
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
        let palette = self.palette(diag.severity());

        writeln!(
            out,
            "{h}{}{r}: {}",
            diag.severity().as_str(),
            Clean::text(diag.message()),
            h = palette.header,
            r = palette.reset
        )?;

        // Split every label into the source it resolves against, keeping any that
        // resolve nowhere for a trailing note.
        let mut located: Vec<Located<'_>> = Vec::new();
        let mut unlocated: Vec<&Label> = Vec::new();
        for (primary, label) in labels(diag) {
            match resolve(map, label.span()) {
                Some((id, start, end)) => located.push(Located {
                    id,
                    start,
                    end,
                    primary,
                    message: label.message(),
                }),
                None => unlocated.push(label),
            }
        }

        // One frame per file: the primary's file first, the rest in map order.
        // A stable sort keeps each file's labels in the order they were given, so
        // grouping is one pass however many files and labels there are.
        let primary_file = located.iter().find(|l| l.primary).map(|l| l.id);
        located.sort_by_key(|l| (Some(l.id) != primary_file, l.id.to_u32()));
        for group in located.chunk_by(|a, b| a.id == b.id) {
            if let Some(file) = map.source(group[0].id) {
                write_frame(out, map, file, group, &palette)?;
            }
        }

        for label in &unlocated {
            writeln!(
                out,
                " = {n}note:{r} span {} is outside the loaded sources",
                label.span(),
                n = palette.note,
                r = palette.reset
            )?;
        }
        for note in diag.notes() {
            writeln!(
                out,
                " = {n}note:{r} {}",
                Clean::text(note),
                n = palette.note,
                r = palette.reset
            )?;
        }
        for help in diag.help() {
            writeln!(
                out,
                " = {h}help:{r} {}",
                Clean::text(help),
                h = palette.help,
                r = palette.reset
            )?;
        }

        Ok(())
    }
}

/// The primary label paired with `true`, then every secondary paired with
/// `false`, in the order they were added.
fn labels(diag: &Diagnostic) -> impl Iterator<Item = (bool, &Label)> {
    core::iter::once((true, diag.primary())).chain(diag.secondary().iter().map(|l| (false, l)))
}

/// A label resolved to its source: local byte offsets into that source's text,
/// already clamped to it.
struct Located<'a> {
    id: SourceId,
    start: usize,
    end: usize,
    primary: bool,
    message: &'a str,
}

/// Resolves a global span to the source its start belongs to and local offsets
/// into that source's text, the end clamped to the text.
///
/// The start position decides the source, exactly as [`SourceMap::locate`]
/// resolves it. Since source-lang 1.0.1 that includes the end of the last
/// source, so a zero-width end-of-input span there lands in that source. At an
/// interior boundary the position is the next source's first byte and lands
/// there (source-lang's documented 1.x limitation, fixed by its 2.0 layout).
fn resolve(map: &SourceMap, span: Span) -> Option<(SourceId, usize, usize)> {
    let (id, local) = map.locate(span.start())?;
    let file = map.source(id)?;
    let base = file.span().start().to_u32();
    let end = (span.end().to_u32().saturating_sub(base) as usize).min(file.text().len());
    Some((id, local.to_usize(), end))
}

/// The local offset an end-of-input caret is drawn at.
///
/// A zero-width label at the very end of a text that ends with a line break
/// would otherwise land on the phantom empty line after the break. It is drawn
/// instead just after the last character of the last real line, where the
/// reader sees the input stop: on the break itself (`\n`, or the `\r` of
/// `\r\n`). Every other position is returned unchanged.
fn eof_anchor(text: &str, at: usize) -> usize {
    if at != text.len() {
        return at;
    }
    let bytes = text.as_bytes();
    match bytes {
        [.., b'\r', b'\n'] => at - 2,
        [.., b'\n'] => at - 1,
        _ => at,
    }
}

/// One label's underline on one line: where the carets sit and what they say.
struct Mark<'a> {
    line: u32,
    /// Byte range of the line's content in the source text, terminator excluded.
    lo: usize,
    hi: usize,
    lead: usize,
    width: usize,
    /// The message, present only on the label's last covered line.
    message: Option<&'a str>,
    /// Primary labels draw `^` and sort ahead of secondary ones on a line.
    primary: bool,
}

/// A label's covered line range, for deciding which gaps between shown lines
/// are elisions inside a span (drawn as `...`) rather than unrelated distance.
#[derive(Clone, Copy)]
struct Cover {
    first: u32,
    last: u32,
}

/// A span covering more than this many lines is elided: only its first
/// [`ELIDE_CONTEXT`] and last [`ELIDE_CONTEXT`] lines are shown. Eliding fewer
/// than two lines would print a `...` in place of a single line, which saves
/// nothing, hence `2 * ELIDE_CONTEXT + 1`.
const ELIDE_ABOVE: u32 = 2 * ELIDE_CONTEXT + 1;

/// Lines shown at each end of an elided span.
const ELIDE_CONTEXT: u32 = 2;

/// Renders the frame for one source: its location line and every line any of
/// its labels touches (long spans elided), with the underlines stacked beneath
/// each.
///
/// Cost is bounded by what is printed plus `O(log lines)` per label: no line
/// index is built over the file. Line numbers come from the map's cached line
/// table ([`SourceMap::line_col`]), and line boundaries are found by scanning
/// outward from the label's own position, so a frame over a 50,000-line file
/// costs no more than one over a 5-line file.
fn write_frame(
    out: &mut impl fmt::Write,
    map: &SourceMap,
    file: &SourceFile,
    labels: &[Located<'_>],
    palette: &Palette,
) -> fmt::Result {
    let text = file.text();

    // Resolve each label into per-line marks, and remember the anchor (the
    // primary if it is here, else the earliest label) for the location line.
    let mut marks: Vec<Mark<'_>> = Vec::new();
    let mut covers: Vec<Cover> = Vec::with_capacity(labels.len());
    let mut anchor: Option<(u32, u32)> = None;
    let mut anchor_is_primary = false;
    for label in labels {
        let start = eof_anchor(text, label.start);
        let end = if label.end == label.start {
            start
        } else {
            label.end
        };
        let lc = line_col_at(map, file, start);

        let here = (lc.line, lc.col);
        let better = match anchor {
            None => true,
            Some(_) if label.primary && !anchor_is_primary => true,
            Some(_) if !label.primary && anchor_is_primary => false,
            Some(a) => here < a,
        };
        if better {
            anchor = Some(here);
            anchor_is_primary = label.primary;
        }

        let cover = push_marks(&mut marks, map, file, start, end, lc.line, label);
        covers.push(cover);
    }

    let Some((anchor_line, anchor_col)) = anchor else {
        return Ok(());
    };

    // Group marks by line once: line ascending, then left to right, primary ahead
    // of secondary at the same column. The sort is stable, so equal marks keep
    // label order and the output stays deterministic.
    marks.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.lead.cmp(&b.lead))
            .then(b.primary.cmp(&a.primary))
    });
    covers.sort_unstable_by_key(|c| c.first);

    // The gutter is as wide as the largest line number the frame shows.
    let max_line = marks.last().map_or(anchor_line, |m| m.line);
    let gw = decimal_width(max_line);

    let g = palette.gutter;
    let r = palette.reset;

    writeln!(
        out,
        "{:gw$}{g}-->{r} {}:{anchor_line}:{anchor_col}",
        "",
        Clean::name(file.name())
    )?;
    writeln!(out, "{:gw$} {g}|{r}", "")?;

    // Sweep the covers alongside the lines: `reach` is the furthest last line of
    // any span starting at or before the current line, so a gap to the next shown
    // line is an elision inside a span exactly when `reach` spans it.
    let mut next_cover = 0;
    let mut reach = 0u32;
    let mut prev_line: Option<u32> = None;
    for row in marks.chunk_by(|a, b| a.line == b.line) {
        let first = &row[0];
        let line = first.line;

        if let Some(prev) = prev_line {
            while next_cover < covers.len() && covers[next_cover].first <= prev {
                reach = reach.max(covers[next_cover].last);
                next_cover += 1;
            }
            if line > prev + 1 && reach >= line {
                writeln!(out, "{g}...{r}")?;
            }
        }
        prev_line = Some(line);

        let line_text = text.get(first.lo..first.hi).unwrap_or("");
        write!(out, "{g}{line:>gw$}{r} {g}|{r} ")?;
        write_source_line(out, line_text)?;
        out.write_char('\n')?;

        for mark in row {
            let mc = palette.marker(mark.primary);
            let marker = if mark.primary { '^' } else { '-' };
            write!(out, "{:gw$} {g}|{r} {:lead$}{mc}", "", "", lead = mark.lead)?;
            for _ in 0..mark.width {
                out.write_char(marker)?;
            }
            match mark.message {
                Some(message) => writeln!(out, " {}{r}", Clean::text(message))?,
                None => writeln!(out, "{r}")?,
            }
        }
    }

    writeln!(out, "{:gw$} {g}|{r}", "")
}

/// Computes the underline marks for one label (local `start..end` in `file`,
/// starting on 1-based `first_line`) and appends them to `marks`. A span over at
/// most [`ELIDE_ABOVE`] lines gets a mark on every line; a longer one only on its
/// first and last [`ELIDE_CONTEXT`] lines. Returns the label's covered lines.
///
/// Only the lines that get a mark are scanned, so the cost is the length of the
/// shown lines plus two `O(log lines)` lookups, never the length of the span.
fn push_marks<'a>(
    marks: &mut Vec<Mark<'a>>,
    map: &SourceMap,
    file: &SourceFile,
    start: usize,
    end: usize,
    first_line: u32,
    label: &Located<'a>,
) -> Cover {
    let text = file.text();
    let first_start = line_start_at(text, start);

    // The last line the span touches: the line of its final covered byte, or the
    // start line for a zero-width span.
    let (last_line, last_start) = if end > start {
        let last_byte = floor_boundary(text, end - 1);
        (
            line_col_at(map, file, last_byte).line,
            line_start_at(text, last_byte),
        )
    } else {
        (first_line, first_start)
    };

    let mut push = |line: u32, lo: usize| {
        let hi = line_content_end(text, lo);
        let line_text = text.get(lo..hi).unwrap_or("");

        // Intersect the span with this line's content, then express it as offsets
        // into the line, floored to character boundaries.
        let seg_start = start.max(lo);
        let seg_end = end.min(hi).max(seg_start);
        let a = floor_boundary(line_text, seg_start - lo);
        let b = floor_boundary(line_text, seg_end - lo).max(a);
        let (lead, upto) = display_cols(line_text, a, b);

        marks.push(Mark {
            line,
            lo,
            hi,
            lead,
            width: (upto - lead).max(1),
            message: (line == last_line && !label.message.is_empty()).then_some(label.message),
            primary: label.primary,
        });
    };

    let count = last_line - first_line + 1;
    let head = if count > ELIDE_ABOVE {
        ELIDE_CONTEXT
    } else {
        count
    };

    // Walk forward from the first line for the head (every line when not elided).
    let mut lo = first_start;
    for k in 0..head {
        push(first_line + k, lo);
        lo = next_line_start(text, lo);
    }

    // Walk backward from the last line for the tail of an elided span.
    if count > ELIDE_ABOVE {
        let mut starts = [last_start; ELIDE_CONTEXT as usize];
        for i in (0..starts.len() - 1).rev() {
            // `starts[i + 1]` begins a line after the first, so it is past a `\n`.
            starts[i] = line_start_at(text, starts[i + 1].saturating_sub(1));
        }
        for (k, &lo) in starts.iter().enumerate() {
            push(last_line + 1 - ELIDE_CONTEXT + k as u32, lo);
        }
    }

    Cover {
        first: first_line,
        last: last_line,
    }
}

/// The 1-based line and character column of local offset `at` in `file`.
///
/// Answered by [`SourceMap::line_col`] on the global position, which source-lang
/// 1.0.1 serves from a line table built once per source (`O(log lines)`). The
/// renderer only asks for offsets `locate` attributed to `file` (inside its
/// text, or its end when it is the last source), so the answer is for this file;
/// the fallback rebuilds an index and is unreachable while that holds, but keeps
/// the result right regardless (an end offset of a non-last source would map to
/// the next source).
fn line_col_at(map: &SourceMap, file: &SourceFile, at: usize) -> LineCol {
    let text = file.text();
    let at = floor_boundary(text, at);
    let global = file.span().start().to_u32().saturating_add(at as u32);
    match map.line_col(BytePos::new(global)) {
        Some((id, lc)) if map.source(id).is_some_and(|f| core::ptr::eq(f, file)) => lc,
        _ => LineIndex::new(text).line_col(BytePos::new(at as u32)),
    }
}

/// The byte offset at which the line containing `at` begins.
fn line_start_at(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    text.as_bytes()[..at]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1)
}

/// The byte offset just after the line break that ends the line starting at
/// `lo`, or the text's end for the last line.
fn next_line_start(text: &str, lo: usize) -> usize {
    let lo = lo.min(text.len());
    text.as_bytes()[lo..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(text.len(), |i| lo + i + 1)
}

/// The end of the content of the line starting at `lo`: before its `\n`, and
/// before a `\r` directly preceding that `\n`. A final line without a `\n` keeps
/// everything, a trailing lone `\r` included. This is `span-lang`'s
/// `LineIndex::line_span` model, so the shown text is unchanged from 1.0.0.
fn line_content_end(text: &str, lo: usize) -> usize {
    let lo = lo.min(text.len());
    let bytes = text.as_bytes();
    match bytes[lo..].iter().position(|&b| b == b'\n') {
        Some(i) => {
            let end = lo + i;
            if end > lo && bytes[end - 1] == b'\r' {
                end - 1
            } else {
                end
            }
        }
        None => text.len(),
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

/// Whether `ch` must be written as a visible `\u{..}` escape rather than raw.
///
/// The set is everything that can drive a terminal or disguise text: the C0
/// controls (so `ESC`, and with it every ANSI/OSC sequence, `BEL`, backspace,
/// and a carriage return that would overwrite the line), `DEL`, the C1 controls
/// (which include the 8-bit `CSI`/`OSC`), the Unicode line and paragraph
/// separators, and the bidi embedding, override, isolate, and mark controls that
/// reorder how a line displays. A tab is never escaped; a newline is escaped
/// only where it is not layout (`keep_newline` false).
const fn needs_escape(ch: char, keep_newline: bool) -> bool {
    match ch {
        '\t' => false,
        '\n' => !keep_newline,
        '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => true,
        '\u{2028}' | '\u{2029}' => true,
        '\u{61c}' | '\u{200e}' | '\u{200f}' => true,
        '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => true,
        _ => false,
    }
}

/// Writes the visible escape for `ch`: `\u{` + lowercase hex + `}`.
fn write_escape(out: &mut impl fmt::Write, ch: char) -> fmt::Result {
    write!(out, "\\u{{{:x}}}", u32::from(ch))
}

/// The number of columns [`write_escape`] uses for `ch`: the five fixed
/// characters `\u{}` plus one per hex digit.
const fn escape_width(ch: char) -> usize {
    let mut n = ch as u32;
    let mut digits = 1;
    while n >= 16 {
        n /= 16;
        digits += 1;
    }
    4 + digits
}

/// The display column reached after drawing `ch` starting at `col`: a tab
/// advances to the next [`TAB_WIDTH`] stop, an escaped control by the width of
/// its escape, and anything else by its Unicode display width (0, 1, or 2).
///
/// This one function is the column model: [`write_source_line`] draws with it and
/// [`display_cols`] measures with it, so the carets cannot drift from the text.
fn advance(col: usize, ch: char) -> usize {
    if ch == '\t' {
        col + TAB_WIDTH - (col % TAB_WIDTH)
    } else if needs_escape(ch, false) {
        col + escape_width(ch)
    } else {
        col + unicode_lang::char_width(ch)
    }
}

/// The display columns of byte offsets `a <= b` within `line`, both on character
/// boundaries, measured in one pass.
fn display_cols(line: &str, a: usize, b: usize) -> (usize, usize) {
    // Fast path: in printable ASCII every byte is one column.
    if line.get(..b).is_some_and(is_plain_ascii) {
        return (a, b);
    }
    let mut col = 0;
    let mut at_a = 0;
    for (i, ch) in line.char_indices() {
        if i == a {
            at_a = col;
        }
        if i >= b {
            return (at_a, col);
        }
        col = advance(col, ch);
    }
    // `a` or `b` is the line's end.
    if a >= line.len() {
        at_a = col;
    }
    (at_a, col)
}

/// Whether `s` is printable ASCII only (`' '..='~'`): nothing to escape or
/// expand, and one column per byte. The common case, checked a word at a time
/// by the optimiser, so the general per-character paths run only when needed.
fn is_plain_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// Writes one source line as it is displayed: tabs expanded to spaces, control
/// characters escaped, everything else verbatim. It occupies exactly the columns
/// [`advance`] counts. Safe runs are written in one call each, not per character.
fn write_source_line(out: &mut impl fmt::Write, line: &str) -> fmt::Result {
    if is_plain_ascii(line) {
        return out.write_str(line);
    }
    let mut col = 0;
    let mut run = 0;
    for (i, ch) in line.char_indices() {
        let next = advance(col, ch);
        if ch == '\t' {
            out.write_str(&line[run..i])?;
            write!(out, "{:w$}", "", w = next - col)?;
            run = i + 1;
        } else if needs_escape(ch, false) {
            out.write_str(&line[run..i])?;
            write_escape(out, ch)?;
            run = i + ch.len_utf8();
        }
        col = next;
    }
    out.write_str(&line[run..])
}

/// A string written with its control characters escaped (see [`needs_escape`]).
/// Used for every piece of caller-supplied text outside the source lines.
struct Clean<'a> {
    text: &'a str,
    keep_newline: bool,
}

impl<'a> Clean<'a> {
    /// A message, label, note, or help text: newlines are layout and kept.
    const fn text(text: &'a str) -> Self {
        Self {
            text,
            keep_newline: true,
        }
    }

    /// A file name: one line by definition, so a newline is escaped too.
    const fn name(text: &'a str) -> Self {
        Self {
            text,
            keep_newline: false,
        }
    }
}

impl fmt::Display for Clean<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if is_plain_ascii(self.text) {
            return f.write_str(self.text);
        }
        let mut run = 0;
        for (i, ch) in self.text.char_indices() {
            if needs_escape(ch, self.keep_newline) {
                f.write_str(&self.text[run..i])?;
                write_escape(f, ch)?;
                run = i + ch.len_utf8();
            }
        }
        f.write_str(&self.text[run..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The display width of a whole line under the renderer's column model.
    fn visual_width(s: &str) -> usize {
        display_cols(s, s.len(), s.len()).1
    }

    fn drawn(s: &str) -> String {
        let mut out = String::new();
        write_source_line(&mut out, s).unwrap();
        out
    }

    #[test]
    fn test_decimal_width_counts_digits() {
        assert_eq!(decimal_width(0), 1);
        assert_eq!(decimal_width(9), 1);
        assert_eq!(decimal_width(10), 2);
        assert_eq!(decimal_width(999), 3);
        assert_eq!(decimal_width(1000), 4);
    }

    #[test]
    fn test_visual_width_counts_narrow_chars_as_one() {
        assert_eq!(visual_width("αβ"), 2);
        assert_eq!(visual_width("abc"), 3);
    }

    #[test]
    fn test_visual_width_uses_display_width() {
        // CJK is two columns, a combining mark zero, an emoji two.
        assert_eq!(visual_width("世界"), 4);
        assert_eq!(visual_width("e\u{301}"), 1);
        assert_eq!(visual_width("😀"), 2);
    }

    #[test]
    fn test_visual_width_expands_tabs_to_stops() {
        assert_eq!(visual_width("\t"), 4);
        assert_eq!(visual_width("a\t"), 4);
        assert_eq!(visual_width("abcd\t"), 8);
        assert_eq!(visual_width("\t\t"), 8);
        // A tab stop counts display columns: one wide char plus a tab is 4.
        assert_eq!(visual_width("世\t"), 4);
    }

    #[test]
    fn test_drawn_line_occupies_exactly_its_visual_width() {
        for s in [
            "",
            "a",
            "\t",
            "a\tb",
            "\tx\t",
            "abcd\te",
            "\u{1b}[2J",
            "a\rb",
            "世\t界",
        ] {
            let out = drawn(s);
            assert!(!out.contains('\t'), "for {s:?}");
            assert!(!out.contains('\u{1b}'), "for {s:?}");
            assert_eq!(
                out.chars().map(unicode_lang::char_width).sum::<usize>(),
                visual_width(s),
                "for {s:?}"
            );
        }
    }

    #[test]
    fn test_controls_are_escaped_visibly() {
        assert_eq!(drawn("a\u{1b}[31mb"), "a\\u{1b}[31mb");
        assert_eq!(drawn("x\ry"), "x\\u{d}y");
        assert_eq!(drawn("\u{9b}"), "\\u{9b}");
        assert_eq!(drawn("\u{202e}"), "\\u{202e}");
        assert_eq!(escape_width('\u{1b}'), "\\u{1b}".len());
        assert_eq!(escape_width('\u{202e}'), "\\u{202e}".len());
        assert_eq!(escape_width('\0'), "\\u{0}".len());
    }

    #[test]
    fn test_clean_keeps_newlines_only_in_text() {
        let text = alloc::format!("{}", Clean::text("a\nb\u{7}"));
        assert_eq!(text, "a\nb\\u{7}");
        let name = alloc::format!("{}", Clean::name("a\nb"));
        assert_eq!(name, "a\\u{a}b");
        let plain = alloc::format!("{}", Clean::text("tab\tok"));
        assert_eq!(plain, "tab\tok");
    }

    #[test]
    fn test_floor_boundary_moves_into_char_start() {
        assert_eq!(floor_boundary("αβ", 1), 0);
        assert_eq!(floor_boundary("αβ", 2), 2);
        assert_eq!(floor_boundary("ab", 9), 2);
    }

    #[test]
    fn test_line_scans_follow_the_line_index_model() {
        let text = "a\r\nbc\nlast\r";
        assert_eq!(line_start_at(text, 0), 0);
        assert_eq!(line_start_at(text, 4), 3);
        assert_eq!(line_start_at(text, text.len()), 6);
        assert_eq!(next_line_start(text, 0), 3);
        assert_eq!(next_line_start(text, 6), text.len());
        // `\r\n` loses both bytes; a final lone `\r` is content.
        assert_eq!(line_content_end(text, 0), 1);
        assert_eq!(line_content_end(text, 3), 5);
        assert_eq!(line_content_end(text, 6), text.len());
        let index = LineIndex::new(text);
        for (line, lo) in [(1, 0), (2, 3), (3, 6)] {
            let span = index.line_span(line).unwrap();
            assert_eq!(
                (span.start().to_usize(), span.end().to_usize()),
                (lo, line_content_end(text, lo))
            );
        }
    }

    #[test]
    fn test_eof_anchor_steps_back_over_the_final_break() {
        assert_eq!(eof_anchor("abc", 3), 3);
        assert_eq!(eof_anchor("abc\n", 4), 3);
        assert_eq!(eof_anchor("abc\r\n", 5), 3);
        assert_eq!(eof_anchor("", 0), 0);
        assert_eq!(eof_anchor("\n", 1), 0);
        // Not at the end: unchanged.
        assert_eq!(eof_anchor("abc\n", 2), 2);
    }
}
