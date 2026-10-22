//! Byte-offset spans into the original source, with lazy line/column resolution.
//!
//! Every node on the [`crate::tape::Tape`] and every [`crate::diagnostic::Diagnostic`]
//! refers to source by `Span`. We deliberately keep spans as raw byte offsets (`u32`)
//! rather than line/column pairs: it is one of the things that keeps the hot path
//! allocation- and branch-free. Line/column is computed only when a human needs it.

/// A half-open byte range `[start, start + len)` into the source buffer.
///
/// `u32` caps a single document at 4 GiB, which is far beyond any real USFM file
/// and keeps [`crate::tape::Node`] at 16 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    #[inline]
    pub const fn new(start: u32, len: u32) -> Self {
        Self { start, len }
    }

    #[inline]
    pub const fn empty(at: u32) -> Self {
        Self { start: at, len: 0 }
    }

    #[inline]
    pub const fn end(self) -> u32 {
        self.start + self.len
    }

    #[inline]
    pub const fn range(self) -> core::ops::Range<usize> {
        self.start as usize..self.end() as usize
    }

    /// Zero-copy slice of the source this span points at.
    #[inline]
    pub fn text<'s>(self, source: &'s str) -> &'s str {
        &source[self.range()]
    }

    /// Resolve to 1-based line/column. O(span.start); call only for diagnostics/UI.
    pub fn line_col(self, source: &str) -> LineCol {
        let prefix = &source.as_bytes()[..self.start as usize];
        let line = 1 + memchr::memchr_iter(b'\n', prefix).count() as u32;
        let line_start = memchr::memrchr(b'\n', prefix).map_or(0, |i| i + 1);
        let col = 1 + (self.start as usize - line_start) as u32;
        LineCol { line, col }
    }
}

/// 1-based line and column, resolved on demand from a [`Span`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Coordinate space for source offsets leaving the crate.
///
/// Internally everything is UTF-8 byte offsets ([`Span`]); that is the cheapest
/// thing for the parser and the only thing Rust callers want. But a *host* wants
/// offsets it can index its own strings with, and the answer differs per host:
/// JavaScript strings are UTF-16 code units, CPython `str` is code points. Handing
/// out byte offsets and letting the host convert pushes the conversion into the
/// slower language and gets it silently wrong on non-ASCII text, so we convert
/// here instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Offsets {
    /// UTF-8 byte offsets — identical to [`Span`]. The default, and what the CLI uses.
    #[default]
    Utf8,
    /// UTF-16 code units, for `String.prototype.slice` in JavaScript.
    Utf16,
    /// Unicode scalar values, for `str[a:b]` in Python.
    Chars,
}

impl Offsets {
    /// Stable machine-readable name, as accepted by the FFI `offsets` option.
    pub fn as_str(self) -> &'static str {
        match self {
            Offsets::Utf8 => "utf8",
            Offsets::Utf16 => "utf16",
            Offsets::Chars => "chars",
        }
    }

    /// Numeric code stored in the packed tape header, so a host can never be wrong
    /// about which space a buffer is in.
    pub fn code(self) -> u32 {
        match self {
            Offsets::Utf8 => 0,
            Offsets::Utf16 => 1,
            Offsets::Chars => 2,
        }
    }

    /// Parse the FFI `offsets` option. Unrecognized and absent values both fall back
    /// to `default`, so a host never has to validate before calling.
    pub fn from_name(name: Option<&str>, default: Offsets) -> Offsets {
        match name {
            Some("utf8") => Offsets::Utf8,
            Some("utf16") => Offsets::Utf16,
            Some("chars") => Offsets::Chars,
            _ => default,
        }
    }
}

