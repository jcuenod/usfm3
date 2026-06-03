//! Stage 2 — tape construction (the recovering state machine).
//!
//! Walks the source guided by stage-1 marker offsets and emits [`Node`]s onto the
//! tape, maintaining a small open-container stack so it can wire up `link` jump
//! pointers and synthesize [`NodeKind::BlockEnd`] nodes at implicit block
//! boundaries. It never bails: anything malformed becomes a diagnostic plus an
//! `Error`/`Text` node and parsing continues.
//!
//! Node-span discipline (so the tape stays lossless): every source byte lands in
//! exactly one node. Marker tokens own their `\tag`/`\tag*` bytes (and, for
//! chapter/verse, the trailing number token); everything else is `Text`.

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::marker::{Category, Closing, MarkerId, MarkerTable, TextRole};
use crate::span::Span;
use crate::tape::{flags, Node, NodeKind, Tape};

use super::scan;

pub struct BuildOutput {
    pub tape: Tape,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn build(source: &str, table: &MarkerTable, diagnostics: bool) -> BuildOutput {
    Builder::new(source, table, diagnostics).run()
}

struct Builder<'a> {
    src: &'a str,
    bytes: &'a [u8],
    table: &'a MarkerTable,
    nodes: Vec<Node>,
    diags: Vec<Diagnostic>,
    /// Whether to collect diagnostics at all (off on the hot path).
    collect_diags: bool,
    /// Indices of currently open container nodes (paragraphs, char spans, notes),
    /// innermost last.
    stack: Vec<u32>,
}

impl<'a> Builder<'a> {
    fn new(src: &'a str, table: &'a MarkerTable, collect_diags: bool) -> Self {
        let bytes = src.as_bytes();
        let mut nodes = Vec::with_capacity(scan::estimate_nodes(bytes));
        // Root at index 0; its `link` (matching close) is patched at the end.
        nodes.push(Node {
            kind: NodeKind::Root,
            flags: flags::SYNTHETIC,
            marker: MarkerId::UNKNOWN,
            start: 0,
            len: bytes.len() as u32,
            link: 0,
        });
        Self { src, bytes, table, nodes, diags: Vec::new(), collect_diags, stack: Vec::new() }
    }

    /// Record a diagnostic (no-op when diagnostics are disabled).
    #[inline]
    fn report(&mut self, make: impl FnOnce() -> Diagnostic) {
        if self.collect_diags {
            self.diags.push(make());
        }
    }

    fn run(mut self) -> BuildOutput {
        let len = self.bytes.len();
        let mut pos = 0usize;
        while pos < len {
            // Stage 1: jump to the next marker. Bytes in between are text.
            match memchr::memchr(b'\\', &self.bytes[pos..]) {
                None => {
                    self.emit_text(pos, len);
                    pos = len;
                }
                Some(rel) => {
                    let b = pos + rel;
                    // An escaped quote (`\"`) inside an attribute value is literal,
                    // not a marker. The spec is silent on this, but the reference
                    // tooling and real translations use it for quotes in captions,
                    // so capture the whole `|…` region (to the marker's real close)
                    // as one Attribute node rather than letting the `\"` fragment it.
                    // The `bytes[b+1] == '"'` guard keeps this off the common path.
                    if b + 1 < len
                        && self.bytes[b + 1] == b'"'
                        && self.top_takes_attributes()
                    {
                        if let Some(bar_rel) = memchr::memchr(b'|', &self.bytes[pos..b]) {
                            let bar = pos + bar_rel;
                            if bar > pos {
                                self.push_leaf(
                                    NodeKind::Text,
                                    Span::new(pos as u32, (bar - pos) as u32),
                                    MarkerId::UNKNOWN,
                                );
                            }
                            let end = self.scan_attr_region(bar);
                            self.push_leaf(
                                NodeKind::Attribute,
                                Span::new(bar as u32, (end - bar) as u32),
                                MarkerId::UNKNOWN,
                            );
                            if let Some(&open) = self.stack.last() {
                                self.nodes[open as usize].flags |= flags::HAS_ATTRS;
                            }
                            pos = end;
                            continue;
                        }
                    }
                    self.emit_text(pos, b);
                    pos = self.parse_marker(b);
                }
            }
        }
        // Close anything still open at EOF (closer = None → "still open at EOF").
        self.close_blocks_down_to(0, len, None);
        let last = (self.nodes.len() - 1) as u32;
        self.nodes[0].link = last;
        BuildOutput { tape: Tape { nodes: self.nodes }, diagnostics: self.diags }
    }

