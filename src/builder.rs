//! Stack-based tree builder (Phase 2 parser) for USFM 3.x.
//!
//! Consumes the token stream produced by [`crate::lexer`] and produces an AST
//! ([`crate::ast::Document`]) together with a list of diagnostics.

use crate::ast::{Attribute, Document, Node, Span};
use crate::diagnostics::{Diagnostic, DiagnosticList};
use crate::lexer::{self, Token};
use crate::markers::{self, MarkerKind};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Result of parsing a USFM string.
pub struct ParseResult {
    pub document: Document,
    pub diagnostics: DiagnosticList,
}

/// Parse a USFM source string into a [`Document`] with diagnostics.
pub fn parse(input: &str) -> ParseResult {
    let tokens = lexer::tokenize(input);
    let mut builder = TreeBuilder::new();
    for (token, span) in tokens {
        builder.handle_token(token, span);
    }
    builder.finish()
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// A node that has been opened but not yet finalized.
struct OpenNode {
    marker: String,
    kind: MarkerKind,
    span: Span,
    children: Vec<Node>,
    /// For Note nodes: the caller character extracted from the first text.
    caller: Option<String>,
    /// For Figure nodes: collected attributes.
    attributes: Vec<Attribute>,
}

/// The tree-building state machine.
struct TreeBuilder {
    /// Stack of open container nodes (innermost on top).
    stack: Vec<OpenNode>,
    /// Children of the implicit root.
    root_children: Vec<Node>,
    diagnostics: DiagnosticList,

    // Book / chapter tracking for sid generation.
    current_book_code: Option<String>,
    current_chapter: Option<String>,

    // Pending milestones -- \c and \v consume the *next* text token as their
    // number argument.
    pending_chapter: Option<Span>,
    pending_verse: Option<Span>,
}

// ---------------------------------------------------------------------------
// Token dispatch
// ---------------------------------------------------------------------------

impl TreeBuilder {
    fn new() -> Self {
        TreeBuilder {
            stack: Vec::new(),
            root_children: Vec::new(),
            diagnostics: DiagnosticList::new(),
            current_book_code: None,
            current_chapter: None,
            pending_chapter: None,
            pending_verse: None,
        }
    }

    fn handle_token(&mut self, token: Token, span: Span) {
        match token {
            Token::Chapter => self.handle_chapter(span),
            Token::Verse => self.handle_verse(span),
            Token::Milestone(m) => self.handle_milestone(m, span),
            Token::NestedMarker(m) => self.handle_nested_open(m, span),
            Token::NestedClosingMarker(m) => self.handle_nested_close(m, span),
            Token::ClosingMarker(m) => self.handle_close(m, span),
            Token::Marker(m) => self.handle_marker(m, span),
            Token::Attributes(a) => self.handle_attributes(a, span),
            Token::Text(t) => self.append_text(t),
            Token::Newline => { /* ignored for tree building */ }
        }
    }

    // -----------------------------------------------------------------
    // Marker handling
    // -----------------------------------------------------------------

    fn handle_marker(&mut self, m: &str, span: Span) {
        let name = lexer::strip_marker_backslash(m);
        let info = markers::lookup_marker(name);

        match info.kind {
            MarkerKind::Header => {
                // Close character markers and paragraphs above.
                self.force_close_notes();
                self.close_paragraph(&span);
                self.push_open(name.to_string(), MarkerKind::Header, span);
            }

            MarkerKind::Paragraph => {
                self.force_close_notes();
                self.close_paragraph(&span);
                self.push_open(name.to_string(), MarkerKind::Paragraph, span);
            }

            MarkerKind::Note => {
                self.push_open(name.to_string(), MarkerKind::Note, span);
            }

            MarkerKind::Character => {
                if self.in_note_context() {
                    // Inside a note, sibling character markers (like \fr, \ft)
                    // implicitly close the previous character marker.
                    self.close_character_in_note(&span);
                } else if self.in_character_context() {
                    // Outside a note, nesting character markers without \+
                    // prefix is a warning.
                    self.diagnostics
                        .push(Diagnostic::missing_nesting_prefix(name, span.clone()));
                }
                self.push_open(name.to_string(), MarkerKind::Character, span);
            }

            MarkerKind::TableCell => {
                // Implicitly close the previous table cell (sibling, not nested).
                self.close_table_cell_in_row();
                self.push_open(name.to_string(), MarkerKind::TableCell, span);
            }

            MarkerKind::Figure => {
                self.push_open(name.to_string(), MarkerKind::Figure, span);
            }

            MarkerKind::SidebarStart => {
                self.force_close_notes();
                self.close_paragraph(&span);
                self.push_open(name.to_string(), MarkerKind::SidebarStart, span);
            }

            MarkerKind::SidebarEnd => {
                self.close_sidebar(&span);
            }

            MarkerKind::Meta => {
                self.force_close_notes();
                self.close_paragraph(&span);
                self.push_open(name.to_string(), MarkerKind::Meta, span);
            }

            MarkerKind::Unknown => {
                // Don't emit diagnostics for \z-prefix markers (USFM 3.0 custom namespace).
                if !name.starts_with('z') {
                    self.diagnostics
                        .push(Diagnostic::unknown_marker(name, span.clone()));
                }
                self.push_open(name.to_string(), MarkerKind::Unknown, span);
            }

            // Chapter and Verse are handled via Token::Chapter / Token::Verse,
            // but the marker lookup might return them for edge cases. Treat them
            // the same as their dedicated token handlers.
            MarkerKind::Chapter => self.handle_chapter(span),
            MarkerKind::Verse => self.handle_verse(span),

            MarkerKind::MilestoneStart | MarkerKind::MilestoneEnd => {
                // Should come through Token::Milestone, but handle gracefully.
                self.handle_milestone(m, span);
            }
        }
    }

    // -----------------------------------------------------------------
    // Chapter and verse
    // -----------------------------------------------------------------

    fn handle_chapter(&mut self, span: Span) {
        // Flush any pending chapter/verse that never got a number.
        self.flush_pending_chapter();
        self.flush_pending_verse();

        self.force_close_notes();
        self.close_paragraph(&span);
        self.pending_chapter = Some(span);
    }

    fn handle_verse(&mut self, span: Span) {
        // Flush any prior pending verse that never got a number.
        self.flush_pending_verse();
        self.pending_verse = Some(span);
    }

    /// If a pending chapter was set but never consumed by a text token,
    /// emit a diagnostic and create a Chapter node with an empty number.
    fn flush_pending_chapter(&mut self) {
        if let Some(span) = self.pending_chapter.take() {
            self.diagnostics
                .push(Diagnostic::missing_chapter_number(span.clone()));
            let node = Node::Chapter {
                marker: "c".into(),
                number: String::new(),
                sid: None,
                span,
            };
            self.append_node(node);
        }
    }

    /// If a pending verse was set but never consumed by a text token,
    /// emit a diagnostic and create a Verse node with an empty number.
    fn flush_pending_verse(&mut self) {
        if let Some(span) = self.pending_verse.take() {
            self.diagnostics
                .push(Diagnostic::missing_verse_number(span.clone()));
            let node = Node::Verse {
                marker: "v".into(),
                number: String::new(),
                sid: None,
                span,
            };
            self.append_node(node);
        }
    }

    // -----------------------------------------------------------------
    // Milestone
    // -----------------------------------------------------------------

    fn handle_milestone(&mut self, m: &str, span: Span) {
        let name = lexer::strip_marker_backslash(m);
        let node = Node::Milestone {
            marker: name.to_string(),
            attributes: Vec::new(),
            span,
        };
        self.append_node(node);
    }

    // -----------------------------------------------------------------
    // Nested markers
    // -----------------------------------------------------------------

    fn handle_nested_open(&mut self, m: &str, span: Span) {
        // Strip backslash to get "+marker", then skip the '+'.
        let with_plus = lexer::strip_marker_backslash(m);
        let name = &with_plus[1..]; // skip '+'
        self.push_open(name.to_string(), MarkerKind::Character, span);
    }

    fn handle_nested_close(&mut self, m: &str, span: Span) {
        // Strip backslash and star to get "+marker", then skip '+'.
        let with_plus = lexer::strip_closing_star(m);
        let name = &with_plus[1..]; // skip '+'
        self.close_matching_marker(name, &span);
    }

    // -----------------------------------------------------------------
    // Closing markers
    // -----------------------------------------------------------------

    fn handle_close(&mut self, m: &str, span: Span) {
        let name = lexer::strip_closing_star(m);
        self.close_matching_marker(name, &span);
    }

    /// Walk the stack looking for a matching opener for `name`.
    /// Close everything above the match (emitting diagnostics).
    /// If no match is found, emit a stray-close diagnostic.
    fn close_matching_marker(&mut self, name: &str, span: &Span) {
        // Is this a note-closing marker?
        let is_note_close = matches!(name, "f" | "fe" | "x" | "ef" | "ex");

        // Find the matching opener.
        let match_idx = self.stack.iter().rposition(|open| {
            if is_note_close {
                open.kind == MarkerKind::Note && open.marker == name
            } else {
                open.marker == name
            }
        });

        match match_idx {
            Some(idx) => {
                // Close everything above the match (mis-nested).
                while self.stack.len() > idx + 1 {
                    let top = self.stack.pop().unwrap();
                    // When closing a Note, child Character/Unknown markers are
                    // implicitly closed per USFM spec — not an error.
                    if !is_note_close
                        || !matches!(
                            top.kind,
                            MarkerKind::Character | MarkerKind::Unknown | MarkerKind::TableCell
                        )
                    {
                        self.diagnostics.push(Diagnostic::misnested_close(
                            &top.marker,
                            name,
                            span.clone(),
                        ));
                    }
                    let node = self.finalize_open_node(top);
                    // Append to new top or root.
                    self.append_node(node);
                }
                // Close the match itself.
                let matched = self.stack.pop().unwrap();
                let node = self.finalize_open_node(matched);
                self.append_node(node);
            }
            None => {
                self.diagnostics
                    .push(Diagnostic::stray_close(name, span.clone()));
            }
        }
    }

    // -----------------------------------------------------------------
    // Attributes
    // -----------------------------------------------------------------

    fn handle_attributes(&mut self, a: &str, _span: Span) {
        let attrs = parse_attributes(a);

        // Try to attach to the most recently opened character/figure marker on
        // the stack, or to the most recently appended milestone in the current
        // context.

        // First, check the stack for a character or figure marker.
        for open in self.stack.iter_mut().rev() {
            if open.kind == MarkerKind::Character || open.kind == MarkerKind::Figure {
                open.attributes.extend(attrs);
                return;
            }
        }

        // Otherwise, try attaching to the last milestone node in the current
        // child list (either top-of-stack children or root_children).
        let children = if let Some(top) = self.stack.last_mut() {
            &mut top.children
        } else {
            &mut self.root_children
        };

        if let Some(last) = children.last_mut() {
            if let Node::Milestone { attributes, .. } = last {
                attributes.extend(attrs);
            }
        }
    }

    // -----------------------------------------------------------------
    // Text handling
    // -----------------------------------------------------------------

    fn append_text(&mut self, text: &str) {
        // 1. Pending chapter consumes the first word as the chapter number.
        if let Some(span) = self.pending_chapter.take() {
            let (number, rest) = split_first_word(text);
            let number = number.to_string();
            self.current_chapter = Some(number.clone());
            let sid = self
                .current_book_code
                .as_ref()
                .map(|book| format!("{} {}", book, number));
            let node = Node::Chapter {
                marker: "c".into(),
                number,
                sid,
                span,
            };
            self.append_node(node);
            if !rest.is_empty() {
                self.append_text_raw(rest);
            }
            return;
        }

        // 2. Pending verse consumes the first word as the verse number.
        if let Some(span) = self.pending_verse.take() {
            let (number, rest) = split_first_word(text);
            let number = number.to_string();
            let sid = match (&self.current_book_code, &self.current_chapter) {
                (Some(book), Some(ch)) => Some(format!("{} {}:{}", book, ch, number)),
                _ => None,
            };
            let node = Node::Verse {
                marker: "v".into(),
                number,
                sid,
                span,
            };
            self.append_node(node);
            if !rest.is_empty() {
                self.append_text_raw(rest);
            }
            return;
        }

        // 3. If the top of stack is an \id header that hasn't received its
        //    book code yet, extract it.
        if let Some(top) = self.stack.last_mut() {
            if top.kind == MarkerKind::Header && top.marker == "id" && top.children.is_empty() {
                let (code, rest) = split_first_word(text);
                self.current_book_code = Some(code.to_string());
                // Store the book code in a special way -- we'll use it in
                // finalize_open_node to create a Node::Book.
                // For now, record it as caller (ab)using that field.
                top.caller = Some(code.to_string());
                if !rest.is_empty() {
                    top.children.push(Node::text(rest));
                }
                return;
            }
        }

        // 4. If the top of stack is a Note that hasn't received its caller yet,
        //    extract the first non-whitespace character as the caller.
        if let Some(top) = self.stack.last_mut() {
            if top.kind == MarkerKind::Note && top.caller.is_none() {
                let trimmed = text.trim_start();
                if !trimmed.is_empty() {
                    // The caller is the first non-whitespace character.
                    let mut chars = trimmed.chars();
                    let caller_char = chars.next().unwrap();
                    top.caller = Some(caller_char.to_string());
                    let remainder = chars.as_str().trim_start();
                    if !remainder.is_empty() {
                        top.children.push(Node::text(remainder));
                    }
                    return;
                }
            }
        }

        // 5. Normal text -- just append.
        self.append_text_raw(text);
    }

    /// Append a text node to the current context without any special processing.
    fn append_text_raw(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let node = Node::text(text);
        self.append_node(node);
    }

    // -----------------------------------------------------------------
    // Stack manipulation helpers
    // -----------------------------------------------------------------

    /// Push a new open node onto the stack.
    fn push_open(&mut self, marker: String, kind: MarkerKind, span: Span) {
        self.stack.push(OpenNode {
            marker,
            kind,
            span,
            children: Vec::new(),
            caller: None,
            attributes: Vec::new(),
        });
    }

    /// Append a finished node to the current parent (top of stack or root).
    fn append_node(&mut self, node: Node) {
        if let Some(top) = self.stack.last_mut() {
            top.children.push(node);
        } else {
            self.root_children.push(node);
        }
    }

    /// Inside a note, close character markers on top of the stack until we
    /// reach the Note itself. This handles sibling note sub-markers like
    /// `\fr ... \ft ...` where `\ft` implicitly closes `\fr`.
    fn close_character_in_note(&mut self, _trigger_span: &Span) {
        loop {
            let top_kind = self.stack.last().map(|o| o.kind);
            match top_kind {
                Some(MarkerKind::Character) | Some(MarkerKind::Unknown) | Some(MarkerKind::TableCell) => {
                    let top = self.stack.pop().unwrap();
                    let node = self.finalize_open_node(top);
                    self.append_node(node);
                }
                // Stop at the Note boundary (or anything else).
                _ => break,
            }
        }
    }

    /// Inside a table row, close the current table cell (if any) so the
    /// next cell becomes a sibling rather than a nested child.
    fn close_table_cell_in_row(&mut self) {
        loop {
            let top_kind = self.stack.last().map(|o| o.kind);
            match top_kind {
                Some(MarkerKind::TableCell) => {
                    let top = self.stack.pop().unwrap();
                    let node = self.finalize_open_node(top);
                    self.append_node(node);
                }
                _ => break,
            }
        }
    }

    /// Close all character markers on top of the stack, then close the current
    /// paragraph (or header/meta) if one exists.
    fn close_paragraph(&mut self, trigger_span: &Span) {
        // Walk the stack from the top. Close character, unknown, and figure
        // markers (they are implicitly closed). Stop when we hit a paragraph,
        // header, meta, sidebar, or note -- close the paragraph/header/meta if
        // found.
        loop {
            let top_kind = self.stack.last().map(|o| o.kind);
            match top_kind {
                Some(MarkerKind::Character) | Some(MarkerKind::Unknown) | Some(MarkerKind::Figure) | Some(MarkerKind::TableCell) => {
                    let top = self.stack.pop().unwrap();
                    self.diagnostics.push(Diagnostic::implicitly_closed(
                        &top.marker,
                        top.span.clone(),
                        trigger_span.clone(),
                    ));
                    let node = self.finalize_open_node(top);
                    self.append_node(node);
                }
                Some(MarkerKind::Paragraph) | Some(MarkerKind::Header) | Some(MarkerKind::Meta) => {
                    let top = self.stack.pop().unwrap();
                    let node = self.finalize_open_node(top);
                    self.append_node(node);
                    break;
                }
                // Don't close notes or sidebars via paragraph close.
                _ => break,
            }
        }
    }

    /// Force-close any open Note nodes, emitting `unclosed_note` diagnostics.
    fn force_close_notes(&mut self) {
        // Walk from the top of the stack. If we encounter a Note, close
        // everything above it (they are contained in the note), then close
        // the note.
        loop {
            let note_idx = self.stack.iter().rposition(|o| o.kind == MarkerKind::Note);
            match note_idx {
                Some(idx) => {
                    // Close everything above the note.
                    while self.stack.len() > idx + 1 {
                        let top = self.stack.pop().unwrap();
                        let node = self.finalize_open_node(top);
                        self.append_node(node);
                    }
                    // Close the note itself.
                    let note = self.stack.pop().unwrap();
                    self.diagnostics
                        .push(Diagnostic::unclosed_note(&note.marker, note.span.clone()));
                    let node = self.finalize_open_node(note);
                    self.append_node(node);
                }
                None => break,
            }
        }
    }

    /// Close the sidebar: walk the stack for a `SidebarStart`, close everything
    /// in between, then finalize the sidebar node.
    fn close_sidebar(&mut self, trigger_span: &Span) {
        let sidebar_idx = self
            .stack
            .iter()
            .rposition(|o| o.kind == MarkerKind::SidebarStart);
        match sidebar_idx {
            Some(idx) => {
                // Close everything above the sidebar.
                while self.stack.len() > idx + 1 {
                    let top = self.stack.pop().unwrap();
                    self.diagnostics.push(Diagnostic::implicitly_closed(
                        &top.marker,
                        top.span.clone(),
                        trigger_span.clone(),
                    ));
                    let node = self.finalize_open_node(top);
                    self.append_node(node);
                }
                // Close the sidebar.
                let sidebar = self.stack.pop().unwrap();
                let node = self.finalize_open_node(sidebar);
                self.append_node(node);
            }
            None => {
                // Stray \esbe with no matching \esb -- emit diagnostic.
                self.diagnostics
                    .push(Diagnostic::stray_close("esbe", trigger_span.clone()));
            }
        }
    }

    // -----------------------------------------------------------------
    // Context queries
    // -----------------------------------------------------------------

    /// Returns `true` if there is a Character-kind marker on the stack
    /// (not counting note sub-markers).
    fn in_character_context(&self) -> bool {
        self.stack
            .iter()
            .rev()
            .any(|o| o.kind == MarkerKind::Character || o.kind == MarkerKind::TableCell)
    }

    /// Returns `true` if there is a Note-kind marker on the stack.
    fn in_note_context(&self) -> bool {
        self.stack
            .iter()
            .rev()
            .any(|o| o.kind == MarkerKind::Note)
    }

    // -----------------------------------------------------------------
    // Finalize
    // -----------------------------------------------------------------

    /// Convert an [`OpenNode`] into the appropriate [`Node`] variant.
    fn finalize_open_node(&self, open: OpenNode) -> Node {
        match open.kind {
            MarkerKind::Header => {
                if open.marker == "id" {
                    // Special case: \id becomes a Book node.
                    let code = open.caller.unwrap_or_default();
                    Node::Book {
                        marker: open.marker,
                        code,
                        content: open.children,
                        span: open.span,
                    }
                } else {
                    // Other headers (like \h, \toc1, \mt1) become Para nodes
                    // to match USJ.
                    Node::Para {
                        marker: open.marker,
                        content: open.children,
                        span: open.span,
                    }
                }
            }

            MarkerKind::Paragraph => Node::Para {
                marker: open.marker,
                content: open.children,
                span: open.span,
            },

            MarkerKind::Character => {
                // NOTE: The Char AST node does not currently carry an attributes
                // field, so any attributes collected on this OpenNode (e.g. from
                // \w |lemma="...") are silently dropped.  A future AST revision
                // could add attribute support to Char.
                Node::Char {
                    marker: open.marker,
                    content: open.children,
                    span: open.span,
                }
            }

            MarkerKind::Note => {
                let caller = open.caller.unwrap_or_default();
                Node::Note {
                    marker: open.marker,
                    caller,
                    content: open.children,
                    span: open.span,
                }
            }

            MarkerKind::Figure => Node::Figure {
                marker: open.marker,
                content: open.children,
                attributes: open.attributes,
                span: open.span,
            },

            MarkerKind::SidebarStart => Node::Sidebar {
                marker: open.marker,
                content: open.children,
                span: open.span,
            },

            MarkerKind::Meta => Node::Para {
                marker: open.marker,
                content: open.children,
                span: open.span,
            },

            MarkerKind::TableCell => Node::Char {
                marker: open.marker,
                content: open.children,
                span: open.span,
            },

            MarkerKind::Unknown => Node::Unknown {
                marker: open.marker,
                content: open.children,
                span: open.span,
            },

            // These shouldn't normally appear as OpenNodes, but handle them
            // defensively.
            MarkerKind::SidebarEnd
            | MarkerKind::Chapter
            | MarkerKind::Verse
            | MarkerKind::MilestoneStart
            | MarkerKind::MilestoneEnd => Node::Unknown {
                marker: open.marker,
                content: open.children,
                span: open.span,
            },
        }
    }

    /// Finish parsing: close everything on the stack and return the result.
    fn finish(mut self) -> ParseResult {
        // Flush any pending chapter/verse that never got numbers.
        self.flush_pending_chapter();
        self.flush_pending_verse();

        // Close everything still on the stack.
        while let Some(open) = self.stack.pop() {
            // Notes get a specific diagnostic.
            if open.kind == MarkerKind::Note {
                self.diagnostics
                    .push(Diagnostic::unclosed_note(&open.marker, open.span.clone()));
            } else if open.kind == MarkerKind::Character || open.kind == MarkerKind::Unknown || open.kind == MarkerKind::TableCell {
                self.diagnostics
                    .push(Diagnostic::unclosed_at_eof(&open.marker, open.span.clone()));
            }
            // Paragraphs, headers, etc. are implicitly closed at EOF -- no
            // diagnostic needed for those.
            let node = self.finalize_open_node(open);
            // Append to new stack top or root.
            if let Some(top) = self.stack.last_mut() {
                top.children.push(node);
            } else {
                self.root_children.push(node);
            }
        }

        ParseResult {
            document: Document {
                content: self.root_children,
            },
            diagnostics: self.diagnostics,
        }
    }
}

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

/// Parse an attribute string like `|key="value" key2="value2"` or `|default`
/// into a vector of [`Attribute`]s.
///
/// Leading `|` is stripped. A bare value without `key=` is stored with key
/// `"default"`.
pub fn parse_attributes(attr_str: &str) -> Vec<Attribute> {
    let s = attr_str.strip_prefix('|').unwrap_or(attr_str).trim();
    if s.is_empty() {
        return Vec::new();
    }

    let mut attrs = Vec::new();
    let mut remaining = s;

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
                remaining = remaining[eq_pos + 1..].trim_start();

                // Parse the value -- may or may not be quoted.
                if remaining.starts_with('"') {
                    // Quoted value: find the closing quote.
                    remaining = &remaining[1..];
                    if let Some(end_quote) = remaining.find('"') {
                        let value = remaining[..end_quote].to_string();
                        attrs.push(Attribute { key, value });
                        remaining = &remaining[end_quote + 1..];
                    } else {
                        // No closing quote -- take the rest.
                        let value = remaining.to_string();
                        attrs.push(Attribute { key, value });
                        break;
                    }
                } else {
                    // Unquoted value: take until whitespace.
                    let end = remaining
                        .find(char::is_whitespace)
                        .unwrap_or(remaining.len());
                    let value = remaining[..end].to_string();
                    attrs.push(Attribute { key, value });
                    remaining = &remaining[end..];
                }
                continue;
            }
        }

        // No '=' or the part before '=' has spaces -- treat as a bare default value.
        // Take until whitespace.
        let end = remaining
            .find(char::is_whitespace)
            .unwrap_or(remaining.len());
        let value = remaining[..end].to_string();
        attrs.push(Attribute {
            key: "default".to_string(),
            value,
        });
        remaining = &remaining[end..];
    }

    attrs
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_document() {
        let result = parse("\\id GEN Genesis\n\\c 1\n\\p\n\\v 1 In the beginning");
        assert!(!result.document.content.is_empty());
        // Should have Book, Chapter, Para nodes
        match &result.document.content[0] {
            Node::Book { code, .. } => assert_eq!(code, "GEN"),
            other => panic!("expected Book, got {:?}", other),
        }
    }

    #[test]
    fn test_character_markers() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 The \\nd Lord\\nd* spoke");
        // Find the Para node and check it has Char child
        let has_char = result.document.content.iter().any(|n| {
            if let Node::Para { content, .. } = n {
                content
                    .iter()
                    .any(|c| matches!(c, Node::Char { marker, .. } if marker == "nd"))
            } else {
                false
            }
        });
        assert!(has_char);
    }

    #[test]
    fn test_footnote() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\f + \\fr 1.1 \\ft note text\\f* more");
        // Should have a Note node inside the Para
        let has_note = result.document.content.iter().any(|n| {
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
            .document
            .content
            .iter()
            .filter(|n| matches!(n, Node::Para { .. }))
            .count();
        assert_eq!(para_count, 2);
    }

    #[test]
    fn test_stray_close_marker() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\nd* stray");
        assert!(result.diagnostics.has_errors());
    }

    #[test]
    fn test_unclosed_at_eof() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\nd Lord");
        // \nd not closed - should have diagnostic
        assert!(!result.diagnostics.is_empty());
    }

    #[test]
    fn test_milestone() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\qt1-s text \\qt1-e");
        let has_milestone = result.document.content.iter().any(|n| {
            if let Node::Para { content, .. } = n {
                content
                    .iter()
                    .any(|c| matches!(c, Node::Milestone { .. }))
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
            .iter()
            .filter(|d| d.code == crate::diagnostics::DiagnosticCode::MissingNestingPrefix)
            .count();
        assert_eq!(nesting_warnings, 0);
    }

    #[test]
    fn test_empty_input() {
        let result = parse("");
        assert!(result.document.content.is_empty());
    }

    #[test]
    fn test_poetry_paragraphs() {
        let result = parse("\\id GEN\n\\c 1\n\\q1\n\\v 1 Line one\n\\q2 Line two");
        let para_count = result
            .document
            .content
            .iter()
            .filter(|n| matches!(n, Node::Para { .. }))
            .count();
        assert_eq!(para_count, 2);
    }

    // -- Additional unit tests -----------------------------------------------

    #[test]
    fn test_parse_attributes_key_value() {
        let attrs = parse_attributes(r#"|lemma="grace" strong="H1234""#);
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].key, "lemma");
        assert_eq!(attrs[0].value, "grace");
        assert_eq!(attrs[1].key, "strong");
        assert_eq!(attrs[1].value, "H1234");
    }

    #[test]
    fn test_parse_attributes_default() {
        let attrs = parse_attributes("|grace");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].key, "default");
        assert_eq!(attrs[0].value, "grace");
    }

    #[test]
    fn test_parse_attributes_empty() {
        let attrs = parse_attributes("|");
        assert!(attrs.is_empty());
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
        let chapter = result.document.content.iter().find_map(|n| {
            if let Node::Chapter { sid, number, .. } = n {
                Some((number.clone(), sid.clone()))
            } else {
                None
            }
        });
        assert_eq!(chapter, Some(("3".into(), Some("GEN 3".into()))));
    }

    #[test]
    fn test_verse_sid_generation() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 2 text");
        let verse = result.document.content.iter().find_map(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().find_map(|c| {
                    if let Node::Verse { sid, number, .. } = c {
                        Some((number.clone(), sid.clone()))
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
        match &result.document.content[0] {
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
    fn test_note_caller_extraction() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\f + footnote text\\f*");
        let note = result.document.content.iter().find_map(|n| {
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
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnclosedNote);
        assert!(has_unclosed_note);
    }

    #[test]
    fn test_header_markers_become_para() {
        let result = parse("\\id GEN\n\\h Genesis");
        // \h should become a Para node.
        let has_para = result.document.content.iter().any(|n| {
            matches!(n, Node::Para { marker, .. } if marker == "h")
        });
        assert!(has_para);
    }

    #[test]
    fn test_unknown_marker_diagnostic() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\notreal text\\notreal*");
        let has_unknown = result
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(has_unknown);
    }

    #[test]
    fn test_z_prefix_no_diagnostic() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text\\zcustom*");
        let has_unknown = result
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(!has_unknown, "\\z-prefix markers should not produce UnknownMarker diagnostics");
    }

    #[test]
    fn test_missing_nesting_prefix_warning() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\add text \\nd Lord\\nd*\\add*");
        // \nd inside \add without \+ prefix should emit a warning.
        let nesting_warnings = result
            .diagnostics
            .iter()
            .filter(|d| d.code == crate::diagnostics::DiagnosticCode::MissingNestingPrefix)
            .count();
        assert_eq!(nesting_warnings, 1);
    }

    #[test]
    fn test_sidebar() {
        let result = parse("\\id GEN\n\\c 1\n\\esb\n\\p Sidebar content\n\\esbe");
        let has_sidebar = result
            .document
            .content
            .iter()
            .any(|n| matches!(n, Node::Sidebar { .. }));
        assert!(has_sidebar);
    }

    #[test]
    fn test_multiple_chapters() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\n\\c 2\n\\p\n\\v 1 more text");
        let chapter_count = result
            .document
            .content
            .iter()
            .filter(|n| matches!(n, Node::Chapter { .. }))
            .count();
        assert_eq!(chapter_count, 2);
    }

    #[test]
    fn test_verse_range() {
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 3-4 combined text");
        let verse = result.document.content.iter().find_map(|n| {
            if let Node::Para { content, .. } = n {
                content.iter().find_map(|c| {
                    if let Node::Verse { number, .. } = c {
                        Some(number.clone())
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
}