/// Forward-only byte-offset translator: byte offset -> [`Offsets`] space, plus
/// 1-based line/column in that same space.
///
/// One scanner amortizes to a *single* pass over the source across monotonically
/// increasing queries, which is what both callers give it: tape nodes are emitted in
/// non-decreasing `start` order (the tiling invariant), and diagnostics are sorted by
/// `(start, end)`. A backwards query is still answered correctly — the scan just
/// restarts from 0 — so neither caller has to guarantee ordering for *correctness*,
/// only for speed.
///
/// This replaces per-call [`Span::line_col`], which is O(`span.start`) and therefore
/// made diagnostic serialization O(N x len).
pub struct OffsetMap<'s> {
    src: &'s [u8],
    space: Offsets,
    /// `true` when the whole source is ASCII, in which case all three spaces coincide
    /// and `at` needs no scanning at all.
    ascii: bool,
    /// Byte position the incremental state currently describes.
    byte: u32,
    /// Position of `byte` in `space`.
    out: u32,
    /// 1-based line number at `byte`.
    line: u32,
    /// Position of the current line's first character, in `space`.
    line_start: u32,
}

impl<'s> OffsetMap<'s> {
    pub fn new(source: &'s str, space: Offsets) -> Self {
        OffsetMap {
            src: source.as_bytes(),
            space,
            ascii: source.is_ascii(),
            byte: 0,
            out: 0,
            line: 1,
            line_start: 0,
        }
    }

    /// Translate a byte offset into `space`.
    ///
    /// Free under [`Offsets::Utf8`] (it *is* the byte offset) and free for any space
    /// over an ASCII source (all three coincide). Otherwise costs the walk from the
    /// previous query.
    #[inline]
    pub fn at(&mut self, byte: u32) -> u32 {
        if self.space == Offsets::Utf8 || self.ascii {
            return byte.min(self.src.len() as u32);
        }
        self.advance_to(byte);
        self.out
    }

    /// Translate a byte offset into `space` and resolve 1-based line/column there.
    ///
    /// Unlike [`Self::at`] this always walks, because line breaks have to be counted
    /// regardless of space.
    pub fn point_at(&mut self, byte: u32) -> Point {
        self.advance_to(byte);
        Point {
            offset: self.out,
            line: self.line,
            col: self.out - self.line_start + 1,
        }
    }

    /// Length of the byte range `[start, end)` measured in `space`, **without**
    /// touching the incremental state.
    ///
    /// This exists so callers can resolve a span's *end* without moving the scanner
    /// past it. Spans nest — a diagnostic covering `[0, 100)` can be followed by one
    /// covering `[5, 10)`, and a container node's close lies beyond its children — so
    /// advancing to every `end` would force a backwards restart on the next query and
    /// reintroduce the O(N x len) behaviour this type exists to remove. Keep the
    /// incremental state driven by `start` alone, which *is* monotonic.
    ///
    /// Bounded by the span's own length rather than the prefix, so summed over a whole
    /// tape it stays O(source length): marker nodes own only their `\tag` bytes, and
    /// only the synthetic root spans the entire document.
    pub fn span_len(&self, start: u32, end: u32) -> u32 {
        let cap = self.src.len() as u32;
        let (start, end) = (start.min(cap), end.min(cap));
        if end <= start {
            return 0;
        }
        if self.space == Offsets::Utf8 || self.ascii {
            return end - start;
        }
        let bytes = &self.src[start as usize..end as usize];
        match self.space {
            Offsets::Utf16 => bytes
                .iter()
                .filter(|&&b| is_char_start(b))
                .map(|&b| utf16_units(b))
                .sum(),
            _ => bytes.iter().filter(|&&b| is_char_start(b)).count() as u32,
        }
    }

    /// Total source length in all three spaces: `(bytes, utf16 units, chars)`.
    ///
    /// The packed tape header carries all three so that *any* host can validate the
    /// source it holds in O(1), whichever space the offsets are in.
    pub fn lengths(&self) -> (u32, u32, u32) {
        let bytes = self.src.len() as u32;
        if self.ascii {
            return (bytes, bytes, bytes);
        }
        let mut utf16 = 0u32;
        let mut chars = 0u32;
        for &b in self.src {
            if is_char_start(b) {
                chars += 1;
                utf16 += utf16_units(b);
            }
        }
        (bytes, utf16, chars)
    }

