//! A collector for the diagnostics of a compilation: counts by severity,
//! duplicate suppression, ordering by position, and an optional cap.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::{fmt, slice};

use crate::{Applicability, Diagnostic, Label, Severity};

/// The index of `severity` in a per-severity count array.
const fn slot(severity: Severity) -> usize {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Note => 2,
        Severity::Help => 3,
    }
}

/// The diagnostics a compilation produced, with the bookkeeping a driver needs:
/// how many errors and warnings there were, whether to stop, the same
/// diagnostic reported twice kept once, and a cap so a pathological input
/// cannot produce an unbounded list.
///
/// A lexer, parser, and checker each [`push`](Diagnostics::push) into one
/// collector; the driver asks [`has_errors`](Diagnostics::has_errors) between
/// phases, [`sort`](Diagnostics::sort)s by position, and renders the lot with
/// [`Renderer::render_all`](crate::Renderer::render_all) (which ends with an
/// "N more" line when the cap dropped some) or emits them as JSON with
/// [`to_json`](Diagnostics::to_json).
///
/// # Duplicates
///
/// A diagnostic equal to one already stored (same severity, code, message,
/// labels, notes, help, and suggestions) is not stored or counted again —
/// error recovery and re-checking often report the same problem twice. The
/// check costs `O(log n)` plus one comparison per stored diagnostic with the
/// same 64-bit keyed fingerprint (SipHash-2-4), so crafted messages cannot make
/// it quadratic.
///
/// # Cap
///
/// [`with_limit`](Diagnostics::with_limit) bounds how many diagnostics are
/// stored. Past the limit, new diagnostics are still counted by severity (so
/// [`has_errors`](Diagnostics::has_errors) and the counts stay true) and in
/// [`omitted`](Diagnostics::omitted), but not kept: memory stays bounded
/// whatever the input. Because they are not kept, omitted diagnostics are only
/// recognised as duplicates of stored ones, not of each other.
///
/// # Examples
///
/// ```
/// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
///
/// let unknown = |at: u32| {
///     Diagnostic::new(Severity::Error, "unknown name", Label::unlabelled(Span::new(at, at + 1)))
/// };
///
/// let mut diags = Diagnostics::with_limit(2);
/// diags.push(unknown(30));
/// diags.push(unknown(10));
/// diags.push(unknown(10)); // the same diagnostic again: ignored
/// diags.push(Diagnostic::new(Severity::Warning, "unused", Label::unlabelled(Span::empty(0))));
///
/// assert!(diags.has_errors());
/// assert_eq!((diags.error_count(), diags.warning_count()), (2, 1));
/// assert_eq!((diags.len(), diags.omitted()), (2, 1)); // the warning was over the cap
///
/// diags.sort();
/// let starts: Vec<u32> = diags.iter().map(|d| d.primary().span().start().to_u32()).collect();
/// assert_eq!(starts, [10, 30]);
/// ```
#[derive(Clone, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
    /// `prints[i]` is the fingerprint of `items[i]`.
    prints: Vec<u64>,
    /// `(fingerprint, index into items)` for every stored diagnostic: the
    /// duplicate check is a range query over one fingerprint.
    seen: BTreeSet<(u64, usize)>,
    /// Accepted diagnostics per severity, omitted ones included.
    counts: [usize; 4],
    omitted: usize,
    limit: Option<usize>,
}

