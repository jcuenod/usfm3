//! Verse references and the `cat`-speed verse-text extraction.
//!
//! The headline use-case: produce `{ "GEN 1:1": "In the beginning…" }` about as
//! fast as `cat`. Two paths:
//!
//! * [`Document::vrefs_raw`] — zero-copy `(VerseRef, &str)` where the value is the
//!   *raw contiguous* source slice for the verse (markers/notes included). Fastest
//!   possible; the caller cleans if they want.
//! * [`Document::vref_text`] — *cleaned* text (notes excluded, char-marker tags
//!   dropped, whitespace collapsed/trimmed) packed into a single arena
//!   [`VrefText`]. Driven entirely by [`crate::marker::TextRole`], so it is
//!   version-correct for free. Still one read pass + one write pass of ≈equal
//!   bytes — no per-verse allocation.

use std::ops::Range;

use crate::Document;
use crate::marker::{Category, TextRole};
use crate::tape::NodeKind;

// --- public types ----------------------------------------------------------

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

/// A single verse, as seen by [`Document::for_each_vref`]. `text` and `label`
/// borrow internal buffers and are only valid for the duration of the callback.
pub struct VrefRow<'a> {
    pub book: BookId,
    pub chapter: u16,
    /// Leading integer of the verse number (e.g. `1` for `1-2`).
    pub verse: u16,
    /// The raw verse label as written (`"1"`, `"1-2"`, `"1a"`); may be empty.
    pub label: &'a str,
    /// Cleaned verse text (notes excluded, char tags dropped, whitespace collapsed).
    pub text: &'a str,
}

