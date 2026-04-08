/// Semantic validation pass for parsed USFM documents.
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ast::{Document, Node};
use crate::diagnostics::{Diagnostic, DiagnosticList, Span};
use crate::markers::{self, MarkerKind};
use crate::source_map::{SourceMap, SourceNode};

pub fn validate(doc: &Document, source_map: &SourceMap) -> DiagnosticList {
    let mut diagnostics = DiagnosticList::new();
    let mut validator = Validator::new(&mut diagnostics);
    validator.validate(doc, source_map);
    diagnostics
}

const VALID_BOOK_CODES: &[&str] = &[
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV", "TOB", "JDT", "ESG", "WIS", "SIR", "BAR", "LJE", "S3Y", "SUS", "BEL", "1MA", "2MA",
    "3MA", "4MA", "1ES", "2ES", "MAN", "PS2", "ODA", "PSS", "EZA", "5EZ", "6EZ", "DAG", "PS3",
    "2BA", "LBA", "JUB", "ENO", "1MQ", "2MQ", "3MQ", "REP", "4BA", "LAO", "FRT", "BAK", "OTH",
    "INT", "CNC", "GLO", "TDX", "NDX",
];

const NOTE_ONLY_MARKERS: &[&str] = &[
    "fr", "ft", "fk", "fq", "fqa", "fl", "fw", "fp", "fv", "fdc", "xop", "xot", "xnt", "xdc",
];

/// A verse number with an optional alphabetic suffix, e.g. `3` or `3b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersePart {
    num: u32,
    /// Letter suffix such as `'a'` or `'b'`; `None` for plain integers.
    suffix: Option<char>,
}

impl VersePart {
    fn next_expected(self) -> Self {
        match self.suffix {
            None => Self { num: self.num + 1, suffix: None },
            Some(c) => Self { num: self.num, suffix: Some((c as u8 + 1) as char) },
        }
    }
}

impl fmt::Display for VersePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.suffix {
            None => write!(f, "{}", self.num),
            Some(c) => write!(f, "{}{}", self.num, c),
        }
    }
}

fn parse_verse_part(s: &str) -> Option<VersePart> {
    let s = s.trim_start();
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let digits = &s[..end];
    if digits.is_empty() {
        return None;
    }
    let num = digits.parse().ok()?;
    let suffix = s[end..].chars().next().filter(|c| c.is_ascii_lowercase());
    Some(VersePart { num, suffix })
}

fn parse_verse_range_end(s: &str) -> Option<VersePart> {
    if let Some(pos) = s.find('-') {
        parse_verse_part(&s[pos + 1..])
    } else {
        parse_verse_part(s)
    }
}

/// Returns `true` if `actual` is a valid next verse given `expected`.
///
/// Allows:
/// - Same integer, suffix advances (or no suffix expected yet): `3` → `3`, `3` → `3a`, `3a` → `3b`
/// - Next integer after a suffix sequence: `3a` or `3b` → `4`
/// - Standard integer increment: `3` → `4`
fn is_valid_verse_sequence(expected: VersePart, actual: VersePart) -> bool {
    if actual.num != expected.num {
        // Different integer: gap/backward unless transitioning from a suffix sequence to the next integer
        expected.suffix.is_some() && actual.num == expected.num + 1
    } else {
        // Same integer: suffix must not go backward (None < Some('a') < Some('b') < …)
        actual.suffix >= expected.suffix
    }
}

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

struct Validator<'a> {
    diagnostics: &'a mut DiagnosticList,
    expected_chapter: u32,
    seen_chapters: HashSet<u32>,
    expected_verse: VersePart,
    saw_book: bool,
    has_chapter: bool,
    body_started: bool,
    milestone_starts: HashMap<String, Vec<Span>>,
    milestone_ends: HashMap<String, Vec<Span>>,
}

impl<'a> Validator<'a> {
    fn new(diagnostics: &'a mut DiagnosticList) -> Self {
        Self {
            diagnostics,
            expected_chapter: 1,
            seen_chapters: HashSet::new(),
            expected_verse: VersePart { num: 1, suffix: None },
            saw_book: false,
            has_chapter: false,
            body_started: false,
            milestone_starts: HashMap::new(),
            milestone_ends: HashMap::new(),
        }
    }

