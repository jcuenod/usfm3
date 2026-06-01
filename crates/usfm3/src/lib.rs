//! `usfm3` — an error-tolerant, blazingly fast USFM parser.
//!
//! # Architecture (one tape, many views)
//!
//! Parsing produces a single flat [`tape::Tape`] of 16-byte records plus a list of
//! [`diagnostic::Diagnostic`]s. Everything else — the CST [`cst::Cursor`] for
//! editors/linters, USX and USJ serialization, and the `cat`-speed verse-text
//! extraction — is a *view* over that tape and the original source buffer. Text is
//! held as zero-copy spans; there is no per-node allocation and no intermediate
//! model between the tape and any output.
//!
//! ```ignore
//! let doc = usfm3::parse(src, Version::V3_1);
//! for d in doc.diagnostics() { /* underline d.span */ }
//! let usx = doc.to_usx();
//! let vrefs = doc.vref_text();          // cleaned, version-correct
//! for (r, text) in vrefs.iter() { println!("{r} = {text}"); }
//! ```

pub mod cst;
pub mod diagnostic;
pub mod marker;
mod parse;
pub mod span;
pub mod tape;
pub mod vref;

use cst::Cursor;
use diagnostic::Diagnostic;
use marker::{Category, MarkerTable, TextRole};
use tape::{NodeKind, Tape};
use vref::VerseRef;
pub use vref::{BookId, VrefText};

mod serialize;

/// USFM specification version. Selects the [`MarkerTable`] and lint rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Version {
    V3_1,
    V3_2,
}

impl Default for Version {
    fn default() -> Self {
        Version::V3_1
    }
}

/// Parse `source` against the given USFM `version`.
///
/// Never fails: malformed input is recorded in [`Document::diagnostics`] and
/// recovered from. The returned [`Document`] borrows `source` for zero-copy views.
pub fn parse(source: &str, version: Version) -> Document<'_> {
    let table = MarkerTable::for_version(version);
    let out = parse::build::build(source, table);
    Document {
        source,
        version,
        table,
        tape: out.tape,
        diagnostics: out.diagnostics,
    }
}

/// A parsed USFM document: the tape, diagnostics, and zero-copy views over the
/// borrowed source.
pub struct Document<'src> {
    source: &'src str,
    version: Version,
    table: &'static MarkerTable,
    tape: Tape,
    diagnostics: Vec<Diagnostic>,
}

impl<'src> Document<'src> {
    pub fn version(&self) -> Version {
        self.version
    }

    pub fn source(&self) -> &'src str {
        self.source
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn tape(&self) -> &Tape {
        &self.tape
    }

    /// A cursor at the document root for CST traversal / typed-AST construction.
    pub fn cursor(&self) -> Cursor<'_> {
        Cursor::new(&self.tape, self.source, self.table, 0)
    }

    // --- serialization -----------------------------------------------------

    pub fn write_usx<W: std::fmt::Write>(&self, out: &mut W) -> std::fmt::Result {
        serialize::write_usx(self.cursor(), out)
    }
    pub fn write_usj<W: std::fmt::Write>(&self, out: &mut W) -> std::fmt::Result {
        serialize::write_usj(self.cursor(), out)
    }
    pub fn to_usx(&self) -> String {
        let mut s = String::new();
        let _ = self.write_usx(&mut s);
        s
    }
    pub fn to_usj(&self) -> String {
        let mut s = String::new();
        let _ = self.write_usj(&mut s);
        s
    }

    /// CST as JSON, for editor decoration (token spans, marker categories, tree
    /// structure). See [`serialize::write_cst_json`].
    pub fn write_cst_json<W: std::fmt::Write>(&self, out: &mut W) -> std::fmt::Result {
        serialize::write_cst_json(self.cursor(), out)
    }
    pub fn to_cst_json(&self) -> String {
        let mut s = String::new();
        let _ = self.write_cst_json(&mut s);
        s
    }

    /// Diagnostics (errors + warnings) as a JSON array. See
    /// [`serialize::write_diagnostics_json`].
    pub fn write_diagnostics_json<W: std::fmt::Write>(&self, out: &mut W) -> std::fmt::Result {
        serialize::write_diagnostics_json(&self.diagnostics, self.source, out)
    }
    pub fn to_diagnostics_json(&self) -> String {
        let mut s = String::new();
        let _ = self.write_diagnostics_json(&mut s);
        s
    }

    // --- verse references --------------------------------------------------

    /// Zero-copy raw verse slices: the contiguous source between each `\v` and the
    /// next verse/chapter boundary (markers and notes included verbatim).
    pub fn vrefs_raw(&self) -> Vec<(VerseRef, &'src str)> {
        let mut out = Vec::new();
        self.walk_verses(|hit| {
            out.push((hit.reference, &self.source[hit.raw_start..hit.raw_end]));
        });
        out
    }

    /// Cleaned verse text into a single arena: notes excluded, character-marker
    /// tags dropped (inner text kept), whitespace collapsed and trimmed. Driven by
    /// [`TextRole`], so it is correct per version automatically.
    pub fn vref_text(&self) -> VrefText {
        let mut buf = String::with_capacity(self.source.len());
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
        let nodes = &self.tape.nodes;
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
                    if let Some(code) = node.span().text(self.source).split_whitespace().next() {
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
                    let label = node.span().text(self.source).split_whitespace().nth(1).unwrap_or("");
                    pending = Some((reference, label, node.span().end() as usize, i));
                }
                _ => {}
            }
        }
        flush(&mut pending, book, chapter, self.source.len(), &mut f);
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

