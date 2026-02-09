use serde::Serialize;

/// Byte-offset range in the original USFM source.
pub type Span = std::ops::Range<usize>;

// ---------------------------------------------------------------------------
// Top-level document
// ---------------------------------------------------------------------------

/// The top-level document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Document {
    pub content: Vec<Node>,
}

impl Document {
    /// Creates an empty document.
    pub fn new() -> Self {
        Self {
            content: Vec::new(),
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

/// A node in the USFM document tree.
///
/// Modeled after the USJ (Unified Scripture JSON) specification so that
/// conversion to both USX (XML) and USJ (JSON) is straightforward.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum Node {
    /// `\id` -- book identification.
    #[serde(rename = "book")]
    Book {
        marker: String,
        code: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// `\c` -- chapter milestone (NOT a container).
    #[serde(rename = "chapter")]
    Chapter {
        marker: String,
        number: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        altnumber: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pubnumber: Option<String>,
        #[serde(skip)]
        span: Span,
    },

    /// `\v` -- verse milestone (NOT a container).
    #[serde(rename = "verse")]
    Verse {
        marker: String,
        number: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        altnumber: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pubnumber: Option<String>,
        #[serde(skip)]
        span: Span,
    },

    /// `\p`, `\q1`, `\m`, `\li1`, `\b`, etc. -- paragraph-level container.
    #[serde(rename = "para")]
    Para {
        marker: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// `\nd`, `\bk`, `\add`, `\it`, etc. -- character-level container.
    #[serde(rename = "char")]
    Char {
        marker: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        attributes: Vec<Attribute>,
        #[serde(skip)]
        span: Span,
    },

    /// `\f`, `\x` -- footnote or cross-reference container.
    #[serde(rename = "note")]
    Note {
        marker: String,
        caller: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        category: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// `\qt1-s`, `\qt1-e`, `\ts-s`, etc. -- milestone (empty element with attributes).
    #[serde(rename = "ms")]
    Milestone {
        marker: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        attributes: Vec<Attribute>,
        #[serde(skip)]
        span: Span,
    },

    /// `\fig` -- figure with attributes.
    #[serde(rename = "figure")]
    Figure {
        marker: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        attributes: Vec<Attribute>,
        #[serde(skip)]
        span: Span,
    },

    /// `\esb` ... `\esbe` -- sidebar container.
    #[serde(rename = "sidebar")]
    Sidebar {
        marker: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        category: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// `\periph` -- peripheral content section.
    #[serde(rename = "periph")]
    Periph {
        #[serde(skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        attributes: Vec<Attribute>,
        #[serde(skip)]
        span: Span,
    },

    /// Table container wrapping consecutive `\tr` rows.
    #[serde(rename = "table")]
    Table {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// `\tr` -- table row container.
    #[serde(rename = "table:row")]
    TableRow {
        marker: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// `\th1`, `\tc2`, etc. -- table cell.
    #[serde(rename = "table:cell")]
    TableCell {
        marker: String,
        align: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// Unrecognized marker (from `\z` namespace or genuinely unknown).
    /// Preserved so no data is lost.
    #[serde(rename = "unknown")]
    Unknown {
        marker: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        content: Vec<Node>,
        #[serde(skip)]
        span: Span,
    },

    /// Plain text content -- serialized as a bare string in USJ.
    /// Must be the last variant because of `#[serde(untagged)]`.
    #[serde(untagged)]
    Text(String),
}

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

/// A key-value attribute (e.g. `|src="image.png"`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attribute {
    pub key: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Node helper methods
// ---------------------------------------------------------------------------

impl Node {
    /// Shorthand for creating a [`Node::Text`].
    pub fn text(s: impl Into<String>) -> Self {
        Node::Text(s.into())
    }

    /// Returns the content/children slice.
    ///
    /// Returns an empty slice for [`Node::Text`], [`Node::Chapter`],
    /// [`Node::Verse`], and [`Node::Milestone`] variants.
    pub fn children(&self) -> &[Node] {
        match self {
            Node::Book { content, .. }
            | Node::Para { content, .. }
            | Node::Char { content, .. }
            | Node::Note { content, .. }
            | Node::Figure { content, .. }
            | Node::Sidebar { content, .. }
            | Node::Periph { content, .. }
            | Node::Table { content, .. }
            | Node::TableRow { content, .. }
            | Node::TableCell { content, .. }
            | Node::Unknown { content, .. } => content,
            Node::Chapter { .. } | Node::Verse { .. } | Node::Milestone { .. } | Node::Text(_) => {
                &[]
            }
        }
    }

    /// Returns a mutable reference to the children vector, if the variant has one.
    ///
    /// Returns `None` for [`Node::Text`], [`Node::Chapter`], [`Node::Verse`],
    /// and [`Node::Milestone`] variants.
    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::Book { content, .. }
            | Node::Para { content, .. }
            | Node::Char { content, .. }
            | Node::Note { content, .. }
            | Node::Figure { content, .. }
            | Node::Sidebar { content, .. }
            | Node::Periph { content, .. }
            | Node::Table { content, .. }
            | Node::TableRow { content, .. }
            | Node::TableCell { content, .. }
            | Node::Unknown { content, .. } => Some(content),
            Node::Chapter { .. } | Node::Verse { .. } | Node::Milestone { .. } | Node::Text(_) => {
                None
            }
        }
    }

    /// Returns the marker string if the node has one.
    ///
    /// Returns `None` for [`Node::Text`].
    pub fn marker(&self) -> Option<&str> {
        match self {
            Node::Book { marker, .. }
            | Node::Chapter { marker, .. }
            | Node::Verse { marker, .. }
            | Node::Para { marker, .. }
            | Node::Char { marker, .. }
            | Node::Note { marker, .. }
            | Node::Milestone { marker, .. }
            | Node::Figure { marker, .. }
            | Node::Sidebar { marker, .. }
            | Node::TableRow { marker, .. }
            | Node::TableCell { marker, .. }
            | Node::Unknown { marker, .. } => Some(marker),
            Node::Table { .. } | Node::Periph { .. } | Node::Text(_) => None,
        }
    }

    /// Returns a reference to the source span if the node has one.
    ///
    /// Returns `None` for [`Node::Text`].
    pub fn span(&self) -> Option<&Span> {
        match self {
            Node::Book { span, .. }
            | Node::Chapter { span, .. }
            | Node::Verse { span, .. }
            | Node::Para { span, .. }
            | Node::Char { span, .. }
            | Node::Note { span, .. }
            | Node::Milestone { span, .. }
            | Node::Figure { span, .. }
            | Node::Sidebar { span, .. }
            | Node::Periph { span, .. }
            | Node::Table { span, .. }
            | Node::TableRow { span, .. }
            | Node::TableCell { span, .. }
            | Node::Unknown { span, .. } => Some(span),
            Node::Text(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // -- Construction helpers ------------------------------------------------

    #[test]
    fn create_document() {
        let doc = Document::new();
        assert!(doc.content.is_empty());
    }

    #[test]
    fn create_book_node() {
        let node = Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
            span: 0..5,
        };
        assert_eq!(node.marker(), Some("id"));
        assert_eq!(node.span(), Some(&(0..5)));
        assert!(node.children().is_empty());
    }

    #[test]
    fn create_chapter_node() {
        let mut node = Node::Chapter {
            marker: "c".into(),
            number: "1".into(),
            sid: Some("GEN 1".into()),
            altnumber: None,
            pubnumber: None,
            span: 10..14,
        };
        assert_eq!(node.marker(), Some("c"));
        assert_eq!(node.span(), Some(&(10..14)));
        // Chapter is a milestone: no children.
        assert!(node.children().is_empty());
        assert!(node.children_mut().is_none());
    }

    #[test]
    fn create_verse_node() {
        let node = Node::Verse {
            marker: "v".into(),
            number: "3-4".into(),
            sid: Some("GEN 1:3-4".into()),
            altnumber: None,
            pubnumber: None,
            span: 20..25,
        };
        assert_eq!(node.marker(), Some("v"));
        assert!(node.children().is_empty());
    }

    #[test]
    fn create_para_node() {
        let node = Node::Para {
            marker: "p".into(),
            content: vec![Node::text("Hello world")],
            span: 30..50,
        };
        assert_eq!(node.marker(), Some("p"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn create_char_node() {
        let node = Node::Char {
            marker: "nd".into(),
            content: vec![Node::text("Lord")],
            attributes: vec![],
            span: 40..55,
        };
        assert_eq!(node.marker(), Some("nd"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn create_note_node() {
        let node = Node::Note {
            marker: "f".into(),
            caller: "+".into(),
            category: None,
            content: vec![Node::text("A footnote.")],
            span: 60..80,
        };
        assert_eq!(node.marker(), Some("f"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn create_milestone_node() {
        let node = Node::Milestone {
            marker: "qt1-s".into(),
            attributes: vec![Attribute {
                key: "who".into(),
                value: "Jesus".into(),
            }],
            span: 90..100,
        };
        assert_eq!(node.marker(), Some("qt1-s"));
        assert!(node.children().is_empty());
    }

    #[test]
    fn create_figure_node() {
        let node = Node::Figure {
            marker: "fig".into(),
            content: vec![Node::text("Caption text")],
            attributes: vec![
                Attribute {
                    key: "src".into(),
                    value: "image.png".into(),
                },
                Attribute {
                    key: "alt".into(),
                    value: "Description".into(),
                },
            ],
            span: 100..130,
        };
        assert_eq!(node.marker(), Some("fig"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn create_sidebar_node() {
        let node = Node::Sidebar {
            marker: "esb".into(),
            category: None,
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("Sidebar content")],
                span: 140..160,
            }],
            span: 135..170,
        };
        assert_eq!(node.marker(), Some("esb"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn create_text_node() {
        let mut node = Node::text("In the beginning");
        assert_eq!(node.marker(), None);
        assert_eq!(node.span(), None);
        assert!(node.children().is_empty());
        assert!(node.children_mut().is_none());
    }

    #[test]
    fn create_unknown_node() {
        let node = Node::Unknown {
            marker: "zcustom".into(),
            content: vec![Node::text("custom data")],
            span: 200..220,
        };
        assert_eq!(node.marker(), Some("zcustom"));
        assert_eq!(node.children().len(), 1);
    }

    // -- Helper methods ------------------------------------------------------

    #[test]
    fn text_shorthand() {
        let node = Node::text("hello");
        assert_eq!(node, Node::Text("hello".into()));
    }

    #[test]
    fn children_returns_empty_for_text() {
        let node = Node::text("abc");
        assert!(node.children().is_empty());
    }

    #[test]
    fn children_mut_returns_some_for_para() {
        let mut node = Node::Para {
            marker: "p".into(),
            content: vec![],
            span: 0..1,
        };
        let kids = node.children_mut().unwrap();
        kids.push(Node::text("appended"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn children_mut_returns_none_for_verse() {
        let mut node = Node::Verse {
            marker: "v".into(),
            number: "1".into(),
            sid: None,
            altnumber: None,
            pubnumber: None,
            span: 0..3,
        };
        assert!(node.children_mut().is_none());
    }

    #[test]
    fn marker_returns_none_for_text() {
        assert_eq!(Node::text("x").marker(), None);
    }

    #[test]
    fn span_returns_none_for_text() {
        assert_eq!(Node::text("x").span(), None);
    }

    // -- Serialization (USJ-like JSON) ---------------------------------------

    #[test]
    fn serialize_document_to_usj() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![Node::text("Genesis")],
                    span: 0..20,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 20..25,
                },
                Node::Para {
                    marker: "p".into(),
                    content: vec![
                        Node::Verse {
                            marker: "v".into(),
                            number: "1".into(),
                            sid: Some("GEN 1:1".into()),
                            altnumber: None,
                            pubnumber: None,
                            span: 30..33,
                        },
                        Node::text("In the beginning God created the heavens and the earth."),
                    ],
                    span: 25..90,
                },
            ],
        };

        let value: Value = serde_json::to_value(&doc).unwrap();

        // Top-level structure
        assert!(value["content"].is_array());

        // Book node
        let book = &value["content"][0];
        assert_eq!(book["type"], "book");
        assert_eq!(book["marker"], "id");
        assert_eq!(book["code"], "GEN");
        assert_eq!(book["content"][0], "Genesis");

        // Chapter node
        let chapter = &value["content"][1];
        assert_eq!(chapter["type"], "chapter");
        assert_eq!(chapter["marker"], "c");
        assert_eq!(chapter["number"], "1");
        assert_eq!(chapter["sid"], "GEN 1");
        // span must NOT appear in JSON
        assert!(chapter.get("span").is_none());

        // Para node
        let para = &value["content"][2];
        assert_eq!(para["type"], "para");
        assert_eq!(para["marker"], "p");

        // Verse inside para
        let verse = &para["content"][0];
        assert_eq!(verse["type"], "verse");
        assert_eq!(verse["number"], "1");
        assert_eq!(verse["sid"], "GEN 1:1");

        // Text inside para (untagged -- bare string)
        let text = &para["content"][1];
        assert_eq!(
            text,
            "In the beginning God created the heavens and the earth."
        );
    }

    #[test]
    fn serialize_note_to_json() {
        let note = Node::Note {
            marker: "f".into(),
            caller: "+".into(),
            category: None,
            content: vec![
                Node::Char {
                    marker: "fr".into(),
                    content: vec![Node::text("1.1")],
                    attributes: vec![],
                    span: 0..5,
                },
                Node::Char {
                    marker: "ft".into(),
                    content: vec![Node::text("Some manuscripts read ...")],
                    attributes: vec![],
                    span: 5..30,
                },
            ],
            span: 0..35,
        };

        let value: Value = serde_json::to_value(&note).unwrap();
        assert_eq!(value["type"], "note");
        assert_eq!(value["marker"], "f");
        assert_eq!(value["caller"], "+");
        assert_eq!(value["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn serialize_milestone_to_json() {
        let ms = Node::Milestone {
            marker: "qt1-s".into(),
            attributes: vec![Attribute {
                key: "who".into(),
                value: "Jesus".into(),
            }],
            span: 0..10,
        };

        let value: Value = serde_json::to_value(&ms).unwrap();
        assert_eq!(value["type"], "ms");
        assert_eq!(value["marker"], "qt1-s");
        assert_eq!(value["attributes"][0]["key"], "who");
        assert_eq!(value["attributes"][0]["value"], "Jesus");
    }

    #[test]
    fn serialize_figure_to_json() {
        let fig = Node::Figure {
            marker: "fig".into(),
            content: vec![Node::text("A beautiful landscape")],
            attributes: vec![Attribute {
                key: "src".into(),
                value: "landscape.jpg".into(),
            }],
            span: 0..50,
        };

        let value: Value = serde_json::to_value(&fig).unwrap();
        assert_eq!(value["type"], "figure");
        assert_eq!(value["marker"], "fig");
        assert_eq!(value["content"][0], "A beautiful landscape");
        assert_eq!(value["attributes"][0]["key"], "src");
    }

    #[test]
    fn serialize_unknown_to_json() {
        let node = Node::Unknown {
            marker: "zcustom".into(),
            content: vec![Node::text("data")],
            span: 0..15,
        };

        let value: Value = serde_json::to_value(&node).unwrap();
        assert_eq!(value["type"], "unknown");
        assert_eq!(value["marker"], "zcustom");
        assert_eq!(value["content"][0], "data");
    }

    #[test]
    fn serialize_empty_content_omitted() {
        let para = Node::Para {
            marker: "b".into(),
            content: vec![],
            span: 0..3,
        };

        let value: Value = serde_json::to_value(&para).unwrap();
        assert_eq!(value["type"], "para");
        assert_eq!(value["marker"], "b");
        // Empty content should be omitted
        assert!(value.get("content").is_none());
    }

    #[test]
    fn serialize_optional_sid_omitted_when_none() {
        let verse = Node::Verse {
            marker: "v".into(),
            number: "1".into(),
            sid: None,
            altnumber: None,
            pubnumber: None,
            span: 0..3,
        };

        let value: Value = serde_json::to_value(&verse).unwrap();
        assert_eq!(value["type"], "verse");
        assert!(value.get("sid").is_none());
    }

    #[test]
    fn serialize_text_as_bare_string() {
        let text = Node::text("plain text");
        let value: Value = serde_json::to_value(&text).unwrap();
        assert_eq!(value, json!("plain text"));
    }

    #[test]
    fn serialize_char_to_json() {
        let node = Node::Char {
            marker: "nd".into(),
            content: vec![Node::text("Lord")],
            attributes: vec![],
            span: 0..10,
        };

        let value: Value = serde_json::to_value(&node).unwrap();
        assert_eq!(value["type"], "char");
        assert_eq!(value["marker"], "nd");
        assert_eq!(value["content"][0], "Lord");
    }

    #[test]
    fn serialize_sidebar_to_json() {
        let node = Node::Sidebar {
            marker: "esb".into(),
            category: None,
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("Sidebar text")],
                span: 5..20,
            }],
            span: 0..25,
        };

        let value: Value = serde_json::to_value(&node).unwrap();
        assert_eq!(value["type"], "sidebar");
        assert_eq!(value["marker"], "esb");
        assert_eq!(value["content"].as_array().unwrap().len(), 1);
    }

    // -- Span type -----------------------------------------------------------

    #[test]
    fn span_is_std_range() {
        // Confirm that Span is exactly std::ops::Range<usize>.
        let s: Span = 10..20;
        assert_eq!(s.start, 10);
        assert_eq!(s.end, 20);
        assert_eq!(s.len(), 10);
    }
}
