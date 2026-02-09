/// Semantic validation pass for parsed USFM documents.
///
/// This module runs **after** parsing. The parser always produces a tree;
/// validation checks that the tree makes semantic sense (correct book codes,
/// sequential chapter/verse numbering, milestone pairing, etc.) and emits
/// diagnostics for anything it finds.

use std::collections::{HashMap, HashSet};

use crate::ast::{Document, Node, Span};
use crate::diagnostics::{Diagnostic, DiagnosticList};
use crate::markers::{self, MarkerKind};

// ── Public entry point ──────────────────────────────────────────────────────

/// Validate a parsed USFM document and return any diagnostics.
pub fn validate(doc: &Document) -> DiagnosticList {
    let mut diagnostics = DiagnosticList::new();
    let mut validator = Validator::new(&mut diagnostics);
    validator.validate(doc);
    diagnostics
}

// ── Valid book codes ────────────────────────────────────────────────────────

/// The set of valid 3-letter book codes (standard 66 + deuterocanonical +
/// peripheral).
const VALID_BOOK_CODES: &[&str] = &[
    // OT
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA",
    "1KI", "2KI", "1CH", "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO",
    "ECC", "SNG", "ISA", "JER", "LAM", "EZK", "DAN", "HOS", "JOL", "AMO",
    "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    // NT
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH",
    "PHP", "COL", "1TH", "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS",
    "1PE", "2PE", "1JN", "2JN", "3JN", "JUD", "REV",
    // Deuterocanonical / Apocrypha
    "TOB", "JDT", "ESG", "WIS", "SIR", "BAR", "LJE", "S3Y", "SUS", "BEL",
    "1MA", "2MA", "3MA", "4MA", "1ES", "2ES", "MAN", "PS2", "ODA", "PSS",
    "EZA", "5EZ", "6EZ", "DAG", "PS3", "2BA", "LBA", "JUB", "ENO",
    "1MQ", "2MQ", "3MQ", "REP", "4BA", "LAO",
    // Peripheral
    "FRT", "BAK", "OTH", "INT", "CNC", "GLO", "TDX", "NDX",
];

/// Markers that are *exclusively* note sub-markers and should never appear
/// outside of a `\f` or `\x` note.
const NOTE_ONLY_MARKERS: &[&str] = &[
    "fr", "ft", "fk", "fq", "fqa", "fl", "fw", "fp", "fv", "fdc",
    "xop", "xot", "xnt", "xdc",
];

// ── Verse number helpers ────────────────────────────────────────────────────

/// Parse the leading integer from a verse number string.
///
/// `"1"` -> `Some(1)`, `"3-4"` -> `Some(3)`, `"1a"` -> `Some(1)`,
/// `"abc"` -> `None`
fn parse_verse_start(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Parse the ending integer from a verse range string. If the string
/// contains a hyphen, the number after the hyphen is parsed; otherwise the
/// leading integer is returned (same as `parse_verse_start`).
///
/// `"3-4"` -> `Some(4)`, `"1"` -> `Some(1)`, `"2b-3a"` -> `Some(3)`
fn parse_verse_end(s: &str) -> Option<u32> {
    if let Some(pos) = s.find('-') {
        parse_verse_start(&s[pos + 1..])
    } else {
        parse_verse_start(s)
    }
}

// ── Validator ───────────────────────────────────────────────────────────────

struct Validator<'a> {
    diagnostics: &'a mut DiagnosticList,
}

impl<'a> Validator<'a> {
    fn new(diagnostics: &'a mut DiagnosticList) -> Self {
        Validator { diagnostics }
    }

    fn validate(&mut self, doc: &Document) {
        self.check_id_marker(doc);
        self.check_chapter_sequence(doc);
        self.check_verse_sequence(doc);
        self.check_text_before_id(doc);
        self.check_headers_after_body(doc);
        self.check_note_submarkers(doc);
        self.check_milestone_pairs(doc);
    }

    // ── 1. \id must be the first marker ─────────────────────────────────

    fn check_id_marker(&mut self, doc: &Document) {
        match doc.content.first() {
            Some(Node::Book { code, span, .. }) => {
                // Validate the book code (check 2).
                if !is_valid_book_code(code) {
                    self.diagnostics
                        .push(Diagnostic::invalid_book_code(code, span.clone()));
                }
            }
            _ => {
                // Missing or non-Book first node.
                self.diagnostics.push(Diagnostic::missing_id_marker());
            }
        }
    }