impl VrefRow<'_> {
    /// Format the reference as `"GEN 1:1"` (preserving a range/sub-verse label).
    pub fn key(&self) -> String {
        if self.label.is_empty() {
            format!("{} {}:{}", self.book.code(), self.chapter, self.verse)
        } else {
            format!("{} {}:{}", self.book.code(), self.chapter, self.label)
        }
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

// --- extraction (Document methods) -----------------------------------------

impl<'src> Document<'src> {
    /// Zero-copy raw verse slices: the contiguous source between each `\v` and the
    /// next verse/chapter boundary (markers and notes included verbatim).
    pub fn vrefs_raw(&self) -> Vec<(VerseRef, &'src str)> {
        let mut out = Vec::new();
        self.walk_verses(|hit| {
            out.push((hit.reference, &self.source()[hit.raw_start..hit.raw_end]));
        });
        out
    }

    /// Cleaned verse text into a single arena: notes excluded, character-marker
    /// tags dropped (inner text kept), whitespace collapsed and trimmed. Driven by
    /// [`TextRole`], so it is correct per version automatically.
    pub fn vref_text(&self) -> VrefText {
        let mut buf = String::with_capacity(self.source().len());
        let mut index = Vec::new();
        self.walk_verses(|hit| {
            let begin = buf.len() as u32;
            hit.body.write_clean(self, &mut buf);
            let cleaned_end = buf.len() as u32;
            index.push((hit.reference, begin..cleaned_end));
        });
        VrefText { buf, index }
    }

    /// The headline use-case, streaming: invokes `f` once per verse with a
    /// borrowed [`VrefRow`] (reference parts + cleaned text). Allocation-free per
    /// verse beyond one reused text buffer — this is the path for `cat`-speed
    /// extraction straight to a sink.
    pub fn for_each_vref(&self, mut f: impl FnMut(VrefRow<'_>)) {
        let mut buf = String::new();
        self.walk_verses(|hit| {
            buf.clear();
            hit.body.write_clean(self, &mut buf);
            f(VrefRow {
                book: hit.book,
                chapter: hit.chapter,
                verse: hit.reference.verse(),
                label: hit.label,
                text: &buf,
            });
        });
    }

    /// Convenience: an ordered `{ "GEN 1:1" => "In the beginning…" }` dictionary,
    /// suitable for direct JSON serialization. Keys preserve the original verse
    /// label (so ranges like `1-2` survive).
    ///
    /// Unlike the faithful primitives ([`Document::for_each_vref`],
    /// [`Document::vref_text`], [`Document::vrefs_raw`]), this is a *map* and so:
    /// - **drops verses whose cleaned text is empty** (e.g. a `\v` whose only
    ///   content is a footnote, or a number-only milestone), and
    /// - **merges duplicate references** (a verse split across structure, or
    ///   malformed repeats) by joining their text with a space — so the result
    ///   has unique keys and is valid as a JSON object.
    ///
    /// Allocates per entry; prefer the primitives on the hot path.
    pub fn vref_dict(&self) -> Vec<(String, String)> {
        // Fast path: collect every verse with only the two String allocations the
        // output inherently needs (key + cleaned text) — no dedup bookkeeping.
        let mut rows: Vec<(String, String)> = Vec::new();
        self.for_each_vref(|row| {
            if !row.text.is_empty() {
                rows.push((row.key(), row.text.to_string()));
            }
        });

        // Duplicate references are anomalous (a split or malformed verse). Detect
        // them in one read-only pass over borrowed keys — no clones, one hash each.
        let has_duplicates = {
            let mut seen = std::collections::HashSet::with_capacity(rows.len());
            rows.iter().any(|(key, _)| !seen.insert(key.as_str()))
        };
        if !has_duplicates {
            return rows; // overwhelmingly common case
        }

        // Rare path: merge duplicate keys (join text with a space), keep order.
        let mut order: Vec<(String, String)> = Vec::with_capacity(rows.len());
        let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (key, text) in rows {
            match index.get(&key) {
                Some(&i) => {
                    let slot: &mut String = &mut order[i].1;
                    slot.push(' ');
                    slot.push_str(&text);
                }
                None => {
                    index.insert(key.clone(), order.len());
                    order.push((key, text));
                }
            }
        }
        order
    }

    /// Shared verse traversal: invokes `f` once per `\v`, with the reference, the
    /// raw label, the raw byte range, and a [`VerseBody`] handle for cleaning.
    fn walk_verses(&self, mut f: impl FnMut(VerseHit<'src>)) {
        let nodes = &self.tape().nodes;
        let mut book = BookId::UNKNOWN;
        let mut chapter: u16 = 0;
        let mut expect_book = false;
        // (reference, label, body_start, open_idx)
        let mut pending: Option<(VerseRef, &'src str, usize, usize)> = None;

        // Emits the pending verse (if any) with the given context. Takes context
        // by value so the loop can keep mutating `book`/`chapter`.
        fn flush<'s>(
            pending: &mut Option<(VerseRef, &'s str, usize, usize)>,
            book: BookId,
            chapter: u16,
            boundary: usize,
            f: &mut dyn FnMut(VerseHit<'s>),
        ) {
            if let Some((reference, label, body_start, open_idx)) = pending.take() {
                f(VerseHit {
                    reference,
                    book,
                    chapter,
                    label,
                    raw_start: body_start,
                    raw_end: boundary,
                    body: VerseBody { open_idx, boundary },
                });
            }
        }

        for (i, node) in nodes.iter().enumerate() {
            match node.kind {
                NodeKind::MarkerOpen
                    if self.table.info(node.marker).map(|m| m.category)
                        == Some(Category::Identification) =>
                {
                    expect_book = self.table.info(node.marker).map(|m| m.name) == Some("id");
                }
                NodeKind::Text if expect_book => {
                    if let Some(code) = node.span().text(self.source()).split_whitespace().next() {
                        if let Some(b) = BookId::from_code(code) {
                            book = b;
                        }
                    }
                    expect_book = false;
                }
                NodeKind::Chapter => {
                    flush(&mut pending, book, chapter, node.start as usize, &mut f);
                    chapter = node.link as u16; // number packed in `link`
                }
                NodeKind::Verse => {
                    flush(&mut pending, book, chapter, node.start as usize, &mut f);
                    let reference = VerseRef::new(book, chapter, node.link as u16);
                    let span_text = node.span().text(self.source());
                    let label = span_text.split_whitespace().nth(1).unwrap_or_else(|| {
                        // Fallback: malformed \v11 — number embedded in marker name
                        let name = span_text.trim_start_matches('\\');
                        let d = name.find(|c: char| c.is_ascii_digit()).unwrap_or(name.len());
                        &name[d..]
                    });
                    pending = Some((reference, label, node.span().end() as usize, i));
                }
                _ => {}
            }
        }
        flush(&mut pending, book, chapter, self.source().len(), &mut f);
    }
}

/// One verse occurrence yielded by `Document::walk_verses`.
struct VerseHit<'a> {
    reference: VerseRef,
    book: BookId,
    chapter: u16,
    label: &'a str,
    raw_start: usize,
    raw_end: usize,
    body: VerseBody,
}

/// Handle to a verse's content on the tape, for cleaned-text extraction.
struct VerseBody {
    open_idx: usize,
    boundary: usize,
}

impl VerseBody {
    /// Append cleaned text for this verse to `buf`, using [`TextRole`] to decide
    /// which nodes contribute, skipping excluded subtrees via the `link` pointer,
    /// and collapsing/trimming whitespace.
    fn write_clean(&self, doc: &Document<'_>, buf: &mut String) {
        let nodes = &doc.tape().nodes;
        let mut i = self.open_idx + 1;
        let start_len = buf.len();
        // Carried across nodes: did we just pass whitespace that a following
        // non-space character should turn into a single separator? This is what
        // keeps a marker boundary with no surrounding whitespace (e.g. a footnote
        // wedged between `.` and `”`) from inserting a spurious space.
        let mut pending_space = false;
        while i < nodes.len() {
            let node = &nodes[i];
            if node.start as usize >= self.boundary {
                break;
            }
            match node.kind {
                NodeKind::Text => {
                    push_collapsed(node.span().text(doc.source()), buf, start_len, &mut pending_space);
                    i += 1;
                }
                NodeKind::MarkerOpen => {
                    match doc.table.info(node.marker).map(|m| m.text_role) {
                        Some(TextRole::Excluded) => {
                            i = (node.link as usize) + 1; // skip subtree
                        }
                        // Content/Boundary markers contribute no characters of their
                        // own; spacing is decided purely by the source whitespace
                        // around them, handled in `push_collapsed`.
                        _ => i += 1,
                    }
                }
                _ => i += 1,
            }
        }
    }
}

/// Append `s` to `buf`, collapsing each run of whitespace to at most one space.
/// A separator is emitted only before a non-space character that follows
/// whitespace *and* only when the verse already has content — so leading and
/// trailing whitespace are dropped, and adjacent characters split solely by a
/// (dropped) marker stay adjacent. `verse_start` marks where this verse began in
/// the shared arena; `pending_space` carries the "saw whitespace" state across
/// the node boundary.
fn push_collapsed(s: &str, buf: &mut String, verse_start: usize, pending_space: &mut bool) {
    for ch in s.chars() {
        if ch.is_whitespace() {
            *pending_space = true;
        } else {
            if *pending_space && buf.len() > verse_start {
                buf.push(' ');
            }
            *pending_space = false;
            buf.push(ch);
        }
    }
}
