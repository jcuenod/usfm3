use serde::Serialize;
use std::borrow::Cow;

use crate::markers::MarkerName;

/// The top-level semantic USFM document.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Document<'a> {
    pub content: Vec<Node<'a>>,
}

impl<'a> Document<'a> {
    /// Creates an empty document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Converts all borrowed data to owned, returning a `'static` document.
    pub fn into_owned(self) -> Document<'static> {
        Document {
            content: self.content.into_iter().map(|n| n.into_owned()).collect(),
        }
    }
}

/// A semantic node in the USFM document tree.
///
/// This tree intentionally omits source spans; source-backed location data lives
/// in the parallel source-map tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Node<'a> {
    /// `\id` -- book identification.
    Book {
        marker: MarkerName,
        code: Cow<'a, str>,
        content: Vec<Node<'a>>,
    },

    /// `\c` -- chapter milestone (not a container).
    Chapter(Box<ChapterData<'a>>),

    /// `\v` -- verse milestone (not a container).
    Verse(Box<VerseData<'a>>),

    /// `\p`, `\q1`, `\m`, `\li1`, `\b`, etc.
    Para {
        marker: MarkerName,
        content: Vec<Node<'a>>,
    },

    /// `\nd`, `\bk`, `\add`, `\it`, etc.
    Char(Box<CharData<'a>>),

    /// `\f`, `\x` -- footnote or cross-reference container.
    Note {
        marker: MarkerName,
        caller: Cow<'a, str>,
        category: Option<Cow<'a, str>>,
        content: Vec<Node<'a>>,
    },

    /// `\qt1-s`, `\qt1-e`, `\ts-s`, etc.
    Milestone {
        marker: MarkerName,
        attributes: Vec<Attribute<'a>>,
    },

    /// `\fig` -- figure with attributes.
    Figure {
        marker: MarkerName,
        content: Vec<Node<'a>>,
        attributes: Vec<Attribute<'a>>,
    },

    /// `\esb` ... `\esbe` -- sidebar container.
    Sidebar {
        marker: MarkerName,
        category: Option<Cow<'a, str>>,
        content: Vec<Node<'a>>,
    },

    /// `\periph` -- peripheral content section.
    Periph {
        alt: Option<Cow<'a, str>>,
        content: Vec<Node<'a>>,
        attributes: Vec<Attribute<'a>>,
    },

    /// Table container wrapping consecutive `\tr` rows.
    Table { content: Vec<Node<'a>> },

    /// `\tr` -- table row container.
    TableRow {
        marker: MarkerName,
        content: Vec<Node<'a>>,
    },

    /// `\th1`, `\tc2`, etc. -- table cell.
    TableCell {
        marker: MarkerName,
        align: Cow<'a, str>,
        content: Vec<Node<'a>>,
    },

    /// `\ref` -- scripture reference with target location.
    Ref {
        content: Vec<Node<'a>>,
        attributes: Vec<Attribute<'a>>,
    },

    /// Unrecognized marker (from `\z` namespace or genuinely unknown).
    Unknown {
        marker: MarkerName,
        content: Vec<Node<'a>>,
    },

    /// `//` -- optional line break.
    OptBreak,

    /// Plain text content.
    Text(Cow<'a, str>),
}

/// Information attached to a chapter marker (`\c`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChapterData<'a> {
    pub marker: MarkerName,
    pub number: Cow<'a, str>,
    pub sid: Option<Cow<'a, str>>,
    pub altnumber: Option<Cow<'a, str>>,
    pub pubnumber: Option<Cow<'a, str>>,
}

/// Information attached to a verse marker (`\v`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerseData<'a> {
    pub marker: MarkerName,
    pub number: Cow<'a, str>,
    pub sid: Option<Cow<'a, str>>,
    pub altnumber: Option<Cow<'a, str>>,
    pub pubnumber: Option<Cow<'a, str>>,
}

/// Information attached to a character-style marker (`\nd`, `\add`, etc.).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharData<'a> {
    pub marker: MarkerName,
    pub content: Vec<Node<'a>>,
    pub attributes: Vec<Attribute<'a>>,
}

/// A key-value attribute (e.g. `|src="image.png"`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attribute<'a> {
    pub key: Cow<'a, str>,
    pub value: Cow<'a, str>,
}

impl<'a> ChapterData<'a> {
    pub fn into_owned(self) -> ChapterData<'static> {
        ChapterData {
            marker: self.marker,
            number: Cow::Owned(self.number.into_owned()),
            sid: self.sid.map(|s| Cow::Owned(s.into_owned())),
            altnumber: self.altnumber.map(|s| Cow::Owned(s.into_owned())),
            pubnumber: self.pubnumber.map(|s| Cow::Owned(s.into_owned())),
        }
    }
}

impl<'a> VerseData<'a> {
    pub fn into_owned(self) -> VerseData<'static> {
        VerseData {
            marker: self.marker,
            number: Cow::Owned(self.number.into_owned()),
            sid: self.sid.map(|s| Cow::Owned(s.into_owned())),
            altnumber: self.altnumber.map(|s| Cow::Owned(s.into_owned())),
            pubnumber: self.pubnumber.map(|s| Cow::Owned(s.into_owned())),
        }
    }
}

impl<'a> CharData<'a> {
    pub fn into_owned(self) -> CharData<'static> {
        CharData {
            marker: self.marker,
            content: self.content.into_iter().map(|n| n.into_owned()).collect(),
            attributes: self
                .attributes
                .into_iter()
                .map(|a| a.into_owned())
                .collect(),
        }
    }
}

impl<'a> Attribute<'a> {
    pub fn into_owned(self) -> Attribute<'static> {
        Attribute {
            key: Cow::Owned(self.key.into_owned()),
            value: Cow::Owned(self.value.into_owned()),
        }
    }
}

