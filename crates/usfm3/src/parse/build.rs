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

use crate::diagnostic::Diagnostic;
use crate::marker::{Category, Closing, MarkerId, MarkerTable};
use crate::span::Span;
use crate::tape::{flags, Node, NodeKind, Tape};

use super::scan;

pub struct BuildOutput {
    pub tape: Tape,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn build(source: &str, table: &MarkerTable) -> BuildOutput {
    Builder::new(source, table).run()
}

struct Builder<'a> {
    src: &'a str,
    bytes: &'a [u8],
    table: &'a MarkerTable,
    nodes: Vec<Node>,
    diags: Vec<Diagnostic>,
    /// Indices of currently open container nodes (paragraphs, char spans, notes),
    /// innermost last.
    stack: Vec<u32>,
}

impl<'a> Builder<'a> {
    fn new(src: &'a str, table: &'a MarkerTable) -> Self {
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
        Self { src, bytes, table, nodes, diags: Vec::new(), stack: Vec::new() }
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
                    self.emit_text(pos, b);
                    pos = self.parse_marker(b);
                }
            }
        }
        // Close anything still open at EOF.
        self.close_blocks_down_to(0, len);
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
    /// markers (`\w …|lemma="…"\w*`) and milestones (`\qt-s |who="…"\*`).
    fn top_takes_attributes(&self) -> bool {
        self.stack
            .last()
            .and_then(|&i| self.table.info(self.nodes[i as usize].marker))
            .map(|m| matches!(m.category, Category::Character | Category::Milestone))
            .unwrap_or(false)
    }

    /// Close open containers until the stack has `target_len` entries, at byte
    /// `at`. Required-closing markers closed this way produce a warning.
    fn close_blocks_down_to(&mut self, target_len: usize, at: usize) {
        while self.stack.len() > target_len {
            let open = self.stack.pop().unwrap();
            let info = self.table.info(self.nodes[open as usize].marker);
            let (kind, required) = match info.map(|m| (m.usx_element, m.closing)) {
                Some(("para", _)) | Some(("book", _)) => (NodeKind::BlockEnd, false),
                Some((_, Closing::Required)) => (NodeKind::MarkerClose, true),
                _ => (NodeKind::BlockEnd, false),
            };
            if required {
                let span = self.nodes[open as usize].span();
                let name = info.map(|m| m.name).unwrap_or("?");
                self.diags.push(Diagnostic::warning(
                    "unclosed-marker",
                    span,
                    format!("\\{name} was left unclosed and was closed implicitly at a block boundary"),
                ));
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
            self.diags.push(Diagnostic::error(
                "stray-backslash",
                Span::new(b as u32, (j - b) as u32),
                "stray '\\' at end of input",
            ));
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
            self.diags.push(Diagnostic::error(
                "stray-backslash",
                Span::new(b as u32, (j - b) as u32),
                "'\\' not followed by a marker tag",
            ));
            self.push_leaf(NodeKind::Error, Span::new(b as u32, (j - b) as u32), MarkerId::UNKNOWN);
            return j;
        }

        let id = self.table.lookup(tag);

        if is_close {
            return self.handle_close(b, j, id, tag);
        }

        match id.and_then(|id| self.table.info(id).map(|m| (id, m.category))) {
            Some((id, Category::Paragraph)) | Some((id, Category::Identification)) => {
                // A new block ends every currently-open block (and any unclosed
                // inline markers within it).
                self.close_blocks_down_to(0, b);
                let idx = self.push_open(Span::new(b as u32, (j - b) as u32), id);
                self.stack.push(idx);
                j
            }
            Some((id, Category::Chapter)) => {
                self.close_blocks_down_to(0, b);
                let (num, end) = self.read_number(j);
                let idx = self.push_leaf(NodeKind::Chapter, Span::new(b as u32, (end - b) as u32), id);
                self.nodes[idx as usize].link = num as u32; // number, not a jump pointer
                self.nodes[idx as usize].flags |= flags::SELF_CLOSING;
                end
            }
            Some((id, Category::Verse)) => {
                // A verse is inline within its paragraph; it does not close blocks.
                let (num, end) = self.read_number(j);
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
                // with explicit closing (`\bk \nd …`) nest until their `\xxx*`.
                let info = self.table.info(id);
                let implicit_sibling = !nested
                    && info.map(|m| m.category == Category::Character && m.closing == Closing::None)
                        == Some(true);
                if implicit_sibling {
                    if let Some(&top) = self.stack.last() {
                        let top_is_sibling = self
                            .table
                            .info(self.nodes[top as usize].marker)
                            .map(|m| m.category == Category::Character && m.closing == Closing::None)
                            == Some(true);
                        if top_is_sibling {
                            self.stack.pop();
                            self.emit_close(top, b, NodeKind::MarkerClose, true);
                        }
                    }
                }
                let idx = self.push_open(Span::new(b as u32, (j - b) as u32), id);
                self.stack.push(idx);
                j
            }
            None => {
                // Unknown marker: keep its text (error-tolerant), but don't give it
                // block/close semantics we can't know.
                self.diags.push(Diagnostic::warning(
                    "unknown-marker",
                    Span::new(b as u32, (j - b) as u32),
                    format!("\\{tag} is not a known marker for this USFM version"),
                ));
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
                // Close any inner markers left open inside this one (warnings).
                self.close_blocks_down_to(pos_in_stack + 1, b);
                let open = self.stack.pop().unwrap();
                self.emit_close(open, b, NodeKind::MarkerClose, false);
                // The explicit `\tag*` token itself is a real (lossless) node.
                self.nodes.last_mut().unwrap().len = (j - b) as u32;
                let _ = span;
                j
            }
            None => {
                self.diags.push(Diagnostic::warning(
                    "stray-close",
                    span,
                    format!("\\{tag}* has no matching opening marker"),
                ));
                self.push_leaf(NodeKind::Error, span, id.unwrap_or(MarkerId::UNKNOWN));
                j
            }
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