impl Diagnostics {
    /// Creates an empty collector with no cap.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Diagnostics;
    ///
    /// let diags = Diagnostics::new();
    /// assert!(diags.is_empty());
    /// assert!(!diags.has_errors());
    /// ```
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty collector that stores at most `limit` diagnostics.
    ///
    /// Diagnostics past the limit are counted (by severity and in
    /// [`omitted`](Diagnostics::omitted)) but not stored. Use a limit whenever
    /// the input is untrusted: a hostile source can otherwise produce one
    /// diagnostic per byte. A limit of `0` stores nothing and only counts.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::with_limit(1);
    /// for at in 0..5 {
    ///     diags.push(Diagnostic::new(Severity::Error, "bad", Label::unlabelled(Span::empty(at))));
    /// }
    /// assert_eq!((diags.len(), diags.omitted(), diags.error_count()), (1, 4, 5));
    /// ```
    #[inline]
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: Some(limit),
            ..Self::default()
        }
    }

    /// Adds `diag`, returning `true` if it was new and `false` if an identical
    /// diagnostic is already stored (in which case nothing changes).
    ///
    /// A new diagnostic is counted by severity; it is stored unless the
    /// collector is at its [limit](Diagnostics::with_limit), in which case it
    /// is counted in [`omitted`](Diagnostics::omitted) instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let diag = Diagnostic::new(Severity::Error, "bad", Label::unlabelled(Span::empty(3)));
    /// let mut diags = Diagnostics::new();
    /// assert!(diags.push(diag.clone()));
    /// assert!(!diags.push(diag)); // a duplicate
    /// assert_eq!(diags.len(), 1);
    /// ```
    pub fn push(&mut self, diag: Diagnostic) -> bool {
        let print = fingerprint(&diag);
        let duplicate = self
            .seen
            .range((print, 0)..=(print, usize::MAX))
            .any(|&(_, i)| self.items.get(i) == Some(&diag));
        if duplicate {
            return false;
        }

        let count = &mut self.counts[slot(diag.severity())];
        *count = count.saturating_add(1);

        if self.limit.is_some_and(|limit| self.items.len() >= limit) {
            self.omitted = self.omitted.saturating_add(1);
            return true;
        }
        // A fresh index is never in the set already, so this always inserts.
        let _inserted = self.seen.insert((print, self.items.len()));
        self.items.push(diag);
        self.prints.push(print);
        true
    }

    /// Sorts the stored diagnostics by where they point: the primary label's
    /// start, then its end, then severity (errors first). The sort is stable,
    /// so diagnostics that tie keep the order they were pushed in.
    ///
    /// Positions are global, so diagnostics in different sources sort in the
    /// order the sources were added to the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let at = |sev, pos: u32| Diagnostic::new(sev, "m", Label::unlabelled(Span::empty(pos)));
    /// let mut diags: Diagnostics = [
    ///     at(Severity::Warning, 9),
    ///     at(Severity::Warning, 2),
    ///     at(Severity::Error, 9),
    /// ]
    /// .into_iter()
    /// .collect();
    ///
    /// diags.sort();
    /// let order: Vec<_> = diags
    ///     .iter()
    ///     .map(|d| (d.primary().span().start().to_u32(), d.severity()))
    ///     .collect();
    /// assert_eq!(order, [(2, Severity::Warning), (9, Severity::Error), (9, Severity::Warning)]);
    /// ```
    pub fn sort(&mut self) {
        let mut entries: Vec<(Diagnostic, u64)> =
            self.items.drain(..).zip(self.prints.drain(..)).collect();
        entries.sort_by_key(|(d, _)| {
            let span = d.primary().span();
            (span.start(), span.end(), d.severity())
        });
        self.seen.clear();
        for (index, (diag, print)) in entries.into_iter().enumerate() {
            let _inserted = self.seen.insert((print, index));
            self.items.push(diag);
            self.prints.push(print);
        }
    }

    /// Removes every diagnostic and resets the counts, keeping the limit and
    /// the allocated storage — for a long-lived host (a language server, a
    /// watch-mode build) that re-checks a document.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::with_limit(10);
    /// diags.push(Diagnostic::new(Severity::Error, "bad", Label::unlabelled(Span::empty(0))));
    /// diags.clear();
    /// assert!(diags.is_empty() && !diags.has_errors());
    /// assert_eq!(diags.limit(), Some(10));
    /// ```
    pub fn clear(&mut self) {
        self.items.clear();
        self.prints.clear();
        self.seen.clear();
        self.counts = [0; 4];
        self.omitted = 0;
    }

    /// Returns whether any error was pushed, stored or omitted.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Warning, "w", Label::unlabelled(Span::empty(0))));
    /// assert!(!diags.has_errors());
    /// diags.push(Diagnostic::new(Severity::Error, "e", Label::unlabelled(Span::empty(0))));
    /// assert!(diags.has_errors());
    /// ```
    #[inline]
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.error_count() > 0
    }

    /// Returns the number of distinct errors pushed, including any omitted by
    /// the cap.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Error, "e", Label::unlabelled(Span::empty(0))));
    /// assert_eq!(diags.error_count(), 1);
    /// ```
    #[inline]
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.count(Severity::Error)
    }

    /// Returns the number of distinct warnings pushed, including any omitted by
    /// the cap.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Warning, "w", Label::unlabelled(Span::empty(0))));
    /// assert_eq!(diags.warning_count(), 1);
    /// ```
    #[inline]
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.count(Severity::Warning)
    }

    /// Returns the number of distinct diagnostics pushed at `severity`,
    /// including any omitted by the cap.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Note, "n", Label::unlabelled(Span::empty(0))));
    /// assert_eq!(diags.count(Severity::Note), 1);
    /// assert_eq!(diags.count(Severity::Help), 0);
    /// ```
    #[inline]
    #[must_use]
    pub const fn count(&self, severity: Severity) -> usize {
        self.counts[slot(severity)]
    }

    /// Returns how many new diagnostics were pushed past the
    /// [limit](Diagnostics::with_limit) and not stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Diagnostics;
    ///
    /// assert_eq!(Diagnostics::with_limit(5).omitted(), 0);
    /// ```
    #[inline]
    #[must_use]
    pub const fn omitted(&self) -> usize {
        self.omitted
    }

    /// Returns the cap set by [`with_limit`](Diagnostics::with_limit), or
    /// `None` for an uncapped collector.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Diagnostics;
    ///
    /// assert_eq!(Diagnostics::new().limit(), None);
    /// assert_eq!(Diagnostics::with_limit(3).limit(), Some(3));
    /// ```
    #[inline]
    #[must_use]
    pub const fn limit(&self) -> Option<usize> {
        self.limit
    }

    /// Returns the number of stored diagnostics (omitted ones excluded).
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Diagnostics;
    ///
    /// assert_eq!(Diagnostics::new().len(), 0);
    /// ```
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns whether no diagnostic is stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::Diagnostics;
    ///
    /// assert!(Diagnostics::new().is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns the stored diagnostics as a slice, in push order (or sorted
    /// order after [`sort`](Diagnostics::sort)).
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Error, "e", Label::unlabelled(Span::empty(0))));
    /// assert_eq!(diags.as_slice()[0].message(), "e");
    /// ```
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.items
    }

    /// Iterates over the stored diagnostics, in the order of
    /// [`as_slice`](Diagnostics::as_slice).
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Error, "e", Label::unlabelled(Span::empty(0))));
    /// assert_eq!(diags.iter().count(), 1);
    /// ```
    #[inline]
    pub fn iter(&self) -> slice::Iter<'_, Diagnostic> {
        self.items.iter()
    }

    /// Consumes the collector and returns the stored diagnostics.
    ///
    /// # Examples
    ///
    /// ```
    /// use diag_lang::{Diagnostic, Diagnostics, Label, Severity, Span};
    ///
    /// let mut diags = Diagnostics::new();
    /// diags.push(Diagnostic::new(Severity::Error, "e", Label::unlabelled(Span::empty(0))));
    /// let all: Vec<Diagnostic> = diags.into_vec();
    /// assert_eq!(all.len(), 1);
    /// ```
    #[inline]
    #[must_use]
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }
}