/// One verse occurrence yielded by [`Document::walk_verses`].
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
        let nodes = &doc.tape.nodes;
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
                    push_collapsed(node.span().text(doc.source), buf, start_len, &mut pending_space);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_exposes_views() {
        let doc = parse("\\id GEN\n\\c 1\n\\v 1 In the beginning", Version::V3_1);
        assert!(doc.diagnostics().is_empty());
        assert_eq!(doc.cursor().kind(), tape::NodeKind::Root);
        assert!(doc.to_usx().starts_with("<usx"));
        assert!(doc.to_usj().contains("\"type\":\"USJ\""));
        assert_eq!(
            doc.vref_dict(),
            vec![("GEN 1:1".to_string(), "In the beginning".to_string())]
        );
    }

    #[test]
    fn vref_strips_notes_and_char_tags() {
        let src = "\
\\id GEN
\\c 1
\\s1 The Creation
\\p
\\v 1 In the \\nd Lord\\nd*\\f + \\ft a footnote\\f* beginning.
\\v 2 And\\x + \\xt ref\\x* the earth.";
        let doc = parse(src, Version::V3_1);
        let dict = doc.vref_dict();
        assert_eq!(dict.len(), 2);
        // \nd content kept, footnote excluded, heading not attributed to a verse.
        assert_eq!(dict[0], ("GEN 1:1".into(), "In the Lord beginning.".into()));
        // cross-reference excluded.
        assert_eq!(dict[1], ("GEN 1:2".into(), "And the earth.".into()));
    }

    #[test]
    fn no_space_around_markers_lacking_whitespace() {
        // A footnote wedged between `.` and `”` (no surrounding whitespace) must
        // not introduce a space; poetry line breaks (\q + newline) must.
        let src = "\
\\id PSA
\\c 132
\\q1
\\v 5 until I find a place for the LORD,
\\q2 a dwelling for the Mighty One of Jacob.\\f + \\ft note\\f*”";
        let doc = parse(src, Version::V3_1);
        assert_eq!(
            doc.vref_dict(),
            vec![(
                "PSA 132:5".into(),
                "until I find a place for the LORD, a dwelling for the Mighty One of Jacob.”".into()
            )]
        );
    }

    #[test]
    fn dict_drops_empty_and_merges_duplicates() {
        // \v 1 has only a footnote → empty → dropped. \v 2 appears twice → merged.
        let src = "\\id GEN\n\\c 1\n\\v 1 \\f + \\ft note\\f*\n\\v 2 first part\n\\v 2 second part";
        let doc = parse(src, Version::V3_1);
        assert_eq!(
            doc.vref_dict(),
            vec![("GEN 1:2".into(), "first part second part".into())]
        );
    }

    #[test]
    fn milestone_does_not_error() {
        // \qt-s\* ... \qt-e\* milestones parse cleanly; the quoted text is kept.
        let doc = parse("\\c 1\n\\v 1 He said \\qt-s\\*Follow me\\qt-e\\* today.", Version::V3_1);
        assert!(doc.diagnostics().is_empty(), "{:?}", doc.diagnostics());
        assert_eq!(doc.vref_dict()[0].1, "He said Follow me today.");
    }

    #[test]
    fn warns_on_unclosed_char_marker() {
        // \add never closed; the \p block boundary closes it implicitly → warning.
        let doc = parse("\\c 1\n\\v 1 \\add words\n\\p more", Version::V3_1);
        assert!(doc.diagnostics().iter().any(|d| d.code == "unclosed-marker"));
    }

    #[test]
    fn verse_ref_packs_and_formats() {
        let r = VerseRef::new(BookId::from_code("GEN").unwrap(), 1, 1);
        assert_eq!(r.to_string(), "GEN 1:1");
        assert_eq!(r.chapter(), 1);
        assert_eq!(r.verse(), 1);
    }
}
