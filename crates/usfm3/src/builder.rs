//! Tree-walk CST→AST lowering for USFM 3.x.
//!
//! Walks the CST tree structure directly to produce an AST
//! ([`crate::ast::Document`]) together with a list of diagnostics.

use crate::ast::{Attribute, ChapterData, CharData, Document, Node, VerseData};
use crate::cst::{self, CstDocument, CstKind, CstNode, CstNodeId, MarkerTokenKind};
use crate::diagnostics::{Diagnostic, DiagnosticList, Span};
use crate::markers::{self, MarkerKind, MarkerName};
use crate::source_map::{SourceMap, SourceNode, SourceSpans};
use crate::{AstDocument, ParseOptions};
use std::borrow::Cow;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static LOWER_INVOCATIONS: Cell<usize> = const { Cell::new(0) };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub type ParseResult<'a> = AstDocument<'a>;

#[derive(Debug, Clone)]
struct SpannedNode<'a> {
    node: Node<'a>,
    source: SourceNode,
}

impl<'a> SpannedNode<'a> {
    fn new(node: Node<'a>, source: SourceNode) -> Self {
        Self { node, source }
    }

    fn text(s: impl Into<Cow<'a, str>>) -> Self {
        Self::new(Node::Text(s.into()), SourceNode::leaf())
    }

    fn optbreak() -> Self {
        Self::new(Node::OptBreak, SourceNode::leaf())
    }

    fn marker(&self) -> Option<MarkerName> {
        self.node.marker()
    }

    fn source_span(&self) -> Option<&Span> {
        self.source.spans.as_ref().map(|spans| &spans.node)
    }
}

/// Parse a USFM source string into an eager AST document with diagnostics.
pub fn parse(input: &str) -> AstDocument<'static> {
    let cst = cst::parse(input);
    lower(&cst, ParseOptions { diagnostics: true }).into_owned()
}

pub fn parse_owned(input: String) -> AstDocument<'static> {
    let cst = cst::parse_owned(input);
    lower(&cst, ParseOptions { diagnostics: true }).into_owned()
}

pub fn lower<'a>(document: &'a CstDocument, options: ParseOptions) -> AstDocument<'a> {
    #[cfg(test)]
    LOWER_INVOCATIONS.with(|count| count.set(count.get() + 1));

    let mut ctx = LowerCtx::new(document, options.diagnostics);
    let root = document.node(document.root_id());
    let mut content = ctx.lower_children(root);
    trim_trailing_text(&mut content);

    let (ast, source_map) = split_document(content);

    let diagnostics = if options.diagnostics {
        let mut diagnostics = ctx.diagnostics;
        diagnostics.extend(crate::validation::validate(&ast, &source_map));
        diagnostics.sort_in_document_order();
        Some(diagnostics.into_inner())
    } else {
        None
    };
    AstDocument {
        ast,
        source_map,
        diagnostics,
    }
}

#[cfg(test)]
pub(crate) fn parse_from_cst<'a>(document: &'a CstDocument) -> AstDocument<'a> {
    lower(document, ParseOptions { diagnostics: true })
}

#[cfg(test)]
pub(crate) fn reset_lower_invocations_for_tests() {
    LOWER_INVOCATIONS.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn lower_invocations_for_tests() -> usize {
    LOWER_INVOCATIONS.with(Cell::get)
}

// ---------------------------------------------------------------------------
// Lowering context
// ---------------------------------------------------------------------------

struct LowerCtx<'a> {
    doc: &'a CstDocument,
    collect_diagnostics: bool,
    diagnostics: DiagnosticList,
    current_book_code: Option<Cow<'a, str>>,
    current_chapter: Option<Cow<'a, str>>,
    seen_id: bool,
    text_scratch: String,
}

impl<'a> LowerCtx<'a> {
    fn new(doc: &'a CstDocument, collect_diagnostics: bool) -> Self {
        Self {
            doc,
            collect_diagnostics,
            diagnostics: DiagnosticList::new(),
            current_book_code: None,
            current_chapter: None,
            seen_id: false,
            text_scratch: String::new(),
        }
    }

    // -----------------------------------------------------------------
    // Tree traversal helpers
    // -----------------------------------------------------------------

