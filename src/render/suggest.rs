//! Rendering fix-it suggestions as `help:` blocks that show the source the way
//! it would read with the fix applied.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use source_lang::{SourceFile, SourceId, SourceMap};

use super::{
    Clean, ELIDE_ABOVE, ELIDE_CONTEXT, Palette, decimal_width, display_cols, line_col_at,
    line_content_end, line_start_at, write_source_line,
};
use crate::apply::{Planned, plan};
use crate::{Edit, Suggestion};

/// Writes one suggestion: its `help:` line, then for each source its edits
/// touch, a block showing the edited lines.
///
/// A block uses one of two styles, as rustc does:
///
/// - **Underline** (no edit deletes text without replacing it): the lines as
///   they read after the fix, with `+` under inserted text and `~` under
///   replacement text.
/// - **Diff** (some edit only deletes): the lines before the fix marked `-`,
///   then the lines after it marked `+`, because a deletion leaves nothing to
///   underline.
///
/// A block over more than five lines shows its first two and last two with
/// `...` between. A block in a different source from the primary label starts
/// with a `--> file:line:col` line. If any edit cannot be placed (outside the
/// loaded sources, splitting a character, or overlapping another edit), only
/// the `help:` line is written: the renderer never guesses at a fix.
pub(super) fn write_suggestion(
    out: &mut impl fmt::Write,
    map: &SourceMap,
    suggestion: &Suggestion,
    primary_file: Option<SourceId>,
    palette: &Palette,
) -> fmt::Result {
    writeln!(
        out,
        "{h}help:{r} {}",
        Clean::text(suggestion.message()),
        h = palette.help,
        r = palette.reset
    )?;
    let Some(groups) = group_edits(map, suggestion.edits(), primary_file) else {
        return Ok(());
    };
    for (id, file, edits) in &groups {
        write_block(out, map, file, edits, Some(*id) != primary_file, palette)?;
    }
    Ok(())
}

/// The edits grouped by the source their start falls in (the primary label's
/// source first, the rest in map order) and validated per source, or `None` if
/// any edit cannot be located or any group is invalid.
fn group_edits<'a>(
    map: &'a SourceMap,
    edits: &'a [Edit],
    primary_file: Option<SourceId>,
) -> Option<Vec<(SourceId, &'a SourceFile, Vec<Planned<'a>>)>> {
    let mut located: Vec<(SourceId, &Edit)> = Vec::with_capacity(edits.len());
    for edit in edits {
        let (id, _) = map.locate(edit.span().start())?;
        located.push((id, edit));
    }
    located.sort_by_key(|(id, _)| (Some(*id) != primary_file, id.to_u32()));

    let mut groups = Vec::new();
    for group in located.chunk_by(|a, b| a.0 == b.0) {
        let (id, _) = *group.first()?;
        let file = map.source(id)?;
        let base = file.span().start().to_u32();
        let planned = plan(file.text(), base, group.iter().map(|&(_, edit)| edit)).ok()?;
        if !planned.is_empty() {
            groups.push((id, file, planned));
        }
    }
    Some(groups)
}

/// Where one edit's text landed in the rewritten region.
struct Placed {
    start: usize,
    end: usize,
    /// An insertion draws `+`; a replacement draws `~`.
    insertion: bool,
}