    /// Walk the incremental state up to `byte`, restarting from 0 if asked to go
    /// backwards. Clamped to the source length so a malformed span cannot panic.
    fn advance_to(&mut self, byte: u32) {
        let byte = byte.min(self.src.len() as u32);
        if byte < self.byte {
            self.byte = 0;
            self.out = 0;
            self.line = 1;
            self.line_start = 0;
        }
        for &b in &self.src[self.byte as usize..byte as usize] {
            self.out += match self.space {
                Offsets::Utf8 => 1,
                Offsets::Utf16 => {
                    if is_char_start(b) {
                        utf16_units(b)
                    } else {
                        0
                    }
                }
                Offsets::Chars => u32::from(is_char_start(b)),
            };
            if b == b'\n' {
                // `out` now sits just past the newline, i.e. at the next line's start.
                self.line += 1;
                self.line_start = self.out;
            }
        }
        self.byte = byte;
    }
}

/// A byte begins a UTF-8 scalar iff it is not a `10xxxxxx` continuation byte.
#[inline]
const fn is_char_start(b: u8) -> bool {
    b & 0xC0 != 0x80
}

/// UTF-16 code units contributed by the scalar starting at `b`. A 4-byte sequence
/// (lead byte `>= 0xF0`) is astral and encodes as a surrogate *pair*.
#[inline]
const fn utf16_units(b: u8) -> u32 {
    if b >= 0xF0 { 2 } else { 1 }
}