impl fmt::Debug for Diagnostics {
    /// Shows the stored diagnostics, the per-severity counts, what the cap
    /// omitted, and the limit; not the internal duplicate index.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Diagnostics")
            .field("items", &self.items)
            .field("errors", &self.error_count())
            .field("warnings", &self.warning_count())
            .field("notes", &self.count(Severity::Note))
            .field("help", &self.count(Severity::Help))
            .field("omitted", &self.omitted)
            .field("limit", &self.limit)
            .finish()
    }
}

impl Extend<Diagnostic> for Diagnostics {
    /// Pushes each diagnostic in turn, exactly as [`Diagnostics::push`].
    fn extend<I: IntoIterator<Item = Diagnostic>>(&mut self, iter: I) {
        for diag in iter {
            let _new = self.push(diag);
        }
    }
}

impl FromIterator<Diagnostic> for Diagnostics {
    /// Collects into an uncapped collector, pushing each diagnostic in turn.
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(iter: I) -> Self {
        let mut diags = Self::new();
        diags.extend(iter);
        diags
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = alloc::vec::IntoIter<Diagnostic>;

    /// Iterates over the stored diagnostics by value.
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = slice::Iter<'a, Diagnostic>;

    /// Iterates over the stored diagnostics by reference.
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// The 64-bit fingerprint of everything [`Diagnostic`]'s equality compares.
///
/// Equal diagnostics always have equal fingerprints. Every variable-length part
/// is preceded by its length, so different diagnostics never feed the same
/// byte stream. A collision only costs one extra equality comparison; the
/// keyed SipHash makes collisions as hard to construct as guessing its output.
fn fingerprint(diag: &Diagnostic) -> u64 {
    let mut h = Sip::new();
    h.write_u64(slot(diag.severity()) as u64);
    match diag.code() {
        Some(code) => {
            h.write_u64(1);
            h.write_str(code.prefix());
            h.write_u64(u64::from(code.number()));
        }
        None => h.write_u64(0),
    }
    h.write_str(diag.message());
    h.write_label(diag.primary());
    h.write_u64(diag.secondary().len() as u64);
    for label in diag.secondary() {
        h.write_label(label);
    }
    h.write_u64(diag.notes().len() as u64);
    for note in diag.notes() {
        h.write_str(note);
    }
    h.write_u64(diag.help().len() as u64);
    for help in diag.help() {
        h.write_str(help);
    }
    h.write_u64(diag.suggestions().len() as u64);
    for suggestion in diag.suggestions() {
        h.write_str(suggestion.message());
        h.write_u64(match suggestion.applicability() {
            Applicability::MachineApplicable => 0,
            Applicability::MaybeIncorrect => 1,
            Applicability::HasPlaceholders => 2,
            Applicability::Unspecified => 3,
        });
        h.write_u64(suggestion.edits().len() as u64);
        for edit in suggestion.edits() {
            h.write_u64(u64::from(edit.span().start().to_u32()));
            h.write_u64(u64::from(edit.span().end().to_u32()));
            h.write_str(edit.replacement());
        }
    }
    h.finish()
}

/// SipHash-2-4 (Aumasson & Bernstein) under a fixed key, streaming.
///
/// Written here rather than taken from `core::hash::SipHasher` (deprecated) or
/// a crate (no third-party runtime dependencies). The key is public, so this is
/// not a MAC; it is used because, unlike multiply-xor hashes, it has no known
/// algebraic way to produce many inputs with one output, which keeps duplicate
/// detection `O(log n)` on hostile messages.
struct Sip {
    v: [u64; 4],
    /// Buffered bytes not yet forming a full 8-byte word, little-endian.
    tail: u64,
    tail_len: u32,
    /// Total bytes written, mod 256 at finalisation.
    len: u64,
}

impl Sip {
    /// The fixed 128-bit key, as two little-endian words.
    const K0: u64 = 0x0706_0504_0302_0100;
    const K1: u64 = 0x0f0e_0d0c_0b0a_0908;

    const fn new() -> Self {
        Self::with_key(Self::K0, Self::K1)
    }

    const fn with_key(k0: u64, k1: u64) -> Self {
        Self {
            v: [
                k0 ^ 0x736f_6d65_7073_6575,
                k1 ^ 0x646f_7261_6e64_6f6d,
                k0 ^ 0x6c79_6765_6e65_7261,
                k1 ^ 0x7465_6462_7974_6573,
            ],
            tail: 0,
            tail_len: 0,
            len: 0,
        }
    }

    #[inline]
    fn round(&mut self) {
        let [mut v0, mut v1, mut v2, mut v3] = self.v;
        v0 = v0.wrapping_add(v1);
        v1 = v1.rotate_left(13) ^ v0;
        v0 = v0.rotate_left(32);
        v2 = v2.wrapping_add(v3);
        v3 = v3.rotate_left(16) ^ v2;
        v0 = v0.wrapping_add(v3);
        v3 = v3.rotate_left(21) ^ v0;
        v2 = v2.wrapping_add(v1);
        v1 = v1.rotate_left(17) ^ v2;
        v2 = v2.rotate_left(32);
        self.v = [v0, v1, v2, v3];
    }

    #[inline]
    fn compress(&mut self, m: u64) {
        self.v[3] ^= m;
        self.round();
        self.round();
        self.v[0] ^= m;
    }

    fn write(&mut self, bytes: &[u8]) {
        self.len = self.len.wrapping_add(bytes.len() as u64);
        let mut rest = bytes;
        // Top up a partial word first, then whole words, then buffer the rest.
        while self.tail_len > 0 {
            let Some((&b, more)) = rest.split_first() else {
                return;
            };
            self.tail |= u64::from(b) << (8 * self.tail_len);
            self.tail_len += 1;
            rest = more;
            if self.tail_len == 8 {
                let word = self.tail;
                self.compress(word);
                self.tail = 0;
                self.tail_len = 0;
            }
        }
        let mut words = rest.chunks_exact(8);
        for word in &mut words {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(word);
            self.compress(u64::from_le_bytes(buf));
        }
        for &b in words.remainder() {
            self.tail |= u64::from(b) << (8 * self.tail_len);
            self.tail_len += 1;
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.write(&n.to_le_bytes());
    }

    fn write_str(&mut self, s: &str) {
        self.write_u64(s.len() as u64);
        self.write(s.as_bytes());
    }

    fn write_label(&mut self, label: &Label) {
        self.write_u64(u64::from(label.span().start().to_u32()));
        self.write_u64(u64::from(label.span().end().to_u32()));
        self.write_str(label.message());
    }

    fn finish(mut self) -> u64 {
        let last = ((self.len & 0xff) << 56) | self.tail;
        self.compress(last);
        self.v[2] ^= 0xff;
        for _ in 0..4 {
            self.round();
        }
        self.v[0] ^ self.v[1] ^ self.v[2] ^ self.v[3]
    }
}

#[cfg(test)]
mod tests {
    use span_lang::Span;

    use super::*;
    use crate::{Code, Edit, Suggestion};

    #[test]
    fn test_siphash_matches_the_reference_vector() {
        // The SipHash paper's test vector: key 00..0f, message 00..0e.
        let message: Vec<u8> = (0u8..15).collect();
        let mut h = Sip::new();
        h.write(&message);
        assert_eq!(h.finish(), 0xa129_ca61_49be_45e5);

        // Streaming in pieces gives the same answer as one write.
        let mut pieces = Sip::new();
        for chunk in message.chunks(3) {
            pieces.write(chunk);
        }
        assert_eq!(pieces.finish(), 0xa129_ca61_49be_45e5);
    }

    #[test]
    fn test_fingerprint_separates_every_field() {
        let base = Diagnostic::new(Severity::Error, "m", Label::new(Span::new(1, 2), "l"));
        let variants = [
            base.clone(),
            Diagnostic::new(Severity::Warning, "m", Label::new(Span::new(1, 2), "l")),
            Diagnostic::new(Severity::Error, "n", Label::new(Span::new(1, 2), "l")),
            Diagnostic::new(Severity::Error, "m", Label::new(Span::new(1, 3), "l")),
            Diagnostic::new(Severity::Error, "m", Label::new(Span::new(1, 2), "k")),
            base.clone().with_code(Code::new("E", 1).unwrap()),
            base.clone()
                .with_secondary(Label::unlabelled(Span::empty(0))),
            base.clone().with_note("x"),
            base.clone().with_help("x"),
            base.clone()
                .with_suggestion(Suggestion::new("x", Applicability::Unspecified)),
            base.clone().with_suggestion(
                Suggestion::new("x", Applicability::Unspecified).with_edit(Edit::insert(0, "")),
            ),
        ];
        for (i, a) in variants.iter().enumerate() {
            for b in &variants[i + 1..] {
                assert_ne!(fingerprint(a), fingerprint(b), "{a:?} vs {b:?}");
            }
            assert_eq!(fingerprint(a), fingerprint(&a.clone()));
        }
    }
}