    // --- emission helpers --------------------------------------------------

    /// Emit text in `[from, to)`, splitting off a trailing `|attributes` segment
    /// when we are inside a marker that carries attributes (character or milestone).
    fn emit_text(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        if self.top_takes_attributes() {
            if let Some(rel) = memchr::memchr(b'|', &self.bytes[from..to]) {
                let bar = from + rel;
                self.push_leaf(NodeKind::Text, Span::new(from as u32, (bar - from) as u32), MarkerId::UNKNOWN);
                self.push_leaf(NodeKind::Attribute, Span::new(bar as u32, (to - bar) as u32), MarkerId::UNKNOWN);
                if let Some(&open) = self.stack.last() {
                    self.nodes[open as usize].flags |= flags::HAS_ATTRS;
                }
                return;
            }
        }
        self.push_leaf(NodeKind::Text, Span::new(from as u32, (to - from) as u32), MarkerId::UNKNOWN);
    }

    fn push_leaf(&mut self, kind: NodeKind, span: Span, marker: MarkerId) -> u32 {
        let idx = self.nodes.len() as u32;
        self.nodes.push(Node { kind, flags: 0, marker, start: span.start, len: span.len, link: idx });
        idx
    }

    fn push_open(&mut self, span: Span, marker: MarkerId) -> u32 {
        let idx = self.nodes.len() as u32;
        self.nodes.push(Node {
            kind: NodeKind::MarkerOpen,
            flags: 0,
            marker,
            start: span.start,
            len: span.len,
            link: idx, // patched when closed
        });
        idx
    }

    /// Emit a close for `open_idx` at byte `at`, wiring both jump pointers.
    fn emit_close(&mut self, open_idx: u32, at: usize, kind: NodeKind, synthetic: bool) {
        let close_idx = self.nodes.len() as u32;
        let marker = self.nodes[open_idx as usize].marker;
        self.nodes.push(Node {
            kind,
            flags: if synthetic { flags::SYNTHETIC } else { 0 },
            marker,
            start: at as u32,
            len: 0,
            link: open_idx,
        });
        self.nodes[open_idx as usize].link = close_idx;
    }

    // --- stack helpers -----------------------------------------------------

    /// Whether the innermost open marker takes `|key="value"` attributes — char
    /// markers (`\w …|lemma="…"\w*`), milestones (`\qt-s |who="…"\*`), and `\periph`
    /// (`\periph My Title|id="title"`).
    fn top_takes_attributes(&self) -> bool {
        self.stack
            .last()
            .and_then(|&i| self.table.info(self.nodes[i as usize].marker))
            .map(|m| matches!(m.category, Category::Character | Category::Milestone) || m.usx_element == "periph")
            .unwrap_or(false)
    }

    /// The stack length to close down to when a new ordinary block begins: just
    /// inside the innermost open block container (`\esb`, `\periph`), so its
    /// paragraphs nest within it, or `0` at the top level.
    fn container_floor(&self) -> usize {
        self.stack
            .iter()
            .rposition(|&oi| self.is_container(oi))
            .map(|p| p + 1)
            .unwrap_or(0)
    }

    /// Scan an attribute region starting at the `|`, returning the byte offset of
    /// the marker's real close. Escaped quotes (`\"`) are skipped so embedded
    /// quotes don't prematurely end the region.
    fn scan_attr_region(&self, from: usize) -> usize {
        let len = self.bytes.len();
        let mut p = from;
        loop {
            match memchr::memchr(b'\\', &self.bytes[p..]) {
                None => return len,
                Some(rel) => {
                    let m = p + rel;
                    if m + 1 < len && self.bytes[m + 1] == b'"' {
                        p = m + 2; // escaped quote — part of the value
                        continue;
                    }
                    return m; // the marker's closing token
                }
            }
        }
    }

    /// Close an open `\rem` (and anything still open inside it) — used when a verse
    /// ends a remark nested mid-paragraph, leaving the paragraph itself open.
    fn close_rem(&mut self, at: usize) {
        if let Some(pos) = self.stack.iter().rposition(|&oi| {
            self.table.info(self.nodes[oi as usize].marker).map(|m| m.usx_element) == Some("rem")
        }) {
            self.close_blocks_down_to(pos, at, Some("v"));
        }
    }

