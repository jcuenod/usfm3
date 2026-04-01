use serde::Serialize;

use crate::markers::MarkerName;

/// Byte-offset range in the original USFM source.
pub type Span = std::ops::Range<usize>;

// ---------------------------------------------------------------------------
// Node metadata
// ---------------------------------------------------------------------------

/// Source-location metadata attached to a structural node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct NodeSpans {
    pub node: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<Span>,
}

impl NodeSpans {
    /// Create metadata with only the primary node span populated.
    pub fn node(span: Span) -> Self {
        Self {
            node: span,
            code: None,
            number: None,
            close: None,
        }
    }

    /// Attach a book-code span.
    pub fn with_code(mut self, span: Span) -> Self {
        self.code = Some(span);
        self
    }

    /// Attach a chapter/verse-number span.
    pub fn with_number(mut self, span: Span) -> Self {
        self.number = Some(span);
        self
    }

    /// Attach an explicit close-marker span.
    pub fn with_close(mut self, span: Span) -> Self {
        self.close = Some(span);
        self
    }
}

// ---------------------------------------------------------------------------
// Top-level document
// ---------------------------------------------------------------------------

/// The top-level document.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// `\id` -- book identification.
    Book {
        marker: MarkerName,
        code: String,
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `\c` -- chapter milestone (NOT a container).
    Chapter {
        marker: MarkerName,
        number: String,
        sid: Option<String>,
        altnumber: Option<String>,
        pubnumber: Option<String>,
        spans: NodeSpans,
    },

    /// `\v` -- verse milestone (NOT a container).
    Verse {
        marker: MarkerName,
        number: String,
        sid: Option<String>,
        altnumber: Option<String>,
        pubnumber: Option<String>,
        spans: NodeSpans,
    },

    /// `\p`, `\q1`, `\m`, `\li1`, `\b`, etc. -- paragraph-level container.
    Para {
        marker: MarkerName,
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `\nd`, `\bk`, `\add`, `\it`, etc. -- character-level container.
    Char {
        marker: MarkerName,
        content: Vec<Node>,
        attributes: Vec<Attribute>,
        spans: NodeSpans,
    },

    /// `\f`, `\x` -- footnote or cross-reference container.
    Note {
        marker: MarkerName,
        caller: String,
        category: Option<String>,
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `\qt1-s`, `\qt1-e`, `\ts-s`, etc. -- milestone (empty element with attributes).
    Milestone {
        marker: MarkerName,
        attributes: Vec<Attribute>,
        spans: NodeSpans,
    },

    /// `\fig` -- figure with attributes.
    Figure {
        marker: MarkerName,
        content: Vec<Node>,
        attributes: Vec<Attribute>,
        spans: NodeSpans,
    },

    /// `\esb` ... `\esbe` -- sidebar container.
    Sidebar {
        marker: MarkerName,
        category: Option<String>,
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `\periph` -- peripheral content section.
    Periph {
        alt: Option<String>,
        content: Vec<Node>,
        attributes: Vec<Attribute>,
        spans: NodeSpans,
    },

    /// Table container wrapping consecutive `\tr` rows.
    Table {
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `\tr` -- table row container.
    TableRow {
        marker: MarkerName,
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `\th1`, `\tc2`, etc. -- table cell.
    TableCell {
        marker: MarkerName,
        align: String,
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `\ref` -- scripture reference with target location.
    Ref {
        content: Vec<Node>,
        attributes: Vec<Attribute>,
        spans: NodeSpans,
    },

    /// Unrecognized marker (from `\z` namespace or genuinely unknown).
    /// Preserved so no data is lost.
    Unknown {
        marker: MarkerName,
        content: Vec<Node>,
        spans: NodeSpans,
    },

    /// `//` -- optional line break.
    OptBreak,

    /// Plain text content.
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
            | Node::Ref { content, .. }
            | Node::Unknown { content, .. } => content,
            Node::Chapter { .. }
            | Node::Verse { .. }
            | Node::Milestone { .. }
            | Node::OptBreak
            | Node::Text(_) => &[],
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
            | Node::Ref { content, .. }
            | Node::Unknown { content, .. } => Some(content),
            Node::Chapter { .. }
            | Node::Verse { .. }
            | Node::Milestone { .. }
            | Node::OptBreak
            | Node::Text(_) => None,
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
            | Node::Unknown { marker, .. } => Some(marker.as_str()),
            Node::Table { .. }
            | Node::Periph { .. }
            | Node::Ref { .. }
            | Node::OptBreak
            | Node::Text(_) => None,
        }
    }

    /// Returns a reference to the source span if the node has one.
    ///
    /// Returns `None` for [`Node::Text`].
    pub fn span(&self) -> Option<&Span> {
        self.spans().map(|spans| &spans.node)
    }

    /// Returns the full source metadata if the node has it.
    pub fn spans(&self) -> Option<&NodeSpans> {
        match self {
            Node::Book { spans, .. }
            | Node::Chapter { spans, .. }
            | Node::Verse { spans, .. }
            | Node::Para { spans, .. }
            | Node::Char { spans, .. }
            | Node::Note { spans, .. }
            | Node::Milestone { spans, .. }
            | Node::Figure { spans, .. }
            | Node::Sidebar { spans, .. }
            | Node::Periph { spans, .. }
            | Node::Table { spans, .. }
            | Node::TableRow { spans, .. }
            | Node::TableCell { spans, .. }
            | Node::Ref { spans, .. }
            | Node::Unknown { spans, .. } => Some(spans),
            Node::OptBreak | Node::Text(_) => None,
        }
    }

    /// Returns mutable source metadata if the node has it.
    pub fn spans_mut(&mut self) -> Option<&mut NodeSpans> {
        match self {
            Node::Book { spans, .. }
            | Node::Chapter { spans, .. }
            | Node::Verse { spans, .. }
            | Node::Para { spans, .. }
            | Node::Char { spans, .. }
            | Node::Note { spans, .. }
            | Node::Milestone { spans, .. }
            | Node::Figure { spans, .. }
            | Node::Sidebar { spans, .. }
            | Node::Periph { spans, .. }
            | Node::Table { spans, .. }
            | Node::TableRow { spans, .. }
            | Node::TableCell { spans, .. }
            | Node::Ref { spans, .. }
            | Node::Unknown { spans, .. } => Some(spans),
            Node::OptBreak | Node::Text(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usj::{UsjOptions, to_usj_value, to_usj_value_with_options};
    use serde_json::{Value, json};

    fn node_value(node: Node) -> Value {
        to_usj_value(&Document {
            content: vec![node],
        })
        .unwrap()["content"][0]
            .clone()
    }

    fn node_value_with_spans(node: Node) -> Value {
        to_usj_value_with_options(
            &Document {
                content: vec![node],
            },
            UsjOptions {
                include_spans: true,
            },
        )
        .unwrap()["content"][0]
            .clone()
    }

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
            spans: NodeSpans::node(0..5).with_code(0..3),
        };
        assert_eq!(node.marker(), Some("id"));
        assert_eq!(node.span(), Some(&(0..5)));
        assert!(node.children().is_empty());
        assert_eq!(node.spans().unwrap().code, Some(0..3));
    }

    #[test]
    fn create_chapter_node() {
        let mut node = Node::Chapter {
            marker: "c".into(),
            number: "1".into(),
            sid: Some("GEN 1".into()),
            altnumber: None,
            pubnumber: None,
            spans: NodeSpans::node(10..14).with_number(12..13),
        };
        assert_eq!(node.marker(), Some("c"));
        assert_eq!(node.span(), Some(&(10..14)));
        assert!(node.children().is_empty());
        assert!(node.children_mut().is_none());
        assert_eq!(node.spans().unwrap().number, Some(12..13));
    }

    #[test]
    fn create_verse_node() {
        let node = Node::Verse {
            marker: "v".into(),
            number: "3-4".into(),
            sid: Some("GEN 1:3-4".into()),
            altnumber: None,
            pubnumber: None,
            spans: NodeSpans::node(20..25).with_number(22..25),
        };
        assert_eq!(node.marker(), Some("v"));
        assert!(node.children().is_empty());
        assert_eq!(node.spans().unwrap().number, Some(22..25));
    }

    #[test]
    fn create_para_node() {
        let node = Node::Para {
            marker: "p".into(),
            content: vec![Node::text("Hello world")],
            spans: NodeSpans::node(30..50),
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
            spans: NodeSpans::node(40..55).with_close(53..55),
        };
        assert_eq!(node.marker(), Some("nd"));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.spans().unwrap().close, Some(53..55));
    }

    #[test]
    fn create_note_node() {
        let node = Node::Note {
            marker: "f".into(),
            caller: "+".into(),
            category: None,
            content: vec![Node::text("A footnote.")],
            spans: NodeSpans::node(60..80).with_close(78..80),
        };
        assert_eq!(node.marker(), Some("f"));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.spans().unwrap().close, Some(78..80));
    }

    #[test]
    fn create_milestone_node() {
        let node = Node::Milestone {
            marker: "qt1-s".into(),
            attributes: vec![Attribute {
                key: "who".into(),
                value: "Jesus".into(),
            }],
            spans: NodeSpans::node(90..100),
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
            spans: NodeSpans::node(100..130).with_close(125..130),
        };
        assert_eq!(node.marker(), Some("fig"));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.spans().unwrap().close, Some(125..130));
    }

    #[test]
    fn create_sidebar_node() {
        let node = Node::Sidebar {
            marker: "esb".into(),
            category: None,
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("Sidebar content")],
                spans: NodeSpans::node(140..160),
            }],
            spans: NodeSpans::node(135..170).with_close(166..170),
        };
        assert_eq!(node.marker(), Some("esb"));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.spans().unwrap().close, Some(166..170));
    }

    #[test]
    fn create_text_node() {
        let node = Node::text("In the beginning");
        assert_eq!(node.marker(), None);
        assert_eq!(node.span(), None);
        assert!(node.children().is_empty());
        assert!(matches!(node, Node::Text(_)));
    }

    #[test]
    fn create_unknown_node() {
        let node = Node::Unknown {
            marker: "zcustom".into(),
            content: vec![Node::text("custom data")],
            spans: NodeSpans::node(200..220).with_close(218..220),
        };
        assert_eq!(node.marker(), Some("zcustom"));
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.spans().unwrap().close, Some(218..220));
    }

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
            spans: NodeSpans::node(0..1),
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
            spans: NodeSpans::node(0..3).with_number(2..3),
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

    #[test]
    fn node_spans_builders_attach_optional_metadata() {
        let spans = NodeSpans::node(10..20)
            .with_code(11..14)
            .with_number(15..16)
            .with_close(18..20);

        assert_eq!(spans.node, 10..20);
        assert_eq!(spans.code, Some(11..14));
        assert_eq!(spans.number, Some(15..16));
        assert_eq!(spans.close, Some(18..20));
    }

    #[test]
    fn spans_mut_allows_updating_subspans() {
        let mut node = Node::Sidebar {
            marker: "esb".into(),
            category: None,
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("Sidebar")],
                spans: NodeSpans::node(5..15),
            }],
            spans: NodeSpans::node(0..20),
        };

        node.spans_mut().unwrap().close = Some(18..20);

        assert_eq!(node.spans().unwrap().close, Some(18..20));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn serialize_document_to_usj() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![Node::text("Genesis")],
                    spans: NodeSpans::node(0..20).with_code(0..3),
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    spans: NodeSpans::node(20..25).with_number(22..23),
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
                            spans: NodeSpans::node(30..33).with_number(32..33),
                        },
                        Node::text("In the beginning God created the heavens and the earth."),
                    ],
                    spans: NodeSpans::node(25..90),
                },
            ],
        };

        let value = to_usj_value(&doc).unwrap();

        assert!(value["content"].is_array());
        let book = &value["content"][0];
        assert_eq!(book["type"], "book");
        assert_eq!(book["marker"], "id");
        assert_eq!(book["code"], "GEN");
        assert_eq!(book["content"][0], "Genesis");

        let chapter = &value["content"][1];
        assert_eq!(chapter["type"], "chapter");
        assert_eq!(chapter["marker"], "c");
        assert_eq!(chapter["number"], "1");
        assert_eq!(chapter["sid"], "GEN 1");
        assert!(chapter.get("spans").is_none());

        let para = &value["content"][2];
        assert_eq!(para["type"], "para");
        assert_eq!(para["marker"], "p");

        let verse = &para["content"][0];
        assert_eq!(verse["type"], "verse");
        assert_eq!(verse["number"], "1");
        assert_eq!(verse["sid"], "GEN 1:1");

        let text = &para["content"][1];
        assert_eq!(
            text,
            "In the beginning God created the heavens and the earth."
        );
    }

    #[test]
    fn serialize_document_with_spans() {
        let value = node_value_with_spans(Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
            spans: NodeSpans::node(0..10).with_code(0..3),
        });

        let spans = &value["spans"];
        assert_eq!(spans["node"]["start"], 0);
        assert_eq!(spans["node"]["end"], 10);
        assert_eq!(spans["code"]["start"], 0);
        assert_eq!(spans["code"]["end"], 3);
        assert!(spans.get("number").is_none());
    }

    #[test]
    fn serialize_note_to_json() {
        let value = node_value(Node::Note {
            marker: "f".into(),
            caller: "+".into(),
            category: None,
            content: vec![
                Node::Char {
                    marker: "fr".into(),
                    content: vec![Node::text("1.1")],
                    attributes: vec![],
                    spans: NodeSpans::node(0..5),
                },
                Node::Char {
                    marker: "ft".into(),
                    content: vec![Node::text("Some manuscripts read ...")],
                    attributes: vec![],
                    spans: NodeSpans::node(5..30),
                },
            ],
            spans: NodeSpans::node(0..35),
        });

        assert_eq!(value["type"], "note");
        assert_eq!(value["marker"], "f");
        assert_eq!(value["caller"], "+");
        assert_eq!(value["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn serialize_milestone_to_json() {
        let value = node_value(Node::Milestone {
            marker: "qt1-s".into(),
            attributes: vec![Attribute {
                key: "who".into(),
                value: "Jesus".into(),
            }],
            spans: NodeSpans::node(0..10),
        });

        assert_eq!(value["type"], "ms");
        assert_eq!(value["marker"], "qt1-s");
        assert_eq!(value["attributes"][0]["key"], "who");
        assert_eq!(value["attributes"][0]["value"], "Jesus");
    }

    #[test]
    fn serialize_figure_to_json() {
        let value = node_value(Node::Figure {
            marker: "fig".into(),
            content: vec![Node::text("A beautiful landscape")],
            attributes: vec![Attribute {
                key: "src".into(),
                value: "landscape.jpg".into(),
            }],
            spans: NodeSpans::node(0..50),
        });

        assert_eq!(value["type"], "figure");
        assert_eq!(value["marker"], "fig");
        assert_eq!(value["content"][0], "A beautiful landscape");
        assert_eq!(value["attributes"][0]["key"], "src");
    }

    #[test]
    fn serialize_unknown_to_json() {
        let value = node_value(Node::Unknown {
            marker: "zcustom".into(),
            content: vec![Node::text("data")],
            spans: NodeSpans::node(0..15),
        });

        assert_eq!(value["type"], "unknown");
        assert_eq!(value["marker"], "zcustom");
        assert_eq!(value["content"][0], "data");
    }

    #[test]
    fn serialize_empty_content_omitted() {
        let value = node_value(Node::Para {
            marker: "b".into(),
            content: vec![],
            spans: NodeSpans::node(0..3),
        });

        assert_eq!(value["type"], "para");
        assert_eq!(value["marker"], "b");
        assert!(value.get("content").is_none());
    }

    #[test]
    fn serialize_optional_sid_omitted_when_none() {
        let value = node_value(Node::Verse {
            marker: "v".into(),
            number: "1".into(),
            sid: None,
            altnumber: None,
            pubnumber: None,
            spans: NodeSpans::node(0..3).with_number(2..3),
        });

        assert_eq!(value["type"], "verse");
        assert!(value.get("sid").is_none());
    }

    #[test]
    fn serialize_text_as_bare_string() {
        let value = to_usj_value(&Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("plain text")],
                spans: NodeSpans::node(0..10),
            }],
        })
        .unwrap();
        assert_eq!(value["content"][0]["content"][0], json!("plain text"));
    }

    #[test]
    fn serialize_char_to_json() {
        let value = node_value(Node::Char {
            marker: "nd".into(),
            content: vec![Node::text("Lord")],
            attributes: vec![],
            spans: NodeSpans::node(0..10),
        });

        assert_eq!(value["type"], "char");
        assert_eq!(value["marker"], "nd");
        assert_eq!(value["content"][0], "Lord");
    }

    #[test]
    fn serialize_sidebar_to_json() {
        let value = node_value(Node::Sidebar {
            marker: "esb".into(),
            category: None,
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("Sidebar text")],
                spans: NodeSpans::node(5..20),
            }],
            spans: NodeSpans::node(0..25),
        });

        assert_eq!(value["type"], "sidebar");
        assert_eq!(value["marker"], "esb");
        assert_eq!(value["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn span_is_std_range() {
        let s: Span = 10..20;
        assert_eq!(s.start, 10);
        assert_eq!(s.end, 20);
        assert_eq!(s.len(), 10);
    }
}
