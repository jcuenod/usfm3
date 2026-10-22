//! The tape as a packed `u32` buffer — the CST view for hosts that walk it rather
//! than materialize it.
//!
//! [`crate::Document::to_cst_json`] re-encodes each 16-byte [`Node`] as ~80–150 bytes
//! of JSON text, which the host then re-parses into an object graph. For an editor
//! reparsing a book-sized file on every keystroke that is the dominant cost, and all
//! of it is avoidable: a [`Node`] is already four `u32`s, so the host can be handed
//! the integers directly and decode them with shifts.
//!
//! Two things are *not* verbatim copies of the tape, and both are deliberate:
//!
//! 1. **Offsets are translated** into the host's coordinate space ([`Offsets`]).
//!    That is why this is an explicit pack rather than a `transmute` of `&[Node]` —
//!    and the upside is that the wire format is independent of Rust struct layout
//!    and of endianness, so [`Node`] can gain a field without breaking any host.
//! 2. **`link` is never translated.** It is an index (or a chapter/verse *number*),
//!    not a source offset. See [`Node::link`].

use crate::Document;
use crate::span::{OffsetMap, Offsets};

/// Buffer tag, `b"USFT"` read big-endian. Lets a host reject a buffer that is not a
/// packed tape at all (a truncated `postMessage`, the wrong array).
pub const PACKED_MAGIC: u32 = 0x5553_4654;

/// Wire format version. A host switches on this, so the header may grow later.
pub const PACKED_FORMAT: u32 = 1;

/// Header size in `u32` words.
pub const PACKED_HEADER_WORDS: usize = 8;

/// Words per node.
pub const PACKED_NODE_WORDS: usize = 4;

impl Document<'_> {
    /// The tape as a packed `u32` buffer: an 8-word header, then 4 words per node.
    ///
    /// Header:
    ///
    /// | word | meaning |
    /// |------|---------|
    /// | 0 | [`PACKED_MAGIC`] |
    /// | 1 | [`PACKED_FORMAT`] |
    /// | 2 | node count |
    /// | 3 | offset space ([`Offsets::code`]) |
    /// | 4 | USFM version rank ([`crate::Version::rank`]) |
    /// | 5 | source length in bytes |
    /// | 6 | source length in UTF-16 code units |
    /// | 7 | source length in chars |
    ///
    /// All three lengths are always present so that *any* host can validate the
    /// source string it holds in O(1) — `src.length` in JS, `len(src)` in Python —
    /// whichever space the offsets happen to be in. Getting this wrong is not a
    /// hypothetical: a host that has to encode the source just to check its length
    /// pays more than the whole rest of the call.
    ///
    /// Node `i` starts at `PACKED_HEADER_WORDS + i * PACKED_NODE_WORDS`:
    ///
    /// | word | meaning |
    /// |------|---------|
    /// | 0 | `kind \| flags << 8 \| marker << 16` |
    /// | 1 | `start`, in the header's offset space |
    /// | 2 | `len`, in the header's offset space |
    /// | 3 | `link`, verbatim |
    ///
    /// This is the whole tape, which is the same information
    /// [`Document::to_cst_json`] exposes plus the flag bits that JSON drops. Note the
    /// node count is *not* the CST JSON node count: the JSON folds every
    /// [`crate::low_level::NodeKind::MarkerClose`] into its parent's `close` field
    /// instead of emitting a node.
    pub fn to_packed_tape(&self, offsets: Offsets) -> Vec<u32> {
        let nodes = &self.tape.nodes;
        let mut out = Vec::with_capacity(PACKED_HEADER_WORDS + PACKED_NODE_WORDS * nodes.len());

        let mut map = OffsetMap::new(self.source, offsets);
        let (bytes, utf16, chars) = map.lengths();

        out.push(PACKED_MAGIC);
        out.push(PACKED_FORMAT);
        out.push(nodes.len() as u32);
        out.push(offsets.code());
        out.push(self.version.rank());
        out.push(bytes);
        out.push(utf16);
        out.push(chars);

        for node in nodes {
            out.push(node.kind as u32 | (node.flags as u32) << 8 | (node.marker.0 as u32) << 16);
            // `start` drives the scanner — node starts are non-decreasing (the tiling
            // invariant), so this is one pass over the source in total. The length is
            // measured locally rather than by translating `end`, because a container's
            // end lies past its children and would force the scanner backwards.
            out.push(map.at(node.start));
            out.push(map.span_len(node.start, node.span().end()));
            // NOT an offset. `link` is a jump pointer for MarkerOpen/MarkerClose/
            // BlockEnd, the chapter/verse *number* for Chapter/Verse, the node's own
            // index for Text/Attribute/Error, and the last node index for Root.
            // Translating it would corrupt all four.
            out.push(node.link);
        }

        out
    }
}