    fn validate(&mut self, doc: &Document, source_map: &SourceMap) {
        let first = doc.content.first();
        let first_source = source_map.content.first();
        match first {
            Some(Node::Book { code, .. }) => {
                if !is_valid_book_code(code) {
                    self.diagnostics
                        .push(Diagnostic::invalid_book_code(code, span_of(first_source)));
                }
            }
            Some(Node::Text(_)) => {
                self.diagnostics
                    .push(Diagnostic::text_before_id(span_of(first_source)));
                self.diagnostics.push(Diagnostic::missing_id_marker());
            }
            _ => {
                self.diagnostics.push(Diagnostic::missing_id_marker());
            }
        }

        for (node, source) in zip_nodes(&doc.content, &source_map.content) {
            match node {
                Node::Book { .. } => {
                    if self.saw_book {
                        self.diagnostics
                            .push(Diagnostic::duplicate_id(span_of(Some(source))));
                    }
                    self.saw_book = true;
                }
                Node::Chapter(_) => {
                    self.body_started = true;
                    self.has_chapter = true;
                    self.expected_verse = VersePart { num: 1, suffix: None };
                    self.handle_chapter_sequence(node, source);
                }
                Node::Para { marker, .. } => {
                    if marker.kind() == MarkerKind::Header
                        && self.body_started
                        && !is_body_header_marker(marker.as_str())
                    {
                        self.diagnostics.push(Diagnostic::header_after_body(
                            marker.as_str(),
                            span_of(Some(source)),
                        ));
                    }
                    if !self.has_chapter
                        && marker.kind() == MarkerKind::Paragraph
                        && !is_introduction_marker(marker.as_str())
                    {
                        self.diagnostics
                            .push(Diagnostic::body_paragraph_before_chapter(
                                marker.as_str(),
                                span_of(Some(source)),
                            ));
                    }
                }
                _ => {}
            }

            if !matches!(node, Node::Chapter(_)) {
                self.walk(node, source, false);
            }
        }

        if self.saw_book && !self.has_chapter {
            self.diagnostics.push(Diagnostic::missing_chapter_marker());
        }

        self.finish_milestone_pairs();
    }

    fn handle_chapter_sequence(&mut self, node: &Node, source: &SourceNode) {
        let Node::Chapter(data) = node else {
            return;
        };
        let Ok(num) = data.number.parse::<u32>() else {
            return;
        };
        if !self.seen_chapters.insert(num) {
            self.diagnostics
                .push(Diagnostic::duplicate_chapter(num, span_of(Some(source))));
        }
        if num != self.expected_chapter {
            self.diagnostics.push(Diagnostic::invalid_chapter_sequence(
                self.expected_chapter,
                num,
                span_of(Some(source)),
            ));
        }
        self.expected_chapter = num + 1;
    }