    /// Iterate direct children of a CST node.
    fn children_iter(&self, parent: &CstNode) -> ChildIter<'a> {
        ChildIter {
            doc: self.doc,
            next: parent.first_child,
        }
    }

    /// Lower all structural (non-leaf) children of a CST node into AST nodes.
    fn lower_children(&mut self, parent: &CstNode) -> Vec<SpannedNode<'a>> {
        let parent_is_root = matches!(parent.kind, CstKind::Document);
        let parent_is_table = matches!(parent.kind, CstKind::Table);
        let mut verse_warned = false;
        let mut result: Vec<SpannedNode<'a>> = Vec::new();
        let mut child_id = parent.first_child;
        while let Some(id) = child_id {
            let node = self.doc.node(id);
            child_id = node.next_sibling;
            if node.kind.is_leaf() {
                // Stray closing markers produce diagnostics.
                if let CstKind::ClosingMarkerToken { normalized, .. } = &node.kind {
                    self.push_diagnostic(|| 
                        Diagnostic::stray_close(normalized.as_str(), node.span.clone())
                            .with_anchor_cst(id.index()),
                    );
                }
                // Stray \esbe (sidebar end with no matching \esb)
                if let CstKind::MarkerToken { normalized, .. } = &node.kind
                    && normalized.kind() == MarkerKind::SidebarEnd
                {
                    self.push_diagnostic(|| 
                        Diagnostic::stray_close(normalized.as_str(), node.span.clone())
                            .with_anchor_cst(id.index()),
                    );
                }
                continue;
            }
            // Root-level verse recovery: wrap in implicit \p
            if parent_is_root && matches!(node.kind, CstKind::Verse { .. }) {
                if !verse_warned {
                    verse_warned = true;
                    self.push_diagnostic(|| 
                        Diagnostic::verse_outside_paragraph(node.span.clone())
                            .with_anchor_cst(id.index()),
                    );
                }
                let verse_span = node.span.clone();
                let mut para_children: Vec<SpannedNode<'a>> = Vec::new();
                let mut after_verse = true;
                if let Some(v) = self.lower_verse(id, node) {
                    para_children.push(v);
                }
                // Collect subsequent root-level verses and interleaved text
                while let Some(next_id) = child_id {
                    let next = self.doc.node(next_id);
                    if matches!(next.kind, CstKind::Verse { .. }) {
                        after_verse = true;
                        if let Some(v) = self.lower_verse(next_id, next) {
                            para_children.push(v);
                        }
                        child_id = next.next_sibling;
                        continue;
                    }
                    if next.kind.is_leaf() {
                        match &next.kind {
                            CstKind::TextToken => {
                                after_verse = false;
                                let text = self.doc.source_text(next_id);
                                self.append_normalized_text(&mut para_children, text);
                            }
                            CstKind::WhitespaceToken => {
                                if !after_verse {
                                    Self::append_text_to(&mut para_children, " ");
                                }
                            }
                            CstKind::NewlineToken => {
                                after_verse = false;
                                if let Some(prev) = para_children
                                    .last_mut()
                                    .map(|node| &mut node.node)
                                    .and_then(|node| match node {
                                        Node::Text(prev) => Some(prev),
                                        _ => None,
                                    })
                                    && !prev.ends_with(' ')
                                    && !prev.ends_with('\u{00a0}')
                                {
                                    match prev {
                                        Cow::Borrowed(s) => {
                                            let mut owned = s.to_string();
                                            owned.push(' ');
                                            *prev = Cow::Owned(owned);
                                        }
                                        Cow::Owned(s) => s.push(' '),
                                    }
                                }
                            }
                            _ => {}
                        }
                        child_id = next.next_sibling;
                        continue;
                    }
                    break;
                }
                let (content, children) = split_nodes(para_children);
                result.push(SpannedNode::new(
                    Node::Para {
                        marker: MarkerName::from("p"),
                        content,
                    },
                    SourceNode::structural(
                        SourceSpans::node(verse_span),
                        children,
                        Some(id.index()),
                    ),
                ));
                continue;
            }
            if let Some(ast_node) = self.lower_structural(id, node) {
                // ca/cp/va/vp metadata absorption
                let maybe_marker = ast_node.marker();
                if let Some(m) = maybe_marker
                    && matches!(m.as_str(), "ca" | "cp" | "va" | "vp")
                    && let Some(text) = extract_node_text(&ast_node)
                {
                    // Remove preceding whitespace-only text node
                    if let Some(Node::Text(t)) = result.last().map(|node| &node.node)
                        && t.trim().is_empty()
                    {
                        result.pop();
                    }
                    match m.as_str() {
                        "ca" => Self::set_chapter_altnumber(&mut result, text.into()),
                        "cp" => Self::set_chapter_pubnumber(&mut result, text.into()),
                        "va" => Self::set_verse_altnumber(&mut result, text.into()),
                        "vp" => Self::set_verse_pubnumber(&mut result, text.into()),
                        _ => unreachable!(),
                    }
                    continue;
                }
                // Table row grouping (only when parent is not already a Table)
                if !parent_is_table && matches!(&ast_node.node, Node::TableRow { .. }) {
                    if let Some(Node::Table { content, .. }) =
                        result.last_mut().map(|node| &mut node.node)
                    {
                        content.push(ast_node.node);
                        if let Some(last) = result.last_mut() {
                            last.source.children.push(ast_node.source);
                        }
                    } else {
                        let span = ast_node.source_span().cloned().unwrap_or(0..0);
                        result.push(SpannedNode::new(
                            Node::Table {
                                content: vec![ast_node.node],
                            },
                            SourceNode::structural(
                                SourceSpans::node(span),
                                vec![ast_node.source],
                                None,
                            ),
                        ));
                    }
                    continue;
                }
                result.push(ast_node);
            }
        }
        result
    }

    /// Lower a single structural CST node into an AST node.
    fn lower_structural(&mut self, id: CstNodeId, node: &'a CstNode) -> Option<SpannedNode<'a>> {
        match &node.kind {
            CstKind::Book { marker } => self.lower_book(id, node, marker),
            CstKind::Chapter { .. } => self.lower_chapter(id, node),
            CstKind::Verse { .. } => self.lower_verse(id, node),
            CstKind::Para { marker } => self.lower_para(id, node, marker),
            CstKind::Char { marker } => self.lower_char(id, node, marker),
            CstKind::Ref => self.lower_ref(id, node),
            CstKind::Note { marker } => self.lower_note(id, node, marker),
            CstKind::Milestone { marker } => self.lower_milestone(id, node, marker),
            CstKind::Figure { marker } => self.lower_figure(id, node, marker),
            CstKind::Sidebar { marker } => self.lower_sidebar(id, node, marker),
            CstKind::Periph { marker } => self.lower_periph(id, node, marker),
            CstKind::Table => self.lower_table(id, node),
            CstKind::TableRow { marker } => self.lower_table_row(id, node, marker),
            CstKind::TableCell { marker } => self.lower_table_cell(id, node, marker),
            CstKind::Unknown { marker } => self.lower_unknown(id, node, marker),
            _ => None,
        }
    }

    // -----------------------------------------------------------------
    // Leaf text collection
    // -----------------------------------------------------------------

    /// Collect content from the leaf children of a structural node,
    /// handling whitespace normalization, text merging, attributes, and
    /// closing marker spans.
    fn collect_content(
        &mut self,
        parent: &CstNode,
        _parent_id: CstNodeId,
        is_block: bool,
        close_span: &mut Option<Span>,
        attributes: &mut Vec<Attribute<'a>>,
        marker_name: &str,
    ) -> Vec<SpannedNode<'a>> {
        let mut children: Vec<SpannedNode<'a>> = Vec::new();
        let mut after_open = true;
        let mut after_close = false;
        let mut pending_space = false;

        let mut child_id_opt = parent.first_child;
        while let Some(child_id) = child_id_opt {
            let child = self.doc.node(child_id);
            child_id_opt = child.next_sibling;

            match &child.kind {
                CstKind::MarkerToken { normalized, .. } => {
                    // Stray \esbe inside a paragraph/element
                    if normalized.kind() == MarkerKind::SidebarEnd {
                        self.push_diagnostic(|| 
                            Diagnostic::stray_close(normalized.as_str(), child.span.clone())
                                .with_anchor_cst(child_id.index()),
                        );
                    }
                    after_open = true;
                    after_close = false;
                    continue;
                }
                CstKind::ClosingMarkerToken { normalized, .. } => {
                    let close_name = normalized.as_str().trim_start_matches('+');
                    let parent_name = marker_name.trim_start_matches('+');
                    if close_name == parent_name {
                        *close_span = Some(child.span.clone());
                        after_close = true;
                        after_open = false;
                        pending_space = false;
                    } else {
                        self.push_diagnostic(|| 
                            Diagnostic::stray_close(normalized.as_str(), child.span.clone())
                                .with_anchor_cst(child_id.index()),
                        );
                    }
                    continue;
                }
                CstKind::MilestoneEndToken => {
                    after_close = true;
                    after_open = false;
                    pending_space = false;
                    continue;
                }
                CstKind::WhitespaceToken => {
                    if after_open || pending_space {
                        continue;
                    }
                    if after_close {
                        pending_space = true;
                        continue;
                    }
                    Self::append_text_to(&mut children, " ");
                    continue;
                }
                CstKind::NewlineToken => {
                    if after_open || after_close {
                        if after_close {
                            pending_space = true;
                        }
                        continue;
                    }
                    // Newline becomes word boundary — use pending_space to merge
                    // with any adjacent whitespace tokens (spec: newline + spaces
                    // reduce to a single newline, which is a word boundary)
                    pending_space = true;
                    continue;
                }
                CstKind::AttributesToken => {
                    let text = self.doc.source_text(child_id);
                    let parsed = match parse_attributes(text) {
                        Some(attrs) => attrs,
                        None => {
                            self.push_diagnostic(|| 
                                Diagnostic::malformed_attributes(child.span.clone())
                                    .with_anchor_cst(child_id.index()),
                            );
                            Self::append_text_to(&mut children, text);
                            continue;
                        }
                    };
                    if !parsed.is_empty() {
                        let resolved = resolve_default_attr_keys(marker_name, parsed);
                        attributes.extend(resolved);
                    }
                    after_open = false;
                    after_close = false;
                    continue;
                }
                CstKind::TextToken => {
                    if pending_space {
                        pending_space = false;
                        Self::append_text_to(&mut children, " ");
                    }
                    after_open = false;
                    after_close = false;
                    let text = self.doc.source_text(child_id);
                    self.append_normalized_text(&mut children, text);
                    continue;
                }
                _ if !child.kind.is_leaf() => {
                    // Structural child — recurse
                    if pending_space {
                        pending_space = false;
                        Self::append_text_to(&mut children, " ");
                    }
                    after_open = false;
                    after_close = false;
                    if let Some(ast_child) = self.lower_structural(child_id, child) {
                        // ca/cp/va/vp metadata absorption
                        let maybe_marker = ast_child.marker();
                        if let Some(m) = maybe_marker
                            && matches!(m.as_str(), "ca" | "cp" | "va" | "vp")
                            && let Some(text) = extract_node_text(&ast_child)
                        {
                            if let Some(Node::Text(t)) = children.last().map(|node| &node.node)
                                && t.trim().is_empty()
                            {
                                children.pop();
                            }
                            match m.as_str() {
                                "ca" => Self::set_chapter_altnumber(&mut children, text.into()),
                                "cp" => Self::set_chapter_pubnumber(&mut children, text.into()),
                                "va" => Self::set_verse_altnumber(&mut children, text.into()),
                                "vp" => Self::set_verse_pubnumber(&mut children, text.into()),
                                _ => unreachable!(),
                            }
                            // Skip whitespace after consumed metadata
                            after_close = true;
                            continue;
                        }
                        // Table row grouping
                        if matches!(&ast_child.node, Node::TableRow { .. }) {
                            if let Some(last) = children.last_mut()
                                && let Node::Table { content, .. } = &mut last.node
                            {
                                content.push(ast_child.node);
                                last.source.children.push(ast_child.source);
                                continue;
                            }
                            let span = ast_child.source_span().cloned().unwrap_or(0..0);
                            children.push(SpannedNode::new(
                                Node::Table {
                                    content: vec![ast_child.node],
                                },
                                SourceNode::structural(
                                    SourceSpans::node(span),
                                    vec![ast_child.source],
                                    None,
                                ),
                            ));
                        } else {
                            children.push(ast_child);
                        }
                    }
                    continue;
                }
                _ => continue,
            }
        }

        if is_block {
            trim_trailing_text(&mut children);
        } else if pending_space {
            // For character-level elements, a trailing word-boundary space
            // (from a newline before the next sibling marker) is preserved —
            // USFM spec: keep trailing WS in char element when followed by
            // more content.  The caller will trim if not needed.
            Self::append_text_to(&mut children, " ");
        }
        children
    }

    fn push_diagnostic(&mut self, make_diagnostic: impl FnOnce() -> Diagnostic) {
        if self.collect_diagnostics {
            self.diagnostics.push(make_diagnostic());
        }
    }

    // -----------------------------------------------------------------
    // Node lowering methods
    // -----------------------------------------------------------------

    fn lower_book(
        &mut self,
        id: CstNodeId,
        node: &'a CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        if marker == "id" {
            if self.seen_id {
                self.push_diagnostic(|| 
                    Diagnostic::duplicate_id(node.span.clone()).with_anchor_cst(id.index()),
                );
                return None;
            }
            self.seen_id = true;
        }

        let mut code = Cow::Borrowed("");
        let mut code_span: Option<Span> = None;
        let mut children: Vec<SpannedNode<'a>> = Vec::new();
        let mut after_open = true;
        let mut got_code = false;

        let mut child_id_opt = node.first_child;
        while let Some(child_id) = child_id_opt {
            let child = self.doc.node(child_id);
            child_id_opt = child.next_sibling;
            match &child.kind {
                CstKind::MarkerToken { .. } => {
                    after_open = true;
                    continue;
                }
                CstKind::WhitespaceToken | CstKind::NewlineToken => {
                    if after_open {
                        continue;
                    }
                    if got_code {
                        Self::append_text_to(&mut children, " ");
                    }
                    continue;
                }
                CstKind::TextToken => {
                    after_open = false;
                    let text = self.doc.source_text(child_id);
                    if !got_code {
                        got_code = true;
                        let (c, rest) = split_first_word(text);
                        let book_code = Cow::Borrowed(c);
                        self.current_book_code = Some(book_code.clone());
                        code = book_code;
                        code_span = Some(child.span.start..child.span.start + c.len());
                        if !rest.is_empty() {
                            if rest.contains('~') {
                                children.push(SpannedNode::text(Cow::Owned(
                                    rest.replace('~', "\u{00a0}"),
                                )));
                            } else {
                                children.push(SpannedNode::text(Cow::Borrowed(rest)));
                            };
                        }
                    } else {
                        self.append_normalized_text(&mut children, text);
                    }
                    continue;
                }
                _ if !child.kind.is_leaf() => {
                    after_open = false;
                    if let Some(ast_child) = self.lower_structural(child_id, child) {
                        children.push(ast_child);
                    }
                    continue;
                }
                _ => continue,
            }
        }

        trim_trailing_text(&mut children);
        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = code_span {
            spans = spans.with_code(cs);
        }
        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Book {
                marker: *marker,
                code,
                content,
            },
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    fn lower_chapter(&mut self, id: CstNodeId, node: &CstNode) -> Option<SpannedNode<'a>> {
        let marker_span = self.find_marker_span(node);
        let (number_str, number_span) = self.find_text_child(node);
        let number: Cow<'a, str> = Cow::Owned(number_str);

        if number.is_empty() {
            self.push_diagnostic(|| 
                Diagnostic::missing_chapter_number(marker_span.clone()).with_anchor_cst(id.index()),
            );
        }

        if number.starts_with('0') && number.len() > 1 {
            self.push_diagnostic(|| 
                Diagnostic::leading_zeros(&number, marker_span.clone()).with_anchor_cst(id.index()),
            );
        }

        self.current_chapter = Some(number.clone());
        let book = self.current_book_code.as_deref().unwrap_or("");
        let sid = Some(Cow::Owned(format!(
            "{} {}",
            book,
            strip_leading_zeros(&number)
        )));

        let mut spans = SourceSpans::node(marker_span);
        if let Some(ns) = number_span {
            spans = spans.with_number(ns);
        }

        Some(SpannedNode::new(
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number,
                sid,
                altnumber: None,
                pubnumber: None,
            })),
            SourceNode::structural(spans, Vec::new(), Some(id.index())),
        ))
    }

    fn lower_verse(&mut self, id: CstNodeId, node: &CstNode) -> Option<SpannedNode<'a>> {
        let marker_span = self.find_marker_span(node);
        let (number_str, number_span) = self.find_text_child(node);
        let number: Cow<'a, str> = Cow::Owned(number_str);

        if number.is_empty() {
            self.push_diagnostic(|| 
                Diagnostic::missing_verse_number(marker_span.clone()).with_anchor_cst(id.index()),
            );
        }

        if number.starts_with('0') && number.len() > 1 {
            self.push_diagnostic(|| 
                Diagnostic::leading_zeros(&number, marker_span.clone()).with_anchor_cst(id.index()),
            );
        }

        let book = self.current_book_code.as_deref().unwrap_or("");
        let ch = self.current_chapter.as_deref().unwrap_or("");
        let sid = Some(Cow::Owned(format!(
            "{} {}:{}",
            book,
            strip_leading_zeros(ch),
            strip_leading_zeros(&number)
        )));

        let mut spans = SourceSpans::node(marker_span);
        if let Some(ns) = number_span {
            spans = spans.with_number(ns);
        }

        Some(SpannedNode::new(
            Node::Verse(Box::new(VerseData {
                marker: "v".into(),
                number,
                sid,
                altnumber: None,
                pubnumber: None,
            })),
            SourceNode::structural(spans, Vec::new(), Some(id.index())),
        ))
    }

    fn lower_para(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        // \usfm marker — discard entirely (version string is absorbed)
        if marker == "usfm" {
            return None;
        }

        if marker == "addpn" {
            self.push_diagnostic(|| 
                Diagnostic::deprecated_marker(
                    marker.as_str(),
                    "nested \\pn ...\\pn* within \\add ...\\add*",
                    node.span.clone(),
                )
                .with_anchor_cst(id.index()),
            );
        }

        let is_block = true;
        let mut close_span = None;
        let mut attributes = Vec::new();
        let children = self.collect_content(
            node,
            id,
            is_block,
            &mut close_span,
            &mut attributes,
            marker.as_str(),
        );

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Para {
                marker: *marker,
                content,
            },
            SourceNode::structural(
                SourceSpans::node(node.span.clone()),
                children,
                Some(id.index()),
            ),
        ))
    }

    fn lower_char(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let nested = self.is_nested_marker(node);

        if marker == "addpn" {
            self.push_diagnostic(|| 
                Diagnostic::deprecated_marker(
                    marker.as_str(),
                    "nested \\pn ...\\pn* within \\add ...\\add*",
                    node.span.clone(),
                )
                .with_anchor_cst(id.index()),
            );
        }

        let mut close_span = None;
        let mut attributes = Vec::new();
        let children = self.collect_content(
            node,
            id,
            false,
            &mut close_span,
            &mut attributes,
            marker.as_str(),
        );

        // Check for implicit close / unclosed diagnostics
        self.check_close_diagnostics(id, node, marker, &close_span);

        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = close_span {
            spans = spans.with_close(cs);
        }

        let clean_marker_str = marker.as_str().trim_start_matches('+');

        if clean_marker_str == "ref" {
            let (content, children) = split_nodes(children);
            return Some(SpannedNode::new(
                Node::Ref {
                    content,
                    attributes,
                },
                SourceNode::structural(spans, children, Some(id.index())),
            ));
        }

        if clean_marker_str == "xt" && nested {
            let has_ref_child = children.iter().any(|n| matches!(&n.node, Node::Ref { .. }));
            let href_value = attributes
                .iter()
                .find(|a| a.key == "link-href")
                .map(|a| a.value.clone());
            let final_children = if let Some(ref loc) = href_value {
                if !has_ref_child && !children.is_empty() {
                    let (content, children) = split_nodes(children);
                    vec![SpannedNode::new(
                        Node::Ref {
                            content,
                            attributes: vec![Attribute {
                                key: Cow::Borrowed("loc"),
                                value: loc.clone(),
                            }],
                        },
                        SourceNode::structural(
                            SourceSpans::node(node.span.clone()),
                            children,
                            Some(id.index()),
                        ),
                    )]
                } else {
                    children
                }
            } else {
                children
            };
            let (content, children) = split_nodes(final_children);
            return Some(SpannedNode::new(
                Node::Char(Box::new(CharData {
                    marker: *marker,
                    content,
                    attributes,
                })),
                SourceNode::structural(spans, children, Some(id.index())),
            ));
        }

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Char(Box::new(CharData {
                marker: *marker,
                content,
                attributes,
            })),
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    fn lower_ref(&mut self, id: CstNodeId, node: &CstNode) -> Option<SpannedNode<'a>> {
        let ref_marker = MarkerName::from("ref");
        let mut close_span = None;
        let mut attributes = Vec::new();
        let children =
            self.collect_content(node, id, false, &mut close_span, &mut attributes, "ref");

        self.check_close_diagnostics(id, node, &ref_marker, &close_span);

        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = close_span {
            spans = spans.with_close(cs);
        }

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Ref {
                content,
                attributes,
            },
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    fn lower_note(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let mut close_span = None;
        let mut attributes = Vec::new();
        let mut caller = None;
        let mut children: Vec<SpannedNode<'a>> = Vec::new();
        let mut after_open = true;
        let mut after_close = false;
        let mut pending_space = false;

        let mut child_id_opt = node.first_child;
        while let Some(child_id) = child_id_opt {
            let child = self.doc.node(child_id);
            child_id_opt = child.next_sibling;
            match &child.kind {
                CstKind::MarkerToken { .. } => {
                    after_open = true;
                    after_close = false;
                    continue;
                }
                CstKind::ClosingMarkerToken { .. } => {
                    close_span = Some(child.span.clone());
                    after_close = true;
                    after_open = false;
                    pending_space = false;
                    continue;
                }
                CstKind::WhitespaceToken => {
                    if after_open || pending_space {
                        continue;
                    }
                    if after_close {
                        pending_space = true;
                        continue;
                    }
                    Self::append_text_to(&mut children, " ");
                    continue;
                }
                CstKind::NewlineToken => {
                    if after_open || after_close {
                        if after_close {
                            pending_space = true;
                        }
                        continue;
                    }
                    pending_space = true;
                    continue;
                }
                CstKind::AttributesToken => {
                    let text = self.doc.source_text(child_id);
                    if let Some(parsed) = parse_attributes(text) {
                        if !parsed.is_empty() {
                            let resolved = resolve_default_attr_keys(marker.as_str(), parsed);
                            attributes.extend(resolved);
                        }
                    } else {
                        self.push_diagnostic(|| 
                            Diagnostic::malformed_attributes(child.span.clone())
                                .with_anchor_cst(child_id.index()),
                        );
                        Self::append_text_to(&mut children, text);
                    }
                    after_open = false;
                    after_close = false;
                    continue;
                }
                CstKind::TextToken => {
                    if pending_space {
                        pending_space = false;
                        Self::append_text_to(&mut children, " ");
                    }
                    after_open = false;
                    after_close = false;
                    let text = self.doc.source_text(child_id);
                    if caller.is_none() {
                        let trimmed = text.trim_start();
                        if !trimmed.is_empty() {
                            let (c, rest) = split_first_word(trimmed);
                            caller = Some(Cow::Borrowed(c));
                            if !rest.is_empty() {
                                if rest.contains('~') {
                                    children.push(SpannedNode::text(Cow::Owned(
                                        rest.replace('~', "\u{00a0}"),
                                    )));
                                } else {
                                    children.push(SpannedNode::text(Cow::Borrowed(rest)));
                                }
                            }
                        }
                    } else {
                        self.append_normalized_text(&mut children, text);
                    }
                    continue;
                }
                _ if !child.kind.is_leaf() => {
                    if pending_space {
                        pending_space = false;
                        Self::append_text_to(&mut children, " ");
                    }
                    after_open = false;
                    after_close = false;
                    if let Some(ast_child) = self.lower_structural(child_id, child) {
                        children.push(ast_child);
                    }
                    continue;
                }
                _ => continue,
            }
        }

        // Check for unclosed note
        if close_span.is_none() {
            self.push_diagnostic(|| 
                Diagnostic::unclosed_note(marker.as_str(), node.span.clone())
                    .with_anchor_cst(id.index()),
            );
        }

        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = close_span {
            spans = spans.with_close(cs);
        }

        let (category, cat_children) = extract_category(children);

        let (content, children) = split_nodes(cat_children);
        Some(SpannedNode::new(
            Node::Note {
                marker: *marker,
                caller: caller.unwrap_or(Cow::Borrowed("")),
                category,
                content,
            },
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    fn lower_milestone(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let mut attributes = Vec::new();
        let mut has_milestone_end = false;

        for (child_id, child) in self.children_iter(node) {
            match &child.kind {
                CstKind::AttributesToken => {
                    let text = self.doc.source_text(child_id);
                    if let Some(parsed) = parse_attributes(text)
                        && !parsed.is_empty()
                    {
                        let resolved = resolve_default_attr_keys(marker.as_str(), parsed);
                        attributes.extend(resolved);
                    }
                }
                CstKind::MilestoneEndToken => {
                    has_milestone_end = true;
                }
                _ => {}
            }
        }

        if !has_milestone_end {
            self.push_diagnostic(|| 
                Diagnostic::missing_milestone_self_close(marker.as_str(), node.span.clone())
                    .with_anchor_cst(id.index()),
            );
        }

        Some(SpannedNode::new(
            Node::Milestone {
                marker: *marker,
                attributes,
            },
            SourceNode::structural(
                SourceSpans::node(node.span.clone()),
                Vec::new(),
                Some(id.index()),
            ),
        ))
    }

    fn lower_figure(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let mut close_span = None;
        let mut attributes = Vec::new();
        let children = self.collect_content(
            node,
            id,
            false,
            &mut close_span,
            &mut attributes,
            marker.as_str(),
        );

        self.check_close_diagnostics(id, node, marker, &close_span);

        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = close_span {
            spans = spans.with_close(cs);
        }

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Figure {
                marker: *marker,
                content,
                attributes,
            },
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    fn lower_sidebar(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let mut close_span = None;
        let mut attributes = Vec::new();
        let children = self.collect_content(
            node,
            id,
            false,
            &mut close_span,
            &mut attributes,
            marker.as_str(),
        );

        if close_span.is_none() {
            self.push_diagnostic(|| 
                Diagnostic::unclosed_at_eof(marker.as_str(), node.span.clone())
                    .with_anchor_cst(id.index()),
            );
        }

        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = close_span {
            spans = spans.with_close(cs);
        }

        let (category, cat_children) = extract_category(children);

        let (content, children) = split_nodes(cat_children);
        Some(SpannedNode::new(
            Node::Sidebar {
                marker: *marker,
                category,
                content,
            },
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    fn lower_periph(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let mut _close_span = None;
        let mut attributes = Vec::new();
        let mut alt: Option<Cow<'a, str>> = None;
        let mut children: Vec<SpannedNode<'a>> = Vec::new();
        let mut after_open = true;

        let mut child_id_opt = node.first_child;
        while let Some(child_id) = child_id_opt {
            let child = self.doc.node(child_id);
            child_id_opt = child.next_sibling;
            match &child.kind {
                CstKind::MarkerToken { .. } => {
                    after_open = true;
                    continue;
                }
                CstKind::ClosingMarkerToken { .. } => {
                    _close_span = Some(child.span.clone());
                    continue;
                }
                CstKind::WhitespaceToken | CstKind::NewlineToken => {
                    if after_open {
                        continue;
                    }
                    Self::append_text_to(&mut children, " ");
                    continue;
                }
                CstKind::AttributesToken => {
                    let text = self.doc.source_text(child_id);
                    if let Some(parsed) = parse_attributes(text)
                        && !parsed.is_empty()
                    {
                        let resolved = resolve_default_attr_keys(marker.as_str(), parsed);
                        attributes.extend(resolved);
                    }
                    after_open = false;
                    continue;
                }
                CstKind::TextToken => {
                    after_open = false;
                    let text = self.doc.source_text(child_id);
                    if alt.is_none() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            alt = Some(Cow::Borrowed(trimmed));
                        }
                        continue;
                    }
                    self.append_normalized_text(&mut children, text);
                    continue;
                }
                _ if !child.kind.is_leaf() => {
                    after_open = false;
                    if let Some(ast_child) = self.lower_structural(child_id, child) {
                        children.push(ast_child);
                    }
                    continue;
                }
                _ => continue,
            }
        }

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Periph {
                alt,
                content,
                attributes,
            },
            SourceNode::structural(
                SourceSpans::node(node.span.clone()),
                children,
                Some(id.index()),
            ),
        ))
    }

    fn lower_table(&mut self, id: CstNodeId, node: &CstNode) -> Option<SpannedNode<'a>> {
        let children = self.lower_children(node);
        if children.is_empty() {
            return None;
        }
        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Table { content },
            SourceNode::structural(
                SourceSpans::node(node.span.clone()),
                children,
                Some(id.index()),
            ),
        ))
    }

    fn lower_table_row(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let mut close_span = None;
        let mut attributes = Vec::new();
        let mut children = self.collect_content(
            node,
            id,
            true,
            &mut close_span,
            &mut attributes,
            marker.as_str(),
        );

        // Per spec, trim trailing whitespace from the last cell's content
        // (whitespace before the next \tr is structural, not content)
        if let Some(last) = children.last_mut()
            && matches!(last.node, Node::TableCell { .. })
        {
            trim_trailing_text_on_children(last);
        }

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::TableRow {
                marker: *marker,
                content,
            },
            SourceNode::structural(
                SourceSpans::node(node.span.clone()),
                children,
                Some(id.index()),
            ),
        ))
    }

    fn lower_table_cell(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        let mut close_span = None;
        let mut attributes = Vec::new();
        let children = self.collect_content(
            node,
            id,
            false,
            &mut close_span,
            &mut attributes,
            marker.as_str(),
        );

        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = close_span {
            spans = spans.with_close(cs);
        }

        let without_span = if let Some(dash) = marker.rfind('-') {
            let after = &marker.as_str()[dash + 1..];
            if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
                &marker.as_str()[..dash]
            } else {
                marker.as_str()
            }
        } else {
            marker.as_str()
        };
        let base = without_span.trim_end_matches(|c: char| c.is_ascii_digit());
        let align = if base.ends_with('r') {
            Cow::Borrowed("end")
        } else if base == "thc" || base == "tcc" {
            Cow::Borrowed("center")
        } else {
            Cow::Borrowed("start")
        };

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::TableCell {
                marker: *marker,
                align,
                content,
            },
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    fn lower_unknown(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
    ) -> Option<SpannedNode<'a>> {
        // Check if this is a self-closing \zfoo\* pattern (milestone-like)
        if self.has_milestone_end_child(node) && self.has_no_content_children(node) {
            let mut attributes = Vec::new();
            // Collect any attributes from the node's children
            let mut child_id_opt = node.first_child;
            while let Some(cid) = child_id_opt {
                let child = self.doc.node(cid);
                child_id_opt = child.next_sibling;
                if let CstKind::AttributesToken = &child.kind {
                    let text = self.doc.source_text(cid);
                    if let Some(parsed) = parse_attributes(text)
                        && !parsed.is_empty()
                    {
                        let resolved = resolve_default_attr_keys(marker.as_str(), parsed);
                        attributes.extend(resolved);
                    }
                }
            }
            return Some(SpannedNode::new(
                Node::Milestone {
                    marker: *marker,
                    attributes,
                },
                SourceNode::structural(
                    SourceSpans::node(node.span.clone()),
                    Vec::new(),
                    Some(id.index()),
                ),
            ));
        }

        if !marker.as_str().starts_with('z') {
            self.push_diagnostic(|| 
                Diagnostic::unknown_marker(marker.as_str(), node.span.clone())
                    .with_anchor_cst(id.index()),
            );
        }

        let mut close_span = None;
        let mut attributes = Vec::new();
        let children = self.collect_content(
            node,
            id,
            false,
            &mut close_span,
            &mut attributes,
            marker.as_str(),
        );

        self.check_close_diagnostics(id, node, marker, &close_span);

        let mut spans = SourceSpans::node(node.span.clone());
        if let Some(cs) = close_span {
            spans = spans.with_close(cs);
        }

        let (content, children) = split_nodes(children);
        Some(SpannedNode::new(
            Node::Unknown {
                marker: *marker,
                content,
            },
            SourceNode::structural(spans, children, Some(id.index())),
        ))
    }

    // -----------------------------------------------------------------
    // Diagnostic helpers
    // -----------------------------------------------------------------

    /// Check if an inline node was left open and emit diagnostics.
    fn check_close_diagnostics(
        &mut self,
        id: CstNodeId,
        node: &CstNode,
        marker: &MarkerName,
        close_span: &Option<Span>,
    ) {
        if close_span.is_some() {
            return;
        }
        if marker.as_str().starts_with('z') {
            return;
        }
        let marker_kind = marker.kind();
        match marker_kind {
            MarkerKind::Character | MarkerKind::Unknown => {
                if let Some(parent_id) = node.parent {
                    let parent = self.doc.node(parent_id);
                    if matches!(
                        parent.kind,
                        CstKind::Document | CstKind::Para { .. } | CstKind::Book { .. }
                    ) {
                        if node.span.end < self.doc.source().len() {
                            let closer = self.find_implicit_closer(node);
                            self.push_diagnostic(|| 
                                Diagnostic::implicitly_closed(
                                    marker.as_str(),
                                    node.span.clone(),
                                    &closer,
                                )
                                .with_anchor_cst(id.index()),
                            );
                        } else {
                            self.push_diagnostic(|| 
                                Diagnostic::unclosed_at_eof(marker.as_str(), node.span.clone())
                                    .with_anchor_cst(id.index()),
                            );
                        }
                    } else if let Some(next_sib) = node.next_sibling {
                        let next = self.doc.node(next_sib);
                        if let CstKind::ClosingMarkerToken { normalized, .. } = &next.kind
                            && normalized != marker
                        {
                            let parent = self.doc.node(parent_id);
                            let is_parent_note_closer = match &parent.kind {
                                CstKind::Note { marker: p_marker } => {
                                    normalized.as_str().trim_start_matches('+')
                                        == p_marker.as_str().trim_start_matches('+')
                                }
                                _ => false,
                            };

                            if !is_parent_note_closer {
                                self.push_diagnostic(|| 
                                    Diagnostic::misnested_close(
                                        marker.as_str(),
                                        normalized.as_str(),
                                        next.span.clone(),
                                    )
                                    .with_anchor_cst(id.index()),
                                );
                            }
                        }
                    } else if node.span.end >= self.doc.source().len() {
                        self.push_diagnostic(|| 
                            Diagnostic::unclosed_at_eof(marker.as_str(), node.span.clone())
                                .with_anchor_cst(id.index()),
                        );
                    }
                } else {
                    self.push_diagnostic(|| 
                        Diagnostic::unclosed_at_eof(marker.as_str(), node.span.clone())
                            .with_anchor_cst(id.index()),
                    );
                }
            }
            MarkerKind::Figure => {
                self.push_diagnostic(|| 
                    Diagnostic::unclosed_at_eof(marker.as_str(), node.span.clone())
                        .with_anchor_cst(id.index()),
                );
            }
            _ => {}
        }
    }

    /// Find what marker implicitly closed a node.
    fn find_implicit_closer(&self, node: &CstNode) -> String {
        let mut sib = node.next_sibling;
        while let Some(sib_id) = sib {
            let s = self.doc.node(sib_id);
            match &s.kind {
                CstKind::Para { marker }
                | CstKind::Book { marker }
                | CstKind::Sidebar { marker }
                | CstKind::Periph { marker } => {
                    return marker.as_str().to_string();
                }
                CstKind::Chapter { .. } => return "c".to_string(),
                CstKind::Table => return "tr".to_string(),
                CstKind::TableRow { .. } => return "tr".to_string(),
                _ => {
                    sib = s.next_sibling;
                }
            }
        }
        if let Some(parent_id) = node.parent {
            let parent = self.doc.node(parent_id);
            if let Some(psib) = parent.next_sibling {
                let ps = self.doc.node(psib);
                if let CstKind::Para { marker } = &ps.kind {
                    return marker.as_str().to_string();
                }
                if let CstKind::Chapter { .. } = &ps.kind {
                    return "c".to_string();
                }
            }
        }
        "EOF".to_string()
    }

    // -----------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------

    /// Check if a CST node has a MilestoneEndToken child.
    fn has_milestone_end_child(&self, node: &CstNode) -> bool {
        let mut child_id = node.first_child;
        while let Some(id) = child_id {
            let child = self.doc.node(id);
            if matches!(child.kind, CstKind::MilestoneEndToken) {
                return true;
            }
            child_id = child.next_sibling;
        }
        false
    }

    /// Check if a CST node has no content children (only markers, whitespace, attributes).
    fn has_no_content_children(&self, node: &CstNode) -> bool {
        let mut child_id = node.first_child;
        while let Some(id) = child_id {
            let child = self.doc.node(id);
            if !matches!(
                child.kind,
                CstKind::MarkerToken { .. }
                    | CstKind::WhitespaceToken
                    | CstKind::NewlineToken
                    | CstKind::AttributesToken
                    | CstKind::MilestoneEndToken
            ) {
                return false;
            }
            child_id = child.next_sibling;
        }
        true
    }

    /// Find the span of the first MarkerToken child.
    fn find_marker_span(&self, node: &CstNode) -> Span {
        let mut child_id = node.first_child;
        while let Some(id) = child_id {
            let child = self.doc.node(id);
            if matches!(child.kind, CstKind::MarkerToken { .. }) {
                return child.span.clone();
            }
            child_id = child.next_sibling;
        }
        node.span.clone()
    }

    /// Find the first TextToken child and return its text + span.
    fn find_text_child(&self, node: &CstNode) -> (String, Option<Span>) {
        let mut child_id = node.first_child;
        while let Some(id) = child_id {
            let child = self.doc.node(id);
            if matches!(child.kind, CstKind::TextToken) {
                let text = self.doc.source_text(id).to_string();
                return (text, Some(child.span.clone()));
            }
            child_id = child.next_sibling;
        }
        (String::new(), None)
    }

    /// Check if a Char node was opened with \+ prefix.
    fn is_nested_marker(&self, node: &CstNode) -> bool {
        let mut child_id = node.first_child;
        while let Some(id) = child_id {
            let child = self.doc.node(id);
            if let CstKind::MarkerToken { token_kind, .. } = &child.kind {
                return *token_kind == MarkerTokenKind::Nested;
            }
            child_id = child.next_sibling;
        }
        false
    }

    /// Append text to a children vec, merging with previous Text node.
    fn append_text_to(children: &mut Vec<SpannedNode<'a>>, text: &str) {
        if let Some(SpannedNode {
            node: Node::Text(prev),
            ..
        }) = children.last_mut()
        {
            match prev {
                Cow::Borrowed(s) => {
                    let mut owned = s.to_string();
                    owned.push_str(text);
                    *prev = Cow::Owned(owned);
                }
                Cow::Owned(s) => s.push_str(text),
            }
        } else {
            children.push(SpannedNode::text(Cow::Owned(text.to_string())));
        }
    }

    /// Normalize and append text (handle ~, \r, //, space collapse).
    fn append_normalized_text(&mut self, children: &mut Vec<SpannedNode<'a>>, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut prev_space = false;
        let mut prev_slash = false;
        let mut needs_normalization = false;
        for ch in text.chars() {
            if ch == '\r' || ch == '~' || (ch == ' ' && prev_space) || (ch == '/' && prev_slash) {
                needs_normalization = true;
                break;
            }
            prev_space = ch == ' ';
            prev_slash = ch == '/';
        }

        if !needs_normalization {
            Self::append_text_to(children, text);
            return;
        }

        let mut scratch = std::mem::take(&mut self.text_scratch);
        scratch.clear();
        let mut chars = text.chars().peekable();
        let mut prev_space = false;
        while let Some(ch) = chars.next() {
            match ch {
                '\r' => {}
                '~' => {
                    scratch.push('\u{00a0}');
                    prev_space = false;
                }
                ' ' => {
                    if !prev_space {
                        scratch.push(' ');
                    }
                    prev_space = true;
                }
                '/' if chars.peek() == Some(&'/') => {
                    if !scratch.is_empty() {
                        Self::append_text_to(children, &scratch);
                        scratch.clear();
                    }
                    chars.next();
                    children.push(SpannedNode::optbreak());
                    prev_space = false;
                }
                _ => {
                    scratch.push(ch);
                    prev_space = false;
                }
            }
        }

        if !scratch.is_empty() {
            Self::append_text_to(children, &scratch);
        }
        self.text_scratch = scratch;
    }

    // -----------------------------------------------------------------
    // Alt/pub number setters
    // -----------------------------------------------------------------

    fn set_chapter_altnumber(children: &mut Vec<SpannedNode<'a>>, value: Cow<'a, str>) {
        for node in children.iter_mut().rev() {
            if let Node::Chapter(data) = &mut node.node {
                data.altnumber = Some(value);
                return;
            }
        }
    }

    fn set_chapter_pubnumber(children: &mut Vec<SpannedNode<'a>>, value: Cow<'a, str>) {
        for node in children.iter_mut().rev() {
            if let Node::Chapter(data) = &mut node.node {
                data.pubnumber = Some(value);
                return;
            }
        }
    }

    fn set_verse_altnumber(children: &mut Vec<SpannedNode<'a>>, value: Cow<'a, str>) {
        for node in children.iter_mut().rev() {
            if let Node::Verse(data) = &mut node.node {
                data.altnumber = Some(value);
                return;
            }
        }
    }

    fn set_verse_pubnumber(children: &mut Vec<SpannedNode<'a>>, value: Cow<'a, str>) {
        for node in children.iter_mut().rev() {
            if let Node::Verse(data) = &mut node.node {
                data.pubnumber = Some(value);
                return;
            }
        }
    }
}