    /// Whether the open node at `idx` is a block container (sidebar / periph).
    fn is_container(&self, idx: u32) -> bool {
        matches!(
            self.table.info(self.nodes[idx as usize].marker).map(|m| m.usx_element),
            Some("sidebar") | Some("periph")
        )
    }

    /// Lookahead: is there an explicit `\<tag>*` close for this marker before the
    /// enclosing note or block boundary? Resolves dual-mode markers (`\xt`): a
    /// closed one nests; an unclosed one is a note-text sibling. Bounded by the
    /// surrounding note/paragraph, so it stays local (and cheap) on the parse path.
    fn has_close_ahead(&self, tag: &str, from: usize) -> bool {
        let len = self.bytes.len();
        let mut p = from;
        while p < len {
            let Some(rel) = memchr::memchr(b'\\', &self.bytes[p..]) else { return false };
            let m = p + rel;
            let mut k = m + 1;
            if k < len && self.bytes[k] == b'+' {
                k += 1;
            }
            let ts = k;
            while k < len && (self.bytes[k].is_ascii_alphanumeric() || self.bytes[k] == b'-') {
                k += 1;
            }
            let mtag = &self.src[ts..k];
            let is_close = k < len && self.bytes[k] == b'*';
            if is_close && mtag == tag {
                return true;
            }
            // A block marker, or the close of an enclosing note, ends the search.
            if let Some(info) = self.table.lookup(mtag).and_then(|id| self.table.info(id)) {
                let boundary = if is_close {
                    info.category == Category::Note
                } else {
                    matches!(
                        info.category,
                        Category::Paragraph | Category::Identification | Category::Chapter
                    )
                };
                if boundary {
                    return false;
                }
            }
            p = if is_close { k + 1 } else { k };
        }
        false
    }

    /// Close open containers until the stack has `target_len` entries, at byte
    /// `at`. A marker left open here is diagnosed by *why* it closed: the close was
    /// triggered either by a following marker (`closer = Some("p")`) or by reaching
    /// end of input (`closer = None`). The specific code mirrors rsusfm3's
    /// vocabulary so editors can phrase the fix precisely.
    fn close_blocks_down_to(&mut self, target_len: usize, at: usize, closer: Option<&str>) {
        while self.stack.len() > target_len {
            let open = self.stack.pop().unwrap();
            let info = self.table.info(self.nodes[open as usize].marker);
            let category = info.map(|m| m.category);
            let usx = info.map(|m| m.usx_element);
            let name = info.map(|m| m.name).unwrap_or("?");
            let span = self.nodes[open as usize].span();
            // Does an implicit close of this marker warrant a diagnostic? Notes and
            // explicit-closing characters do; sidebars do; paragraphs / note-text
            // siblings (`\fr`, `\ft`, …) are designed to end at a boundary.
            let kind = match info.map(|m| (m.usx_element, m.closing)) {
                Some(("para", _)) | Some(("book", _)) => NodeKind::BlockEnd,
                Some((_, Closing::Required)) => NodeKind::MarkerClose,
                _ => NodeKind::BlockEnd,
            };
            let diagnose = matches!(info.map(|m| m.closing), Some(Closing::Required))
                || usx == Some("sidebar")
                || category == Some(Category::Milestone);

            if diagnose {
                let make: Box<dyn FnOnce() -> Diagnostic> = if category == Some(Category::Milestone) {
                    // Every milestone must self-close with `\*` (`\zaln-s |…\*`).
                    Box::new(move || {
                        Diagnostic::error(
                            DiagnosticCode::MissingMilestoneSelfClose,
                            span,
                            format!("milestone \\{name} is missing its self-closing \\*"),
                        )
                    })
                } else if category == Some(Category::Note) {
                    Box::new(move || {
                        Diagnostic::error(
                            DiagnosticCode::UnclosedNote,
                            span,
                            format!("note \\{name} was not closed before a paragraph/chapter boundary"),
                        )
                    })
                } else if let Some(by) = closer {
                    let by = by.to_string();
                    Box::new(move || {
                        Diagnostic::error(
                            DiagnosticCode::ImplicitClose,
                            span,
                            format!("\\{name} is missing its explicit \\{name}*; it was closed implicitly by \\{by}"),
                        )
                    })
                } else {
                    Box::new(move || {
                        Diagnostic::error(
                            DiagnosticCode::UnclosedAtEof,
                            span,
                            format!("\\{name} was still open at end of input"),
                        )
                    })
                };
                self.report(make);
                self.nodes[open as usize].flags |= flags::HAS_ERROR;
            }
            self.emit_close(open, at, kind, true);
        }
    }

