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

use super::render::json_str;

// --- public types ----------------------------------------------------------

/// A packed verse reference: `book:u8 | chapter:u12 | verse:u12`.
///
/// Integer-keyed so map inserts during extraction are trivially cheap; the human
/// string (`"GEN 1:1"`) is formatted only on demand. Only codes in [`BOOK_CODES`]
/// can be packed; any other `\id` code is [`BookId::UNKNOWN`] here, while
/// [`VrefRow::book_code`] and [`Document::vref_dict`] keep the code as written.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VerseRef(pub u32);

impl VerseRef {
    #[inline]
    pub fn new(book: BookId, chapter: u16, verse: u16) -> Self {
        VerseRef(
            ((book.0 as u32) << 24) | (((chapter as u32) & 0xFFF) << 12) | ((verse as u32) & 0xFFF),
        )
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
        write!(
            f,
            "{} {}:{}",
            self.book().code(),
            self.chapter(),
            self.verse()
        )
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
        BOOK_CODES
            .iter()
            .position(|&c| c == code)
            .map(|i| BookId(i as u8))
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
    /// The `\id` code as written (`"GEN"`, `"php"`, `"XYZ"`), or `"???"` when the
    /// document has no `\id`.
    pub book_code: &'a str,
    pub chapter: u16,
    /// Leading integer of the verse number (e.g. `1` for `1-2`).
    pub verse: u16,
    /// The raw verse label as written (`"1"`, `"1-2"`, `"1a"`); may be empty.
    pub label: &'a str,
    /// Cleaned verse text (notes excluded, char tags dropped, whitespace collapsed).
    pub text: &'a str,
}

impl VrefRow<'_> {
    /// Format the reference as `"GEN 1:1"`, using the book code as written and
    /// preserving a range/sub-verse label.
    pub fn key(&self) -> String {
        if self.label.is_empty() {
            format!("{} {}:{}", self.book_code, self.chapter, self.verse)
        } else {
            format!("{} {}:{}", self.book_code, self.chapter, self.label)
        }
    }
}

/// USFM book identifiers: the 66 canonical books (first, so their [`BookId`]s are
/// stable), the deuterocanon/apocrypha, and the peripheral and extra-material codes.
/// Codes are case-sensitive: `php` is not `PHP`.
pub static BOOK_CODES: &[&str] = &[
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV", "TOB", "JDT", "ESG", "WIS", "SIR", "BAR", "LJE", "S3Y", "SUS", "BEL", "1MA", "2MA",
    "3MA", "4MA", "1ES", "2ES", "MAN", "PS2", "ODA", "PSS", "EZA", "5EZ", "6EZ", "DAG", "PS3",
    "2BA", "LBA", "JUB", "ENO", "1MQ", "2MQ", "3MQ", "REP", "4BA", "LAO", "FRT", "BAK", "OTH",
    "INT", "CNC", "GLO", "TDX", "NDX", "XXA", "XXB", "XXC", "XXD", "XXE", "XXF", "XXG",
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
                book_code: hit.book_code,
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

    /// The vref dictionary as a JSON object string: `{"GEN 1:1":"In the beginning…"}`.
    ///
    /// Keys are unique and in document order (same semantics as [`Document::vref_dict`]
    /// — empty verses dropped, duplicates merged), so the result is a well-formed JSON
    /// object. This is the single-string form that FFI bindings (wasm/Python) hand
    /// across the boundary and parse natively, mirroring [`Document::to_usj`] /
    /// [`Document::to_cst_json`].
    pub fn to_vref_json(&self) -> String {
        let dict = self.vref_dict();
        let mut out = String::with_capacity(dict.len() * 24 + 2);
        out.push('{');
        for (i, (key, value)) in dict.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = json_str(key, &mut out);
            out.push(':');
            let _ = json_str(value, &mut out);
        }
        out.push('}');
        out
    }

    /// Shared verse traversal: invokes `f` once per `\v`, with the reference, the
    /// raw label, the raw byte range, and a [`VerseBody`] handle for cleaning.
    fn walk_verses(&self, mut f: impl FnMut(VerseHit<'src>)) {
        let nodes = &self.tape().nodes;
        let mut book = BookId::UNKNOWN;
        let mut book_code: &'src str = "???";
        let mut chapter: u16 = 0;
        let mut expect_book = false;
        let mut pending: Option<Pending<'src>> = None;

        // Emits the pending verse (if any), ending its body at `boundary`.
        fn flush<'s>(
            pending: &mut Option<Pending<'s>>,
            boundary: usize,
            f: &mut dyn FnMut(VerseHit<'s>),
        ) {
            if let Some(Pending {
                reference,
                book_code,
                chapter,
                label,
                body_start,
                open_idx,
            }) = pending.take()
            {
                f(VerseHit {
                    reference,
                    book: reference.book(),
                    book_code,
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
                        book_code = code;
                        book = BookId::from_code(code).unwrap_or(BookId::UNKNOWN);
                    }
                    expect_book = false;
                }
                NodeKind::Chapter => {
                    flush(&mut pending, node.start as usize, &mut f);
                    chapter = node.link as u16; // number packed in `link`
                }
                NodeKind::Verse => {
                    flush(&mut pending, node.start as usize, &mut f);
                    let reference = VerseRef::new(book, chapter, node.link as u16);
                    let span_text = node.span().text(self.source());
                    let label = span_text.split_whitespace().nth(1).unwrap_or_else(|| {
                        // Fallback: malformed \v11 — number embedded in marker name
                        let name = span_text.trim_start_matches('\\');
                        let d = name
                            .find(|c: char| c.is_ascii_digit())
                            .unwrap_or(name.len());
                        &name[d..]
                    });
                    pending = Some(Pending {
                        reference,
                        book_code,
                        chapter,
                        label,
                        body_start: node.span().end() as usize,
                        open_idx: i,
                    });
                }
                _ => {}
            }
        }
        flush(&mut pending, self.source().len(), &mut f);
    }
}