/// Iterator over children of a CST node.
struct ChildIter<'a> {
    doc: &'a CstDocument,
    next: Option<CstNodeId>,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = (CstNodeId, &'a CstNode);

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        let node = self.doc.node(id);
        self.next = node.next_sibling;
        Some((id, node))
    }
}

fn split_nodes<'a>(nodes: Vec<SpannedNode<'a>>) -> (Vec<Node<'a>>, Vec<SourceNode>) {
    nodes
        .into_iter()
        .map(|node| (node.node, node.source))
        .unzip()
}

fn split_document<'a>(nodes: Vec<SpannedNode<'a>>) -> (Document<'a>, SourceMap) {
    let (content, source_content) = split_nodes(nodes);
    (
        Document { content },
        SourceMap {
            content: source_content,
        },
    )
}

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

/// Parse an attribute string like `|key="value" key2="value2"` or `|default`
/// into a vector of [`Attribute`]s.
///
/// Leading `|` is stripped. A bare value without `key=` is stored with key
/// `"default"`.
///
/// Returns `None` when the attribute string is malformed (e.g. unquoted
/// values in `key=value` pairs). The caller should treat the raw string
/// as text content in that case.
pub fn parse_attributes<'a>(attr_str: &'a str) -> Option<Vec<Attribute<'a>>> {
    let s = attr_str.strip_prefix('|').unwrap_or(attr_str);

    if s.is_empty() {
        return Some(Vec::new());
    }

    // No '=' in the string → bare default value.
    // Preserve whitespace (e.g. "| " → default value " ").
    if !s.contains('=') {
        return Some(vec![Attribute {
            key: Cow::Borrowed("default"),
            value: Cow::Owned(s.to_string()),
        }]);
    }

    // Has '=' → parse key="value" pairs.  All values must be quoted.
    let mut attrs = Vec::new();
    let mut remaining = s.trim_start();

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        if let Some(eq_pos) = remaining.find('=') {
            // Check that the part before '=' looks like a key (no spaces).
            let before_eq = &remaining[..eq_pos];
            if !before_eq.contains(' ') && !before_eq.contains('"') {
                let key = before_eq.trim().to_string();
                remaining = &remaining[eq_pos + 1..];

                // Value must be quoted.
                if remaining.starts_with('"') {
                    // Quoted value: find the closing (unescaped) quote.
                    remaining = &remaining[1..];
                    if let Some(end_quote) = find_unescaped_quote(remaining) {
                        let raw_value = &remaining[..end_quote];
                        let value = if raw_value.contains("\\\"") {
                            Cow::Owned(raw_value.replace("\\\"", "\""))
                        } else {
                            Cow::Owned(raw_value.to_string())
                        };
                        attrs.push(Attribute {
                            key: Cow::Owned(key),
                            value,
                        });
                        remaining = &remaining[end_quote + 1..];
                    } else {
                        // No closing quote -- take the rest.
                        let value = if remaining.contains("\\\"") {
                            Cow::Owned(remaining.replace("\\\"", "\""))
                        } else {
                            Cow::Owned(remaining.to_string())
                        };
                        attrs.push(Attribute {
                            key: Cow::Owned(key),
                            value,
                        });
                        break;
                    }
                } else {
                    // Unquoted value — malformed attributes.
                    return None;
                }
                continue;
            }
        }

        // Can't match as key=value (spaces before '=') — malformed.
        return None;
    }

    Some(attrs)
}