    fn walk(&mut self, node: &Node, source: &SourceNode, inside_note: bool) {
        match node {
            Node::Verse(data) => {
                if let Some(v_start) = parse_verse_part(&data.number) {
                    if !is_valid_verse_sequence(self.expected_verse, v_start) {
                        self.diagnostics.push(Diagnostic::invalid_verse_sequence(
                            &self.expected_verse.to_string(),
                            &data.number,
                            span_of(Some(source)),
                        ));
                    }
                    self.expected_verse = parse_verse_range_end(&data.number)
                        .unwrap_or(v_start)
                        .next_expected();
                }
            }
            Node::Char(data) => {
                if !inside_note && is_note_only_marker(data.marker.as_str()) {
                    self.diagnostics
                        .push(Diagnostic::note_submarker_outside_note(
                            data.marker.as_str(),
                            span_of(Some(source)),
                        ));
                }
                if data.content.iter().any(|n| matches!(n, Node::Verse(_))) {
                    self.diagnostics.push(Diagnostic::char_crosses_verse(
                        data.marker.as_str(),
                        span_of(Some(source)),
                    ));
                }

                let clean_marker = data
                    .marker
                    .strip_prefix('+')
                    .unwrap_or(data.marker.as_str());
                for &req in markers::required_attributes(clean_marker) {
                    if !data.attributes.iter().any(|a| a.key == req) {
                        self.diagnostics
                            .push(Diagnostic::missing_required_attribute(
                                clean_marker,
                                req,
                                span_of(Some(source)),
                            ));
                    }
                }
                if data.attributes.iter().any(|a| a.key == "default")
                    && markers::default_attribute(clean_marker).is_none()
                {
                    self.diagnostics
                        .push(Diagnostic::default_attribute_not_defined(
                            clean_marker,
                            span_of(Some(source)),
                        ));
                }
                if data.attributes.iter().any(|a| a.value.trim().is_empty()) {
                    self.diagnostics
                        .push(Diagnostic::malformed_attributes(span_of(Some(source))));
                }
                if clean_marker == "w" && data.content.is_empty() && data.attributes.is_empty() {
                    self.diagnostics
                        .push(Diagnostic::empty_word_marker(span_of(Some(source))));
                }
                self.walk_children(&data.content, source, inside_note);
            }
            Node::Note { content, .. } => self.walk_children(content, source, true),
            Node::Figure { content, .. } => {
                if content_is_blank(content) {
                    self.diagnostics
                        .push(Diagnostic::empty_figure(span_of(Some(source))));
                }
                self.walk_children(content, source, inside_note);
            }
            Node::Milestone { marker, .. } => {
                if let Some(base) = milestone_base(marker.as_str()) {
                    if marker.as_str().ends_with("-s") {
                        self.milestone_starts
                            .entry(base.to_string())
                            .or_default()
                            .push(span_of(Some(source)));
                    } else if marker.as_str().ends_with("-e") {
                        self.milestone_ends
                            .entry(base.to_string())
                            .or_default()
                            .push(span_of(Some(source)));
                    }
                }
            }
            Node::Para { marker, content } => {
                if marker == "b" && !content.is_empty() {
                    self.diagnostics
                        .push(Diagnostic::non_empty_blank_line(span_of(Some(source))));
                }
                self.walk_children(content, source, inside_note);
            }
            Node::Table { content }
            | Node::TableRow { content, .. }
            | Node::Book { content, .. }
            | Node::Sidebar { content, .. }
            | Node::Periph { content, .. }
            | Node::Ref { content, .. }
            | Node::Unknown { content, .. } => self.walk_children(content, source, inside_note),
            _ => {}
        }

        if let Node::TableRow { content, .. } = node {
            self.validate_table_row(content, source);
        }
    }

    fn walk_children(&mut self, content: &[Node], source: &SourceNode, inside_note: bool) {
        for (child, child_source) in zip_nodes(content, &source.children) {
            self.walk(child, child_source, inside_note);
        }
    }

    fn validate_table_row(&mut self, content: &[Node], source: &SourceNode) {
        let mut expected = 1;
        for (cell, source) in zip_nodes(content, &source.children) {
            let Node::TableCell { marker, .. } = cell else {
                continue;
            };
            let Some((start_col, end_col)) = parse_table_cell_columns(marker.as_str()) else {
                continue;
            };
            if start_col != expected {
                self.diagnostics
                    .push(Diagnostic::invalid_table_column_sequence(
                        expected,
                        start_col,
                        span_of(Some(source)),
                    ));
            }
            expected = end_col + 1;
        }
    }

    fn finish_milestone_pairs(&mut self) {
        for (marker, spans) in &self.milestone_starts {
            let end_count = self.milestone_ends.get(marker).map_or(0, Vec::len);
            if spans.len() > end_count {
                for span in spans.iter().skip(end_count) {
                    self.diagnostics.push(Diagnostic::milestone_mismatch(
                        &format!("{marker}-s"),
                        span.clone(),
                    ));
                }
            }
        }
        for (marker, spans) in &self.milestone_ends {
            let start_count = self.milestone_starts.get(marker).map_or(0, Vec::len);
            if spans.len() > start_count {
                for span in spans.iter().skip(start_count) {
                    self.diagnostics.push(Diagnostic::milestone_mismatch(
                        &format!("{marker}-e"),
                        span.clone(),
                    ));
                }
            }
        }
    }
}

