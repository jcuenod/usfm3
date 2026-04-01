use crate::ast::Span;
use crate::diagnostics::DiagnosticList;
use crate::lexer::{self, Token};
use crate::markers::{self, MarkerKind};

#[derive(Debug, Clone)]
pub struct CstParseResult {
    pub cst: CstDocument,
    pub diagnostics: DiagnosticList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CstNodeId(usize);

impl CstNodeId {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct CstDocument {
    source: String,
    root: CstNodeId,
    nodes: Vec<CstNode>,
    leaf_ids: Vec<CstNodeId>,
}

#[derive(Debug, Clone)]
pub struct CstNode {
    pub kind: CstKind,
    pub span: Span,
    pub parent: Option<CstNodeId>,
    pub prev_sibling: Option<CstNodeId>,
    pub next_sibling: Option<CstNodeId>,
    pub first_child: Option<CstNodeId>,
    pub last_child: Option<CstNodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CstKind {
    Document,
    Book { marker: String },
    Chapter { marker: String },
    Verse { marker: String },
    Para { marker: String },
    Char { marker: String },
    Note { marker: String },
    Milestone { marker: String },
    Figure { marker: String },
    Sidebar { marker: String },
    Periph { marker: String },
    Table,
    TableRow { marker: String },
    TableCell { marker: String },
    Ref,
    Unknown { marker: String },
    MarkerToken {
        normalized: String,
        token_kind: MarkerTokenKind,
    },
    ClosingMarkerToken {
        normalized: String,
        token_kind: ClosingTokenKind,
    },
    MilestoneEndToken,
    AttributesToken,
    TextToken,
    WhitespaceToken,
    NewlineToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerTokenKind {
    Regular,
    Nested,
    Chapter,
    Verse,
    Milestone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosingTokenKind {
    Regular,
    Nested,
}

impl CstKind {
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            CstKind::MarkerToken { .. }
                | CstKind::ClosingMarkerToken { .. }
                | CstKind::MilestoneEndToken
                | CstKind::AttributesToken
                | CstKind::TextToken
                | CstKind::WhitespaceToken
                | CstKind::NewlineToken
        )
    }
}

impl CstDocument {
    pub fn root_id(&self) -> CstNodeId {
        self.root
    }

    pub fn node(&self, id: CstNodeId) -> &CstNode {
        &self.nodes[id.0]
    }

    pub fn nodes(&self) -> &[CstNode] {
        &self.nodes
    }

