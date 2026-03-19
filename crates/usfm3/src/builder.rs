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
    /// True when the marker was opened with `\+` nesting prefix.
    nested: bool,
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

    // \usfm marker — absorb the following text (version string) and discard.
    pending_usfm: bool,

    // Whether we've already seen an \id marker (first one wins).
    seen_id: bool,

    // Whitespace after an opening marker is structural (skip).
    after_open_marker: bool,

    // Deferred newline: emitted as " " when consumed, only if last child is Text.
    pending_newline: bool,

    // Whitespace after va/vp/ca/cp metadata consumption is structural (skip).
    consumed_metadata: bool,

    // Set after a closing marker (\em*, \+nd*, \f*, \*) is processed.
    // Whitespace and newlines after close markers are deferred via
    // pending_close_space — emitted only when followed by text.
    after_close_marker: bool,
    pending_close_space: bool,

    // Tracks a milestone that expects a `\*` self-close.
    // Stores (marker_name, span) of the milestone awaiting `\*`.
    pending_milestone_close: Option<(String, Span)>,
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
            pending_usfm: false,
            seen_id: false,
            after_open_marker: false,
            pending_newline: false,
            consumed_metadata: false,
            after_close_marker: false,
            pending_close_space: false,
            pending_milestone_close: None,
        }
    }

    fn handle_token(&mut self, token: Token, span: Span) {
        // Clear after_open_marker for any non-Whitespace, non-Newline token.
        // Newlines right after an opening marker are structural (not content).
        if !matches!(token, Token::Whitespace(_) | Token::Newline) {
            self.after_open_marker = false;
            self.after_close_marker = false;
        }
        // Clear pending_close_space for non-WS/NL/Text tokens.  Text tokens
        // consume it in `append_text`; everything else discards it.
        if !matches!(
            token,
            Token::Whitespace(_) | Token::Newline | Token::Text(_)
        ) {
            self.pending_close_space = false;
        }
        // Check for missing milestone self-close: if we're expecting `\*`
        // after a milestone and see a non-whitespace/non-attributes/non-MilestoneEnd
        // token, emit a diagnostic.
        if !matches!(
            token,
            Token::Whitespace(_) | Token::Newline | Token::Attributes(_) | Token::MilestoneEnd
        ) {
            self.flush_pending_milestone_close();
        }
        match token {
            Token::Whitespace(_) => self.handle_whitespace(),
            Token::Chapter => self.handle_chapter(span),
            Token::Verse => self.handle_verse(span),
            Token::Milestone(m) => self.handle_milestone(m, span),
            Token::NestedMarker(m) => self.handle_nested_open(m, span),
            Token::NestedClosingMarker(m) => self.handle_nested_close(m, span),
            Token::ClosingMarker(m) => self.handle_close(m, span),
            Token::Marker(m) => self.handle_marker(m, span),
            Token::Attributes(a) => self.handle_attributes(a, span),
            Token::Text(t) => self.append_text(t),
            Token::MilestoneEnd => self.handle_milestone_end(span),
            Token::Newline => self.handle_newline(),
        }
    }

    /// If a milestone was expecting `\*` and didn't get one, emit a diagnostic.
    fn flush_pending_milestone_close(&mut self) {
        if let Some((marker, span)) = self.pending_milestone_close.take() {
            self.diagnostics
                .push(Diagnostic::missing_milestone_self_close(&marker, span));
        }
    }

    // -----------------------------------------------------------------
    // Whitespace handling
    // -----------------------------------------------------------------

    fn handle_whitespace(&mut self) {
        if self.after_open_marker {
            return;
        }
        // consumed_metadata must be checked before after_close_marker:
        // when \va*/\vp*/\ca*/\cp* is consumed as metadata, both flags are
        // set simultaneously; the whitespace is structural (skip entirely).
        if self.consumed_metadata {
            self.consumed_metadata = false;
            self.after_close_marker = false;
            return;
        }
        if self.after_close_marker {
            self.pending_close_space = true;
            return;
        }
        if self.pending_usfm {
            return;
        }
        if self.pending_chapter.is_some() || self.pending_verse.is_some() {
            return;
        }
        if self.pending_newline {
            return;
        }
        // Root-level whitespace (outside any marker) is structural, not content.
        if self.stack.is_empty() {
            return;
        }
        self.append_text_raw(" ");
    }

    // -----------------------------------------------------------------
    // Marker handling
    // -----------------------------------------------------------------

    fn handle_marker(&mut self, m: &str, span: Span) {
        let name = lexer::strip_marker_backslash(m);

        // \usfm marker — absorb the version string and discard.
        if name == "usfm" {
            self.pending_usfm = true;
            return;
        }

        // Duplicate \id — first one wins, skip subsequent ones.
        if name == "id" {
            if self.seen_id {
                self.diagnostics.push(Diagnostic::duplicate_id(span));
                return;
            }
            self.seen_id = true;
        }

        let info = markers::lookup_marker(name);

        // For table-row markers, the newline between rows is structural
        // (not a word boundary) -- just clear it.  For all other markers,
        // consume it: block-level trailing space is stripped by
        // `trim_trailing_text` during finalization; inline space acts as
        // a word boundary.
        if info.kind == MarkerKind::TableRow {
            self.pending_newline = false;
        } else {
            self.consume_pending_newline();
        }

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
                let closed_sibling = if self.in_note_context()
                    && info.valid_in_note
                    && self.is_same_note_family(name)
                    && name != "fv"
                {
                    // Note sub-markers (\fr, \ft, \fq, etc.) are siblings —
                    // close the previous sub-marker.  Only close when the
                    // incoming marker belongs to the same note family (e.g.
                    // \ft closes \fr in a \f note, but \xt nests inside \ft).
                    // \fv (footnote verse number) is an inline marker that
                    // nests inside other note sub-markers rather than closing
                    // them.
                    self.close_character_in_note(&span)
                } else {
                    // Outside note sub-marker sibling handling, incoming
                    // character markers naturally nest by stack order.
                    false
                };
                self.push_open(name.to_string(), MarkerKind::Character, span);
                if closed_sibling {
                    // The structural space after this sub-marker also serves
                    // as the word boundary between the previous sub-marker's
                    // content and this one's.  Preserve it as content rather
                    // than stripping it.
                    self.after_open_marker = false;
                }
            }

            MarkerKind::TableRow => {
                // Table rows are paragraph-level: close notes and paragraphs first.
                self.force_close_notes();
                self.close_paragraph(&span);
                // Close previous table cell and row if any.
                self.close_table_cell_in_row();
                self.close_table_row();
                self.push_open(name.to_string(), MarkerKind::TableRow, span);
            }

            MarkerKind::TableCell => {
                // Implicitly close the previous table cell (sibling, not nested).
                self.close_table_cell_in_row();
                self.push_open(name.to_string(), MarkerKind::TableCell, span);
            }

            MarkerKind::Periph => {
                // Periph acts as a section-level container (like sidebar).
                self.force_close_notes();
                self.close_paragraph(&span);
                self.push_open(name.to_string(), MarkerKind::Periph, span);
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
                if name == "cat" && self.in_note_or_sidebar_context() {
                    // \cat inside a note/sidebar: nest inside it.
                    // extract_category() will pull it out during finalization.
                    self.push_open(name.to_string(), MarkerKind::Meta, span);
                } else if name == "rem" && !self.in_note_context() && self.has_open_paragraph() {
                    // \rem inside a paragraph: nest inside it rather than
                    // closing the paragraph.  Close any open inline/meta
                    // markers above the paragraph first.
                    self.close_inline_above_paragraph();
                    self.push_open(name.to_string(), MarkerKind::Meta, span);
                } else {
                    self.force_close_notes();
                    self.close_paragraph(&span);
                    self.push_open(name.to_string(), MarkerKind::Meta, span);
                }
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
        self.consume_pending_newline();
        // Flush any pending chapter/verse that never got a number.
        self.flush_pending_chapter();
        self.flush_pending_verse();

        self.force_close_notes();
        self.close_paragraph(&span);
        self.pending_chapter = Some(span);
    }

    fn handle_verse(&mut self, span: Span) {
        self.consume_pending_newline(); // word boundary
        // Flush any prior pending verse that never got a number.
        self.flush_pending_verse();
        // Close any open Meta markers (e.g. \rem nested inside a paragraph)
        // so verse content becomes a sibling, not a child of the remark.
        self.close_open_meta();
        // If there's no open paragraph, this verse is outside of one — emit a diagnostic and open an implicit one to contain it.
        if self.stack.is_empty() {
            self.diagnostics
                .push(Diagnostic::verse_outside_paragraph(span.clone()));
            self.push_open("p".to_string(), MarkerKind::Paragraph, span.clone());
        }
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
                altnumber: None,
                pubnumber: None,
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
                altnumber: None,
                pubnumber: None,
                span,
            };
            self.append_node(node);
        }
    }

    // -----------------------------------------------------------------
    // Milestone
    // -----------------------------------------------------------------

    fn handle_milestone(&mut self, m: &str, span: Span) {
        // Flush any previous pending milestone close before starting a new one.
        self.flush_pending_milestone_close();
        self.consume_pending_newline();
        let name = lexer::strip_marker_backslash(m);
        let node = Node::Milestone {
            marker: name.to_string(),
            attributes: Vec::new(),
            span: span.clone(),
        };
        self.append_node(node);
        // Track this milestone as expecting `\*`.
        self.pending_milestone_close = Some((name.to_string(), span));
        // Skip whitespace between milestone marker and its attributes.
        self.after_open_marker = true;
    }

    /// Handle `\*` — the milestone attribute block terminator.
    ///
    /// If there is an open node on the stack with no children (i.e., a
    /// self-closing marker like `\ts\*` or `\zms\*`), pop it and convert
    /// it into a milestone node. Otherwise it's just closing an attribute
    /// block for the most recently appended milestone (handled by
    /// `handle_attributes`).
    fn handle_milestone_end(&mut self, _span: Span) {
        // The `\*` was found — milestone is properly closed.
        self.pending_milestone_close = None;
        self.consume_pending_newline();
        if let Some(top) = self.stack.last()
            && top.children.is_empty()
            && top.caller.is_none()
        {
            let open = self.stack.pop().unwrap();
            let node = Node::Milestone {
                marker: open.marker,
                attributes: open.attributes,
                span: open.span,
            };
            self.append_node(node);
        }
        self.after_close_marker = true;
    }

    // -----------------------------------------------------------------
    // Nested markers
    // -----------------------------------------------------------------

    fn handle_nested_open(&mut self, m: &str, span: Span) {
        self.consume_pending_newline(); // inline
        // Strip backslash to get "+marker", then skip the '+'.
        let with_plus = lexer::strip_marker_backslash(m);
        let name = &with_plus[1..]; // skip '+'
        self.push_open(name.to_string(), MarkerKind::Character, span);
        // Mark as nested (\+ prefix).
        self.stack.last_mut().unwrap().nested = true;
    }

    fn handle_nested_close(&mut self, m: &str, span: Span) {
        self.consume_pending_newline();
        // Strip backslash and star to get "+marker", then skip '+'.
        let with_plus = lexer::strip_closing_star(m);
        let name = &with_plus[1..]; // skip '+'
        self.close_matching_marker(name, &span);
        self.after_close_marker = true;
    }

    // -----------------------------------------------------------------
    // Closing markers
    // -----------------------------------------------------------------

    fn handle_close(&mut self, m: &str, span: Span) {
        self.consume_pending_newline();
        let name = lexer::strip_closing_star(m);
        self.close_matching_marker(name, &span);
        self.after_close_marker = true;
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

    fn handle_attributes(&mut self, a: &str, span: Span) {
        self.consume_pending_newline();
        let attrs = match parse_attributes(a) {
            Some(attrs) => attrs,
            None => {
                // Malformed attributes — emit diagnostic and treat the raw
                // string (including |) as text content.
                self.diagnostics
                    .push(Diagnostic::malformed_attributes(span));
                self.append_text_raw(a);
                return;
            }
        };

        if attrs.is_empty() {
            return;
        }

        // Try to attach to the most recently opened character/figure marker on
        // the stack, or to the most recently appended milestone in the current
        // context.

        // First, check the stack for a character, figure, or periph marker.
        for open in self.stack.iter_mut().rev() {
            if open.kind == MarkerKind::Character
                || open.kind == MarkerKind::Figure
                || open.kind == MarkerKind::Periph
            {
                // Resolve bare "default" attribute keys to marker-specific names
                // (e.g. "default" → "lemma" for \w).
                let resolved = resolve_default_attr_keys(&open.marker, attrs);
                open.attributes.extend(resolved);
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

        if let Some(last) = children.last_mut()
            && let Node::Milestone {
                marker, attributes, ..
            } = last
        {
            let resolved = resolve_default_attr_keys(marker, attrs);
            attributes.extend(resolved);
        }
    }

    // -----------------------------------------------------------------
    // Text handling
    // -----------------------------------------------------------------

    fn append_text(&mut self, text: &str) {
        // 0. \usfm marker — absorb the version text and discard.
        if self.pending_usfm {
            self.pending_usfm = false;
            return;
        }

        // Emit deferred space from a closing marker (gap restoration).
        if self.pending_close_space {
            self.pending_close_space = false;
            self.after_close_marker = false;
            self.append_text_raw(" ");
        }

        // Consume deferred newline as word boundary before appending text.
        self.consume_pending_newline();

        // 1. Pending chapter consumes the first word as the chapter number.
        if let Some(span) = self.pending_chapter.take() {
            let (number, rest) = split_first_word(text);
            let number = number.to_string();
            if number.starts_with('0') && number.len() > 1 {
                self.diagnostics
                    .push(Diagnostic::leading_zeros(&number, span.clone()));
            }
            self.current_chapter = Some(number.clone());
            let book = self.current_book_code.as_deref().unwrap_or("");
            let sid = Some(format!("{} {}", book, strip_leading_zeros(&number)));
            let node = Node::Chapter {
                marker: "c".into(),
                number,
                sid,
                altnumber: None,
                pubnumber: None,
                span,
            };
            self.append_node(node);
            if !rest.is_empty() {
                self.append_text_raw(rest);
            } else {
                self.after_open_marker = true;
            }
            return;
        }

        // 2. Pending verse consumes the first word as the verse number.
        if let Some(span) = self.pending_verse.take() {
            let (number, rest) = split_first_word(text);
            let number = number.to_string();
            if number.starts_with('0') && number.len() > 1 {
                self.diagnostics
                    .push(Diagnostic::leading_zeros(&number, span.clone()));
            }
            let book = self.current_book_code.as_deref().unwrap_or("");
            let ch = self.current_chapter.as_deref().unwrap_or("");
            let sid = Some(format!(
                "{} {}:{}",
                book,
                strip_leading_zeros(ch),
                strip_leading_zeros(&number)
            ));
            let node = Node::Verse {
                marker: "v".into(),
                number,
                sid,
                altnumber: None,
                pubnumber: None,
                span,
            };
            self.append_node(node);
            if !rest.is_empty() {
                self.append_text_raw(rest);
            } else {
                self.after_open_marker = true;
            }
            return;
        }

        // 3. If the top of stack is an \id header that hasn't received its
        //    book code yet, extract it.
        if let Some(top) = self.stack.last_mut()
            && top.kind == MarkerKind::Header
            && top.marker == "id"
            && top.children.is_empty()
        {
            let (code, rest) = split_first_word(text);
            self.current_book_code = Some(code.to_string());
            // Store the book code in a special way -- we'll use it in
            // finalize_open_node to create a Node::Book.
            // For now, record it as caller (ab)using that field.
            top.caller = Some(code.to_string());
            if !rest.is_empty() {
                let rest = rest.replace('~', "\u{00a0}");
                top.children.push(Node::text(&rest));
            }
            return;
        }

        // 3b. If the top of stack is a \periph that hasn't received its alt text
        //     yet, extract the entire text as alt.
        if let Some(top) = self.stack.last_mut()
            && top.kind == MarkerKind::Periph
            && top.caller.is_none()
        {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                top.caller = Some(trimmed.to_string());
            }
            return;
        }

        // 4. If the top of stack is a Note that hasn't received its caller yet,
        //    extract the first word as the caller.
        if let Some(top) = self.stack.last_mut()
            && top.kind == MarkerKind::Note
            && top.caller.is_none()
        {
            let trimmed = text.trim_start();
            if !trimmed.is_empty() {
                let (caller, remainder) = split_first_word(trimmed);
                top.caller = Some(caller.to_string());
                if !remainder.is_empty() {
                    let remainder = remainder.replace('~', "\u{00a0}");
                    top.children.push(Node::text(&remainder));
                }
                return;
            }
        }

        // 5. Normal text -- just append.
        self.append_text_raw(text);
    }

    /// Handle a newline token: set the `pending_newline` flag so that the
    /// next handler can decide whether to emit "\n" (paragraph boundary) or
    /// " " (word boundary) depending on context.
    fn handle_newline(&mut self) {
        if self.pending_chapter.is_some() || self.pending_verse.is_some() || self.pending_usfm {
            return;
        }
        if self.after_open_marker {
            return;
        }
        if self.consumed_metadata {
            self.consumed_metadata = false;
            self.after_close_marker = false;
            return;
        }
        if self.after_close_marker {
            // Defer as pending_close_space — will be emitted only if
            // the next real token is text.
            self.pending_close_space = true;
            return;
        }
        self.pending_newline = true;
    }

    /// Consume the deferred newline by pushing a space onto the last text
    /// child in the current context — matching old `handle_newline` behaviour.
    /// If the last child is not a text node, the newline is silently dropped
    /// (no word-boundary to insert).
    fn consume_pending_newline(&mut self) {
        if self.pending_newline {
            self.pending_newline = false;
            let children = if let Some(top) = self.stack.last_mut() {
                &mut top.children
            } else {
                &mut self.root_children
            };
            if let Some(Node::Text(prev)) = children.last_mut()
                && !prev.ends_with(' ')
                && !prev.ends_with('\u{00a0}')
            {
                prev.push(' ');
            }
        }
    }

    /// Append a text node to the current context without any special processing.
    /// Strips `\r` (from CRLF line endings) and replaces `~` with non-breaking
    /// space (U+00A0) per USFM spec. Merges with a preceding text node if one
    /// exists, so that sequences like `" "` + `"text"` become `" text"`.
    fn append_text_raw(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // Strip carriage returns (CRLF → LF normalisation).
        let clean: std::borrow::Cow<str> = if text.contains('\r') {
            text.replace('\r', "").into()
        } else {
            text.into()
        };
        if clean.is_empty() {
            return;
        }
        // Collapse runs of multiple spaces into a single space.
        let collapsed: std::borrow::Cow<str> = if clean.contains("  ") {
            let mut result = String::with_capacity(clean.len());
            let mut prev_space = false;
            for ch in clean.chars() {
                if ch == ' ' {
                    if !prev_space {
                        result.push(' ');
                    }
                    prev_space = true;
                } else {
                    prev_space = false;
                    result.push(ch);
                }
            }
            result.into()
        } else {
            clean
        };
        // Replace ~ with non-breaking space per USFM spec.
        let final_text: String = if collapsed.contains('~') {
            collapsed.replace('~', "\u{00a0}")
        } else {
            collapsed.into_owned()
        };

        // Split at `//` (optional line break) and interleave OptBreak nodes.
        if final_text.contains("//") {
            let parts: Vec<&str> = final_text.split("//").collect();
            for (i, part) in parts.iter().enumerate() {
                if !part.is_empty() {
                    self.append_text_fragment(part);
                }
                if i < parts.len() - 1 {
                    self.append_node(Node::OptBreak);
                }
            }
        } else {
            self.append_text_fragment(&final_text);
        }
    }

    /// Append a text fragment, merging with previous text node if possible.
    fn append_text_fragment(&mut self, text: &str) {
        let children = if let Some(top) = self.stack.last_mut() {
            &mut top.children
        } else {
            &mut self.root_children
        };

        if let Some(Node::Text(prev)) = children.last_mut() {
            prev.push_str(text);
        } else {
            children.push(Node::text(text));
        }
    }

    // -----------------------------------------------------------------
    // Alt/pub number helpers
    // -----------------------------------------------------------------

    fn set_last_chapter_altnumber(&mut self, value: String) {
        for node in self.root_children.iter_mut().rev() {
            if let Node::Chapter { altnumber, .. } = node {
                *altnumber = Some(value);
                return;
            }
        }
        for open in self.stack.iter_mut().rev() {
            for node in open.children.iter_mut().rev() {
                if let Node::Chapter { altnumber, .. } = node {
                    *altnumber = Some(value);
                    return;
                }
            }
        }
    }

    fn set_last_chapter_pubnumber(&mut self, value: String) {
        for node in self.root_children.iter_mut().rev() {
            if let Node::Chapter { pubnumber, .. } = node {
                *pubnumber = Some(value);
                return;
            }
        }
        for open in self.stack.iter_mut().rev() {
            for node in open.children.iter_mut().rev() {
                if let Node::Chapter { pubnumber, .. } = node {
                    *pubnumber = Some(value);
                    return;
                }
            }
        }
    }

    fn set_last_verse_altnumber(&mut self, value: String) {
        // Verse is typically inside a paragraph (stack), check there first.
        for open in self.stack.iter_mut().rev() {
            for node in open.children.iter_mut().rev() {
                if let Node::Verse { altnumber, .. } = node {
                    *altnumber = Some(value);
                    return;
                }
            }
        }
        for node in self.root_children.iter_mut().rev() {
            if let Node::Verse { altnumber, .. } = node {
                *altnumber = Some(value);
                return;
            }
        }
    }

    fn set_last_verse_pubnumber(&mut self, value: String) {
        for open in self.stack.iter_mut().rev() {
            for node in open.children.iter_mut().rev() {
                if let Node::Verse { pubnumber, .. } = node {
                    *pubnumber = Some(value);
                    return;
                }
            }
        }
        for node in self.root_children.iter_mut().rev() {
            if let Node::Verse { pubnumber, .. } = node {
                *pubnumber = Some(value);
                return;
            }
        }
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
            nested: false,
        });
        self.after_open_marker = true;
    }

    /// Append a finished node to the current parent (top of stack or root).
    ///
    /// Special handling: when appending a `TableRow` node, wrap it in a `Table`
    /// container (or append to an existing one) so consecutive rows are grouped.
    fn append_node(&mut self, node: Node) {
        // Smart finalization: when a \ca/\cp/\va/\vp node contains only
        // plain text, extract the text and set altnumber/pubnumber on the
        // nearest Chapter/Verse instead of appending the node.
        // If it contains nested markers (complex content), keep it as-is.
        {
            let maybe_marker = match &node {
                Node::Char { marker, .. } | Node::Para { marker, .. } => Some(marker.as_str()),
                _ => None,
            };
            if let Some(m) = maybe_marker
                && matches!(m, "ca" | "cp" | "va" | "vp")
                && let Some(text) = extract_plain_text(node.children())
            {
                // Remove preceding whitespace-only text node (the gap
                // after the previous closing marker, e.g. `\va*`).
                let children = if let Some(top) = self.stack.last_mut() {
                    &mut top.children
                } else {
                    &mut self.root_children
                };
                if let Some(Node::Text(t)) = children.last()
                    && t.trim().is_empty()
                {
                    children.pop();
                }
                match m {
                    "ca" => {
                        self.set_last_chapter_altnumber(text);
                        self.consumed_metadata = true;
                        return;
                    }
                    "cp" => {
                        self.set_last_chapter_pubnumber(text);
                        self.consumed_metadata = true;
                        return;
                    }
                    "va" => {
                        self.set_last_verse_altnumber(text);
                        self.consumed_metadata = true;
                        return;
                    }
                    "vp" => {
                        self.set_last_verse_pubnumber(text);
                        self.consumed_metadata = true;
                        return;
                    }
                    _ => unreachable!(),
                }
            }
        }

        let children = if let Some(top) = self.stack.last_mut() {
            &mut top.children
        } else {
            &mut self.root_children
        };

        // If the node is a TableRow, wrap/merge into a Table container.
        if matches!(&node, Node::TableRow { .. }) {
            if let Some(Node::Table { content, .. }) = children.last_mut() {
                content.push(node);
            } else {
                let span = node.span().cloned().unwrap_or(0..0);
                children.push(Node::Table {
                    content: vec![node],
                    span,
                });
            }
            return;
        }

        children.push(node);
    }

    /// Inside a note, close character markers on top of the stack until we
    /// reach the Note itself. This handles sibling note sub-markers like
    /// `\fr ... \ft ...` where `\ft` implicitly closes `\fr`.
    ///
    /// Returns `true` if at least one node was closed (i.e. there was a
    /// previous sibling sub-marker).  The caller uses this to decide
    /// whether the structural space after the *new* sub-marker should be
    /// preserved as content (word boundary) or stripped.
    fn close_character_in_note(&mut self, _trigger_span: &Span) -> bool {
        let mut closed_any = false;
        loop {
            let top_kind = self.stack.last().map(|o| o.kind);
            match top_kind {
                Some(MarkerKind::Character)
                | Some(MarkerKind::Unknown)
                | Some(MarkerKind::TableCell) => {
                    let top = self.stack.pop().unwrap();
                    let node = self.finalize_open_node(top);
                    self.append_node(node);
                    closed_any = true;
                }
                // Stop at the Note boundary (or anything else).
                _ => break,
            }
        }
        closed_any
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

    /// Close the current table row (if one is open on the stack).
    fn close_table_row(&mut self) {
        if let Some(top_kind) = self.stack.last().map(|o| o.kind)
            && top_kind == MarkerKind::TableRow
        {
            let top = self.stack.pop().unwrap();
            let node = self.finalize_open_node(top);
            self.append_node(node);
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
                Some(MarkerKind::Character)
                | Some(MarkerKind::Unknown)
                | Some(MarkerKind::Figure)
                | Some(MarkerKind::TableCell) => {
                    let top = self.stack.pop().unwrap();
                    if !top.marker.starts_with('z') {
                        self.diagnostics.push(Diagnostic::implicitly_closed(
                            &top.marker,
                            top.span.clone(),
                            trigger_span.clone(),
                        ));
                    }
                    let node = self.finalize_open_node(top);
                    self.append_node(node);
                }
                Some(MarkerKind::Paragraph)
                | Some(MarkerKind::Header)
                | Some(MarkerKind::Meta)
                | Some(MarkerKind::TableRow) => {
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
                    if !top.marker.starts_with('z') {
                        self.diagnostics.push(Diagnostic::implicitly_closed(
                            &top.marker,
                            top.span.clone(),
                            trigger_span.clone(),
                        ));
                    }
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

    /// Returns `true` if there is a Note-kind marker on the stack.
    fn in_note_context(&self) -> bool {
        self.stack.iter().rev().any(|o| o.kind == MarkerKind::Note)
    }

    /// Returns `true` if the given marker name belongs to the same note
    /// family as the innermost open note.  Footnote sub-markers start with
    /// 'f' (e.g. `\fr`, `\ft`), cross-reference sub-markers start with 'x'
    /// (e.g. `\xo`, `\xt`).  This prevents `\xt` from closing `\ft` inside
    /// a `\f` note — it should nest instead.
    fn is_same_note_family(&self, incoming_marker: &str) -> bool {
        let note_family = self
            .stack
            .iter()
            .rev()
            .find(|o| o.kind == MarkerKind::Note)
            .and_then(|o| match o.marker.as_str() {
                "f" | "fe" | "ef" => Some('f'),
                "x" | "ex" => Some('x'),
                _ => o.marker.chars().next(),
            });
        let incoming_family = incoming_marker.chars().next();
        match (note_family, incoming_family) {
            (Some(n), Some(i)) => n == i,
            _ => true, // fallback: treat as same family
        }
    }

    /// Returns `true` if there is a Note or Sidebar marker on the stack.
    fn in_note_or_sidebar_context(&self) -> bool {
        self.stack
            .iter()
            .rev()
            .any(|o| o.kind == MarkerKind::Note || o.kind == MarkerKind::SidebarStart)
    }

    /// Returns `true` if there is a Paragraph-kind marker on the stack.
    fn has_open_paragraph(&self) -> bool {
        self.stack.iter().any(|o| o.kind == MarkerKind::Paragraph)
    }

    /// Close Character, Unknown, and Meta markers on top of the stack,
    /// stopping at a Paragraph (or any other block-level) boundary.
    /// Used when `\rem` nests inside a paragraph without closing it.
    fn close_inline_above_paragraph(&mut self) {
        while let Some(MarkerKind::Character) | Some(MarkerKind::Unknown) | Some(MarkerKind::Meta) =
            self.stack.last().map(|o| o.kind)
        {
            let top = self.stack.pop().unwrap();
            let node = self.finalize_open_node(top);
            self.append_node(node);
        }
    }

    /// Close Meta markers on top of the stack (e.g. `\rem` that was
    /// nested inside a paragraph).  Stops at any non-Meta marker.
    fn close_open_meta(&mut self) {
        while matches!(self.stack.last().map(|o| o.kind), Some(MarkerKind::Meta)) {
            let top = self.stack.pop().unwrap();
            let node = self.finalize_open_node(top);
            self.append_node(node);
        }
    }

    // -----------------------------------------------------------------
    // Finalize
    // -----------------------------------------------------------------

    /// Convert an [`OpenNode`] into the appropriate [`Node`] variant.
    fn finalize_open_node(&self, open: OpenNode) -> Node {
        let mut children = open.children;
        // Only trim trailing whitespace for block-level nodes.
        // Inline elements (Character, Note, Figure) preserve trailing spaces
        // because they separate content from subsequent siblings.
        let is_block = matches!(
            open.kind,
            MarkerKind::Paragraph
                | MarkerKind::Header
                | MarkerKind::Meta
                | MarkerKind::TableRow
                | MarkerKind::SidebarStart
        );
        if is_block {
            trim_trailing_text(&mut children);
        }
        match open.kind {
            MarkerKind::Header => {
                if open.marker == "id" {
                    // Special case: \id becomes a Book node.
                    let code = open.caller.unwrap_or_default();
                    Node::Book {
                        marker: open.marker,
                        code,
                        content: children,
                        span: open.span,
                    }
                } else {
                    // Other headers (like \h, \toc1, \mt1) become Para nodes
                    // to match USJ.
                    Node::Para {
                        marker: open.marker,
                        content: children,
                        span: open.span,
                    }
                }
            }

            MarkerKind::Paragraph => Node::Para {
                marker: open.marker,
                content: children,
                span: open.span,
            },

            MarkerKind::Character => {
                let clean_marker = open.marker.strip_prefix('+').unwrap_or(&open.marker);
                if clean_marker == "ref" {
                    Node::Ref {
                        content: children,
                        attributes: open.attributes,
                        span: open.span,
                    }
                // [TODO] This seems a bit hacky. We should return to this and confirm that it is according to spec (I think .nested only exists for this)
                } else if clean_marker == "xt" && open.nested {
                    // When \+xt (nested) has a link-href attribute and no
                    // explicit \ref children, auto-wrap content in a Ref
                    // node.  Non-nested \xt keeps plain text content.
                    let has_ref_child = children.iter().any(|n| matches!(n, Node::Ref { .. }));
                    let href_value = open
                        .attributes
                        .iter()
                        .find(|a| a.key == "link-href")
                        .map(|a| a.value.clone());
                    let final_children = if let Some(ref loc) = href_value {
                        if !has_ref_child && !children.is_empty() {
                            vec![Node::Ref {
                                content: children,
                                attributes: vec![Attribute {
                                    key: "loc".to_string(),
                                    value: loc.clone(),
                                }],
                                span: open.span.clone(),
                            }]
                        } else {
                            children
                        }
                    } else {
                        children
                    };
                    Node::Char {
                        marker: open.marker,
                        content: final_children,
                        attributes: open.attributes,
                        span: open.span,
                    }
                } else {
                    Node::Char {
                        marker: open.marker,
                        content: children,
                        attributes: open.attributes,
                        span: open.span,
                    }
                }
            }

            MarkerKind::Note => {
                let caller = open.caller.unwrap_or_default();
                let (category, cat_children) = extract_category(children);
                Node::Note {
                    marker: open.marker,
                    caller,
                    category,
                    content: cat_children,
                    span: open.span,
                }
            }

            MarkerKind::Figure => Node::Figure {
                marker: open.marker,
                content: children,
                attributes: open.attributes,
                span: open.span,
            },

            MarkerKind::Periph => Node::Periph {
                alt: open.caller,
                content: children,
                attributes: open.attributes,
                span: open.span,
            },

            MarkerKind::SidebarStart => {
                let (category, cat_children) = extract_category(children);
                Node::Sidebar {
                    marker: open.marker,
                    category,
                    content: cat_children,
                    span: open.span,
                }
            }

            MarkerKind::Meta => Node::Para {
                marker: open.marker,
                content: children,
                span: open.span,
            },

            MarkerKind::TableRow => Node::TableRow {
                marker: open.marker,
                content: children,
                span: open.span,
            },

            MarkerKind::TableCell => {
                // Determine alignment from base marker name (digits and
                // column-span suffix stripped):
                //   thr*/tcr* → "end", thc*/tcc* → "center", others → "start"
                let without_span = if let Some(dash) = open.marker.rfind('-') {
                    let after = &open.marker[dash + 1..];
                    if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
                        &open.marker[..dash]
                    } else {
                        open.marker.as_str()
                    }
                } else {
                    open.marker.as_str()
                };
                let base = without_span.trim_end_matches(|c: char| c.is_ascii_digit());
                let align = if base.ends_with('r') {
                    "end".to_string()
                } else if base == "thc" || base == "tcc" {
                    "center".to_string()
                } else {
                    "start".to_string()
                };
                Node::TableCell {
                    marker: open.marker,
                    align,
                    content: children,
                    span: open.span,
                }
            }

            MarkerKind::Unknown => Node::Unknown {
                marker: open.marker,
                content: children,
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
                content: children,
                span: open.span,
            },
        }
    }

    /// Finish parsing: close everything on the stack and return the result.
    fn finish(mut self) -> ParseResult {
        self.consume_pending_newline();
        self.flush_pending_milestone_close();
        // Flush any pending chapter/verse that never got numbers.
        self.flush_pending_chapter();
        self.flush_pending_verse();

        // Close everything still on the stack.
        while let Some(open) = self.stack.pop() {
            // Notes get a specific diagnostic.
            if open.kind == MarkerKind::Note {
                self.diagnostics
                    .push(Diagnostic::unclosed_note(&open.marker, open.span.clone()));
            } else if open.kind == MarkerKind::SidebarStart
                || open.kind == MarkerKind::Figure
                || ((open.kind == MarkerKind::Character
                    || open.kind == MarkerKind::Unknown
                    || open.kind == MarkerKind::TableCell)
                    && !open.marker.starts_with('z'))
            {
                self.diagnostics
                    .push(Diagnostic::unclosed_at_eof(&open.marker, open.span.clone()));
            }
            // Paragraphs, headers, etc. are implicitly closed at EOF -- no
            // diagnostic needed for those.
            let node = self.finalize_open_node(open);
            // Append to new stack top or root, using append_node so that
            // table-row grouping (and other smart logic) still applies.
            if self.stack.is_empty() {
                self.append_node(node);
            } else if let Some(top) = self.stack.last_mut() {
                top.children.push(node);
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
///
/// Returns `None` when the attribute string is malformed (e.g. unquoted
/// values in `key=value` pairs). The caller should treat the raw string
/// as text content in that case.
pub fn parse_attributes(attr_str: &str) -> Option<Vec<Attribute>> {
    let s = attr_str.strip_prefix('|').unwrap_or(attr_str);

    if s.is_empty() {
        return Some(Vec::new());
    }

    // No '=' in the string → bare default value.
    // Preserve whitespace (e.g. "| " → default value " ").
    if !s.contains('=') {
        return Some(vec![Attribute {
            key: "default".to_string(),
            value: s.to_string(),
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
                        let value = remaining[..end_quote].replace("\\\"", "\"");
                        attrs.push(Attribute { key, value });
                        remaining = &remaining[end_quote + 1..];
                    } else {
                        // No closing quote -- take the rest.
                        let value = remaining.replace("\\\"", "\"");
                        attrs.push(Attribute { key, value });
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
fn resolve_default_attr_keys(marker: &str, attrs: Vec<Attribute>) -> Vec<Attribute> {
    let default_key = markers::default_attribute(marker);
    attrs
        .into_iter()
        .map(|a| {
            // Resolve bare "default" key to marker-specific name.
            if a.key == "default"
                && let Some(key_name) = default_key
            {
                return Attribute {
                    key: key_name.to_string(),
                    value: a.value,
                };
            }
            // Rename \fig's "src" attribute to "file" per USJ spec.
            if marker == "fig" && a.key == "src" {
                return Attribute {
                    key: "file".to_string(),
                    value: a.value,
                };
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
fn trim_trailing_text(children: &mut Vec<Node>) {
    if let Some(Node::Text(s)) = children.last_mut() {
        let trimmed = s.trim_end();
        if trimmed.is_empty() {
            children.pop();
        } else if trimmed.len() != s.len() {
            *s = trimmed.to_string();
        }
    }
}

/// If a `\cat` node is found among the children (possibly wrapped in a `Para`),
/// its text content is returned as the category. The `\cat` node (and any
/// surrounding whitespace-only text) is removed from the children list.
fn extract_category(mut children: Vec<Node>) -> (Option<String>, Vec<Node>) {
    let cat_idx = children.iter().position(|n| match n {
        Node::Char { marker, .. } | Node::Para { marker, .. } => marker == "cat",
        _ => false,
    });
    if let Some(idx) = cat_idx {
        let cat_node = children.remove(idx);
        let text = extract_plain_text(cat_node.children());
        // Also remove a preceding whitespace-only text node if present.
        if idx > 0
            && let Some(Node::Text(t)) = children.get(idx - 1)
            && t.trim().is_empty()
        {
            children.remove(idx - 1);
        }
        (text, children)
    } else {
        (None, children)
    }
}

/// If `content` is all [`Node::Text`] nodes, concatenate and return the trimmed
/// text.  Returns `None` if any non-text node is present or the result is empty.
fn extract_plain_text(content: &[Node]) -> Option<String> {
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
fn strip_leading_zeros(s: &str) -> String {
    s.parse::<u64>()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| s.to_string())
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
        let result =
            parse("\\id GEN\n\\c 1\n\\p\n\\v 1 text\\f + \\fr 1.1 \\ft note text\\f* more");
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
        let has_unclosed_nd = result.diagnostics.iter().any(|d| {
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
        let has_implicit_close_add = result.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::ImplicitClose
                && d.message.contains("\\add")
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
            .document
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
        let has_milestone = result.document.content.iter().any(|n| {
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
        let has_para = result
            .document
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
        assert!(
            !has_unknown,
            "\\z-prefix markers should not produce UnknownMarker diagnostics"
        );
    }

    #[test]
    fn test_z_prefix_implicit_close_no_diagnostic() {
        // \zcustom implicitly closed by paragraph should not produce diagnostics.
        let result = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text\n\\p next para");
        let implicit_close_on_z = result.diagnostics.iter().any(|d| {
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
        let unclosed_eof_on_z = result.diagnostics.iter().any(|d| {
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
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownMarker);
        assert!(
            has_unknown,
            "Non-z unknown markers should still produce UnknownMarker diagnostics"
        );
        let has_eof = result
            .diagnostics
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
