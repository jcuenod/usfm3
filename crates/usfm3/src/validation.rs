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
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    // NT
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV", // Deuterocanonical / Apocrypha
    "TOB", "JDT", "ESG", "WIS", "SIR", "BAR", "LJE", "S3Y", "SUS", "BEL", "1MA", "2MA", "3MA",
    "4MA", "1ES", "2ES", "MAN", "PS2", "ODA", "PSS", "EZA", "5EZ", "6EZ", "DAG", "PS3", "2BA",
    "LBA", "JUB", "ENO", "1MQ", "2MQ", "3MQ", "REP", "4BA", "LAO", // Peripheral
    "FRT", "BAK", "OTH", "INT", "CNC", "GLO", "TDX", "NDX",
];

/// Markers that are *exclusively* note sub-markers and should never appear
/// outside of a `\f` or `\x` note.
const NOTE_ONLY_MARKERS: &[&str] = &[
    "fr", "ft", "fk", "fq", "fqa", "fl", "fw", "fp", "fv", "fdc", "xop", "xot", "xnt", "xdc",
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

/// Parse numbered table-cell markers like `th1`, `tc3`, or `tcr1-2`.
///
/// Returns `(start_col, end_col)` where `end_col == start_col` for cells that
/// do not span multiple columns.
fn parse_table_cell_columns(marker: &str) -> Option<(u32, u32)> {
    let (base, end_col) = if let Some(dash) = marker.rfind('-') {
        let after_dash = &marker[dash + 1..];
        if !after_dash.is_empty() && after_dash.chars().all(|c| c.is_ascii_digit()) {
            (&marker[..dash], after_dash.parse::<u32>().ok())
        } else {
            (marker, None)
        }
    } else {
        (marker, None)
    };

    let digit_start = base.find(|c: char| c.is_ascii_digit())?;
    let start_col = base[digit_start..].parse::<u32>().ok()?;
    let end_col = end_col.unwrap_or(start_col);
    Some((start_col, end_col.max(start_col)))
}

// ── Validator ───────────────────────────────────────────────────────────────

struct Validator<'a> {
    diagnostics: &'a mut DiagnosticList,
    expected_chapter: u32,
    seen_chapters: HashSet<u32>,
    expected_verse: u32,
    saw_book: bool,
    has_chapter: bool,
    body_started: bool,
    milestone_starts: HashMap<String, Vec<Span>>,
    milestone_ends: HashMap<String, Vec<Span>>,
}

impl<'a> Validator<'a> {
    fn new(diagnostics: &'a mut DiagnosticList) -> Self {
        Validator {
            diagnostics,
            expected_chapter: 1,
            seen_chapters: HashSet::new(),
            expected_verse: 1,
            saw_book: false,
            has_chapter: false,
            body_started: false,
            milestone_starts: HashMap::new(),
            milestone_ends: HashMap::new(),
        }
    }

    fn validate(&mut self, doc: &Document) {
        match doc.content.first() {
            Some(Node::Book { code, .. }) => {
                if !is_valid_book_code(code) {
                    self.diagnostics.push(Diagnostic::invalid_book_code(
                        code,
                        doc.content
                            .first()
                            .and_then(Node::span)
                            .cloned()
                            .unwrap_or(0..0),
                    ));
                }
            }
            Some(Node::Text(_)) => {
                self.diagnostics.push(Diagnostic::text_before_id(0..0));
                self.diagnostics.push(Diagnostic::missing_id_marker());
            }
            _ => {
                self.diagnostics.push(Diagnostic::missing_id_marker());
            }
        }

        for node in &doc.content {
            match node {
                Node::Book { .. } => {
                    if self.saw_book {
                        self.diagnostics.push(Diagnostic::duplicate_id(
                            node.span().cloned().unwrap_or(0..0),
                        ));
                    }
                    self.saw_book = true;
                }
                Node::Chapter { .. } => {
                    self.body_started = true;
                    self.has_chapter = true;
                    self.expected_verse = 1;
                    self.handle_chapter_sequence(node);
                }
                Node::Para { marker, .. } => {
                    if marker.kind() == MarkerKind::Header
                        && self.body_started
                        && !is_body_header_marker(marker.as_str())
                    {
                        self.diagnostics.push(Diagnostic::header_after_body(
                            marker.as_str(),
                            node.span().cloned().unwrap_or(0..0),
                        ));
                    }
                    if !self.has_chapter
                        && marker.kind() == MarkerKind::Paragraph
                        && !is_introduction_marker(marker.as_str())
                    {
                        self.diagnostics
                            .push(Diagnostic::body_paragraph_before_chapter(
                                marker.as_str(),
                                node.span().cloned().unwrap_or(0..0),
                            ));
                    }
                }
                _ => {}
            }

            if !matches!(node, Node::Chapter { .. }) {
                self.walk(node, false);
            }
        }

        if self.saw_book && !self.has_chapter {
            self.diagnostics.push(Diagnostic::missing_chapter_marker());
        }

        self.finish_milestone_pairs();
    }