/// Writes the block for one source's edits (sorted, non-overlapping, at least
/// one).
fn write_block(
    out: &mut impl fmt::Write,
    map: &SourceMap,
    file: &SourceFile,
    edits: &[Planned<'_>],
    show_location: bool,
    palette: &Palette,
) -> fmt::Result {
    let text = file.text();
    let (Some(first), Some(last)) = (edits.first(), edits.last()) else {
        return Ok(());
    };
    // The whole lines the edits touch. Sorted and non-overlapping, so the last
    // edit ends furthest.
    let lo = line_start_at(text, first.start);
    let hi = line_content_end(text, line_start_at(text, last.end)).max(last.end);
    let first_line = line_col_at(map, file, lo).line;
    let old = text.get(lo..hi).unwrap_or("");

    // The region as it reads after the fix, and where each edit's text landed.
    let mut new =
        String::with_capacity(old.len() + edits.iter().map(|e| e.text.len()).sum::<usize>());
    let mut placed: Vec<Placed> = Vec::with_capacity(edits.len());
    let mut at = lo;
    for edit in edits {
        new.push_str(text.get(at..edit.start).unwrap_or(""));
        let start = new.len();
        new.push_str(edit.text);
        placed.push(Placed {
            start,
            end: new.len(),
            insertion: edit.start == edit.end,
        });
        at = edit.end;
    }
    new.push_str(text.get(at..hi).unwrap_or(""));

    let diff = edits.iter().any(|e| e.start < e.end && e.text.is_empty());
    let mut old_lines = line_ranges(old);
    let mut new_lines = line_ranges(&new);
    let mut first_line = first_line;
    if diff {
        // Show only the lines that differ: deleting a whole line pulls the
        // following line into the region, but that line reads the same after.
        let skipped = trim_common(old, &mut old_lines, &new, &mut new_lines);
        first_line = line_number(first_line, skipped);
    }
    let tallest = if diff {
        old_lines.len().max(new_lines.len())
    } else {
        new_lines.len()
    };
    let gw = decimal_width(line_number(first_line, tallest.saturating_sub(1)));

    let (g, r) = (palette.gutter, palette.reset);
    if show_location {
        let lc = line_col_at(map, file, first.start);
        writeln!(
            out,
            "{:gw$}{g}-->{r} {}:{}:{}",
            "",
            Clean::name(file.name()),
            lc.line,
            lc.col
        )?;
    }
    writeln!(out, "{:gw$} {g}|{r}", "")?;
    let block = Block {
        first_line,
        gw,
        palette,
    };
    if diff {
        block.write_lines(out, old, &old_lines, '-', None)?;
        block.write_lines(out, &new, &new_lines, '+', None)?;
    } else {
        block.write_lines(out, &new, &new_lines, '|', Some(&placed))?;
    }
    writeln!(out, "{:gw$} {g}|{r}", "")
}

/// The fixed layout of one block: where its numbering starts, its gutter
/// width, and its colours.
struct Block<'p> {
    first_line: u32,
    gw: usize,
    palette: &'p Palette,
}

impl Block<'_> {
    /// Writes the lines of `text` (given as content ranges), numbered from the
    /// block's first line, with `sign` after the number. Long runs are elided
    /// to their first and last two lines. With `marks`, a marker line follows
    /// each line that holds inserted or replacement text.
    fn write_lines(
        &self,
        out: &mut impl fmt::Write,
        text: &str,
        lines: &[(usize, usize)],
        sign: char,
        marks: Option<&[Placed]>,
    ) -> fmt::Result {
        let (g, r) = (self.palette.gutter, self.palette.reset);
        let gw = self.gw;
        let sign_colour = if sign == '|' { g } else { self.palette.help };
        let n = lines.len();
        let elide = n > ELIDE_ABOVE as usize;
        let context = ELIDE_CONTEXT as usize;
        for (idx, &(a, b)) in lines.iter().enumerate() {
            if elide && idx >= context && idx < n - context {
                if idx == context {
                    writeln!(out, "{g}...{r}")?;
                }
                continue;
            }
            let line = text.get(a..b).unwrap_or("");
            let number = line_number(self.first_line, idx);
            write!(out, "{g}{number:>gw$}{r} {sign_colour}{sign}{r} ")?;
            write_source_line(out, line)?;
            out.write_char('\n')?;
            if let Some(marks) = marks {
                self.write_marks(out, line, a, b, marks)?;
            }
        }
        Ok(())
    }

    /// Writes the marker line under the line at `a..b` of the rewritten text:
    /// `+` under inserted text, `~` under replacement text. Writes nothing if no
    /// edit's text is on the line.
    fn write_marks(
        &self,
        out: &mut impl fmt::Write,
        line: &str,
        a: usize,
        b: usize,
        marks: &[Placed],
    ) -> fmt::Result {
        let (g, r, h) = (self.palette.gutter, self.palette.reset, self.palette.help);
        let gw = self.gw;
        let mut col = 0;
        let mut started = false;
        for mark in marks {
            let from = mark.start.max(a);
            let to = mark.end.min(b);
            if from >= to {
                continue;
            }
            let (lead, upto) = display_cols(line, from - a, to - a);
            if !started {
                write!(out, "{:gw$} {g}|{r} ", "")?;
                started = true;
            }
            let marker = if mark.insertion { '+' } else { '~' };
            write!(out, "{:pad$}{h}", "", pad = lead.saturating_sub(col))?;
            // Text of zero display width (a lone combining mark) still gets one
            // marker, so the edit is never invisible.
            let width = upto.saturating_sub(lead).max(1);
            for _ in 0..width {
                out.write_char(marker)?;
            }
            out.write_str(r)?;
            col = lead.max(col) + width;
        }
        if started {
            out.write_char('\n')?;
        }
        Ok(())
    }
}

