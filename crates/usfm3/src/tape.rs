//! The tape: the flat IR that everything else is a view over.
//!
//! Inspired by simd_json's tape. Instead of a heap of tree nodes, the whole
//! document is one `Vec<Node>` of 16-byte records. Containers (paragraphs,
//! chapters, character spans, notes) store a `link` jump pointer to their
//! matching close, so the flat array can be navigated as a tree in pure index
//! arithmetic — skip a subtree by jumping to `link + 1`.
//!
//! Two invariants make the tape powerful:
//! 1. **Lossless**: concatenating the spans of all non-synthetic nodes in order
//!    reproduces the source byte-for-byte (needed for the editor/CST).
//! 2. **Tree-shaped**: USFM's implicit block structure (paragraphs are not
//!    explicitly closed) is materialized as zero-length, `SYNTHETIC`-flagged
//!    [`NodeKind::BlockEnd`] nodes, so every consumer sees clean open/close
//!    pairs without re-deriving block rules.

use crate::marker::MarkerId;
use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum NodeKind {
    /// Synthetic document root; `link` -> the final close. Always index 0.
    Root,
    /// Opening of a marker. `link` -> matching `MarkerClose`/`BlockEnd`.
    MarkerOpen,
    /// Closing of a marker. `link` -> the matching `MarkerOpen`.
    MarkerClose,
    /// `\c` — a leaf milestone. `link` holds the chapter *number* (not a jump
    /// pointer): chapters/verses have no paired close, so the slot is free.
    Chapter,
    /// `\v` — a leaf milestone. `link` holds the leading verse *number*.
    Verse,
    /// Run of literal text. `span` is zero-copy into the source.
    Text,
    /// A `|key="value"` (or default) attribute attached to the enclosing marker.
    Attribute,
    /// Synthetic, zero-length close inserted at an implicit block boundary.
    BlockEnd,
    /// Bytes we could not interpret; paired with a [`crate::diagnostic::Diagnostic`].
    Error,
}

impl NodeKind {
    /// Stable machine-readable name (used in CST JSON / FFI).
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Root => "root",
            NodeKind::MarkerOpen => "marker",
            NodeKind::MarkerClose => "close",
            NodeKind::Chapter => "chapter",
            NodeKind::Verse => "verse",
            NodeKind::Text => "text",
            NodeKind::Attribute => "attribute",
            NodeKind::BlockEnd => "block_end",
            NodeKind::Error => "error",
        }
    }
}

/// Bit flags packed into [`Node::flags`].
pub mod flags {
    pub const SYNTHETIC: u8 = 1 << 0; // inserted by the parser; skip for losslessness
    pub const SELF_CLOSING: u8 = 1 << 1; // e.g. milestone `\qt-s\*`
    pub const HAS_ATTRS: u8 = 1 << 2; // an Attribute node follows
    pub const HAS_ERROR: u8 = 1 << 3; // a diagnostic references this node
    pub const NESTED: u8 = 1 << 4; // opened with a `+` nesting prefix (`\+nd`)
}

/// One 16-byte tape record. Keep this `#[repr(C)]` and exactly 16 bytes.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Node {
    pub kind: NodeKind,
    pub flags: u8,
    /// Interned marker type (meaningful for `MarkerOpen`/`MarkerClose`/`BlockEnd`).
    pub marker: MarkerId,
    pub start: u32,
    pub len: u32,
    /// Jump pointer (open<->close), or auxiliary payload depending on `kind`.
    pub link: u32,
}

const _: () = assert!(core::mem::size_of::<Node>() == 16);

impl Node {
    #[inline]
    pub fn span(&self) -> Span {
        Span::new(self.start, self.len)
    }
    #[inline]
    pub fn is_synthetic(&self) -> bool {
        self.flags & flags::SYNTHETIC != 0
    }
}

/// The parsed document as a flat array of [`Node`]s. Index 0 is the [`NodeKind::Root`].
#[derive(Debug, Default)]
pub struct Tape {
    pub nodes: Vec<Node>,
}

impl Tape {
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    #[inline]
    pub fn get(&self, idx: u32) -> Option<&Node> {
        self.nodes.get(idx as usize)
    }
}