impl<'a> Node<'a> {
    /// Shorthand for creating a text node.
    pub fn text(s: impl Into<Cow<'a, str>>) -> Self {
        Node::Text(s.into())
    }

    /// Converts all borrowed data to owned, returning a `'static` node.
    pub fn into_owned(self) -> Node<'static> {
        match self {
            Node::Book {
                marker,
                code,
                content,
            } => Node::Book {
                marker,
                code: Cow::Owned(code.into_owned()),
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::Chapter(data) => Node::Chapter(Box::new(data.into_owned())),
            Node::Verse(data) => Node::Verse(Box::new(data.into_owned())),
            Node::Para { marker, content } => Node::Para {
                marker,
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::Char(data) => Node::Char(Box::new(data.into_owned())),
            Node::Note {
                marker,
                caller,
                category,
                content,
            } => Node::Note {
                marker,
                caller: Cow::Owned(caller.into_owned()),
                category: category.map(|c| Cow::Owned(c.into_owned())),
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::Milestone { marker, attributes } => Node::Milestone {
                marker,
                attributes: attributes.into_iter().map(|a| a.into_owned()).collect(),
            },
            Node::Figure {
                marker,
                content,
                attributes,
            } => Node::Figure {
                marker,
                content: content.into_iter().map(|n| n.into_owned()).collect(),
                attributes: attributes.into_iter().map(|a| a.into_owned()).collect(),
            },
            Node::Sidebar {
                marker,
                category,
                content,
            } => Node::Sidebar {
                marker,
                category: category.map(|c| Cow::Owned(c.into_owned())),
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::Periph {
                alt,
                content,
                attributes,
            } => Node::Periph {
                alt: alt.map(|a| Cow::Owned(a.into_owned())),
                content: content.into_iter().map(|n| n.into_owned()).collect(),
                attributes: attributes.into_iter().map(|a| a.into_owned()).collect(),
            },
            Node::Table { content } => Node::Table {
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::TableRow { marker, content } => Node::TableRow {
                marker,
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::TableCell {
                marker,
                align,
                content,
            } => Node::TableCell {
                marker,
                align: Cow::Owned(align.into_owned()),
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::Ref {
                content,
                attributes,
            } => Node::Ref {
                content: content.into_iter().map(|n| n.into_owned()).collect(),
                attributes: attributes.into_iter().map(|a| a.into_owned()).collect(),
            },
            Node::Unknown { marker, content } => Node::Unknown {
                marker,
                content: content.into_iter().map(|n| n.into_owned()).collect(),
            },
            Node::OptBreak => Node::OptBreak,
            Node::Text(t) => Node::Text(Cow::Owned(t.into_owned())),
        }
    }

    /// Returns the content/children slice.
    pub fn children(&self) -> &[Node<'a>] {
        match self {
            Node::Book { content, .. }
            | Node::Para { content, .. }
            | Node::Note { content, .. }
            | Node::Figure { content, .. }
            | Node::Sidebar { content, .. }
            | Node::Periph { content, .. }
            | Node::Table { content, .. }
            | Node::TableRow { content, .. }
            | Node::TableCell { content, .. }
            | Node::Ref { content, .. }
            | Node::Unknown { content, .. } => content,
            Node::Char(data) => &data.content,
            Node::Chapter(_)
            | Node::Verse(_)
            | Node::Milestone { .. }
            | Node::OptBreak
            | Node::Text(_) => &[],
        }
    }

    /// Returns a mutable reference to the children vector, if the variant has one.
    pub fn children_mut(&mut self) -> Option<&mut Vec<Node<'a>>> {
        match self {
            Node::Book { content, .. }
            | Node::Para { content, .. }
            | Node::Note { content, .. }
            | Node::Figure { content, .. }
            | Node::Sidebar { content, .. }
            | Node::Periph { content, .. }
            | Node::Table { content, .. }
            | Node::TableRow { content, .. }
            | Node::TableCell { content, .. }
            | Node::Ref { content, .. }
            | Node::Unknown { content, .. } => Some(content),
            Node::Char(data) => Some(&mut data.content),
            Node::Chapter(_)
            | Node::Verse(_)
            | Node::Milestone { .. }
            | Node::OptBreak
            | Node::Text(_) => None,
        }
    }

    /// Returns the marker string if the node has one.
    pub fn marker(&self) -> Option<MarkerName> {
        match self {
            Node::Book { marker, .. }
            | Node::Para { marker, .. }
            | Node::Note { marker, .. }
            | Node::Milestone { marker, .. }
            | Node::Figure { marker, .. }
            | Node::Sidebar { marker, .. }
            | Node::TableRow { marker, .. }
            | Node::TableCell { marker, .. }
            | Node::Unknown { marker, .. } => Some(*marker),
            Node::Chapter(data) => Some(data.marker),
            Node::Verse(data) => Some(data.marker),
            Node::Char(data) => Some(data.marker),
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
            code: Cow::Borrowed("GEN"),
            content: vec![],
        };

        assert_eq!(node.marker().as_deref(), Some("id"));
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
            Node::Verse(Box::new(VerseData {
                marker: "v".into(),
                number: "1".into(),
                sid: None,
                altnumber: None,
                pubnumber: None,
            }))
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
        let mut verse = Node::Verse(Box::new(VerseData {
            marker: "v".into(),
            number: "1".into(),
            sid: None,
            altnumber: None,
            pubnumber: None,
        }));

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
            Node::Char(Box::new(CharData {
                marker: "nd".into(),
                content: vec![Node::text("Lord")],
                attributes: vec![],
            })),
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
