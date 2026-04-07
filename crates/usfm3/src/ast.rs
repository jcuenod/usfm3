use serde::Serialize;

use crate::markers::MarkerName;

/// The top-level semantic USFM document.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Document {
    pub content: Vec<Node>,
}

impl Document {
    /// Creates an empty document.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A semantic node in the USFM document tree.
///
/// This tree intentionally omits source spans; source-backed location data lives
/// in the parallel source-map tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Node {
    /// `\id` -- book identification.
    Book {
        marker: MarkerName,
        code: String,
        content: Vec<Node>,
    },

    /// `\c` -- chapter milestone (not a container).
    Chapter {
        marker: MarkerName,
        number: String,
        sid: Option<String>,
        altnumber: Option<String>,
        pubnumber: Option<String>,
    },

    /// `\v` -- verse milestone (not a container).
    Verse {
        marker: MarkerName,
        number: String,
        sid: Option<String>,
        altnumber: Option<String>,
        pubnumber: Option<String>,
    },

    /// `\p`, `\q1`, `\m`, `\li1`, `\b`, etc.
    Para {
        marker: MarkerName,
        content: Vec<Node>,
    },

    /// `\nd`, `\bk`, `\add`, `\it`, etc.
    Char {
        marker: MarkerName,
        content: Vec<Node>,
        attributes: Vec<Attribute>,
    },

    /// `\f`, `\x` -- footnote or cross-reference container.
    Note {
        marker: MarkerName,
        caller: String,
        category: Option<String>,
        content: Vec<Node>,
    },

    /// `\qt1-s`, `\qt1-e`, `\ts-s`, etc.
    Milestone {
        marker: MarkerName,
        attributes: Vec<Attribute>,
    },

    /// `\fig` -- figure with attributes.
    Figure {
        marker: MarkerName,
        content: Vec<Node>,
        attributes: Vec<Attribute>,
    },

    /// `\esb` ... `\esbe` -- sidebar container.
    Sidebar {
        marker: MarkerName,
        category: Option<String>,
        content: Vec<Node>,
    },

    /// `\periph` -- peripheral content section.
    Periph {
        alt: Option<String>,
        content: Vec<Node>,
        attributes: Vec<Attribute>,
    },

    /// Table container wrapping consecutive `\tr` rows.
    Table { content: Vec<Node> },

    /// `\tr` -- table row container.
    TableRow {
        marker: MarkerName,
        content: Vec<Node>,
    },

    /// `\th1`, `\tc2`, etc. -- table cell.
    TableCell {
        marker: MarkerName,
        align: String,
        content: Vec<Node>,
    },

    /// `\ref` -- scripture reference with target location.
    Ref {
        content: Vec<Node>,
        attributes: Vec<Attribute>,
    },

    /// Unrecognized marker (from `\z` namespace or genuinely unknown).
    Unknown {
        marker: MarkerName,
        content: Vec<Node>,
    },

    /// `//` -- optional line break.
    OptBreak,

    /// Plain text content.
    Text(String),
}

/// A key-value attribute (e.g. `|src="image.png"`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attribute {
    pub key: String,
    pub value: String,
}

impl Node {
    /// Shorthand for creating a text node.
    pub fn text(s: impl Into<String>) -> Self {
        Node::Text(s.into())
    }

    /// Returns the content/children slice.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_new_starts_empty() {
        let doc = Document::new();
        assert!(doc.content.is_empty());
    }

    #[test]
    fn text_helper_creates_text_node() {
        assert_eq!(Node::text("hello"), Node::Text("hello".into()));
    }

    #[test]
    fn marker_is_reported_for_structural_nodes() {
        let node = Node::Book {
            marker: "id".into(),
            code: "GEN".into(),
            content: vec![],
        };

        assert_eq!(node.marker(), Some("id"));
    }

    #[test]
    fn marker_is_none_for_non_marker_variants() {
        assert_eq!(Node::text("x").marker(), None);
        assert_eq!(Node::OptBreak.marker(), None);
        assert_eq!(
            Node::Periph {
                alt: None,
                content: vec![],
                attributes: vec![],
            }
            .marker(),
            None
        );
    }

    #[test]
    fn children_are_exposed_for_container_nodes() {
        let node = Node::Para {
            marker: "p".into(),
            content: vec![Node::text("Hello world")],
        };

        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn children_are_empty_for_leaf_like_nodes() {
        assert!(Node::text("abc").children().is_empty());
        assert!(Node::OptBreak.children().is_empty());
        assert!(
            Node::Verse {
                marker: "v".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            }
            .children()
            .is_empty()
        );
    }

    #[test]
    fn children_mut_is_available_for_containers() {
        let mut node = Node::Para {
            marker: "p".into(),
            content: vec![],
        };

        node.children_mut().unwrap().push(Node::text("appended"));
        assert_eq!(node.children().len(), 1);
    }

    #[test]
    fn children_mut_is_none_for_non_containers() {
        let mut verse = Node::Verse {
            marker: "v".into(),
            number: "1".into(),
            sid: None,
            altnumber: None,
            pubnumber: None,
        };

        assert!(verse.children_mut().is_none());
    }

    #[test]
    fn helper_methods_cover_all_container_variants() {
        let nodes = vec![
            Node::Book {
                marker: "id".into(),
                code: "GEN".into(),
                content: vec![Node::text("Genesis")],
            },
            Node::Char {
                marker: "nd".into(),
                content: vec![Node::text("Lord")],
                attributes: vec![],
            },
            Node::Note {
                marker: "f".into(),
                caller: "+".into(),
                category: None,
                content: vec![Node::text("Footnote")],
            },
            Node::Figure {
                marker: "fig".into(),
                content: vec![Node::text("Caption")],
                attributes: vec![],
            },
            Node::Sidebar {
                marker: "esb".into(),
                category: None,
                content: vec![Node::text("Sidebar")],
            },
            Node::Table {
                content: vec![Node::TableRow {
                    marker: "tr".into(),
                    content: vec![Node::TableCell {
                        marker: "tc1".into(),
                        align: "start".into(),
                        content: vec![Node::text("Cell")],
                    }],
                }],
            },
            Node::Ref {
                content: vec![Node::text("Gen 1:1")],
                attributes: vec![],
            },
            Node::Unknown {
                marker: "zcustom".into(),
                content: vec![Node::text("custom data")],
            },
        ];

        for node in nodes {
            assert!(!node.children().is_empty());
        }
    }
}