    // --- marker parsing ----------------------------------------------------

    /// Parse the marker starting at backslash index `b`; return the new position.
    fn parse_marker(&mut self, b: usize) -> usize {
        let len = self.bytes.len();
        let mut j = b + 1;

        // `\*` — the generic milestone/attribute terminator. Close the nearest
        // open milestone; otherwise recover as an (excluded-from-text) error node
        // without a diagnostic, since a lone `\*` is rare and noisy to flag.
        if j < len && self.bytes[j] == b'*' {
            let end = j + 1;
            if let Some(&top) = self.stack.last() {
                let is_ms = self
                    .table
                    .info(self.nodes[top as usize].marker)
                    .map(|m| m.category)
                    == Some(Category::Milestone);
                if is_ms {
                    self.stack.pop();
                    self.emit_close(top, b, NodeKind::MarkerClose, false);
                    let n = self.nodes.last_mut().unwrap();
                    n.flags |= flags::SELF_CLOSING;
                    n.len = (end - b) as u32;
                    return end;
                }
            }
            self.push_leaf(NodeKind::Error, Span::new(b as u32, (end - b) as u32), MarkerId::UNKNOWN);
            return end;
        }

        // Bare `\` at end of input → recover as an error node.
        if j >= len {
            self.report(|| {
                Diagnostic::error(
                    DiagnosticCode::StrayBackslash,
                    Span::new(b as u32, (j - b) as u32),
                    "stray '\\' at end of input",
                )
            });
            self.push_leaf(NodeKind::Error, Span::new(b as u32, (j - b) as u32), MarkerId::UNKNOWN);
            return j;
        }

        // Optional leading '+' explicitly nests a character marker inside the
        // current one (rather than implicitly closing a sibling).
        let nested = self.bytes[j] == b'+';
        if nested {
            j += 1;
        }
        let tag_start = j;
        while j < len && (self.bytes[j].is_ascii_alphanumeric() || self.bytes[j] == b'-') {
            j += 1;
        }
        let tag = &self.src[tag_start..j];
        let is_close = j < len && self.bytes[j] == b'*';
        if is_close {
            j += 1;
        }

        if tag.is_empty() {
            self.report(|| {
                Diagnostic::error(
                    DiagnosticCode::StrayBackslash,
                    Span::new(b as u32, (j - b) as u32),
                    "'\\' not followed by a marker tag",
                )
            });
            self.push_leaf(NodeKind::Error, Span::new(b as u32, (j - b) as u32), MarkerId::UNKNOWN);
            return j;
        }

        let id = self.table.lookup(tag);

        if is_close {
            return self.handle_close(b, j, id, tag);
        }

        // Deprecated-but-valid markers parse normally and carry an advisory. The
        // deprecation (and its version) is data from the marker table; the lookup is
        // gated so the hot path (diagnostics off) pays nothing.
        if self.collect_diags {
            if let Some(replacement) = id.and_then(|id| self.table.info(id)).and_then(|m| m.deprecated)
            {
                let span = Span::new(b as u32, (j - b) as u32);
                self.diags.push(Diagnostic::warning(
                    DiagnosticCode::DeprecatedMarker,
                    span,
                    format!("\\{tag} is deprecated; prefer {replacement}"),
                ));
            }
        }

        match id.and_then(|id| self.table.info(id).map(|m| (id, m.category))) {
            Some((id, Category::Paragraph)) | Some((id, Category::Identification)) => {
                let usx = self.table.info(id).map(|m| m.usx_element).unwrap_or("");
                if usx == "sidebar-end" {
                    // `\esbe` closes the nearest open sidebar and anything inside it,
                    // owning its own token bytes as the sidebar's close node.
                    let sidebar = self.stack.iter().rposition(|&oi| {
                        self.table.info(self.nodes[oi as usize].marker).map(|m| m.usx_element)
                            == Some("sidebar")
                    });
                    if let Some(pos) = sidebar {
                        self.close_blocks_down_to(pos + 1, b, Some(tag));
                        let open = self.stack.pop().unwrap();
                        self.emit_close(open, b, NodeKind::BlockEnd, false);
                        self.nodes.last_mut().unwrap().len = (j - b) as u32;
                    } else {
                        self.report(|| {
                            Diagnostic::error(
                                DiagnosticCode::StrayCloseMarker,
                                Span::new(b as u32, (j - b) as u32),
                                "\\esbe has no matching \\esb".to_string(),
                            )
                        });
                        self.push_leaf(NodeKind::Error, Span::new(b as u32, (j - b) as u32), id);
                    }
                    return j;
                }
                if usx == "rem" {
                    // Inside a body paragraph, \rem nests as a child remark instead
                    // of ending the paragraph; at the top level it is an ordinary block.
                    let nest = self.stack.last().is_some_and(|&top| {
                        self.table.info(self.nodes[top as usize].marker).map(|m| {
                            (m.category, m.text_role)
                        }) == Some((Category::Paragraph, TextRole::Boundary))
                    });
                    if nest {
                        let idx = self.push_open(Span::new(b as u32, (j - b) as u32), id);
                        self.stack.push(idx);
                        return j;
                    }
                }
                // Containers (`\esb`, `\periph`) start a fresh block group at the top
                // level; ordinary blocks only close down to the innermost container,
                // so they nest inside an open sidebar/periph.
                let is_container = matches!(usx, "sidebar" | "periph");
                let target = if is_container { 0 } else { self.container_floor() };
                self.close_blocks_down_to(target, b, Some(tag));
                let idx = self.push_open(Span::new(b as u32, (j - b) as u32), id);
                self.stack.push(idx);
                j
            }
            Some((id, Category::Chapter)) => {
                self.close_blocks_down_to(0, b, Some(tag));
                let (num, end) = self.read_number_from_tag_or_text(id, tag, j);
                // Absorb any inline text on the same line as \c (e.g. `\c 4 Title`).
                // Old translations sometimes embed a chapter title directly on the \c
                // line. Extending the span consumes it so it is never emitted as a
                // floating root-level Text node in USJ/USX (chapters have no text body).
                let end = self.consume_line_remainder(end);
                let idx = self.push_leaf(NodeKind::Chapter, Span::new(b as u32, (end - b) as u32), id);
                self.nodes[idx as usize].link = num as u32; // number, not a jump pointer
                self.nodes[idx as usize].flags |= flags::SELF_CLOSING;
                end
            }
            Some((id, Category::Verse)) => {
                // A verse is inline within its paragraph; it does not close blocks
                // — except a \rem nested in the paragraph, which ends at the verse.
                self.close_rem(b);
                let (num, end) = self.read_number_from_tag_or_text(id, tag, j);
                let idx = self.push_leaf(NodeKind::Verse, Span::new(b as u32, (end - b) as u32), id);
                self.nodes[idx as usize].link = num as u32;
                self.nodes[idx as usize].flags |= flags::SELF_CLOSING;
                end
            }
            Some((id, Category::Character))
            | Some((id, Category::Note))
            | Some((id, Category::Milestone))
            | Some((id, Category::Other)) => {
                // Note-internal markers (`\fr \ft \fq` — Character + no explicit
                // close) don't nest: an unprefixed one implicitly closes an open
                // sibling of the same kind, so they serialize as siblings. Markers
                // with explicit closing (`\bk \nd …`) nest until their `\xxx*`. A
                // dual-mode marker (`\xt`, Closing::Optional) nests when it is
                // explicitly closed ahead, else behaves as a note-text sibling.
                let info = self.table.info(id);
                let acts_as_sibling = !nested
                    && match info.map(|m| (m.category, m.closing)) {
                        Some((Category::Character, Closing::None)) => true,
                        Some((Category::Character, Closing::Optional)) => {
                            !self.has_close_ahead(tag, j)
                        }
                        _ => false,
                    };
                if acts_as_sibling {
                    if let Some(&top) = self.stack.last() {
                        let top_is_sibling = self
                            .table
                            .info(self.nodes[top as usize].marker)
                            .map(|m| {
                                m.category == Category::Character
                                    && matches!(m.closing, Closing::None | Closing::Optional)
                            })
                            == Some(true);
                        if top_is_sibling {
                            self.stack.pop();
                            self.emit_close(top, b, NodeKind::MarkerClose, true);
                        }
                    }
                }
                let idx = self.push_open(Span::new(b as u32, (j - b) as u32), id);
                if nested {
                    self.nodes[idx as usize].flags |= flags::NESTED;
                }
                self.stack.push(idx);
                j
            }
            None => {
                // Unknown marker: keep its text (error-tolerant), but don't give it
                // block/close semantics we can't know. Markers in the `z` namespace
                // are the spec's user-extension space, so they're allowed silently;
                // any other unrecognized marker is an error.
                if !tag.starts_with('z') {
                    self.report(|| {
                        Diagnostic::error(
                            DiagnosticCode::UnknownMarker,
                            Span::new(b as u32, (j - b) as u32),
                            format!("\\{tag} is not a known marker for this USFM version"),
                        )
                    });
                }
                let idx = self.push_open(Span::new(b as u32, (j - b) as u32), MarkerId::UNKNOWN);
                self.nodes[idx as usize].flags |= flags::HAS_ERROR | flags::SELF_CLOSING;
                j
            }
        }
    }