    pub fn leaf_ids(&self) -> &[CstNodeId] {
        &self.leaf_ids
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn source_text(&self, id: CstNodeId) -> &str {
        &self.source[self.node(id).span.clone()]
    }

    pub fn to_source_string(&self) -> String {
        let mut out = String::with_capacity(self.source.len());
        for &leaf_id in &self.leaf_ids {
            out.push_str(self.source_text(leaf_id));
        }
        out
    }

    pub fn leaf_at_offset(&self, offset: usize) -> Option<CstNodeId> {
        if self.leaf_ids.is_empty() {
            return None;
        }
        if offset > self.source.len() {
            return None;
        }
        if offset == self.source.len() {
            return self.leaf_ids.last().copied();
        }
        let idx = self
            .leaf_ids
            .binary_search_by(|leaf_id| {
                let span = &self.node(*leaf_id).span;
                if offset < span.start {
                    std::cmp::Ordering::Greater
                } else if offset >= span.end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        self.leaf_ids.get(idx).copied()
    }

    pub fn covering_node_range(&self, start: usize, end: usize) -> Option<CstNodeId> {
        if start > end || end > self.source.len() {
            return None;
        }
        let seed = self.leaf_at_offset(start)?;
        let mut current = seed;
        loop {
            let span = &self.node(current).span;
            if span.start <= start && span.end >= end {
                return Some(current);
            }
            current = self.node(current).parent?;
        }
    }
}

pub fn parse(input: &str) -> CstParseResult {
    let cst = get_cst(input);
    let diagnostics = crate::builder::parse_from_cst(&cst).diagnostics;
    CstParseResult {
        cst,
        diagnostics,
    }
}

pub(crate) fn get_cst(input: &str) -> CstDocument {
    let tokens = lexer::tokenize(input);
    let mut parser = CstParser::new(input);
    for (token, span) in tokens {
        parser.handle_token(token, span);
    }
    parser.finish()
}

#[derive(Debug, Clone)]
struct OpenNode {
    id: CstNodeId,
    kind: MarkerKind,
    marker: String,
}

struct CstParser<'a> {
    source: &'a str,
    nodes: Vec<CstNode>,
    leaf_ids: Vec<CstNodeId>,
    root: CstNodeId,
    stack: Vec<OpenNode>,
    pending_chapter: Option<CstNodeId>,
    pending_verse: Option<CstNodeId>,
    pending_milestone: Option<CstNodeId>,
    pending_usfm: bool,
}

impl<'a> CstParser<'a> {
    fn new(source: &'a str) -> Self {
        let root = CstNodeId(0);
        Self {
            source,
            nodes: vec![CstNode {
                kind: CstKind::Document,
                span: 0..source.len(),
                parent: None,
                prev_sibling: None,
                next_sibling: None,
                first_child: None,
                last_child: None,
            }],
            leaf_ids: Vec::new(),
            root,
            stack: Vec::new(),
            pending_chapter: None,
            pending_verse: None,
            pending_milestone: None,
            pending_usfm: false,
        }
    }

    fn finish(mut self) -> CstDocument {
        self.flush_pending_milestone();
        self.flush_pending_chapter();
        self.flush_pending_verse();
        self.close_all();
        CstDocument {
            source: self.source.to_string(),
            root: self.root,
            nodes: self.nodes,
            leaf_ids: self.leaf_ids,
        }
    }

    fn handle_token(&mut self, token: Token<'a>, span: Span) {
        if !matches!(
            token,
            Token::Whitespace(_) | Token::Newline | Token::Attributes(_) | Token::MilestoneEnd
        ) {
            self.flush_pending_milestone();
        }

        match token {
            Token::Whitespace(text) => {
                self.append_leaf(CstKind::WhitespaceToken, span, Some(text));
            }
            Token::Newline => {
                self.append_leaf(CstKind::NewlineToken, span, None);
            }
            Token::Attributes(text) => self.handle_attributes(span, text),
            Token::Text(text) => self.handle_text(span, text),
            Token::Chapter => self.handle_chapter(span),
            Token::Verse => self.handle_verse(span),
            Token::Milestone(marker) => self.handle_milestone(span, marker),
            Token::NestedMarker(marker) => self.handle_nested_open(span, marker),
            Token::NestedClosingMarker(marker) => self.handle_close(
                span,
                marker,
                true,
                lexer::strip_closing_star(marker).trim_start_matches('+'),
            ),
            Token::ClosingMarker(marker) => {
                self.handle_close(span, marker, false, lexer::strip_closing_star(marker))
            }
            Token::Marker(marker) => self.handle_marker(span, marker),
            Token::MilestoneEnd => self.handle_milestone_end(span),
        }
    }

    fn handle_attributes(&mut self, span: Span, text: &str) {
        if let Some(milestone_id) = self.pending_milestone {
            self.append_leaf_to(CstKind::AttributesToken, span, milestone_id, Some(text));
            return;
        }
        self.append_leaf(CstKind::AttributesToken, span, Some(text));
    }

    fn handle_text(&mut self, span: Span, text: &str) {
        if self.pending_usfm {
            self.pending_usfm = false;
        }

        if let Some(chapter_id) = self.pending_chapter.take() {
            let (number, rest) = split_first_word(text);
            if !number.is_empty() {
                let number_span = span.start..span.start + number.len();
                self.append_leaf_to(CstKind::TextToken, number_span, chapter_id, Some(number));
            }
            self.refresh_span(chapter_id);
            if !rest.is_empty() {
                self.append_split_text(span.start + number.len(), rest);
            }
            return;
        }

        if let Some(verse_id) = self.pending_verse.take() {
            let (number, rest) = split_first_word(text);
            if !number.is_empty() {
                let number_span = span.start..span.start + number.len();
                self.append_leaf_to(CstKind::TextToken, number_span, verse_id, Some(number));
            }
            self.refresh_span(verse_id);
            if !rest.is_empty() {
                self.append_split_text(span.start + number.len(), rest);
            }
            return;
        }

        self.append_leaf(CstKind::TextToken, span, Some(text));
    }

    fn handle_chapter(&mut self, span: Span) {
        self.flush_pending_chapter();
        self.flush_pending_verse();
        self.close_block_context();
        let id = self.open_structural(
            CstKind::Chapter {
                marker: "c".to_string(),
            },
            span.clone(),
        );
        self.append_leaf_to(
            CstKind::MarkerToken {
                normalized: "c".to_string(),
                token_kind: MarkerTokenKind::Chapter,
            },
            span,
            id,
            Some("\\c"),
        );
        self.pending_chapter = Some(id);
    }

    fn handle_verse(&mut self, span: Span) {
        self.flush_pending_verse();
        self.close_open_meta();
        let id = self.open_structural(
            CstKind::Verse {
                marker: "v".to_string(),
            },
            span.clone(),
        );
        self.append_leaf_to(
            CstKind::MarkerToken {
                normalized: "v".to_string(),
                token_kind: MarkerTokenKind::Verse,
            },
            span,
            id,
            Some("\\v"),
        );
        self.pending_verse = Some(id);
    }

    fn handle_milestone(&mut self, span: Span, marker: &str) {
        self.flush_pending_milestone();
        let normalized = lexer::strip_marker_backslash(marker).to_string();
        let id = self.open_structural(
            CstKind::Milestone {
                marker: normalized.clone(),
            },
            span.clone(),
        );
        self.append_leaf_to(
            CstKind::MarkerToken {
                normalized,
                token_kind: MarkerTokenKind::Milestone,
            },
            span,
            id,
            Some(marker),
        );
        self.pending_milestone = Some(id);
    }

    fn handle_nested_open(&mut self, span: Span, marker: &str) {
        let normalized = lexer::strip_marker_backslash(marker)
            .trim_start_matches('+')
            .to_string();
        let id = self.open_structural_from_name(&normalized, span.clone());
        self.append_leaf_to(
            CstKind::MarkerToken {
                normalized,
                token_kind: MarkerTokenKind::Nested,
            },
            span,
            id,
            Some(marker),
        );
    }

    fn handle_close(&mut self, span: Span, marker: &str, nested: bool, normalized: &str) {
        if let Some(idx) = self.find_matching_open(normalized) {
            while self.stack.len() > idx + 1 {
                let top = self.stack.pop().unwrap();
                self.refresh_span(top.id);
            }
            let open = self.stack.pop().unwrap();
            self.append_leaf_to(
                CstKind::ClosingMarkerToken {
                    normalized: normalized.to_string(),
                    token_kind: if nested {
                        ClosingTokenKind::Nested
                    } else {
                        ClosingTokenKind::Regular
                    },
                },
                span,
                open.id,
                Some(marker),
            );
            self.refresh_span(open.id);
            return;
        }
        self.append_leaf(
            CstKind::ClosingMarkerToken {
                normalized: normalized.to_string(),
                token_kind: if nested {
                    ClosingTokenKind::Nested
                } else {
                    ClosingTokenKind::Regular
                },
            },
            span,
            Some(marker),
        );
    }

    fn handle_marker(&mut self, span: Span, marker: &str) {
        let name = lexer::strip_marker_backslash(marker);

        if name == "usfm" {
            self.pending_usfm = true;
        }

        let info = markers::lookup_marker(name);

        if matches!(
            info.kind,
            MarkerKind::Header
                | MarkerKind::Paragraph
                | MarkerKind::TableRow
                | MarkerKind::Periph
                | MarkerKind::SidebarStart
                | MarkerKind::Meta
        ) {
            self.flush_pending_chapter();
            self.flush_pending_verse();
        }

        match info.kind {
            MarkerKind::Header => {
                self.close_block_context();
                let id = self.open_structural(
                    CstKind::Para {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                if name == "id" {
                    self.nodes[id.0].kind = CstKind::Book {
                        marker: name.to_string(),
                    };
                }
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Paragraph => {
                self.close_block_context();
                let id = self.open_structural(
                    CstKind::Para {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Note => {
                let id = self.open_structural(
                    CstKind::Note {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Character => {
                let clean_name = name.trim_start_matches('+');
                let closed_sibling = if self.in_note_context()
                    && info.valid_in_note
                    && self.is_same_note_family(clean_name)
                    && clean_name != "fv"
                {
                    self.close_character_in_note()
                } else {
                    false
                };
                let id = self.open_structural_from_name(clean_name, span.clone());
                if clean_name == "ref" {
                    self.nodes[id.0].kind = CstKind::Ref;
                }
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: clean_name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
                if closed_sibling {
                    self.refresh_span(id);
                }
            }
            MarkerKind::TableRow => {
                self.close_block_context();
                let row_id = self.open_structural(
                    CstKind::TableRow {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    row_id,
                    Some(marker),
                );
            }
            MarkerKind::TableCell => {
                self.close_table_cell_in_row();
                let id = self.open_structural(
                    CstKind::TableCell {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Periph => {
                self.close_block_context();
                let id = self.open_structural(
                    CstKind::Periph {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Figure => {
                let id = self.open_structural(
                    CstKind::Figure {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::SidebarStart => {
                self.close_block_context();
                let id = self.open_structural(
                    CstKind::Sidebar {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::SidebarEnd => self.close_sidebar(span, marker, name),
            MarkerKind::Meta => {
                if name == "cat" && self.in_note_or_sidebar_context() {
                    let id = self.open_structural(
                        CstKind::Para {
                            marker: name.to_string(),
                        },
                        span.clone(),
                    );
                    self.append_leaf_to(
                        CstKind::MarkerToken {
                            normalized: name.to_string(),
                            token_kind: MarkerTokenKind::Regular,
                        },
                        span,
                        id,
                        Some(marker),
                    );
                } else if name == "rem" && !self.in_note_context() && self.has_open_paragraph() {
                    self.close_inline_above_paragraph();
                    let id = self.open_structural(
                        CstKind::Para {
                            marker: name.to_string(),
                        },
                        span.clone(),
                    );
                    self.append_leaf_to(
                        CstKind::MarkerToken {
                            normalized: name.to_string(),
                            token_kind: MarkerTokenKind::Regular,
                        },
                        span,
                        id,
                        Some(marker),
                    );
                } else {
                    self.close_block_context();
                    let id = self.open_structural(
                        CstKind::Para {
                            marker: name.to_string(),
                        },
                        span.clone(),
                    );
                    self.append_leaf_to(
                        CstKind::MarkerToken {
                            normalized: name.to_string(),
                            token_kind: MarkerTokenKind::Regular,
                        },
                        span,
                        id,
                        Some(marker),
                    );
                }
            }
            MarkerKind::Unknown => {
                let id = self.open_structural(
                    CstKind::Unknown {
                        marker: name.to_string(),
                    },
                    span.clone(),
                );
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name.to_string(),
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Chapter => self.handle_chapter(span),
            MarkerKind::Verse => self.handle_verse(span),
            MarkerKind::MilestoneStart | MarkerKind::MilestoneEnd => self.handle_milestone(span, marker),
        }
    }

    fn handle_milestone_end(&mut self, span: Span) {
        if let Some(id) = self.pending_milestone.take() {
            self.append_leaf_to(CstKind::MilestoneEndToken, span, id, Some("\\*"));
            self.refresh_span(id);
            return;
        }
        self.append_leaf(CstKind::MilestoneEndToken, span, Some("\\*"));
    }

    fn open_structural_from_name(&mut self, name: &str, span: Span) -> CstNodeId {
        let kind = match markers::lookup_marker(name).kind {
            MarkerKind::Note => CstKind::Note {
                marker: name.to_string(),
            },
            MarkerKind::Figure => CstKind::Figure {
                marker: name.to_string(),
            },
            MarkerKind::TableRow => CstKind::TableRow {
                marker: name.to_string(),
            },
            MarkerKind::TableCell => CstKind::TableCell {
                marker: name.to_string(),
            },
            MarkerKind::SidebarStart => CstKind::Sidebar {
                marker: name.to_string(),
            },
            MarkerKind::Periph => CstKind::Periph {
                marker: name.to_string(),
            },
            MarkerKind::Paragraph | MarkerKind::Header | MarkerKind::Meta => CstKind::Para {
                marker: name.to_string(),
            },
            MarkerKind::Unknown => CstKind::Unknown {
                marker: name.to_string(),
            },
            _ => CstKind::Char {
                marker: name.to_string(),
            },
        };
        self.open_structural(kind, span)
    }

    fn open_structural(&mut self, kind: CstKind, span: Span) -> CstNodeId {
        let parent = self.current_parent();
        let id = self.push_node(kind, span, parent);
        let marker = match &self.nodes[id.0].kind {
            CstKind::Book { marker }
            | CstKind::Chapter { marker }
            | CstKind::Verse { marker }
            | CstKind::Para { marker }
            | CstKind::Char { marker }
            | CstKind::Note { marker }
            | CstKind::Milestone { marker }
            | CstKind::Figure { marker }
            | CstKind::Sidebar { marker }
            | CstKind::Periph { marker }
            | CstKind::TableRow { marker }
            | CstKind::TableCell { marker }
            | CstKind::Unknown { marker } => marker.clone(),
            CstKind::Ref => "ref".to_string(),
            CstKind::Table => "table".to_string(),
            _ => String::new(),
        };
        if !matches!(
            self.nodes[id.0].kind,
            CstKind::Chapter { .. } | CstKind::Verse { .. } | CstKind::Milestone { .. }
        ) {
            let marker_kind = match &self.nodes[id.0].kind {
                CstKind::Book { .. } => MarkerKind::Header,
                CstKind::Para { marker } => markers::lookup_marker(marker).kind,
                CstKind::Char { .. } | CstKind::Ref => MarkerKind::Character,
                CstKind::Note { .. } => MarkerKind::Note,
                CstKind::Figure { .. } => MarkerKind::Figure,
                CstKind::Sidebar { .. } => MarkerKind::SidebarStart,
                CstKind::Periph { .. } => MarkerKind::Periph,
                CstKind::TableRow { .. } => MarkerKind::TableRow,
                CstKind::TableCell { .. } => MarkerKind::TableCell,
                CstKind::Unknown { .. } => MarkerKind::Unknown,
                _ => markers::lookup_marker(&marker).kind,
            };
            self.stack.push(OpenNode {
                id,
                kind: marker_kind,
                marker,
            });
        }
        id
    }

    fn push_node(&mut self, kind: CstKind, span: Span, parent: CstNodeId) -> CstNodeId {
        let id = if matches!(kind, CstKind::TableRow { .. }) {
            let table_parent = self.ensure_table_parent(parent, span.clone());
            self.push_regular_node(kind, span, table_parent)
        } else {
            self.push_regular_node(kind, span, parent)
        };
        if self.nodes[id.0].kind.is_leaf() {
            self.leaf_ids.push(id);
        }
        id
    }

    fn push_regular_node(&mut self, kind: CstKind, span: Span, parent: CstNodeId) -> CstNodeId {
        let id = CstNodeId(self.nodes.len());
        let prev_sibling = self.nodes[parent.0].last_child;
        self.nodes.push(CstNode {
            kind,
            span,
            parent: Some(parent),
            prev_sibling,
            next_sibling: None,
            first_child: None,
            last_child: None,
        });
        if let Some(prev) = prev_sibling {
            self.nodes[prev.0].next_sibling = Some(id);
        } else {
            self.nodes[parent.0].first_child = Some(id);
        }
        self.nodes[parent.0].last_child = Some(id);
        self.bump_span(parent, id);
        id
    }

    fn ensure_table_parent(&mut self, parent: CstNodeId, span: Span) -> CstNodeId {
        if let Some(last_child) = self.nodes[parent.0].last_child
            && matches!(self.nodes[last_child.0].kind, CstKind::Table)
        {
            return last_child;
        }
        self.push_regular_node(CstKind::Table, span, parent)
    }

    fn append_leaf(&mut self, kind: CstKind, span: Span, expected_text: Option<&str>) -> CstNodeId {
        let parent = self.current_token_parent();
        self.append_leaf_to(kind, span, parent, expected_text)
    }

    fn append_leaf_to(
        &mut self,
        kind: CstKind,
        span: Span,
        parent: CstNodeId,
        expected_text: Option<&str>,
    ) -> CstNodeId {
        if let Some(text) = expected_text {
            debug_assert_eq!(text, &self.source[span.clone()]);
        }
        self.push_node(kind, span, parent)
    }

    fn append_split_text(&mut self, start: usize, text: &str) {
        let mut chars = text.char_indices().peekable();
        while let Some((idx, ch)) = chars.next() {
            let segment_start = start + idx;
            if ch == ' ' || ch == '\t' || ch == '\r' {
                let mut end_rel = idx + ch.len_utf8();
                while let Some((next_idx, next_ch)) = chars.peek().copied() {
                    if next_ch == ' ' || next_ch == '\t' || next_ch == '\r' {
                        end_rel = next_idx + next_ch.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                self.append_leaf(CstKind::WhitespaceToken, segment_start..start + end_rel, None);
                continue;
            }
            let mut end_rel = idx + ch.len_utf8();
            while let Some((next_idx, next_ch)) = chars.peek().copied() {
                if next_ch == ' ' || next_ch == '\t' || next_ch == '\r' {
                    break;
                }
                end_rel = next_idx + next_ch.len_utf8();
                chars.next();
            }
            self.append_leaf(CstKind::TextToken, segment_start..start + end_rel, None);
        }
    }

    fn current_parent(&self) -> CstNodeId {
        self.stack.last().map(|open| open.id).unwrap_or(self.root)
    }

    fn current_token_parent(&self) -> CstNodeId {
        self.pending_chapter
            .or(self.pending_verse)
            .or(self.pending_milestone)
            .unwrap_or_else(|| self.current_parent())
    }

    fn flush_pending_chapter(&mut self) {
        if let Some(id) = self.pending_chapter.take() {
            self.refresh_span(id);
        }
    }

    fn flush_pending_verse(&mut self) {
        if let Some(id) = self.pending_verse.take() {
            self.refresh_span(id);
        }
    }

    fn flush_pending_milestone(&mut self) {
        if let Some(id) = self.pending_milestone.take() {
            self.refresh_span(id);
        }
    }

    fn close_all(&mut self) {
        while let Some(open) = self.stack.pop() {
            self.refresh_span(open.id);
        }
    }

    fn close_table_cell_in_row(&mut self) {
        while matches!(
            self.stack.last().map(|open| open.kind),
            Some(MarkerKind::TableCell)
        ) {
            let open = self.stack.pop().unwrap();
            self.refresh_span(open.id);
        }
    }

    fn close_table_row(&mut self) {
        if matches!(
            self.stack.last().map(|open| open.kind),
            Some(MarkerKind::TableRow)
        ) {
            let open = self.stack.pop().unwrap();
            self.refresh_span(open.id);
        }
    }

    fn close_table_context(&mut self) {
        self.close_table_cell_in_row();
        self.close_table_row();
    }

    fn close_block_context(&mut self) {
        self.force_close_notes();
        self.close_table_context();
        self.close_paragraph();
    }

    fn close_paragraph(&mut self) {
        loop {
            match self.stack.last().map(|open| open.kind) {
                Some(MarkerKind::Character)
                | Some(MarkerKind::Unknown)
                | Some(MarkerKind::Figure) => {
                    let open = self.stack.pop().unwrap();
                    self.refresh_span(open.id);
                }
                Some(MarkerKind::Paragraph) | Some(MarkerKind::Header) | Some(MarkerKind::Meta) => {
                    let open = self.stack.pop().unwrap();
                    self.refresh_span(open.id);
                    break;
                }
                _ => break,
            }
        }
    }

    fn force_close_notes(&mut self) {
        loop {
            let note_idx = self
                .stack
                .iter()
                .rposition(|open| open.kind == MarkerKind::Note);
            let Some(idx) = note_idx else { break };
            while self.stack.len() > idx + 1 {
                let open = self.stack.pop().unwrap();
                self.refresh_span(open.id);
            }
            let note = self.stack.pop().unwrap();
            self.refresh_span(note.id);
        }
    }

    fn close_sidebar(&mut self, span: Span, marker: &str, normalized: &str) {
        let sidebar_idx = self
            .stack
            .iter()
            .rposition(|open| open.kind == MarkerKind::SidebarStart);
        if let Some(idx) = sidebar_idx {
            while self.stack.len() > idx + 1 {
                let open = self.stack.pop().unwrap();
                self.refresh_span(open.id);
            }
            let sidebar = self.stack.pop().unwrap();
            self.append_leaf_to(
                CstKind::MarkerToken {
                    normalized: normalized.to_string(),
                    token_kind: MarkerTokenKind::Regular,
                },
                span,
                sidebar.id,
                Some(marker),
            );
            self.refresh_span(sidebar.id);
            return;
        }
        self.append_leaf(
            CstKind::MarkerToken {
                normalized: normalized.to_string(),
                token_kind: MarkerTokenKind::Regular,
            },
            span,
            Some(marker),
        );
    }

    fn close_character_in_note(&mut self) -> bool {
        let mut closed_any = false;
        loop {
            match self.stack.last().map(|open| open.kind) {
                Some(MarkerKind::Character)
                | Some(MarkerKind::Unknown)
                | Some(MarkerKind::TableCell) => {
                    let open = self.stack.pop().unwrap();
                    self.refresh_span(open.id);
                    closed_any = true;
                }
                _ => break,
            }
        }
        closed_any
    }

    fn close_inline_above_paragraph(&mut self) {
        while matches!(
            self.stack.last().map(|open| open.kind),
            Some(MarkerKind::Character) | Some(MarkerKind::Unknown) | Some(MarkerKind::Meta)
        ) {
            let open = self.stack.pop().unwrap();
            self.refresh_span(open.id);
        }
    }

    fn close_open_meta(&mut self) {
        while matches!(
            self.stack.last().map(|open| open.kind),
            Some(MarkerKind::Meta)
        ) {
            let open = self.stack.pop().unwrap();
            self.refresh_span(open.id);
        }
    }

    fn in_note_context(&self) -> bool {
        self.stack
            .iter()
            .rev()
            .any(|open| open.kind == MarkerKind::Note)
    }

    fn in_note_or_sidebar_context(&self) -> bool {
        self.stack
            .iter()
            .rev()
            .any(|open| open.kind == MarkerKind::Note || open.kind == MarkerKind::SidebarStart)
    }

    fn has_open_paragraph(&self) -> bool {
        self.stack
            .iter()
            .any(|open| open.kind == MarkerKind::Paragraph)
    }

    fn is_same_note_family(&self, incoming_marker: &str) -> bool {
        let note_family = self
            .stack
            .iter()
            .rev()
            .find(|open| open.kind == MarkerKind::Note)
            .and_then(|open| match open.marker.as_str() {
                "f" | "fe" | "ef" => Some('f'),
                "x" | "ex" => Some('x'),
                _ => open.marker.chars().next(),
            });
        match (note_family, incoming_marker.chars().next()) {
            (Some(expected), Some(found)) => expected == found,
            _ => true,
        }
    }

    fn find_matching_open(&self, name: &str) -> Option<usize> {
        let is_note_close = matches!(name, "f" | "fe" | "x" | "ef" | "ex");
        self.stack.iter().rposition(|open| {
            if is_note_close {
                open.kind == MarkerKind::Note && open.marker == name
            } else {
                open.marker == name
            }
        })
    }

    fn refresh_span(&mut self, id: CstNodeId) {
        let Some(first_child) = self.nodes[id.0].first_child else {
            return;
        };
        let last_child = self.nodes[id.0].last_child.unwrap();
        let new_span = self.nodes[first_child.0].span.start..self.nodes[last_child.0].span.end;
        self.nodes[id.0].span = new_span.clone();
        if let Some(parent) = self.nodes[id.0].parent {
            self.bump_span(parent, id);
        }
    }

    fn bump_span(&mut self, parent: CstNodeId, child: CstNodeId) {
        let child_span = self.nodes[child.0].span.clone();
        let is_first_child = self.nodes[parent.0].first_child == Some(child);
        let parent_span = &mut self.nodes[parent.0].span;
        if is_first_child {
            *parent_span = child_span;
        } else {
            parent_span.start = parent_span.start.min(child_span.start);
            parent_span.end = parent_span.end.max(child_span.end);
        }
    }
}

fn split_first_word(s: &str) -> (&str, &str) {
    let idx = s.find(char::is_whitespace).unwrap_or(s.len());
    (&s[..idx], &s[idx..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossless_round_trip_preserves_source() {
        let input = "\\p  \\v 1  In the beginning\n";
        let parsed = parse(input);
        assert_eq!(parsed.cst.to_source_string(), input);
    }

    #[test]
    fn leaf_lookup_covers_whitespace_gap() {
        let input = "\\p  \\v 1  In the beginning";
        let parsed = parse(input);
        let first_gap = parsed.cst.leaf_at_offset(2).unwrap();
        let second_gap = parsed.cst.leaf_at_offset(8).unwrap();
        assert!(matches!(
            parsed.cst.node(first_gap).kind,
            CstKind::WhitespaceToken
        ));
        assert!(matches!(
            parsed.cst.node(second_gap).kind,
            CstKind::WhitespaceToken
        ));
    }
}
