use crate::diagnostics::Span;
use crate::lexer::{self, Token};
use crate::markers::{MarkerKind, MarkerName};
use serde::Serialize;
use std::num::NonZeroUsize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CstNodeId(NonZeroUsize);

impl CstNodeId {
    fn from_index(index: usize) -> Self {
        Self(NonZeroUsize::new(index + 1).expect("CST node indices are one-based"))
    }

    pub fn index(self) -> usize {
        self.0.get() - 1
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
    Book {
        marker: MarkerName,
    },
    Chapter {
        marker: MarkerName,
    },
    Verse {
        marker: MarkerName,
    },
    Para {
        marker: MarkerName,
    },
    Char {
        marker: MarkerName,
    },
    Note {
        marker: MarkerName,
    },
    Milestone {
        marker: MarkerName,
    },
    Figure {
        marker: MarkerName,
    },
    Sidebar {
        marker: MarkerName,
    },
    Periph {
        marker: MarkerName,
    },
    Table,
    TableRow {
        marker: MarkerName,
    },
    TableCell {
        marker: MarkerName,
    },
    Ref,
    Unknown {
        marker: MarkerName,
    },
    MarkerToken {
        normalized: MarkerName,
        token_kind: MarkerTokenKind,
    },
    ClosingMarkerToken {
        normalized: MarkerName,
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

#[derive(Debug, Clone, Copy)]
pub enum LeafTokenKind<'a> {
    Marker {
        normalized: &'a MarkerName,
        token_kind: MarkerTokenKind,
    },
    ClosingMarker {
        normalized: &'a MarkerName,
        token_kind: ClosingTokenKind,
    },
    MilestoneEnd,
    Attributes,
    Text,
    Whitespace,
    Newline,
}

#[derive(Debug, Clone, Copy)]
pub struct LeafToken<'a> {
    pub id: CstNodeId,
    pub parent: CstNodeId,
    pub span: &'a Span,
    pub text: &'a str,
    pub kind: LeafTokenKind<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedCstNode {
    #[serde(rename = "type")]
    pub kind: String,
    pub span: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ExportedCstNode>,
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
        &self.nodes[id.index()]
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

    pub fn leaf_token(&self, id: CstNodeId) -> LeafToken<'_> {
        let node = self.node(id);
        let parent = node.parent.expect("leaf nodes always have a parent");
        let text = self.source_text(id);
        let kind = match &node.kind {
            CstKind::MarkerToken {
                normalized,
                token_kind,
            } => LeafTokenKind::Marker {
                normalized,
                token_kind: *token_kind,
            },
            CstKind::ClosingMarkerToken {
                normalized,
                token_kind,
            } => LeafTokenKind::ClosingMarker {
                normalized,
                token_kind: *token_kind,
            },
            CstKind::MilestoneEndToken => LeafTokenKind::MilestoneEnd,
            CstKind::AttributesToken => LeafTokenKind::Attributes,
            CstKind::TextToken => LeafTokenKind::Text,
            CstKind::WhitespaceToken => LeafTokenKind::Whitespace,
            CstKind::NewlineToken => LeafTokenKind::Newline,
            other => panic!("non-leaf CST node {:?} requested as leaf token", other),
        };
        LeafToken {
            id,
            parent,
            span: &node.span,
            text,
            kind,
        }
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

    pub fn export(&self) -> ExportedCstNode {
        export_node(self, self.root_id())
    }
}

pub fn parse(input: &str) -> CstDocument {
    parse_owned(input.to_string())
}

pub fn parse_owned(input: String) -> CstDocument {
    let (root, nodes, leaf_ids) = {
        let mut parser = CstParser::new(&input);
        for (token, span) in lexer::spanned_tokens(&input) {
            parser.handle_token(token, span);
        }
        parser.finish_parts()
    };
    CstDocument {
        source: input,
        root,
        nodes,
        leaf_ids,
    }
}

pub fn export(document: &CstDocument) -> ExportedCstNode {
    document.export()
}

fn export_node(document: &CstDocument, id: CstNodeId) -> ExportedCstNode {
    let node = document.node(id);
    let (kind, marker, token_kind, text) = match &node.kind {
        CstKind::Document => ("document".to_string(), None, None, None),
        CstKind::Book { marker } => ("book".to_string(), Some(marker.to_string()), None, None),
        CstKind::Chapter { marker } => {
            ("chapter".to_string(), Some(marker.to_string()), None, None)
        }
        CstKind::Verse { marker } => ("verse".to_string(), Some(marker.to_string()), None, None),
        CstKind::Para { marker } => ("para".to_string(), Some(marker.to_string()), None, None),
        CstKind::Char { marker } => ("char".to_string(), Some(marker.to_string()), None, None),
        CstKind::Note { marker } => ("note".to_string(), Some(marker.to_string()), None, None),
        CstKind::Milestone { marker } => (
            "milestone".to_string(),
            Some(marker.to_string()),
            None,
            None,
        ),
        CstKind::Figure { marker } => ("figure".to_string(), Some(marker.to_string()), None, None),
        CstKind::Sidebar { marker } => {
            ("sidebar".to_string(), Some(marker.to_string()), None, None)
        }
        CstKind::Periph { marker } => ("periph".to_string(), Some(marker.to_string()), None, None),
        CstKind::Table => ("table".to_string(), None, None, None),
        CstKind::TableRow { marker } => (
            "table_row".to_string(),
            Some(marker.to_string()),
            None,
            None,
        ),
        CstKind::TableCell { marker } => (
            "table_cell".to_string(),
            Some(marker.to_string()),
            None,
            None,
        ),
        CstKind::Ref => ("ref".to_string(), None, None, None),
        CstKind::Unknown { marker } => {
            ("unknown".to_string(), Some(marker.to_string()), None, None)
        }
        CstKind::MarkerToken {
            normalized,
            token_kind,
        } => (
            "marker_token".to_string(),
            Some(normalized.to_string()),
            Some(export_marker_token_kind(*token_kind).to_string()),
            Some(document.source_text(id).to_string()),
        ),
        CstKind::ClosingMarkerToken {
            normalized,
            token_kind,
        } => (
            "closing_marker_token".to_string(),
            Some(normalized.to_string()),
            Some(export_closing_token_kind(*token_kind).to_string()),
            Some(document.source_text(id).to_string()),
        ),
        CstKind::MilestoneEndToken => (
            "milestone_end_token".to_string(),
            None,
            None,
            Some(document.source_text(id).to_string()),
        ),
        CstKind::AttributesToken => (
            "attributes_token".to_string(),
            None,
            None,
            Some(document.source_text(id).to_string()),
        ),
        CstKind::TextToken => (
            "text_token".to_string(),
            None,
            None,
            Some(document.source_text(id).to_string()),
        ),
        CstKind::WhitespaceToken => (
            "whitespace_token".to_string(),
            None,
            None,
            Some(document.source_text(id).to_string()),
        ),
        CstKind::NewlineToken => (
            "newline_token".to_string(),
            None,
            None,
            Some(document.source_text(id).to_string()),
        ),
    };

    let mut children = Vec::new();
    let mut child_id = node.first_child;
    while let Some(current) = child_id {
        children.push(export_node(document, current));
        child_id = document.node(current).next_sibling;
    }

    ExportedCstNode {
        kind,
        span: node.span.clone(),
        marker,
        token_kind,
        text,
        children,
    }
}

fn export_marker_token_kind(kind: MarkerTokenKind) -> &'static str {
    match kind {
        MarkerTokenKind::Regular => "regular",
        MarkerTokenKind::Nested => "nested",
        MarkerTokenKind::Chapter => "chapter",
        MarkerTokenKind::Verse => "verse",
        MarkerTokenKind::Milestone => "milestone",
    }
}

fn export_closing_token_kind(kind: ClosingTokenKind) -> &'static str {
    match kind {
        ClosingTokenKind::Regular => "regular",
        ClosingTokenKind::Nested => "nested",
    }
}