    // ── 3. Chapter sequence ─────────────────────────────────────────────

    fn check_chapter_sequence(&mut self, doc: &Document) {
        let mut expected: u32 = 1;
        let mut seen = HashSet::new();

        for node in &doc.content {
            if let Node::Chapter { number, span, .. } = node {
                if let Ok(num) = number.parse::<u32>() {
                    // Duplicate check.
                    if !seen.insert(num) {
                        self.diagnostics
                            .push(Diagnostic::duplicate_chapter(num, span.clone()));
                    }
                    // Sequence check.
                    if num != expected {
                        self.diagnostics.push(Diagnostic::invalid_chapter_sequence(
                            expected,
                            num,
                            span.clone(),
                        ));
                    }
                    expected = num + 1;
                }
            }
        }
    }

    // ── 4. Verse sequence (per chapter scope) ───────────────────────────

    fn check_verse_sequence(&mut self, doc: &Document) {
        let mut expected_verse: Option<u32> = None;

        for node in &doc.content {
            match node {
                Node::Chapter { .. } => {
                    // Reset verse tracking for each chapter.
                    expected_verse = None;
                }
                _ => {
                    self.check_verses_in_node(node, &mut expected_verse);
                }
            }
        }
    }

    /// Recursively walk a node and its children looking for `Verse` nodes.
    fn check_verses_in_node(&mut self, node: &Node, expected_verse: &mut Option<u32>) {
        if let Node::Verse { number, span, .. } = node {
            let start = parse_verse_start(number);
            let end = parse_verse_end(number);
            if let Some(v_start) = start {
                match *expected_verse {
                    Some(exp) if v_start != exp => {
                        self.diagnostics.push(Diagnostic::invalid_verse_sequence(
                            &exp.to_string(),
                            number,
                            span.clone(),
                        ));
                    }
                    _ => {}
                }
                // Next expected verse is end-of-range + 1 (or start + 1).
                *expected_verse = Some(end.unwrap_or(v_start) + 1);
            }
        }

        for child in node.children() {
            self.check_verses_in_node(child, expected_verse);
        }
    }

    // ── 5. Text before \id ──────────────────────────────────────────────

    fn check_text_before_id(&mut self, doc: &Document) {
        if let Some(first) = doc.content.first() {
            if matches!(first, Node::Text(_)) {
                self.diagnostics
                    .push(Diagnostic::text_before_id(0..0));
            }
        }
    }

    // ── 6. Header markers after body content ────────────────────────────

    fn check_headers_after_body(&mut self, doc: &Document) {
        let mut body_started = false;

        for node in &doc.content {
            match node {
                Node::Chapter { .. } | Node::Verse { .. } => {
                    body_started = true;
                }
                Node::Para { marker, span, .. } => {
                    let info = markers::lookup_marker(marker);
                    if info.kind == MarkerKind::Header {
                        if body_started {
                            self.diagnostics
                                .push(Diagnostic::header_after_body(marker, span.clone()));
                        }
                    } else {
                        // Any non-header paragraph starts the body.
                        body_started = true;
                    }
                }
                Node::Book { .. } => {
                    // \id is always first and does not start the body.
                }
                _ => {
                    // Other node types do not start the body by themselves
                    // unless they contain body-level content.
                }
            }
        }
    }

    // ── 7. Note sub-markers outside notes ───────────────────────────────

    fn check_note_submarkers(&mut self, doc: &Document) {
        for node in &doc.content {
            self.walk_note_submarkers(node, false);
        }
    }

    fn walk_note_submarkers(&mut self, node: &Node, inside_note: bool) {
        if let Node::Char { marker, span, .. } = node {
            if !inside_note && is_note_only_marker(marker) {
                self.diagnostics
                    .push(Diagnostic::note_submarker_outside_note(marker, span.clone()));
            }
        }

        let is_note = matches!(node, Node::Note { .. });
        for child in node.children() {
            self.walk_note_submarkers(child, inside_note || is_note);
        }
    }

    // ── 8. Milestone pair matching ──────────────────────────────────────