/// A source position translated into an [`Offsets`] space, with 1-based line/column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Point {
    /// Offset in the requested space.
    pub offset: u32,
    pub line: u32,
    /// 1-based column, counted in the requested space.
    pub col: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte offsets 0, 1, 2, 4, 8, 9 for `a`, `\n`, `é` (2 bytes), astral (4 bytes),
    /// `\n`, `b`. Deliberately mixes 1-, 2-, and 4-byte scalars: the 4-byte one is the
    /// case a naive "count chars" remap gets wrong, since it is *two* UTF-16 units.
    const MIXED: &str = "a\né\u{1F600}\nb";

    #[test]
    fn utf8_offsets_are_the_identity() {
        let mut map = OffsetMap::new(MIXED, Offsets::Utf8);
        for byte in [0u32, 1, 2, 4, 8, 9, 10] {
            assert_eq!(map.at(byte), byte);
        }
    }

    #[test]
    fn utf16_and_char_offsets_track_scalar_widths() {
        let bytes = [0u32, 1, 2, 4, 8, 9, 10];
        let mut utf16 = OffsetMap::new(MIXED, Offsets::Utf16);
        assert_eq!(
            bytes.map(|b| utf16.at(b)),
            [0, 1, 2, 3, 5, 6, 7],
            "the astral scalar occupies two UTF-16 units"
        );
        let mut chars = OffsetMap::new(MIXED, Offsets::Chars);
        assert_eq!(bytes.map(|b| chars.at(b)), [0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn offsets_are_slice_exact_against_the_host_encodings() {
        let units: Vec<u16> = MIXED.encode_utf16().collect();
        let scalars: Vec<char> = MIXED.chars().collect();
        let mut utf16 = OffsetMap::new(MIXED, Offsets::Utf16);
        let mut chars = OffsetMap::new(MIXED, Offsets::Chars);
        // Every scalar, sliced out of the source three ways, must agree. This is
        // literally what JS `source.slice()` and Python `src[a:b]` will do.
        for (byte, ch) in MIXED.char_indices() {
            let byte = byte as u32;
            let end = byte + ch.len_utf8() as u32;

            let (s, n) = (utf16.at(byte), utf16.span_len(byte, end));
            let sliced = String::from_utf16(&units[s as usize..(s + n) as usize]).unwrap();
            assert_eq!(sliced, ch.to_string(), "utf16 slice for {ch:?}");

            let (s, n) = (chars.at(byte), chars.span_len(byte, end));
            let sliced: String = scalars[s as usize..(s + n) as usize].iter().collect();
            assert_eq!(sliced, ch.to_string(), "chars slice for {ch:?}");
        }
    }

    #[test]
    fn line_and_column_are_counted_in_the_requested_space() {
        let mut map = OffsetMap::new(MIXED, Offsets::Utf16);
        assert_eq!(map.point_at(0), Point { offset: 0, line: 1, col: 1 });
        assert_eq!(map.point_at(2), Point { offset: 2, line: 2, col: 1 });
        // The astral scalar is the second character on line 2 but the third UTF-16
        // unit; the column counts characters-as-units, i.e. 2.
        assert_eq!(map.point_at(4), Point { offset: 3, line: 2, col: 2 });
        assert_eq!(map.point_at(9), Point { offset: 6, line: 3, col: 1 });
    }

    #[test]
    fn line_and_column_agree_with_span_line_col() {
        // `Span::line_col` counts bytes, so they only have to agree under Utf8 — but
        // that is exactly the compatibility claim for the existing CLI output.
        let mut map = OffsetMap::new(MIXED, Offsets::Utf8);
        for byte in [0u32, 1, 2, 4, 8, 9] {
            let point = map.point_at(byte);
            let lc = Span::empty(byte).line_col(MIXED);
            assert_eq!((point.line, point.col), (lc.line, lc.col), "at byte {byte}");
        }
    }

    #[test]
    fn a_backwards_query_restarts_and_stays_correct() {
        let mut map = OffsetMap::new(MIXED, Offsets::Utf16);
        assert_eq!(map.at(9), 6);
        // Nested spans mean this happens in practice; correctness must not depend on
        // monotonicity (only the amortized cost does).
        assert_eq!(map.at(2), 2);
        assert_eq!(map.point_at(4), Point { offset: 3, line: 2, col: 2 });
    }

    #[test]
    fn span_len_does_not_disturb_the_scanner() {
        let mut map = OffsetMap::new(MIXED, Offsets::Utf16);
        assert_eq!(map.at(2), 2);
        assert_eq!(map.span_len(4, 8), 2, "astral scalar is two units");
        assert_eq!(map.span_len(0, 10), 7, "whole source");
        // Still positioned at byte 2, so a following in-order query is still cheap
        // *and* correct.
        assert_eq!(map.at(4), 3);
    }

    #[test]
    fn lengths_reports_all_three_spaces() {
        assert_eq!(OffsetMap::new(MIXED, Offsets::Utf8).lengths(), (10, 7, 6));
        assert_eq!(
            OffsetMap::new(MIXED, Offsets::Utf8).lengths(),
            (
                MIXED.len() as u32,
                MIXED.encode_utf16().count() as u32,
                MIXED.chars().count() as u32
            )
        );
        // ASCII short-circuit: all three coincide.
        assert_eq!(OffsetMap::new("abc\ndef", Offsets::Utf16).lengths(), (7, 7, 7));
    }

    #[test]
    fn out_of_range_offsets_clamp_instead_of_panicking() {
        let mut map = OffsetMap::new(MIXED, Offsets::Utf16);
        assert_eq!(map.at(9999), 7);
        assert_eq!(map.span_len(9999, 10_000), 0);
        assert_eq!(map.span_len(8, 4), 0, "reversed range is empty, not a panic");
        assert_eq!(map.point_at(9999).offset, 7);
    }

    #[test]
    fn empty_source_is_well_defined() {
        let mut map = OffsetMap::new("", Offsets::Utf16);
        assert_eq!(map.at(0), 0);
        assert_eq!(map.lengths(), (0, 0, 0));
        assert_eq!(map.point_at(0), Point { offset: 0, line: 1, col: 1 });
    }
}
