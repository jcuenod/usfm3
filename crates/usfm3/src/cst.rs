//! The concrete syntax tree: a zero-allocation cursor over the [`Tape`].
//!
//! This is the surface a USFM editor / linter consumes. There is no separate tree
//! data structure — a [`Cursor`] is just `(tape, source, table, index)`, and
//! navigation is index arithmetic over the `link` jump pointers. Children of a
//! container live between its `MarkerOpen` and the matching close; siblings are
//! reached by skipping a subtree via `link + 1`.
//!
//! A typed AST layer (e.g. `Chapter`, `Verse`, `Note` wrappers) will sit on top of
//! this raw cursor later, in the rust-analyzer style.

use crate::marker::{MarkerInfo, MarkerTable};
use crate::span::Span;
use crate::tape::{Node, NodeKind, Tape};

#[derive(Clone, Copy)]
pub struct Cursor<'a> {
    tape: &'a Tape,
    source: &'a str,
    table: &'a MarkerTable,
    idx: u32,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(tape: &'a Tape, source: &'a str, table: &'a MarkerTable, idx: u32) -> Self {
        Self { tape, source, table, idx }
    }

    #[inline]
    pub fn node(&self) -> &'a Node {
        &self.tape.nodes[self.idx as usize]
    }

    #[inline]
    pub fn kind(&self) -> NodeKind {
        self.node().kind
    }

    /// Source text covered by this node (zero-copy). Empty for synthetic nodes.
    #[inline]
    pub fn text(&self) -> &'a str {
        self.node().span().text(self.source)
    }

    #[inline]
    pub fn span(&self) -> Span {
        self.node().span()
    }

    /// Whether this node was inserted by the parser (implicit block end, root)
    /// rather than present in the source — such nodes have zero-length spans.
    #[inline]
    pub fn is_synthetic(&self) -> bool {
        self.node().is_synthetic()
    }

    /// For `\c`/`\v` leaves, the parsed chapter/verse number (packed in `link`).
    #[inline]
    pub fn number(&self) -> Option<u32> {
        match self.kind() {
            NodeKind::Chapter | NodeKind::Verse => Some(self.node().link),
            _ => None,
        }
    }

    /// For a `MarkerOpen`, a cursor at its matching close token (`\xxx*` or a
    /// synthetic block end), so callers can decorate the closing tag too.
    pub fn matching_close(&self) -> Option<Cursor<'a>> {
        match self.kind() {
            NodeKind::MarkerOpen => Some(self.at(self.node().link)),
            _ => None,
        }
    }

    /// Marker metadata for marker-bearing nodes.
    pub fn marker_info(&self) -> Option<&'a MarkerInfo> {
        match self.kind() {
            NodeKind::MarkerOpen
            | NodeKind::MarkerClose
            | NodeKind::BlockEnd
            | NodeKind::Chapter
            | NodeKind::Verse => self.table.info(self.node().marker),
            _ => None,
        }
    }

    /// First child of a container, or `None` for leaves/empty containers.
    pub fn first_child(&self) -> Option<Cursor<'a>> {
        let node = self.node();
        match node.kind {
            NodeKind::Root | NodeKind::MarkerOpen => {
                let child = self.idx + 1;
                // `link` is the matching close; no children if it is adjacent.
                if child < node.link {
                    Some(self.at(child))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Next sibling within the same parent, skipping this node's whole subtree.
    pub fn next_sibling(&self) -> Option<Cursor<'a>> {
        let node = self.node();
        let after = match node.kind {
            NodeKind::MarkerOpen | NodeKind::Root => node.link + 1, // jump past the close
            _ => self.idx + 1,
        };
        // The parent's close sits at the end of the children run; callers that
        // need the precise boundary use the parent's `link`. For the root-level
        // walk this simply stops at the tape end.
        if (after as usize) < self.tape.nodes.len() {
            let next = self.at(after);
            if matches!(next.kind(), NodeKind::MarkerClose | NodeKind::BlockEnd) {
                None
            } else {
                Some(next)
            }
        } else {
            None
        }
    }

    /// Iterate direct children.
    pub fn children(&self) -> impl Iterator<Item = Cursor<'a>> {
        let mut cur = self.first_child();
        std::iter::from_fn(move || {
            let out = cur;
            cur = cur.and_then(|c| c.next_sibling());
            out
        })
    }

    #[inline]
    fn at(&self, idx: u32) -> Cursor<'a> {
        Cursor { idx, ..*self }
    }
}