    fn check_milestone_pairs(&mut self, doc: &Document) {
        let mut starts: HashMap<String, Vec<Span>> = HashMap::new();
        let mut ends: HashMap<String, Vec<Span>> = HashMap::new();

        self.collect_milestones(&doc.content, &mut starts, &mut ends);

        // For each start marker, check that there is a matching end.
        for (base, spans) in &starts {
            let end_count = ends.get(base).map_or(0, |v| v.len());
            if spans.len() > end_count {
                // More starts than ends -- report unmatched starts.
                for span in spans.iter().skip(end_count) {
                    let marker = format!("{}-s", base);
                    self.diagnostics
                        .push(Diagnostic::milestone_mismatch(&marker, span.clone()));
                }
            }
        }

        // For each end marker, check that there is a matching start.
        for (base, spans) in &ends {
            let start_count = starts.get(base).map_or(0, |v| v.len());
            if spans.len() > start_count {
                for span in spans.iter().skip(start_count) {
                    let marker = format!("{}-e", base);
                    self.diagnostics
                        .push(Diagnostic::milestone_mismatch(&marker, span.clone()));
                }
            }
        }
    }

    /// Recursively collect all milestone nodes, split into start and end
    /// buckets by their base marker name.
    fn collect_milestones(
        &self,
        nodes: &[Node],
        starts: &mut HashMap<String, Vec<Span>>,
        ends: &mut HashMap<String, Vec<Span>>,
    ) {
        for node in nodes {
            if let Node::Milestone { marker, span, .. } = node {
                if let Some(base) = marker.strip_suffix("-s") {
                    starts
                        .entry(base.to_string())
                        .or_default()
                        .push(span.clone());
                } else if let Some(base) = marker.strip_suffix("-e") {
                    ends.entry(base.to_string())
                        .or_default()
                        .push(span.clone());
                }
            }
            self.collect_milestones(node.children(), starts, ends);
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn is_valid_book_code(code: &str) -> bool {
    VALID_BOOK_CODES.contains(&code)
}

fn is_note_only_marker(marker: &str) -> bool {
    NOTE_ONLY_MARKERS.contains(&marker)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::diagnostics::DiagnosticCode;

    fn doc_with(nodes: Vec<Node>) -> Document {
        Document { content: nodes }
    }

    // -- parse_verse_start / parse_verse_end ---------------------------------

    #[test]
    fn test_parse_verse_start_simple() {
        assert_eq!(parse_verse_start("1"), Some(1));
        assert_eq!(parse_verse_start("12"), Some(12));
    }

    #[test]
    fn test_parse_verse_start_range() {
        assert_eq!(parse_verse_start("3-4"), Some(3));
    }

    #[test]
    fn test_parse_verse_start_with_letter() {
        assert_eq!(parse_verse_start("1a"), Some(1));
        assert_eq!(parse_verse_start("2b"), Some(2));
    }

    #[test]
    fn test_parse_verse_start_no_digits() {
        assert_eq!(parse_verse_start("abc"), None);
    }

    #[test]
    fn test_parse_verse_end_simple() {
        assert_eq!(parse_verse_end("1"), Some(1));
    }

    #[test]
    fn test_parse_verse_end_range() {
        assert_eq!(parse_verse_end("3-4"), Some(4));
        assert_eq!(parse_verse_end("2b-3a"), Some(3));
    }

    // -- 1. Missing \id marker -----------------------------------------------

    #[test]
    fn test_missing_id() {
        let doc = doc_with(vec![Node::Para {
            marker: "p".into(),
            content: vec![],
            span: 0..2,
        }]);
        let diags = validate(&doc);
        assert!(diags.has_errors());
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::MissingIdMarker));
    }

    #[test]
    fn test_empty_document() {
        let doc = doc_with(vec![]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::MissingIdMarker));
    }

    // -- 2. Book code validation ---------------------------------------------