fn zip_nodes<'a, 'b>(
    nodes: &'a [Node<'b>],
    sources: &'a [SourceNode],
) -> impl Iterator<Item = (&'a Node<'b>, &'a SourceNode)> {
    nodes.iter().zip(sources.iter())
}

fn span_of(source: Option<&SourceNode>) -> Span {
    source
        .and_then(|source| source.spans.as_ref().map(|spans| spans.node.clone()))
        .unwrap_or(0..0)
}

fn content_is_blank(content: &[Node]) -> bool {
    if content.is_empty() {
        return true;
    }

    content.iter().all(|node| match node {
        Node::Text(text) => text.trim().is_empty(),
        _ => false,
    })
}

fn is_valid_book_code(code: &str) -> bool {
    VALID_BOOK_CODES.contains(&code)
}

fn is_note_only_marker(marker: &str) -> bool {
    NOTE_ONLY_MARKERS.contains(&marker)
}

fn is_introduction_marker(marker: &str) -> bool {
    matches!(
        marker,
        "imt"
            | "imt1"
            | "imt2"
            | "imt3"
            | "imt4"
            | "imte"
            | "imte1"
            | "imte2"
            | "is"
            | "is1"
            | "is2"
            | "is3"
            | "ip"
            | "ipi"
            | "im"
            | "imi"
            | "ipq"
            | "imq"
            | "ipr"
            | "ib"
            | "iq"
            | "iq1"
            | "iq2"
            | "iq3"
            | "iex"
            | "iot"
            | "io"
            | "io1"
            | "io2"
            | "io3"
            | "io4"
            | "ili"
            | "ili1"
            | "ili2"
            | "ie"
    )
}

fn is_body_header_marker(marker: &str) -> bool {
    matches!(marker, "cl" | "cp" | "cd")
}