/// Drops the lines that `old` and `new` share at their start and at their end,
/// returning how many were dropped from the start. Nothing is dropped when the
/// two are identical, so a block never ends up empty.
fn trim_common(
    old: &str,
    old_lines: &mut Vec<(usize, usize)>,
    new: &str,
    new_lines: &mut Vec<(usize, usize)>,
) -> usize {
    let same = |i: usize, j: usize| {
        let a = old_lines.get(i).map(|&(a, b)| old.get(a..b).unwrap_or(""));
        let b = new_lines.get(j).map(|&(a, b)| new.get(a..b).unwrap_or(""));
        a.is_some() && a == b
    };
    let (n_old, n_new) = (old_lines.len(), new_lines.len());
    let shortest = n_old.min(n_new);
    let mut head = 0;
    while head < shortest && same(head, head) {
        head += 1;
    }
    let mut tail = 0;
    while tail < shortest - head && same(n_old - 1 - tail, n_new - 1 - tail) {
        tail += 1;
    }
    if head + tail == n_old && head + tail == n_new {
        return 0;
    }
    old_lines.truncate(n_old - tail);
    new_lines.truncate(n_new - tail);
    // `drain` returns the removed lines, which are not needed.
    drop(old_lines.drain(..head));
    drop(new_lines.drain(..head));
    head
}

/// The 1-based number of the line `idx` lines below `first`.
fn line_number(first: u32, idx: usize) -> u32 {
    first.saturating_add(u32::try_from(idx).unwrap_or(u32::MAX))
}

/// The content range of every line of `s`: split at `\n`, with a `\r` directly
/// before the `\n` excluded, as the renderer's frames show lines. An empty
/// string is one empty line.
fn line_ranges(s: &str) -> Vec<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut lines = Vec::new();
    let mut lo = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' {
            let end = if i > lo && bytes.get(i - 1) == Some(&b'\r') {
                i - 1
            } else {
                i
            };
            lines.push((lo, end));
            lo = i + 1;
        }
    }
    lines.push((lo, s.len()));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_ranges_follow_the_frame_model() {
        assert_eq!(line_ranges(""), [(0, 0)]);
        assert_eq!(line_ranges("ab"), [(0, 2)]);
        assert_eq!(line_ranges("a\nb"), [(0, 1), (2, 3)]);
        assert_eq!(line_ranges("a\r\nb\n"), [(0, 1), (3, 4), (5, 5)]);
        assert_eq!(line_ranges("\r\n"), [(0, 0), (2, 2)]);
    }
}