    #[test]
    fn test_valid_book_code() {
        let doc = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
            span: 0..10,
        }]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidBookCode));
    }

    #[test]
    fn test_invalid_book_code() {
        let doc = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "XYZ".into(),
            content: vec![],
            span: 0..10,
        }]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidBookCode));
    }

    // -- 3. Chapter sequence -------------------------------------------------

    #[test]
    fn test_chapter_sequence_valid() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "2".into(),
                sid: None,
                span: 20..24,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidChapterSequence));
    }

    #[test]
    fn test_chapter_sequence_gap() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "3".into(),
                sid: None,
                span: 20..24,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidChapterSequence));
    }

    #[test]
    fn test_duplicate_chapter() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 20..24,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::DuplicateChapter));
    }

    // -- 4. Verse sequence ---------------------------------------------------

    #[test]
    fn test_verse_sequence_valid() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        span: 15..18,
                    },
                    Node::text("Text"),
                    Node::Verse {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        span: 25..28,
                    },
                    Node::text("More text"),
                ],
                span: 14..40,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidVerseSequence));
    }

    #[test]
    fn test_verse_sequence_gap() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        span: 15..18,
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "3".into(),
                        sid: None,
                        span: 25..28,
                    },
                ],
                span: 14..40,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidVerseSequence));
    }

    #[test]
    fn test_verse_range_resets_expected() {
        // After "3-4", the next expected verse is 5.
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        span: 15..18,
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        span: 19..22,
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "3-4".into(),
                        sid: None,
                        span: 23..28,
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "5".into(),
                        sid: None,
                        span: 29..32,
                    },
                ],
                span: 14..40,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidVerseSequence));
    }

    #[test]
    fn test_verse_resets_at_new_chapter() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        span: 15..18,
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        span: 19..22,
                    },
                ],
                span: 14..30,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "2".into(),
                sid: None,
                span: 30..34,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Verse {
                    marker: "v".into(),
                    number: "1".into(),
                    sid: None,
                    span: 35..38,
                }],
                span: 34..45,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidVerseSequence));
    }

    // -- 5. Text before \id -------------------------------------------------

    #[test]
    fn test_text_before_id() {
        let doc = doc_with(vec![
            Node::text("stray text"),
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 10..20,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::TextBeforeId));
    }

    #[test]
    fn test_no_text_before_id() {
        let doc = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
            span: 0..10,
        }]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::TextBeforeId));
    }

    // -- 6. Header after body ------------------------------------------------

    #[test]
    fn test_header_after_body() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
                span: 14..25,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::HeaderAfterBody));
    }

    #[test]
    fn test_header_before_body_ok() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
                span: 10..21,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 21..25,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::HeaderAfterBody));
    }

    // -- 7. Note sub-markers outside notes -----------------------------------

    #[test]
    fn test_note_submarker_outside_note() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Char {
                    marker: "ft".into(),
                    content: vec![Node::text("footnote text")],
                    span: 15..30,
                }],
                span: 10..35,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::NoteSubmarkerOutsideNote));
    }

    #[test]
    fn test_note_submarker_inside_note_ok() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Note {
                    marker: "f".into(),
                    caller: "+".into(),
                    content: vec![Node::Char {
                        marker: "ft".into(),
                        content: vec![Node::text("footnote text")],
                        span: 20..35,
                    }],
                    span: 15..40,
                }],
                span: 10..45,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::NoteSubmarkerOutsideNote));
    }

    #[test]
    fn test_regular_char_marker_outside_note_ok() {
        // \nd is a regular character marker, not a note-only marker.
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Char {
                    marker: "nd".into(),
                    content: vec![Node::text("Lord")],
                    span: 15..25,
                }],
                span: 10..30,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::NoteSubmarkerOutsideNote));
    }

    // -- 8. Milestone pair matching ------------------------------------------

    #[test]
    fn test_milestone_matched_pair() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Milestone {
                marker: "qt1-s".into(),
                attributes: vec![],
                span: 10..20,
            },
            Node::Milestone {
                marker: "qt1-e".into(),
                attributes: vec![],
                span: 30..40,
            },
        ]);
        let diags = validate(&doc);
        assert!(!diags
            .iter()
            .any(|d| d.code == DiagnosticCode::MilestoneMismatch));
    }

    #[test]
    fn test_milestone_unmatched_start() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Milestone {
                marker: "qt1-s".into(),
                attributes: vec![],
                span: 10..20,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::MilestoneMismatch));
    }

    #[test]
    fn test_milestone_unmatched_end() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Milestone {
                marker: "qt1-e".into(),
                attributes: vec![],
                span: 10..20,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags
            .iter()
            .any(|d| d.code == DiagnosticCode::MilestoneMismatch));
    }

    // -- Integration: valid document produces no diagnostics ------------------

    #[test]
    fn test_valid_document_no_warnings() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                span: 0..10,
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                span: 10..14,
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        span: 15..18,
                    },
                    Node::text("In the beginning"),
                ],
                span: 14..40,
            },
        ]);
        let diags = validate(&doc);
        assert!(diags.is_empty());
    }
}
