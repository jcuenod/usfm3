//! Test-only decoder for the packed tape.
//!
//! Deliberately an *independent* reimplementation of what a host does: it decodes the
//! header and the `kind | flags << 8 | marker << 16` word by hand, and reproduces the
//! CST JSON by mirroring `Cursor`'s traversal rules from the packed integers alone.
//! That is what makes `packed_tape_matches_cst_json_across_the_corpus` a real proof
//! rather than a tautology — if the wire format, the `link` overloads, or the
//! traversal contract are misread, this decoder produces different JSON.

// Compiled into more than one test binary, so not every accessor is used by each.
#![allow(dead_code)]

use serde_json::{Map, Value};
use usfm3::{PACKED_FORMAT, PACKED_HEADER_WORDS, PACKED_MAGIC, PACKED_NODE_WORDS};

pub struct Packed {
    pub words: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct PackedNode {
    pub kind_code: u32,
    pub flags: u8,
    pub marker: u16,
    pub start: u32,
    pub len: u32,
    pub link: u32,
}

impl Packed {
    /// Decode and validate the header, exactly as a host constructor would.
    pub fn new(words: Vec<u32>) -> Self {
        assert!(words.len() >= PACKED_HEADER_WORDS, "buffer shorter than the header");
        assert_eq!(words[0], PACKED_MAGIC, "bad magic");
        assert_eq!(words[1], PACKED_FORMAT, "unexpected wire format");
        let packed = Packed { words };
        assert_eq!(
            packed.words.len(),
            PACKED_HEADER_WORDS + PACKED_NODE_WORDS * packed.count(),
            "buffer length disagrees with the node count in the header"
        );
        packed
    }

    pub fn count(&self) -> usize {
        self.words[2] as usize
    }
    pub fn offset_space(&self) -> u32 {
        self.words[3]
    }
    pub fn version_rank(&self) -> u32 {
        self.words[4]
    }
    /// `(bytes, utf16 units, chars)`
    pub fn lengths(&self) -> (u32, u32, u32) {
        (self.words[5], self.words[6], self.words[7])
    }

    pub fn node(&self, i: usize) -> PackedNode {
        let base = PACKED_HEADER_WORDS + i * PACKED_NODE_WORDS;
        let w = self.words[base];
        PackedNode {
            kind_code: w & 0xff,
            flags: ((w >> 8) & 0xff) as u8,
            marker: (w >> 16) as u16,
            start: self.words[base + 1],
            len: self.words[base + 2],
            link: self.words[base + 3],
        }
    }

    pub fn kind<'s>(&self, i: usize, schema: &'s Value) -> &'s str {
        schema["kinds"][self.node(i).kind_code as usize]
            .as_str()
            .expect("kind code out of range for the schema")
    }

    // --- traversal, mirroring `Cursor` -------------------------------------

    fn is_marker_bearing(&self, i: usize, schema: &Value) -> bool {
        matches!(
            self.kind(i, schema),
            "marker" | "close" | "block_end" | "chapter" | "verse"
        )
    }

    fn marker_meta<'s>(&self, i: usize, schema: &'s Value) -> Option<&'s Value> {
        if !self.is_marker_bearing(i, schema) {
            return None;
        }
        // `MarkerTable::info` returns None for MarkerId::UNKNOWN because the index is
        // out of range; array indexing reproduces that for free.
        schema["markers"].get(self.node(i).marker as usize)
    }

    fn first_child(&self, i: usize, schema: &Value) -> Option<usize> {
        let n = self.node(i);
        if !matches!(self.kind(i, schema), "root" | "marker") {
            return None;
        }
        let child = i as u32 + 1;
        (child < n.link).then_some(child as usize)
    }

    fn next_sibling(&self, i: usize, schema: &Value) -> Option<usize> {
        let n = self.node(i);
        let after = match self.kind(i, schema) {
            "marker" | "root" => n.link + 1,
            _ => i as u32 + 1,
        } as usize;
        if after >= self.count() {
            return None;
        }
        (!matches!(self.kind(after, schema), "close" | "block_end")).then_some(after)
    }

    fn children(&self, i: usize, schema: &Value) -> Vec<usize> {
        let mut out = Vec::new();
        let mut cur = self.first_child(i, schema);
        while let Some(idx) = cur {
            out.push(idx);
            cur = self.next_sibling(idx, schema);
        }
        out
    }

    /// Marker name as written, mirroring `Cursor::marker_name`. Requires the buffer to
    /// be in `utf8` space so `start`/`len` index `source` directly.
    fn marker_name<'s>(&self, i: usize, source: &'s str) -> &'s str {
        let n = self.node(i);
        let s = &source[n.start as usize..(n.start + n.len) as usize];
        let s = s.strip_prefix('\\').unwrap_or(s);
        let s = s.strip_prefix('+').unwrap_or(s);
        let end = s
            .find(|c: char| c.is_whitespace() || c == '*' || c == '|' || c == '\\')
            .unwrap_or(s.len());
        &s[..end]
    }

    fn synthetic(&self, i: usize, schema: &Value) -> bool {
        let bit = schema["flags"]["synthetic"].as_u64().unwrap() as u8;
        self.node(i).flags & bit != 0
    }

    /// Rebuild `Document::to_cst_json`'s shape from the packed words.
    pub fn to_cst_json(&self, source: &str, schema: &Value) -> Value {
        assert_eq!(self.offset_space(), 0, "to_cst_json parity needs utf8 offsets");
        self.cst_node(0, source, schema)
    }

    fn cst_node(&self, i: usize, source: &str, schema: &Value) -> Value {
        let n = self.node(i);
        let mut obj = Map::new();
        obj.insert("type".into(), self.kind(i, schema).into());

        if let Some(info) = self.marker_meta(i, schema) {
            obj.insert("marker".into(), self.marker_name(i, source).into());
            obj.insert("category".into(), info["category"].clone());
            obj.insert("role".into(), info["role"].clone());
            obj.insert("closing".into(), info["closing"].clone());
        }
        if matches!(self.kind(i, schema), "chapter" | "verse") {
            // `link` carries the number for these kinds, not a jump pointer.
            obj.insert("number".into(), n.link.into());
        }

        obj.insert("start".into(), n.start.into());
        obj.insert("end".into(), (n.start + n.len).into());
        if self.synthetic(i, schema) {
            obj.insert("synthetic".into(), true.into());
        }

        if self.kind(i, schema) == "marker" {
            let c = n.link as usize;
            let cn = self.node(c);
            let mut close = Map::new();
            close.insert("start".into(), cn.start.into());
            close.insert("end".into(), (cn.start + cn.len).into());
            if self.synthetic(c, schema) {
                close.insert("synthetic".into(), true.into());
            }
            obj.insert("close".into(), Value::Object(close));
        }

        let children = self.children(i, schema);
        if !children.is_empty() {
            obj.insert(
                "children".into(),
                Value::Array(
                    children
                        .into_iter()
                        .map(|c| self.cst_node(c, source, schema))
                        .collect(),
                ),
            );
        }

        Value::Object(obj)
    }
}