    fn handle_chapter_sequence(&mut self, node: &Node) {
        let Node::Chapter { number, .. } = node else {
            return;
        };
        let Ok(num) = number.parse::<u32>() else {
            return;
        };
        if !self.seen_chapters.insert(num) {
            self.diagnostics.push(Diagnostic::duplicate_chapter(
                num,
                node.span().cloned().unwrap_or(0..0),
            ));
        }
        if num != self.expected_chapter {
            self.diagnostics.push(Diagnostic::invalid_chapter_sequence(
                self.expected_chapter,
                num,
                node.span().cloned().unwrap_or(0..0),
            ));
        }
        self.expected_chapter = num + 1;
    }

    fn walk(&mut self, node: &Node, inside_note: bool) {
        match node {
            Node::Verse { number, .. } => {
                let start = parse_verse_start(number);
                let end = parse_verse_end(number);
                let span = node.span().cloned().unwrap_or(0..0);
                if let Some(v_start) = start {
                    if v_start != self.expected_verse {
                        self.diagnostics.push(Diagnostic::invalid_verse_sequence(
                            &self.expected_verse.to_string(),
                            number,
                            span.clone(),
                        ));
                    }
                    self.expected_verse = end.unwrap_or(v_start) + 1;
                }
            }
            Node::Char {
                marker,
                content,
                attributes,
                ..
            } => {
                if !inside_note && is_note_only_marker(marker.as_str()) {
                    self.diagnostics
                        .push(Diagnostic::note_submarker_outside_note(
                            marker.as_str(),
                            node.span().cloned().unwrap_or(0..0),
                        ));
                }
                if content.iter().any(|n| matches!(n, Node::Verse { .. })) {
                    self.diagnostics.push(Diagnostic::char_crosses_verse(
                        marker.as_str(),
                        node.span().cloned().unwrap_or(0..0),
                    ));
                }

                let clean_marker = marker.strip_prefix('+').unwrap_or(marker.as_str());
                for &req in markers::required_attributes(clean_marker) {
                    if !attributes.iter().any(|a| a.key == req) {
                        self.diagnostics
                            .push(Diagnostic::missing_required_attribute(
                                clean_marker,
                                req,
                                node.span().cloned().unwrap_or(0..0),
                            ));
                    }
                }
                if attributes.iter().any(|a| a.key == "default")
                    && markers::default_attribute(clean_marker).is_none()
                {
                    self.diagnostics
                        .push(Diagnostic::default_attribute_not_defined(
                            clean_marker,
                            node.span().cloned().unwrap_or(0..0),
                        ));
                }
                for attr in attributes {
                    if !attr.value.is_empty() && attr.value.trim().is_empty() {
                        self.diagnostics.push(Diagnostic::malformed_attributes(
                            node.span().cloned().unwrap_or(0..0),
                        ));
                        break;
                    }
                }
                if clean_marker == "w" {
                    let has_text = content.iter().any(|n| {
                        if let Node::Text(s) = n {
                            !s.trim().is_empty()
                        } else {
                            true
                        }
                    });
                    if !has_text && attributes.is_empty() {
                        self.diagnostics.push(Diagnostic::empty_word_marker(
                            node.span().cloned().unwrap_or(0..0),
                        ));
                    }
                }
            }
            Node::Figure {
                content,
                attributes,
                marker,
                ..
            } => {
                let has_text = content.iter().any(|n| {
                    if let Node::Text(s) = n {
                        !s.trim().is_empty()
                    } else {
                        false
                    }
                });
                let has_meaningful_attrs = attributes
                    .iter()
                    .any(|a| a.value.chars().any(|c| c != '|' && !c.is_whitespace()));
                if !has_text && !has_meaningful_attrs {
                    self.diagnostics.push(Diagnostic::empty_figure(
                        node.span().cloned().unwrap_or(0..0),
                    ));
                }

                let clean_marker = marker.strip_prefix('+').unwrap_or(marker.as_str());
                if attributes.iter().any(|a| a.key == "default")
                    && markers::default_attribute(clean_marker).is_none()
                {
                    self.diagnostics
                        .push(Diagnostic::default_attribute_not_defined(
                            clean_marker,
                            node.span().cloned().unwrap_or(0..0),
                        ));
                }
            }
            Node::Milestone { marker, .. } => {
                if let Some(base) = marker.strip_suffix("-s") {
                    self.milestone_starts
                        .entry(base.to_string())
                        .or_default()
                        .push(node.span().cloned().unwrap_or(0..0));
                } else if let Some(base) = marker.strip_suffix("-e") {
                    self.milestone_ends
                        .entry(base.to_string())
                        .or_default()
                        .push(node.span().cloned().unwrap_or(0..0));
                }
            }
            Node::Para {
                marker, content, ..
            } => {
                if marker == "b" && !content.is_empty() {
                    self.diagnostics.push(Diagnostic::non_empty_blank_line(
                        node.span().cloned().unwrap_or(0..0),
                    ));
                }
            }
            Node::Table { content, .. } => {
                for row in content {
                    self.check_table_row_columns(row);
                }
            }
            _ => {}
        }
        let is_note = matches!(node, Node::Note { .. });
        for child in node.children() {
            self.walk(child, inside_note || is_note);
        }
    }