#[cfg(test)]
pub(crate) fn get_cst(input: &str) -> CstDocument {
    parse(input)
}

#[derive(Debug, Clone)]
struct OpenNode {
    id: CstNodeId,
    kind: MarkerKind,
    marker: MarkerName,
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
        let root = CstNodeId::from_index(0);
        // Estimate ~1 node per 8 source bytes; ~70% are leaves.
        let estimated_nodes = (source.len() / 8).max(16);
        let estimated_leaves = estimated_nodes * 7 / 10;
        let mut nodes = Vec::with_capacity(estimated_nodes);
        nodes.push(CstNode {
            kind: CstKind::Document,
            span: 0..source.len(),
            parent: None,
            prev_sibling: None,
            next_sibling: None,
            first_child: None,
            last_child: None,
        });
        Self {
            source,
            nodes,
            leaf_ids: Vec::with_capacity(estimated_leaves),
            root,
            stack: Vec::with_capacity(16),
            pending_chapter: None,
            pending_verse: None,
            pending_milestone: None,
            pending_usfm: false,
        }
    }

    fn finish_parts(mut self) -> (CstNodeId, Vec<CstNode>, Vec<CstNodeId>) {
        self.flush_pending_milestone();
        self.flush_pending_chapter();
        self.flush_pending_verse();
        self.close_all();
        (self.root, self.nodes, self.leaf_ids)
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
                self.append_rest_tokens(&span, number.len(), rest);
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
                self.append_rest_tokens(&span, number.len(), rest);
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
                marker: MarkerName::from("c"),
            },
            span.clone(),
        );
        self.append_leaf_to(
            CstKind::MarkerToken {
                normalized: MarkerName::from("c"),
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
                marker: MarkerName::from("v"),
            },
            span.clone(),
        );
        self.append_leaf_to(
            CstKind::MarkerToken {
                normalized: MarkerName::from("v"),
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
        let normalized = MarkerName::from(lexer::strip_marker_backslash(marker));
        let id = self.open_structural(CstKind::Milestone { marker: normalized }, span.clone());
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
        let normalized =
            MarkerName::from(lexer::strip_marker_backslash(marker).trim_start_matches('+'));
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
                    normalized: MarkerName::from(normalized),
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
                normalized: MarkerName::from(normalized),
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
        let name = MarkerName::from(lexer::strip_marker_backslash(marker));

        if name == "usfm" {
            self.pending_usfm = true;
        }

        let info_kind = name.kind();
        let valid_in_note = name.valid_in_note();

        if matches!(
            info_kind,
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

        match info_kind {
            MarkerKind::Header => {
                self.close_block_context();
                let id = self.open_structural(CstKind::Para { marker: name }, span.clone());
                if name == "id" {
                    self.nodes[id.index()].kind = CstKind::Book { marker: name };
                }
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Paragraph => {
                self.close_block_context();
                let id = self.open_structural(CstKind::Para { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Note => {
                let id = self.open_structural(CstKind::Note { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Character => {
                let clean_name = MarkerName::from(name.as_str().trim_start_matches('+'));
                let closed_sibling = if self.in_note_context()
                    && valid_in_note
                    && self.is_same_note_family(clean_name.as_str())
                    && clean_name != "fv"
                {
                    self.close_character_in_note()
                } else {
                    false
                };
                let id = self.open_structural_from_name(&clean_name, span.clone());
                if clean_name == "ref" {
                    self.nodes[id.index()].kind = CstKind::Ref;
                }
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: clean_name,
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
                let row_id = self.open_structural(CstKind::TableRow { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    row_id,
                    Some(marker),
                );
            }
            MarkerKind::TableCell => {
                self.close_table_cell_in_row();
                let id = self.open_structural(CstKind::TableCell { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Periph => {
                self.close_block_context();
                let id = self.open_structural(CstKind::Periph { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Figure => {
                let id = self.open_structural(CstKind::Figure { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::SidebarStart => {
                self.close_block_context();
                let id = self.open_structural(CstKind::Sidebar { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::SidebarEnd => self.close_sidebar(span, marker, name.as_str()),
            MarkerKind::Meta => {
                if name == "cat" && self.in_note_or_sidebar_context() {
                    let id = self.open_structural(CstKind::Para { marker: name }, span.clone());
                    self.append_leaf_to(
                        CstKind::MarkerToken {
                            normalized: name,
                            token_kind: MarkerTokenKind::Regular,
                        },
                        span,
                        id,
                        Some(marker),
                    );
                } else if name == "rem" && !self.in_note_context() && self.has_open_paragraph() {
                    self.close_inline_above_paragraph();
                    let id = self.open_structural(CstKind::Para { marker: name }, span.clone());
                    self.append_leaf_to(
                        CstKind::MarkerToken {
                            normalized: name,
                            token_kind: MarkerTokenKind::Regular,
                        },
                        span,
                        id,
                        Some(marker),
                    );
                } else {
                    self.close_block_context();
                    let id = self.open_structural(CstKind::Para { marker: name }, span.clone());
                    self.append_leaf_to(
                        CstKind::MarkerToken {
                            normalized: name,
                            token_kind: MarkerTokenKind::Regular,
                        },
                        span,
                        id,
                        Some(marker),
                    );
                }
            }
            MarkerKind::Unknown => {
                // Close any existing Unknown or Character siblings on the
                // stack to prevent unbounded nesting of sequential unknown
                // markers (e.g. thousands of \t in a concordance file).
                while matches!(
                    self.stack.last().map(|open| open.kind),
                    Some(MarkerKind::Character) | Some(MarkerKind::Unknown)
                ) {
                    let open = self.stack.pop().unwrap();
                    self.refresh_span(open.id);
                }
                let id = self.open_structural(CstKind::Unknown { marker: name }, span.clone());
                self.append_leaf_to(
                    CstKind::MarkerToken {
                        normalized: name,
                        token_kind: MarkerTokenKind::Regular,
                    },
                    span,
                    id,
                    Some(marker),
                );
            }
            MarkerKind::Chapter => self.handle_chapter(span),
            MarkerKind::Verse => self.handle_verse(span),
            MarkerKind::MilestoneStart | MarkerKind::MilestoneEnd => {
                self.handle_milestone(span, marker)
            }
        }
    }

    fn handle_milestone_end(&mut self, span: Span) {
        if let Some(id) = self.pending_milestone.take() {
            self.append_leaf_to(CstKind::MilestoneEndToken, span, id, Some("\\*"));
            self.refresh_span(id);
            return;
        }
        // \zms\* pattern: unknown/char on stack with no content children → close it
        if let Some(top) = self.stack.last()
            && matches!(top.kind, MarkerKind::Unknown | MarkerKind::Character)
        {
            let top_id = top.id;
            let has_content = self.nodes[top_id.index()]
                .first_child
                .map(|fc| {
                    let mut cid = Some(fc);
                    while let Some(c) = cid {
                        if !matches!(
                            self.nodes[c.index()].kind,
                            CstKind::MarkerToken { .. }
                                | CstKind::WhitespaceToken
                                | CstKind::NewlineToken
                                | CstKind::AttributesToken
                        ) {
                            return true;
                        }
                        cid = self.nodes[c.index()].next_sibling;
                    }
                    false
                })
                .unwrap_or(false);
            if !has_content {
                self.append_leaf_to(CstKind::MilestoneEndToken, span, top_id, Some("\\*"));
                let open = self.stack.pop().unwrap();
                self.refresh_span(open.id);
                return;
            }
        }
        self.append_leaf(CstKind::MilestoneEndToken, span, Some("\\*"));
    }

    fn open_structural_from_name(&mut self, name: &MarkerName, span: Span) -> CstNodeId {
        let kind = match name.kind() {
            MarkerKind::Note => CstKind::Note { marker: *name },
            MarkerKind::Figure => CstKind::Figure { marker: *name },
            MarkerKind::TableRow => CstKind::TableRow { marker: *name },
            MarkerKind::TableCell => CstKind::TableCell { marker: *name },
            MarkerKind::SidebarStart => CstKind::Sidebar { marker: *name },
            MarkerKind::Periph => CstKind::Periph { marker: *name },
            MarkerKind::Paragraph | MarkerKind::Header | MarkerKind::Meta => {
                CstKind::Para { marker: *name }
            }
            MarkerKind::Unknown => CstKind::Unknown { marker: *name },
            _ => CstKind::Char { marker: *name },
        };
        self.open_structural(kind, span)
    }

    fn open_structural(&mut self, kind: CstKind, span: Span) -> CstNodeId {
        let parent = self.current_parent();
        let id = self.push_node(kind, span, parent);
        let marker = match &self.nodes[id.index()].kind {
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
            | CstKind::Unknown { marker } => *marker,
            CstKind::Ref => MarkerName::from("ref"),
            CstKind::Table => MarkerName::from("table"),
            _ => MarkerName::from(""),
        };
        if !matches!(
            self.nodes[id.index()].kind,
            CstKind::Chapter { .. } | CstKind::Verse { .. } | CstKind::Milestone { .. }
        ) {
            let marker_kind = match &self.nodes[id.index()].kind {
                CstKind::Book { .. } => MarkerKind::Header,
                CstKind::Para { marker } => marker.kind(),
                CstKind::Char { .. } | CstKind::Ref => MarkerKind::Character,
                CstKind::Note { .. } => MarkerKind::Note,
                CstKind::Figure { .. } => MarkerKind::Figure,
                CstKind::Sidebar { .. } => MarkerKind::SidebarStart,
                CstKind::Periph { .. } => MarkerKind::Periph,
                CstKind::TableRow { .. } => MarkerKind::TableRow,
                CstKind::TableCell { .. } => MarkerKind::TableCell,
                CstKind::Unknown { .. } => MarkerKind::Unknown,
                _ => marker.kind(),
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
        if self.nodes[id.index()].kind.is_leaf() {
            self.leaf_ids.push(id);
        }
        id
    }

    fn push_regular_node(&mut self, kind: CstKind, span: Span, parent: CstNodeId) -> CstNodeId {
        let id = CstNodeId::from_index(self.nodes.len());
        let prev_sibling = self.nodes[parent.index()].last_child;
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
            self.nodes[prev.index()].next_sibling = Some(id);
        } else {
            self.nodes[parent.index()].first_child = Some(id);
        }
        self.nodes[parent.index()].last_child = Some(id);
        self.bump_span(parent, id);
        id
    }

    fn ensure_table_parent(&mut self, parent: CstNodeId, span: Span) -> CstNodeId {
        if let Some(last_child) = self.nodes[parent.index()].last_child
            && matches!(self.nodes[last_child.index()].kind, CstKind::Table)
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
            let actual = &self.source[span.clone()];
            if text != actual {
                eprintln!(
                    "debug_assert failure: expected {:?}, found {:?} at {:?}",
                    text, actual, span
                );
            }
            // debug_assert_eq!(text, actual);
        }
        self.push_node(kind, span, parent)
    }

    /// After splitting a verse/chapter number from text, emit the remainder:
    /// leading whitespace as a `WhitespaceToken`, then any remaining content
    /// as a `TextToken`.  This keeps trivia distinct from text content.
    fn append_rest_tokens(&mut self, span: &Span, number_len: usize, rest: &str) {
        let rest_start = span.start + number_len;
        let ws_len = rest.len() - rest.trim_start().len();
        if ws_len > 0 {
            self.append_leaf(
                CstKind::WhitespaceToken,
                rest_start..rest_start + ws_len,
                Some(&rest[..ws_len]),
            );
        }
        let content = &rest[ws_len..];
        if !content.is_empty() {
            self.append_leaf(
                CstKind::TextToken,
                rest_start + ws_len..span.end,
                Some(content),
            );
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
                    normalized: MarkerName::from(normalized),
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
                normalized: MarkerName::from(normalized),
                token_kind: MarkerTokenKind::Regular,
            },
            span,
            Some(marker),
        );
    }

    fn close_character_in_note(&mut self) -> bool {
        let mut closed_any = false;
        while let Some(MarkerKind::Character)
        | Some(MarkerKind::Unknown)
        | Some(MarkerKind::TableCell) = self.stack.last().map(|open| open.kind)
        {
            let open = self.stack.pop().unwrap();
            self.refresh_span(open.id);
            closed_any = true;
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
        let Some(first_child) = self.nodes[id.index()].first_child else {
            return;
        };
        let last_child = self.nodes[id.index()].last_child.unwrap();
        let new_span =
            self.nodes[first_child.index()].span.start..self.nodes[last_child.index()].span.end;
        self.nodes[id.index()].span = new_span.clone();
        if let Some(parent) = self.nodes[id.index()].parent {
            self.bump_span(parent, id);
        }
    }

    fn bump_span(&mut self, parent: CstNodeId, child: CstNodeId) {
        let child_span = self.nodes[child.index()].span.clone();
        let is_first_child = self.nodes[parent.index()].first_child == Some(child);
        let parent_span = &mut self.nodes[parent.index()].span;
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
    use crate::builder;

    #[test]
    fn lossless_round_trip_preserves_source() {
        let input = "\\p  \\v 1  In the beginning\n";
        let parsed = parse(input);
        assert_eq!(parsed.to_source_string(), input);
    }

    #[test]
    fn parse_is_cst_only_and_does_not_lower() {
        builder::reset_lower_invocations_for_tests();
        let _ = parse("\\p \\v 1 In the beginning");
        assert_eq!(builder::lower_invocations_for_tests(), 0);
    }

    #[test]
    fn leaf_lookup_covers_whitespace_gap() {
        let input = "\\p  \\v 1  In the beginning";
        let parsed = parse(input);
        let first_gap = parsed.leaf_at_offset(2).unwrap();
        let second_gap = parsed.leaf_at_offset(8).unwrap();
        assert!(matches!(
            parsed.node(first_gap).kind,
            CstKind::WhitespaceToken
        ));
        assert!(matches!(
            parsed.node(second_gap).kind,
            CstKind::WhitespaceToken
        ));
    }
}