    fn handle_close(&mut self, b: usize, j: usize, id: Option<MarkerId>, tag: &str) -> usize {
        let span = Span::new(b as u32, (j - b) as u32);
        let matching = id.and_then(|id| {
            self.stack.iter().rposition(|&oi| self.nodes[oi as usize].marker == id)
        });
        match matching {
            Some(pos_in_stack) => {
                // Close any inner markers left open inside this one — they were
                // implicitly closed by this explicit `\tag*`.
                self.close_blocks_down_to(pos_in_stack + 1, b, Some(tag));
                let open = self.stack.pop().unwrap();
                self.emit_close(open, b, NodeKind::MarkerClose, false);
                // The explicit `\tag*` token itself is a real (lossless) node.
                self.nodes.last_mut().unwrap().len = (j - b) as u32;
                let _ = span;
                j
            }
            None => {
                self.report(|| {
                    Diagnostic::error(
                        DiagnosticCode::StrayCloseMarker,
                        span,
                        format!("\\{tag}* has no matching opening marker"),
                    )
                });
                self.push_leaf(NodeKind::Error, span, id.unwrap_or(MarkerId::UNKNOWN));
                j
            }
        }
    }

    /// Scan forward from `from` consuming everything until the next `\n` or `\`.
    /// Used to absorb inline text on the same line as a `\c` marker (old-style
    /// chapter titles embedded on the `\c` line) into the chapter's span, so they
    /// are never tokenized as floating root-level Text nodes.
    fn consume_line_remainder(&self, from: usize) -> usize {
        let bytes = self.bytes;
        let mut i = from;
        while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\\' {
            i += 1;
        }
        i
    }

    /// Read the verse/chapter number for a Chapter or Verse marker.
    ///
    /// Normally the number follows the marker as the first text token (`\v 11`).
    /// But malformed USFM sometimes omits the space (`\v11`), embedding the number
    /// in the marker name itself. When the tag is longer than the base marker name
    /// and starts with a digit after the base letters, extract the number from the
    /// tag rather than reading it from the text after the marker.
    fn read_number_from_tag_or_text(&self, id: MarkerId, tag: &str, after_tag: usize) -> (u32, usize) {
        let base_len = self.table.info(id).map_or(0, |m| m.name.len());
        let suffix = &tag[base_len..];
        if suffix.starts_with(|c: char| c.is_ascii_digit()) {
            let value = suffix
                .bytes()
                .take_while(|b| b.is_ascii_digit())
                .fold(0u32, |acc, b| acc.saturating_mul(10).saturating_add((b - b'0') as u32));
            (value, after_tag) // number came from tag; no extra bytes consumed
        } else {
            self.read_number(after_tag)
        }
    }

    /// Skip spaces from `from`, then read a number token (digits plus letters,
    /// `-`, `,` for ranges/sub-verses). Returns `(leading_int, end_offset)`.
    /// `end_offset` covers up to and including the number token.
    fn read_number(&self, from: usize) -> (u32, usize) {
        let len = self.bytes.len();
        let mut k = from;
        while k < len && (self.bytes[k] == b' ' || self.bytes[k] == b'\t') {
            k += 1;
        }
        let num_start = k;
        let mut value: u32 = 0;
        let mut saw_digit = false;
        // Leading integer.
        while k < len && self.bytes[k].is_ascii_digit() {
            value = value.saturating_mul(10).saturating_add((self.bytes[k] - b'0') as u32);
            saw_digit = true;
            k += 1;
        }
        // Trailing range/letter part (e.g. "-2", "a") stays inside the token.
        while k < len
            && (self.bytes[k].is_ascii_alphanumeric() || self.bytes[k] == b'-' || self.bytes[k] == b',')
        {
            k += 1;
        }
        if !saw_digit {
            // No number after \c/\v: token is just the marker; body starts at `from`.
            return (0, from);
        }
        let _ = num_start;
        (value, k)
    }
}