    fn check_table_row_columns(&mut self, row: &Node) {
        let Node::TableRow { content, .. } = row else {
            return;
        };

        let mut expected_col = 1;
        for cell in content {
            let Node::TableCell { marker, .. } = cell else {
                continue;
            };
            let Some((start_col, end_col)) = parse_table_cell_columns(marker.as_str()) else {
                continue;
            };
            if start_col != expected_col {
                self.diagnostics
                    .push(Diagnostic::invalid_table_column_sequence(
                        expected_col,
                        start_col,
                        cell.span().cloned().unwrap_or(0..0),
                    ));
            }
            expected_col = end_col + 1;
        }
    }

    fn finish_milestone_pairs(&mut self) {
        for (base, spans) in &self.milestone_starts {
            let end_count = self.milestone_ends.get(base).map_or(0, |v| v.len());
            if spans.len() > end_count {
                for span in spans.iter().skip(end_count) {
                    let marker = format!("{}-s", base);
                    self.diagnostics
                        .push(Diagnostic::milestone_mismatch(&marker, span.clone()));
                }
            }
        }
        for (base, spans) in &self.milestone_ends {
            let start_count = self.milestone_starts.get(base).map_or(0, |v| v.len());
            if spans.len() > start_count {
                for span in spans.iter().skip(start_count) {
                    let marker = format!("{}-e", base);
                    self.diagnostics
                        .push(Diagnostic::milestone_mismatch(&marker, span.clone()));
                }
            }
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

/// Markers classified as Header that legitimately appear after the body
/// has started (i.e., after `\c`). These are not flagged by the
/// header-after-body check.
fn is_body_header_marker(marker: &str) -> bool {
    matches!(marker, "cl" | "cd" | "cp" | "mte" | "mte1" | "mte2")
}

/// Introduction paragraph markers that are allowed before the first `\c`.
/// All USFM introduction markers start with 'i'.
fn is_introduction_marker(marker: &str) -> bool {
    marker.starts_with('i')
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
            spans: NodeSpans::node(0..2),
        }]);
        let diags = validate(&doc);
        assert!(diags.has_errors());
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MissingIdMarker)
        );
    }

    #[test]
    fn test_empty_document() {
        let doc = doc_with(vec![]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MissingIdMarker)
        );
    }

    // -- 2. Book code validation ---------------------------------------------

    #[test]
    fn test_valid_book_code() {
        let doc = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
            spans: NodeSpans::node(0..10).with_code(0..0),
        }]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidBookCode)
        );
    }

    #[test]
    fn test_invalid_book_code() {
        let doc = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "XYZ".into(),
            content: vec![],
            spans: NodeSpans::node(0..10).with_code(0..0),
        }]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidBookCode)
        );
    }

    // -- 3. Chapter sequence -------------------------------------------------

    #[test]
    fn test_chapter_sequence_valid() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "2".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(20..24).with_number(0..0),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidChapterSequence)
        );
    }

    #[test]
    fn test_chapter_sequence_gap() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "3".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(20..24).with_number(0..0),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidChapterSequence)
        );
    }

    #[test]
    fn test_duplicate_chapter() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(20..24).with_number(0..0),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::DuplicateChapter)
        );
    }

    // -- 4. Verse sequence ---------------------------------------------------

    #[test]
    fn test_verse_sequence_valid() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(15..18).with_number(0..0),
                    },
                    Node::text("Text"),
                    Node::Verse {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(25..28).with_number(0..0),
                    },
                    Node::text("More text"),
                ],
                spans: NodeSpans::node(14..40),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidVerseSequence)
        );
    }

    #[test]
    fn test_verse_sequence_gap() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(15..18).with_number(0..0),
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "3".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(25..28).with_number(0..0),
                    },
                ],
                spans: NodeSpans::node(14..40),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidVerseSequence)
        );
    }

    #[test]
    fn test_verse_range_resets_expected() {
        // After "3-4", the next expected verse is 5.
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(15..18).with_number(0..0),
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(19..22).with_number(0..0),
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "3-4".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(23..28).with_number(0..0),
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "5".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(29..32).with_number(0..0),
                    },
                ],
                spans: NodeSpans::node(14..40),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidVerseSequence)
        );
    }

    #[test]
    fn test_verse_resets_at_new_chapter() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(15..18).with_number(0..0),
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(19..22).with_number(0..0),
                    },
                ],
                spans: NodeSpans::node(14..30),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "2".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(30..34).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Verse {
                    marker: "v".into(),
                    number: "1".into(),
                    sid: None,
                    altnumber: None,
                    pubnumber: None,
                    spans: NodeSpans::node(35..38).with_number(0..0),
                }],
                spans: NodeSpans::node(34..45),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidVerseSequence)
        );
    }

    #[test]
    fn test_verse_must_restart_at_one_after_new_chapter() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Verse {
                    marker: "v".into(),
                    number: "2".into(),
                    sid: None,
                    altnumber: None,
                    pubnumber: None,
                    spans: NodeSpans::node(15..18).with_number(0..0),
                }],
                spans: NodeSpans::node(14..25),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidVerseSequence)
        );
    }

    #[test]
    fn test_verse_range_can_open_chapter() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1-2".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(15..20).with_number(0..0),
                    },
                    Node::Verse {
                        marker: "v".into(),
                        number: "3".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(21..24).with_number(0..0),
                    },
                ],
                spans: NodeSpans::node(14..30),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidVerseSequence)
        );
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
                spans: NodeSpans::node(10..20).with_code(0..0),
            },
        ]);
        let diags = validate(&doc);
        assert!(diags.iter().any(|d| d.code == DiagnosticCode::TextBeforeId));
    }

    #[test]
    fn test_no_text_before_id() {
        let doc = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
            spans: NodeSpans::node(0..10).with_code(0..0),
        }]);
        let diags = validate(&doc);
        assert!(!diags.iter().any(|d| d.code == DiagnosticCode::TextBeforeId));
    }

    // -- 6. Header after body ------------------------------------------------

    #[test]
    fn test_header_after_body() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
                spans: NodeSpans::node(14..25),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::HeaderAfterBody)
        );
    }

    #[test]
    fn test_header_before_body_ok() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
                spans: NodeSpans::node(10..21),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(21..25).with_number(0..0),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::HeaderAfterBody)
        );
    }

    #[test]
    fn test_rem_before_h_no_false_positive() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Para {
                marker: "rem".into(),
                content: vec![Node::text("A remark")],
                spans: NodeSpans::node(10..25),
            },
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
                spans: NodeSpans::node(25..36),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::HeaderAfterBody),
            "\\rem before \\h should not trigger header-after-body"
        );
    }

    #[test]
    fn test_intro_para_before_h_no_false_positive() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Para {
                marker: "ip".into(),
                content: vec![Node::text("Introduction paragraph")],
                spans: NodeSpans::node(10..40),
            },
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
                spans: NodeSpans::node(40..51),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::HeaderAfterBody),
            "\\ip before \\h should not trigger header-after-body"
        );
    }

    #[test]
    fn test_cl_after_chapter_no_false_positive() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
                spans: NodeSpans::node(10..21),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(21..25).with_number(0..0),
            },
            Node::Para {
                marker: "cl".into(),
                content: vec![Node::text("Chapter One")],
                spans: NodeSpans::node(25..40),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::HeaderAfterBody),
            "\\cl after \\c should not trigger header-after-body"
        );
    }

    #[test]
    fn test_mte_at_end_of_book_no_false_positive() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::text("Content")],
                spans: NodeSpans::node(14..25),
            },
            Node::Para {
                marker: "mte1".into(),
                content: vec![Node::text("End of Genesis")],
                spans: NodeSpans::node(25..45),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::HeaderAfterBody),
            "\\mte1 at end of book should not trigger header-after-body"
        );
    }

    // -- 7. Note sub-markers outside notes -----------------------------------

    #[test]
    fn test_note_submarker_outside_note() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Char {
                    marker: "ft".into(),
                    content: vec![Node::text("footnote text")],
                    attributes: vec![],
                    spans: NodeSpans::node(15..30),
                }],
                spans: NodeSpans::node(10..35),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::NoteSubmarkerOutsideNote)
        );
    }

    #[test]
    fn test_note_submarker_inside_note_ok() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Note {
                    marker: "f".into(),
                    caller: "+".into(),
                    category: None,
                    content: vec![Node::Char {
                        marker: "ft".into(),
                        content: vec![Node::text("footnote text")],
                        attributes: vec![],
                        spans: NodeSpans::node(20..35),
                    }],
                    spans: NodeSpans::node(15..40),
                }],
                spans: NodeSpans::node(10..45),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::NoteSubmarkerOutsideNote)
        );
    }

    #[test]
    fn test_regular_char_marker_outside_note_ok() {
        // \nd is a regular character marker, not a note-only marker.
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Char {
                    marker: "nd".into(),
                    content: vec![Node::text("Lord")],
                    attributes: vec![],
                    spans: NodeSpans::node(15..25),
                }],
                spans: NodeSpans::node(10..30),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::NoteSubmarkerOutsideNote)
        );
    }

    // -- 8. Milestone pair matching ------------------------------------------

    #[test]
    fn test_milestone_matched_pair() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Milestone {
                marker: "qt1-s".into(),
                attributes: vec![],
                spans: NodeSpans::node(10..20),
            },
            Node::Milestone {
                marker: "qt1-e".into(),
                attributes: vec![],
                spans: NodeSpans::node(30..40),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MilestoneMismatch)
        );
    }

    #[test]
    fn test_milestone_unmatched_start() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Milestone {
                marker: "qt1-s".into(),
                attributes: vec![],
                spans: NodeSpans::node(10..20),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MilestoneMismatch)
        );
    }

    #[test]
    fn test_milestone_unmatched_end() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Milestone {
                marker: "qt1-e".into(),
                attributes: vec![],
                spans: NodeSpans::node(10..20),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::MilestoneMismatch)
        );
    }

    #[test]
    fn test_table_column_sequence_gap() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Table {
                content: vec![Node::TableRow {
                    marker: "tr".into(),
                    content: vec![
                        Node::TableCell {
                            marker: "th1".into(),
                            align: "start".into(),
                            content: vec![Node::text("header1 ")],
                            spans: NodeSpans::node(10..20),
                        },
                        Node::TableCell {
                            marker: "th3".into(),
                            align: "start".into(),
                            content: vec![Node::text("header3")],
                            spans: NodeSpans::node(21..31),
                        },
                    ],
                    spans: NodeSpans::node(10..31),
                }],
                spans: NodeSpans::node(10..31),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidTableColumnSequence)
        );
    }

    #[test]
    fn test_table_column_sequence_with_span_is_valid() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Table {
                content: vec![Node::TableRow {
                    marker: "tr".into(),
                    content: vec![
                        Node::TableCell {
                            marker: "tcr1-2".into(),
                            align: "end".into(),
                            content: vec![Node::text("Total: ")],
                            spans: NodeSpans::node(10..24),
                        },
                        Node::TableCell {
                            marker: "tcc3".into(),
                            align: "center".into(),
                            content: vec![Node::text("186,400")],
                            spans: NodeSpans::node(25..34),
                        },
                    ],
                    spans: NodeSpans::node(10..34),
                }],
                spans: NodeSpans::node(10..34),
            },
        ]);
        let diags = validate(&doc);
        assert!(
            !diags
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidTableColumnSequence)
        );
    }

    // -- Integration: valid document produces no diagnostics ------------------

    #[test]
    fn test_valid_document_no_warnings() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
                spans: NodeSpans::node(0..10).with_code(0..0),
            },
            Node::Chapter {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
                spans: NodeSpans::node(10..14).with_number(0..0),
            },
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                        spans: NodeSpans::node(15..18).with_number(0..0),
                    },
                    Node::text("In the beginning"),
                ],
                spans: NodeSpans::node(14..40),
            },
        ]);
        let diags = validate(&doc);
        assert!(diags.is_empty());
    }
}