/// Find the first unescaped double-quote in a string.
/// A quote preceded by `\` is considered escaped and skipped.
fn find_unescaped_quote(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    (0..bytes.len()).find(|&i| bytes[i] == b'"' && (i == 0 || bytes[i - 1] != b'\\'))
}

/// Replace any `"default"` attribute keys with the marker-specific default
/// attribute name (e.g. `"lemma"` for `\w`, `"gloss"` for `\rb`).
/// Also applies marker-specific key renaming (e.g. `src` → `file` for `\fig`).
fn resolve_default_attr_keys<'a>(marker: &str, attrs: Vec<Attribute<'a>>) -> Vec<Attribute<'a>> {
    let default_key = markers::default_attribute(marker);
    attrs
        .into_iter()
        .map(|mut a| {
            // Resolve bare "default" key to marker-specific name.
            if a.key == "default"
                && let Some(key_name) = default_key
            {
                a.key = Cow::Borrowed(key_name);
                return a;
            }
            // Rename \fig's "src" attribute to "file" per USJ spec.
            if marker == "fig" && a.key == "src" {
                a.key = Cow::Borrowed("file");
                return a;
            }
            a
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Extract a `\cat` child from a list of nodes, returning (category, remaining children).
///
/// Trim trailing whitespace from the last text child of a node list.
/// This removes spurious trailing spaces produced by newline-to-space
/// conversion at block boundaries.
fn trim_trailing_text(children: &mut Vec<SpannedNode>) {
    if let Some(SpannedNode {
        node: Node::Text(s),
        ..
    }) = children.last_mut()
    {
        let trimmed_len = s.trim_end().len();
        if trimmed_len == 0 {
            children.pop();
        } else if trimmed_len != s.len() {
            match s {
                Cow::Borrowed(b) => *s = Cow::Owned(b[..trimmed_len].to_string()),
                Cow::Owned(owned) => {
                    owned.truncate(trimmed_len);
                }
            }
        }
    }
}

fn trim_trailing_text_on_children(node: &mut SpannedNode) {
    match (&mut node.node, &mut node.source.children) {
        (Node::TableCell { content, .. }, source_children)
        | (Node::Para { content, .. }, source_children)
        | (Node::Note { content, .. }, source_children)
        | (Node::Figure { content, .. }, source_children)
        | (Node::Sidebar { content, .. }, source_children)
        | (Node::Periph { content, .. }, source_children)
        | (Node::Table { content, .. }, source_children)
        | (Node::TableRow { content, .. }, source_children)
        | (Node::Book { content, .. }, source_children)
        | (Node::Ref { content, .. }, source_children)
        | (Node::Unknown { content, .. }, source_children) => {
            if let Some(Node::Text(s)) = content.last_mut() {
                let trimmed_len = s.trim_end().len();
                if trimmed_len == 0 {
                    content.pop();
                    source_children.pop();
                } else if trimmed_len != s.len() {
                    match s {
                        Cow::Borrowed(b) => *s = Cow::Owned(b[..trimmed_len].to_string()),
                        Cow::Owned(owned) => {
                            owned.truncate(trimmed_len);
                        }
                    }
                }
            }
        }
        (Node::Char(data), source_children) => {
            if let Some(Node::Text(s)) = data.content.last_mut() {
                let trimmed_len = s.trim_end().len();
                if trimmed_len == 0 {
                    data.content.pop();
                    source_children.pop();
                } else if trimmed_len != s.len() {
                    match s {
                        Cow::Borrowed(b) => *s = Cow::Owned(b[..trimmed_len].to_string()),
                        Cow::Owned(owned) => {
                            owned.truncate(trimmed_len);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// If a `\cat` node is found among the children (possibly wrapped in a `Para`),
/// its text content is returned as the category. The `\cat` node (and any
/// surrounding whitespace-only text) is removed from the children list.
fn extract_category<'a>(
    mut children: Vec<SpannedNode<'a>>,
) -> (Option<Cow<'a, str>>, Vec<SpannedNode<'a>>) {
    let cat_idx = children.iter().position(|n| match &n.node {
        Node::Char(data) => data.marker == "cat",
        Node::Para { marker, .. } => marker == "cat",
        _ => false,
    });
    if let Some(idx) = cat_idx {
        let cat_node = children.remove(idx);
        let text = extract_node_text(&cat_node).map(Cow::Owned);
        // Also remove a preceding whitespace-only text node if present.
        if idx > 0
            && let Some(Node::Text(t)) = children.get(idx - 1).map(|node| &node.node)
            && t.trim().is_empty()
        {
            children.remove(idx - 1);
        }
        (text, children)
    } else {
        (None, children)
    }
}

fn extract_plain_text_nodes(content: &[Node]) -> Option<String> {
    let mut text = String::new();
    for node in content {
        match node {
            Node::Text(s) => text.push_str(s),
            _ => return None,
        }
    }
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn extract_node_text(node: &SpannedNode) -> Option<String> {
    match &node.node {
        Node::Text(text) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        other => extract_plain_text_nodes(other.children()),
    }
}

/// Split a string into the first whitespace-delimited word and the remainder.
fn split_first_word(s: &str) -> (&str, &str) {
    let trimmed = s.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(pos) => {
            let word = &trimmed[..pos];
            let rest = trimmed[pos..].trim_start();
            (word, rest)
        }
        None => (trimmed, ""),
    }
}

/// Strip leading zeros from a numeric string for SID generation.
/// Ranges like "03-04" and non-numeric strings like "1a" are preserved as-is.
fn strip_leading_zeros(s: &str) -> &str {
    let trimmed = s.trim_start_matches('0');
    if trimmed.is_empty() { "0" } else { trimmed }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostics<'b, 'a>(result: &'b AstDocument<'a>) -> &'b [crate::diagnostics::Diagnostic] {
        result
            .diagnostics
            .as_deref()
            .expect("builder::parse() should collect diagnostics in tests")
    }

    #[test]
    fn test_simple_document() {
        let result = parse("\\id GEN Genesis\n\\c 1\n\\p\n\\v 1 In the beginning");
        assert!(!result.ast.content.is_empty());
        // Should have Book, Chapter, Para nodes
        match &result.ast.content[0] {
            Node::Book { code, .. } => assert_eq!(code, "GEN"),
            other => panic!("expected Book, got {:?}", other),
        }
    }

    #[test]
    fn test_character_markers() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 The \\nd Lord\\nd* spoke");
        // Find the Para node and check it has Char child
        let has_char = result.ast.content.iter().any(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().any(|c| match c {
                    Node::Char(data) => data.marker == "nd",
                    _ => false,
                })
            } else {
                false
            }
        });
        assert!(has_char);
    }

    #[test]
    fn test_footnote() {
        let result =
            parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\f + \\fr 1.1 \\ft note text\\f* more");
        // Should have a Note node inside the Para
        let has_note = result.ast.content.iter().any(|n| {
            if let Node::Para { content, .. } = n {
                content
                    .iter()
                    .any(|c| matches!(c, Node::Note { marker, .. } if marker == "f"))
            } else {
                false
            }
        });
        assert!(has_note);
    }

    #[test]
    fn test_implicit_paragraph_close() {
        let result = parse("\\id GEN\n\\c 1\n\\p First para\n\\p Second para");
        // Should have two Para nodes (first implicitly closed by second)
        let para_count = result
            .ast
            .content
            .iter()
            .filter(|n| matches!(n, Node::Para { .. }))
            .count();
        assert_eq!(para_count, 2);
    }

    #[test]
    fn test_stray_close_marker() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\nd* stray");
        assert!(
            diagnostics(&result)
                .iter()
                .any(|diagnostic| diagnostic.severity == crate::diagnostics::Severity::Error)
        );
    }

    #[test]
    fn test_unclosed_at_eof() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\nd Lord");
        let has_unclosed_nd = diagnostics(&result).iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnclosedAtEof
                && d.message.contains("\\nd")
        });
        assert!(
            has_unclosed_nd,
            "\\nd left open at EOF should produce UnclosedAtEof"
        );
    }

    #[test]
    fn test_implicit_close_when_character_crosses_paragraph_boundary() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\add text\n\\p next");
        let has_implicit_close_add = diagnostics(&result).iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::ImplicitClose
                && d.message.contains("\\add")
                && d.message.contains("\\p")
        });
        assert!(
            has_implicit_close_add,
            "\\add crossing a paragraph boundary should produce ImplicitClose"
        );
    }

    #[test]
    fn test_root_level_verse_recovers_into_implicit_paragraph() {
        let result = parse("\\id GEN\n\\c 1\n\\v 1 text\n\\v 2 more");

        let warnings: Vec<_> = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .filter(|d| d.code == crate::diagnostics::DiagnosticCode::VerseOutsideParagraph)
            .collect();
        assert_eq!(
            warnings.len(),
            1,
            "only the first offending root-level verse should warn"
        );
        assert_eq!(warnings[0].severity, crate::diagnostics::Severity::Warning);

        let para = result
            .ast
            .content
            .iter()
            .find(|n| matches!(n, Node::Para { marker, .. } if marker == "p"))
            .expect("root-level verse recovery should synthesize a \\p paragraph");

        let verses = para
            .children()
            .iter()
            .filter(|n| matches!(n, Node::Verse { .. }))
            .count();
        assert_eq!(verses, 2, "implicit paragraph should contain both verses");
    }

    #[test]
    fn test_milestone() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\qt1-s text \\qt1-e");
        let has_milestone = result.ast.content.iter().any(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().any(|c| matches!(c, Node::Milestone { .. }))
            } else {
                false
            }
        });
        assert!(has_milestone);
    }

    #[test]
    fn test_nested_character_marker() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\add text \\+nd Lord\\+nd*\\add*");
        // Should parse without nesting prefix warning
        let nesting_warnings = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .filter(|d| d.code == crate::diagnostics::DiagnosticCode::MissingNestingPrefix)
            .count();
        assert_eq!(nesting_warnings, 0);
    }

    #[test]
    fn test_empty_input() {
        let result = parse("");
        assert!(result.ast.content.is_empty());
    }

    #[test]
    fn test_poetry_paragraphs() {
        let result = parse("\\id GEN\n\\c 1\n\\q1\n\\v 1 Line one\n\\q2 Line two");
        let para_count = result
            .ast
            .content
            .iter()
            .filter(|n| matches!(n, Node::Para { .. }))
            .count();
        assert_eq!(para_count, 2);
    }

    // -- Additional unit tests -----------------------------------------------

    #[test]
    fn test_parse_attributes_key_value() {
        let attrs = parse_attributes(r#"|lemma="grace" strong="H1234""#).unwrap();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].key, "lemma");
        assert_eq!(attrs[0].value, "grace");
        assert_eq!(attrs[1].key, "strong");
        assert_eq!(attrs[1].value, "H1234");
    }

    #[test]
    fn test_parse_attributes_default() {
        let attrs = parse_attributes("|grace").unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].key, "default");
        assert_eq!(attrs[0].value, "grace");
    }

    #[test]
    fn test_parse_attributes_empty() {
        let attrs = parse_attributes("|").unwrap();
        assert!(attrs.is_empty());
    }

    #[test]
    fn test_parse_attributes_escaped_quotes() {
        let attrs = parse_attributes(r#"|alt="He said: \"hello\"" src="img.jpg""#).unwrap();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].key, "alt");
        assert_eq!(attrs[0].value, r#"He said: "hello""#);
        assert_eq!(attrs[1].key, "src");
        assert_eq!(attrs[1].value, "img.jpg");
    }

    #[test]
    fn test_parse_attributes_malformed_unquoted() {
        // Unquoted values → malformed
        assert!(parse_attributes("|lemma=grace strong=\"H1234\"").is_none());
    }

    #[test]
    fn test_parse_attributes_bare_whitespace() {
        // "| " → bare default value is a space
        let attrs = parse_attributes("| ").unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].key, "default");
        assert_eq!(attrs[0].value, " ");
    }

    #[test]
    fn test_split_first_word() {
        assert_eq!(split_first_word("hello world"), ("hello", "world"));
        assert_eq!(split_first_word("only"), ("only", ""));
        assert_eq!(split_first_word("  spaced  out  "), ("spaced", "out  "));
    }

    #[test]
    fn test_chapter_sid_generation() {
        let result = parse("\\id GEN\n\\c 3\n\\p\n\\v 5 text");
        // Find the Chapter node and verify sid.
        let chapter = result.ast.content.iter().find_map(|n| {
            if let Node::Chapter(data) = n {
                Some((data.number.clone(), data.sid.clone()))
            } else {
                None
            }
        });
        assert_eq!(chapter, Some(("3".into(), Some("GEN 3".into()))));
    }

    #[test]
    fn test_verse_sid_generation() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 2 text");
        let verse = result.ast.content.iter().find_map(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().find_map(|c| {
                    if let Node::Verse(data) = c {
                        Some((data.number.clone(), data.sid.clone()))
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        assert_eq!(verse, Some(("2".into(), Some("GEN 1:2".into()))));
    }

    #[test]
    fn test_book_code_extraction() {
        let result = parse("\\id MAT Gospel of Matthew");
        match &result.ast.content[0] {
            Node::Book { code, content, .. } => {
                assert_eq!(code, "MAT");
                // The remainder "Gospel of Matthew" should be in content.
                assert!(!content.is_empty());
                match &content[0] {
                    Node::Text(s) => assert_eq!(s, "Gospel of Matthew"),
                    other => panic!("expected Text, got {:?}", other),
                }
            }
            other => panic!("expected Book, got {:?}", other),
        }
    }

    #[test]
    fn test_cst_lowering_handles_basic_document() {
        let input = "\\id GEN Genesis\n\\c 1\n\\p  \\v 1  In the beginning";
        let cst = cst::get_cst(input);
        let lowered = parse_from_cst(&cst).ast;
        let parsed = parse(input).ast;
        assert_eq!(lowered, parsed);
    }

    #[test]
    fn test_cst_lowering_handles_sidebar_document() {
        let input = "\\id MAT\n\\c 1\n\\esb \\cat People\\cat*\n\\ms \\jmp |link-href=\"article-john_the_baptist\"\\jmp* John the Baptist\n\\p John announced the coming king.\n\\esbe";
        let cst = cst::get_cst(input);
        let lowered = parse_from_cst(&cst).ast;
        let parsed = parse(input).ast;
        assert_eq!(lowered, parsed);
    }

    #[test]
    fn test_note_caller_extraction() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\f + footnote text\\f*");
        let note = result.ast.content.iter().find_map(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().find_map(|c| {
                    if let Node::Note {
                        caller, content, ..
                    } = c
                    {
                        Some((caller.clone(), content.clone()))
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        assert!(note.is_some());
        let (caller, _content) = note.unwrap();
        assert_eq!(caller, "+");
    }

    #[test]
    fn test_unclosed_note_at_paragraph_boundary() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\f + note\n\\p next");
        // The unclosed \f should produce a diagnostic.
        let has_unclosed_note = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnclosedNote);
        assert!(has_unclosed_note);
    }

    #[test]
    fn test_header_markers_become_para() {
        let result = parse("\\id GEN\n\\h Genesis");
        // \h should become a Para node.
        let has_para = result
            .ast
            .content
            .iter()
            .any(|n| matches!(n, Node::Para { marker, .. } if marker == "h"));
        assert!(has_para);
    }

    #[test]
    fn test_unknown_marker_diagnostic() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\notreal text\\notreal*");
        let has_unknown = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(has_unknown);
    }

    #[test]
    fn test_z_prefix_no_diagnostic() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text\\zcustom*");
        let has_unknown = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(
            !has_unknown,
            "\\z-prefix markers should not produce UnknownMarker diagnostics"
        );
    }

    #[test]
    fn test_z_prefix_implicit_close_no_diagnostic() {
        // \zcustom implicitly closed by paragraph should not produce diagnostics.
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text\n\\p next para");
        let implicit_close_on_z = diagnostics(&result).iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::ImplicitClose
                && d.message.contains("zcustom")
        });
        assert!(
            !implicit_close_on_z,
            "\\z-prefix markers should not produce ImplicitClose diagnostics"
        );
    }

    #[test]
    fn test_z_prefix_unclosed_eof_no_diagnostic() {
        // \zcustom left open at EOF should not produce diagnostics.
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text");
        let unclosed_eof_on_z = diagnostics(&result).iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnclosedAtEof
                && d.message.contains("zcustom")
        });
        assert!(
            !unclosed_eof_on_z,
            "\\z-prefix markers should not produce UnclosedAtEof diagnostics"
        );
    }

    #[test]
    fn test_non_z_unknown_still_gets_diagnostics() {
        // Non-z unknown markers should still produce diagnostics.
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\notreal text");
        let has_unknown = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(
            has_unknown,
            "Non-z unknown markers should still produce UnknownMarker diagnostics"
        );
        let has_eof = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnclosedAtEof);
        assert!(
            has_eof,
            "Non-z unknown markers should still produce UnclosedAtEof diagnostics"
        );
    }

    #[test]
    fn test_unprefixed_nested_character_marker_no_warning() {
        let result = parse(
            "\\id GEN\n\\c 1\n\\p\n\\v 1 That is why \\bk The Book of the \\nd Lord\\nd*'s Battles\\bk* speaks",
        );
        let nesting_warnings = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .filter(|d| d.code == crate::diagnostics::DiagnosticCode::MissingNestingPrefix)
            .count();
        assert_eq!(nesting_warnings, 0);
    }

    #[test]
    fn test_unprefixed_nested_marker_still_requires_proper_closing_order() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\add text \\nd Lord\\add*");
        let has_misnested = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::MisnestedMarker);
        assert!(
            has_misnested,
            "closing parent before nested child should produce MisnestedMarker"
        );
    }

    #[test]
    fn test_sidebar() {
        let result = parse("\\id GEN\n\\c 1\n\\esb\n\\p Sidebar content\n\\esbe");
        let has_sidebar = result
            .ast
            .content
            .iter()
            .any(|n| matches!(n, Node::Sidebar { .. }));
        assert!(has_sidebar);
    }

    #[test]
    fn test_multiple_chapters() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\n\\c 2\n\\p\n\\v 1 more text");
        let chapter_count = result
            .ast
            .content
            .iter()
            .filter(|n| matches!(n, Node::Chapter(_)))
            .count();
        assert_eq!(chapter_count, 2);
    }

    #[test]
    fn test_verse_range() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 3-4 combined text");
        let verse = result.ast.content.iter().find_map(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().find_map(|c| {
                    if let Node::Verse(data) = c {
                        Some(data.number.clone())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        assert_eq!(verse, Some("3-4".into()));
    }

    #[test]
    fn test_fm_marker_parses_as_character() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 17 text\\fm GEN 2:9\\fm* more text");

        let has_unknown = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(!has_unknown, "\\fm should not produce UnknownMarker");

        let has_fm_char = result.ast.content.iter().any(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().any(|c| match c {
                    Node::Char(data) => data.marker == "fm",
                    _ => false,
                })
            } else {
                false
            }
        });
        assert!(has_fm_char, "\\fm should parse as a character node");
    }

    #[test]
    fn test_addpn_marker_parses_as_character_with_deprecation_warning() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text \\addpn Added Name\\addpn* more");

        let has_unknown = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(!has_unknown, "\\addpn should not produce UnknownMarker");

        let deprecation_warnings: Vec<_> = result
            .diagnostics
            .as_deref()
            .expect("diagnostics should be available")
            .iter()
            .filter(|d| d.code == crate::diagnostics::DiagnosticCode::DeprecatedMarker)
            .collect();
        assert_eq!(
            deprecation_warnings.len(),
            1,
            "\\addpn should produce exactly one deprecation warning"
        );
        assert!(deprecation_warnings[0].message.contains("\\addpn"));

        let has_addpn_char = result.ast.content.iter().any(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().any(|c| match c {
                    Node::Char(data) => data.marker == "addpn",
                    _ => false,
                })
            } else {
                false
            }
        });
        assert!(has_addpn_char, "\\addpn should parse as a character node");
    }

    #[test]
    fn test_verse_bridge_parsed_correctly() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 12-83 text");
        let verse = result.ast.content.iter().find_map(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().find_map(|c| {
                    if let Node::Verse(data) = c {
                        Some(data.as_ref())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        let verse = verse.expect("should find a Verse node");
        assert_eq!(verse.number.as_ref(), "12-83");
        assert_eq!(verse.sid.as_deref(), Some("GEN 1:12-83"));
    }

    #[test]
    fn test_verse_subverse_letter_parsed_correctly() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 3b text");
        let verse = result.ast.content.iter().find_map(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().find_map(|c| {
                    if let Node::Verse(data) = c {
                        Some(data.as_ref())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        let verse = verse.expect("should find a Verse node");
        assert_eq!(verse.number.as_ref(), "3b");
        assert_eq!(verse.sid.as_deref(), Some("GEN 1:3b"));
    }
}