/// A verse seen but not yet emitted: its body runs to the next boundary. Carries
/// its own book and chapter so a following `\id` or `\c` can't relabel it.
struct Pending<'a> {
    reference: VerseRef,
    book_code: &'a str,
    chapter: u16,
    label: &'a str,
    body_start: usize,
    open_idx: usize,
}

/// One verse occurrence yielded by `Document::walk_verses`.
struct VerseHit<'a> {
    reference: VerseRef,
    book: BookId,
    book_code: &'a str,
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
        let mut pending_space = Gap::None;
        // Set by an inline marker: the one space that delimits `\pn ` from its
        // content is syntax, not text, so it must not become a separator.
        let mut at_delimiter = false;
        while i < nodes.len() {
            let node = &nodes[i];
            if node.start as usize >= self.boundary {
                break;
            }
            match node.kind {
                NodeKind::Text => {
                    let mut text = node.span().text(doc.source());
                    if at_delimiter {
                        at_delimiter = false;
                        if let Some(c) = text.chars().next().filter(|c| c.is_whitespace()) {
                            text = &text[c.len_utf8()..];
                        }
                    }
                    push_collapsed(text, buf, start_len, &mut pending_space);
                    i += 1;
                }
                NodeKind::MarkerOpen => {
                    match doc.table.info(node.marker).map(|m| m.text_role) {
                        Some(TextRole::Excluded) => {
                            i = (node.link as usize) + 1; // skip subtree
                        }
                        // Markers contribute no characters of their own; spacing comes
                        // from the source whitespace around them (`push_collapsed`).
                        // A paragraph boundary always sits on whitespace, so only
                        // inline (and unknown) markers need their delimiter dropped:
                        // `王后\pn 以斯帖\pn*` reads `王后以斯帖`, not `王后 以斯帖`.
                        Some(TextRole::Boundary) => i += 1,
                        _ => {
                            at_delimiter = true;
                            i += 1;
                        }
                    }
                }
                _ => i += 1,
            }
        }
    }
}

/// Whitespace seen since the last character written, carried across nodes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gap {
    None,
    /// Only line breaks: layout, as in aligned USFM's one-word-per-line form.
    LineBreak,
    /// At least one space or tab: a word break the author typed.
    Space,
}

/// Append `s` to `buf`, collapsing each run of whitespace to at most one space.
/// A separator is emitted only before a non-space character that follows
/// whitespace *and* only when the verse already has content — so leading and
/// trailing whitespace are dropped, and adjacent characters split solely by a
/// (dropped) marker stay adjacent. A gap of line breaks alone is not emitted
/// before punctuation that attaches to the preceding word, so
/// `\w Paul\w*⏎\zaln-e\*,` reads `Paul,` rather than `Paul ,`. `verse_start`
/// marks where this verse began in the shared arena; `gap` carries the
/// whitespace state across the node boundary.
fn push_collapsed(s: &str, buf: &mut String, verse_start: usize, gap: &mut Gap) {
    for ch in s.chars() {
        if ch.is_whitespace() {
            if ch != '\n' && ch != '\r' {
                *gap = Gap::Space;
            } else if *gap == Gap::None {
                *gap = Gap::LineBreak;
            }
        } else {
            let separate = match *gap {
                Gap::None => false,
                Gap::LineBreak => !attaches_left(ch),
                Gap::Space => true,
            };
            if separate && buf.len() > verse_start {
                buf.push(' ');
            }
            *gap = Gap::None;
            buf.push(ch);
        }
    }
}

/// Punctuation that closes or trails the word before it. Straight quotes are
/// left out: they open as often as they close.
fn attaches_left(ch: char) -> bool {
    matches!(
        ch,
        ',' | '.'
            | ';'
            | ':'
            | '!'
            | '?'
            | ')'
            | ']'
            | '}'
            | '…'
            | '”'
            | '’'
            | '»'
            | '›'
            | '、'
            | '。'
            | '，'
            | '．'
            | '：'
            | '；'
            | '！'
            | '？'
            | '）'
            | '」'
            | '』'
            | '】'
            | '》'
            | '〉'
            | '،'
            | '؛'
            | '؟'
            | '۔'
            | '।'
            | '॥'
    )
}