fn milestone_base(marker: &str) -> Option<&str> {
    marker
        .strip_suffix("-s")
        .or_else(|| marker.strip_suffix("-e"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Attribute, ChapterData, CharData, Document, Node, VerseData};
    use crate::diagnostics::DiagnosticCode;
    use crate::source_map::{SourceMap, SourceNode, SourceSpans};

    fn doc_with(nodes: Vec<Node>) -> Document {
        Document { content: nodes }
    }

    fn validate_doc(doc: &Document) -> DiagnosticList {
        validate(doc, &source_map_for_document(doc))
    }

    fn source_map_for_document(doc: &Document) -> SourceMap {
        SourceMap {
            content: doc
                .content
                .iter()
                .enumerate()
                .map(|(index, node)| source_node_for_node(node, index * 10))
                .collect(),
        }
    }

    fn source_node_for_node(node: &Node, start: usize) -> SourceNode {
        let children = node
            .children()
            .iter()
            .enumerate()
            .map(|(index, child)| source_node_for_node(child, (start + 1) * 10 + index))
            .collect();

        match node {
            Node::Text(_) | Node::OptBreak => SourceNode::leaf(),
            _ => SourceNode::structural(SourceSpans::node(start..start + 1), children, None),
        }
    }

    #[test]
    fn parse_verse_helpers_cover_ranges_and_suffixes() {
        let vp = |num, suffix| Some(VersePart { num, suffix });
        assert_eq!(parse_verse_part("1"), vp(1, None));
        assert_eq!(parse_verse_part("3-4"), vp(3, None));
        assert_eq!(parse_verse_part("2b"), vp(2, Some('b')));
        assert_eq!(parse_verse_part("abc"), None);

        assert_eq!(parse_verse_range_end("1"), vp(1, None));
        assert_eq!(parse_verse_range_end("3-4"), vp(4, None));
        assert_eq!(parse_verse_range_end("2b-3a"), vp(3, Some('a')));
    }

    #[test]
    fn validation_uses_source_map_spans() {
        let doc = Document {
            content: vec![Node::Book {
                marker: "id".into(),
                code: "BAD".into(),
                content: Vec::new(),
            }],
        };
        let source_map = SourceMap {
            content: vec![SourceNode::structural(
                SourceSpans::node(4..7),
                Vec::new(),
                Some(0),
            )],
        };
        let diagnostics = validate(&doc, &source_map);
        let first = diagnostics.iter().next().unwrap();
        assert_eq!(first.span, 4..7);
    }

    #[test]
    fn missing_id_is_reported() {
        let doc = doc_with(vec![Node::Para {
            marker: "p".into(),
            content: vec![],
        }]);

        let diagnostics = validate_doc(&doc);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::MissingIdMarker)
        );
    }

    #[test]
    fn valid_and_invalid_book_codes_are_distinguished() {
        let valid = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
        }]);
        let invalid = doc_with(vec![Node::Book {
            marker: "id".into(),
            code: "XYZ".into(),
            content: vec![],
        }]);

        assert!(
            !validate_doc(&valid)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidBookCode)
        );
        assert!(
            validate_doc(&invalid)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidBookCode)
        );
    }

    #[test]
    fn chapter_sequence_and_duplicates_are_reported() {
        let valid = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "2".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
        ]);
        let gap = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "3".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
        ]);
        let duplicate = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
        ]);

        assert!(
            !validate_doc(&valid)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidChapterSequence)
        );
        assert!(
            validate_doc(&gap)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidChapterSequence)
        );
        assert!(
            validate_doc(&duplicate)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::DuplicateChapter)
        );
    }

    #[test]
    fn verse_sequence_rules_handle_ranges_and_new_chapters() {
        let valid = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "3-4".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "5".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                ],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "2".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Verse(Box::new(VerseData {
                    marker: "v".into(),
                    number: "1".into(),
                    sid: None,
                    altnumber: None,
                    pubnumber: None,
                }))],
            },
        ]);
        let gap = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "3".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                ],
            },
        ]);

        assert!(
            !validate_doc(&valid)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidVerseSequence)
        );
        assert!(
            validate_doc(&gap)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidVerseSequence)
        );
    }

    #[test]
    fn text_before_id_is_detected() {
        let doc = doc_with(vec![
            Node::text("stray text"),
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
        ]);

        let diagnostics = validate_doc(&doc);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::TextBeforeId)
        );
    }

    #[test]
    fn header_after_body_but_not_body_headers_is_reported() {
        let bad = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Para {
                marker: "h".into(),
                content: vec![Node::text("Genesis")],
            },
        ]);
        let ok = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Para {
                marker: "cl".into(),
                content: vec![Node::text("Chapter One")],
            },
        ]);

        assert!(
            validate_doc(&bad)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::HeaderAfterBody)
        );
        assert!(
            !validate_doc(&ok)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::HeaderAfterBody)
        );
    }

    #[test]
    fn note_only_markers_must_stay_inside_notes() {
        let bad = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Char(Box::new(CharData {
                    marker: "ft".into(),
                    content: vec![Node::text("footnote text")],
                    attributes: vec![],
                }))],
            },
        ]);
        let ok = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Para {
                marker: "p".into(),
                content: vec![Node::Note {
                    marker: "f".into(),
                    caller: "+".into(),
                    category: None,
                    content: vec![Node::Char(Box::new(CharData {
                        marker: "ft".into(),
                        content: vec![Node::text("footnote text")],
                        attributes: vec![],
                    }))],
                }],
            },
        ]);

        assert!(
            validate_doc(&bad)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::NoteSubmarkerOutsideNote)
        );
        assert!(
            !validate_doc(&ok)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::NoteSubmarkerOutsideNote)
        );
    }

    #[test]
    fn milestone_pairing_is_validated() {
        let matched = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Milestone {
                marker: "qt1-s".into(),
                attributes: vec![],
            },
            Node::Milestone {
                marker: "qt1-e".into(),
                attributes: vec![],
            },
        ]);
        let unmatched = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Milestone {
                marker: "qt1-s".into(),
                attributes: vec![],
            },
        ]);

        assert!(
            !validate_doc(&matched)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::MilestoneMismatch)
        );
        assert!(
            validate_doc(&unmatched)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::MilestoneMismatch)
        );
    }

    #[test]
    fn table_column_sequence_accepts_spans_and_rejects_gaps() {
        let valid = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Table {
                content: vec![Node::TableRow {
                    marker: "tr".into(),
                    content: vec![
                        Node::TableCell {
                            marker: "tcr1-2".into(),
                            align: "end".into(),
                            content: vec![Node::text("Total: ")],
                        },
                        Node::TableCell {
                            marker: "tcc3".into(),
                            align: "center".into(),
                            content: vec![Node::text("186,400")],
                        },
                    ],
                }],
            },
        ]);
        let invalid = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Table {
                content: vec![Node::TableRow {
                    marker: "tr".into(),
                    content: vec![
                        Node::TableCell {
                            marker: "th1".into(),
                            align: "start".into(),
                            content: vec![Node::text("header1 ")],
                        },
                        Node::TableCell {
                            marker: "th3".into(),
                            align: "start".into(),
                            content: vec![Node::text("header3")],
                        },
                    ],
                }],
            },
        ]);

        assert!(
            !validate_doc(&valid)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidTableColumnSequence)
        );
        assert!(
            validate_doc(&invalid)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidTableColumnSequence)
        );
    }

    #[test]
    fn char_and_figure_specific_validation_rules_still_hold() {
        let word_with_empty_attribute = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::Char(Box::new(CharData {
                        marker: "w".into(),
                        content: vec![Node::text("word")],
                        attributes: vec![Attribute {
                            key: "lemma".into(),
                            value: "".into(),
                        }],
                    })),
                ],
            },
        ]);
        let empty_figure = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Figure {
                marker: "fig".into(),
                content: vec![Node::text(" ")],
                attributes: vec![],
            },
        ]);

        assert!(
            validate_doc(&word_with_empty_attribute)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidAttributes)
        );
        assert!(
            validate_doc(&empty_figure)
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::EmptyFigure)
        );
    }

    #[test]
    fn valid_document_can_still_produce_no_diagnostics() {
        let doc = doc_with(vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![],
            },
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            })),
            Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::text("In the beginning"),
                ],
            },
        ]);

        assert!(validate_doc(&doc).is_empty());
    }

    #[test]
    fn verse_sequence_handles_sub_verse_letters() {
        fn make_doc(numbers: &[&'static str]) -> Document<'static> {
            let verses = numbers
                .iter()
                .map(|n| {
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: (*n).into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    }))
                })
                .collect();
            doc_with(vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                },
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: None,
                    altnumber: None,
                    pubnumber: None,
                })),
                Node::Para {
                    marker: "p".into(),
                    content: verses,
                },
            ])
        }
        fn has_seq_err(numbers: &[&'static str]) -> bool {
            validate_doc(&make_doc(numbers))
                .iter()
                .any(|d| d.code == DiagnosticCode::InvalidVerseSequence)
        }

        // Single sub-verse is fine — treated as that integer's position
        assert!(!has_seq_err(&["1", "2", "3b", "4"]));
        // Full sub-verse sequence then next integer
        assert!(!has_seq_err(&["1", "2", "3a", "3b", "4"]));
        // Longer sub-verse sequence
        assert!(!has_seq_err(&["1", "2", "3a", "3b", "3c", "4"]));
        // Sub-verse then skip to next integer (3a, then 4 — 3b never appears)
        assert!(!has_seq_err(&["1", "2", "3a", "4"]));

        // Repeated sub-verse is an error
        assert!(has_seq_err(&["1", "2", "3a", "3a", "4"]));
        // Backward sub-verse is an error
        assert!(has_seq_err(&["1", "2", "3b", "3a", "4"]));
        // Plain verse after a sub-verse of the same integer is an error
        assert!(has_seq_err(&["1", "2", "3a", "3", "4"]));
    }
}
