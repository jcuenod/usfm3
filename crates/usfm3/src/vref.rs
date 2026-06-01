//! Verse references and the `cat`-speed verse-text extraction.
//!
//! The headline use-case: produce `{ "GEN 1:1": "In the beginning…" }` about as
//! fast as `cat`. Two paths:
//!
//! * [`crate::Document::vrefs_raw`] — zero-copy `(VerseRef, &str)` where the value
//!   is the *raw contiguous* source slice for the verse (markers/notes included).
//!   Fastest possible; the caller cleans if they want.
//! * [`crate::Document::vref_text`] — *cleaned* text (notes excluded, char-marker
//!   tags dropped, whitespace collapsed/trimmed) packed into a single arena
//!   [`VrefText`]. Driven entirely by [`crate::marker::TextRole`], so it is
//!   version-correct for free. Still one read pass + one write pass of ≈equal
//!   bytes — no per-verse allocation.

use std::ops::Range;

/// A packed verse reference: `book:u8 | chapter:u12 | verse:u12`.
///
/// Integer-keyed so map inserts during extraction are trivially cheap; the human
/// string (`"GEN 1:1"`) is formatted only on demand.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VerseRef(pub u32);

impl VerseRef {
    #[inline]
    pub fn new(book: BookId, chapter: u16, verse: u16) -> Self {
        VerseRef(((book.0 as u32) << 24) | (((chapter as u32) & 0xFFF) << 12) | ((verse as u32) & 0xFFF))
    }
    #[inline]
    pub fn book(self) -> BookId {
        BookId((self.0 >> 24) as u8)
    }
    #[inline]
    pub fn chapter(self) -> u16 {
        ((self.0 >> 12) & 0xFFF) as u16
    }
    #[inline]
    pub fn verse(self) -> u16 {
        (self.0 & 0xFFF) as u16
    }
}

impl std::fmt::Display for VerseRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}:{}", self.book().code(), self.chapter(), self.verse())
    }
}

/// Index of a book within [`BOOK_CODES`]. Stable within a build.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BookId(pub u8);

impl BookId {
    /// No book has been identified (e.g. the document is missing its `\id`).
    /// Formats as `"???"` rather than masquerading as a real book.
    pub const UNKNOWN: BookId = BookId(u8::MAX);

    pub fn from_code(code: &str) -> Option<BookId> {
        // TODO(build-gen): perfect-hash this lookup.
        BOOK_CODES.iter().position(|&c| c == code).map(|i| BookId(i as u8))
    }
    pub fn code(self) -> &'static str {
        BOOK_CODES.get(self.0 as usize).copied().unwrap_or("???")
    }
    #[inline]
    pub fn is_known(self) -> bool {
        (self.0 as usize) < BOOK_CODES.len()
    }
}

/// Cleaned verse text for a whole document: one arena `buf` plus an in-document
/// ordered index of `(ref, range)` into it. `get`/`iter` borrow from the arena.
#[derive(Debug, Default)]
pub struct VrefText {
    pub(crate) buf: String,
    pub(crate) index: Vec<(VerseRef, Range<u32>)>,
}

impl VrefText {
    pub fn get(&self, vref: VerseRef) -> Option<&str> {
        self.index
            .iter()
            .find(|(r, _)| *r == vref)
            .map(|(_, range)| &self.buf[range.start as usize..range.end as usize])
    }
    pub fn iter(&self) -> impl Iterator<Item = (VerseRef, &str)> {
        self.index
            .iter()
            .map(move |(r, range)| (*r, &self.buf[range.start as usize..range.end as usize]))
    }
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// USFM book identifiers. (Canonical 66 for the bootstrap; the full set incl.
/// deuterocanon/apocrypha + peripherals will be generated.)
pub static BOOK_CODES: &[&str] = &[
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV",
];
