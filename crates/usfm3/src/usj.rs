use crate::ast::{Document, Node, NodeSpans};
use serde::Serialize;
use serde::ser::{SerializeMap, SerializeSeq, Serializer};

/// Options controlling USJ serialization.
#[derive(Debug, Clone, Copy, Default)]
pub struct UsjOptions {
    pub include_spans: bool,
}

struct UsjDocumentView<'a> {
    doc: &'a Document,
    options: UsjOptions,
}

impl<'a> UsjDocumentView<'a> {
    fn new(doc: &'a Document, options: UsjOptions) -> Self {
        Self { doc, options }
    }
}

impl Serialize for UsjDocumentView<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("type", "USJ")?;
        map.serialize_entry("version", "3.1")?;
        map.serialize_entry(
            "content",
            &NodeListView {
                nodes: &self.doc.content,
                options: self.options,
            },
        )?;
        map.end()
    }
}

struct NodeListView<'a> {
    nodes: &'a [Node],
    options: UsjOptions,
}

impl Serialize for NodeListView<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.nodes.len()))?;
        for node in self.nodes {
            seq.serialize_element(&NodeView {
                node,
                options: self.options,
            })?;
        }
        seq.end()
    }
}

struct NodeView<'a> {
    node: &'a Node,
    options: UsjOptions,
}

impl NodeView<'_> {
    fn serialize_spans<S>(&self, map: &mut S, spans: &NodeSpans) -> Result<(), S::Error>
    where
        S: SerializeMap,
    {
        if self.options.include_spans {
            map.serialize_entry("spans", spans)?;
        }
        Ok(())
    }

    fn serialize_content<S>(&self, map: &mut S, content: &[Node]) -> Result<(), S::Error>
    where
        S: SerializeMap,
    {
        if !content.is_empty() {
            map.serialize_entry(
                "content",
                &NodeListView {
                    nodes: content,
                    options: self.options,
                },
            )?;
        }
        Ok(())
    }
}

impl Serialize for NodeView<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.node {
            Node::Text(text) => serializer.serialize_str(text),
            Node::OptBreak => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "optbreak")?;
                map.end()
            }
            Node::Book {
                marker,
                code,
                content,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "book")?;
                map.serialize_entry("marker", marker)?;
                map.serialize_entry("code", code)?;
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Chapter {
                marker,
                number,
                sid,
                altnumber,
                pubnumber,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "chapter")?;
                map.serialize_entry("marker", marker)?;
                map.serialize_entry("number", number)?;
                if let Some(sid) = sid {
                    map.serialize_entry("sid", sid)?;
                }
                if let Some(altnumber) = altnumber {
                    map.serialize_entry("altnumber", altnumber)?;
                }
                if let Some(pubnumber) = pubnumber {
                    map.serialize_entry("pubnumber", pubnumber)?;
                }
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Verse {
                marker,
                number,
                sid,
                altnumber,
                pubnumber,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "verse")?;
                map.serialize_entry("marker", marker)?;
                map.serialize_entry("number", number)?;
                if let Some(sid) = sid {
                    map.serialize_entry("sid", sid)?;
                }
                if let Some(altnumber) = altnumber {
                    map.serialize_entry("altnumber", altnumber)?;
                }
                if let Some(pubnumber) = pubnumber {
                    map.serialize_entry("pubnumber", pubnumber)?;
                }
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Para {
                marker,
                content,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "para")?;
                map.serialize_entry("marker", marker)?;
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Char {
                marker,
                content,
                attributes,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "char")?;
                map.serialize_entry("marker", marker)?;
                self.serialize_content(&mut map, content)?;
                if !attributes.is_empty() {
                    map.serialize_entry("attributes", attributes)?;
                }
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Note {
                marker,
                caller,
                category,
                content,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "note")?;
                map.serialize_entry("marker", marker)?;
                map.serialize_entry("caller", caller)?;
                if let Some(category) = category {
                    map.serialize_entry("category", category)?;
                }
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Milestone {
                marker,
                attributes,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "ms")?;
                map.serialize_entry("marker", marker)?;
                if !attributes.is_empty() {
                    map.serialize_entry("attributes", attributes)?;
                }
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Figure {
                marker,
                content,
                attributes,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "figure")?;
                map.serialize_entry("marker", marker)?;
                self.serialize_content(&mut map, content)?;
                if !attributes.is_empty() {
                    map.serialize_entry("attributes", attributes)?;
                }
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Sidebar {
                marker,
                category,
                content,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "sidebar")?;
                map.serialize_entry("marker", marker)?;
                if let Some(category) = category {
                    map.serialize_entry("category", category)?;
                }
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Periph {
                alt,
                content,
                attributes,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "periph")?;
                if let Some(alt) = alt {
                    map.serialize_entry("alt", alt)?;
                }
                self.serialize_content(&mut map, content)?;
                if !attributes.is_empty() {
                    map.serialize_entry("attributes", attributes)?;
                }
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Table { content, spans } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "table")?;
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::TableRow {
                marker,
                content,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "table:row")?;
                map.serialize_entry("marker", marker)?;
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::TableCell {
                marker,
                align,
                content,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "table:cell")?;
                map.serialize_entry("marker", marker)?;
                map.serialize_entry("align", align)?;
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Ref {
                content,
                attributes,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "ref")?;
                self.serialize_content(&mut map, content)?;
                if !attributes.is_empty() {
                    map.serialize_entry("attributes", attributes)?;
                }
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
            Node::Unknown {
                marker,
                content,
                spans,
            } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("type", "unknown")?;
                map.serialize_entry("marker", marker)?;
                self.serialize_content(&mut map, content)?;
                self.serialize_spans(&mut map, spans)?;
                map.end()
            }
        }
    }
}

/// Serialize a Document to a USJ JSON string.
pub fn to_usj_string(doc: &Document) -> Result<String, serde_json::Error> {
    to_usj_string_with_options(doc, UsjOptions::default())
}

/// Serialize a Document to a USJ JSON string with explicit options.
pub fn to_usj_string_with_options(
    doc: &Document,
    options: UsjOptions,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&UsjDocumentView::new(doc, options))
}

/// Serialize a Document to a pretty-printed USJ JSON string.
pub fn to_usj_string_pretty(doc: &Document) -> Result<String, serde_json::Error> {
    to_usj_string_pretty_with_options(doc, UsjOptions::default())
}

/// Serialize a Document to a pretty-printed USJ JSON string with explicit options.
pub fn to_usj_string_pretty_with_options(
    doc: &Document,
    options: UsjOptions,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&UsjDocumentView::new(doc, options))
}

/// Serialize a Document to a serde_json::Value.
pub fn to_usj_value(doc: &Document) -> Result<serde_json::Value, serde_json::Error> {
    to_usj_value_with_options(doc, UsjOptions::default())
}

/// Serialize a Document to a serde_json::Value with explicit options.
pub fn to_usj_value_with_options(
    doc: &Document,
    options: UsjOptions,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(UsjDocumentView::new(doc, options))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use serde_json::json;

    fn sample_document() -> Document {
        Document {
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
        }
    }

    #[test]
    fn test_usj_string() {
        let doc = sample_document();
        let json_str = to_usj_string(&doc).unwrap();
        assert!(json_str.contains("\"type\":\"USJ\""));
        assert!(json_str.contains("\"version\":\"3.1\""));
        assert!(json_str.contains("\"type\":\"book\""));
        assert!(json_str.contains("\"code\":\"GEN\""));
    }

    #[test]
    fn test_usj_pretty() {
        let doc = sample_document();
        let json_str = to_usj_string_pretty(&doc).unwrap();
        assert!(json_str.contains('\n'));
        assert!(json_str.contains("\"type\": \"USJ\""));
    }

    #[test]
    fn test_usj_value() {
        let doc = sample_document();
        let value = to_usj_value(&doc).unwrap();
        assert_eq!(value["type"], "USJ");
        assert_eq!(value["version"], "3.1");
        assert!(value["content"].is_array());
        assert_eq!(value["content"][0]["type"], "book");
        assert_eq!(value["content"][0]["code"], "GEN");
        assert_eq!(value["content"][0]["content"][0], "Genesis");
        assert_eq!(value["content"][1]["type"], "chapter");
        assert_eq!(value["content"][1]["number"], "1");
        assert_eq!(value["content"][2]["type"], "para");
        assert_eq!(value["content"][2]["content"][0]["type"], "verse");
        assert_eq!(
            value["content"][2]["content"][1],
            "In the beginning God created the heavens and the earth."
        );
    }

    #[test]
    fn test_usj_no_spans() {
        let doc = sample_document();
        let json_str = to_usj_string(&doc).unwrap();
        assert!(!json_str.contains("\"spans\""));
    }

    #[test]
    fn test_usj_with_spans() {
        let doc = sample_document();
        let value = to_usj_value_with_options(
            &doc,
            UsjOptions {
                include_spans: true,
            },
        )
        .unwrap();
        let spans = &value["content"][0]["spans"];
        assert_eq!(spans["node"]["start"], 0);
        assert_eq!(spans["node"]["end"], 20);
        assert_eq!(spans["code"]["start"], 0);
        assert_eq!(spans["code"]["end"], 3);
        assert!(value["content"][2]["content"][1].is_string());
    }

    #[test]
    fn test_usj_empty_document() {
        let doc = Document::new();
        let value = to_usj_value(&doc).unwrap();
        assert_eq!(value["type"], "USJ");
        assert_eq!(value["content"], json!([]));
    }

    #[test]
    fn test_usj_text_is_bare_string() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("hello world")],
                spans: NodeSpans::node(0..20),
            }],
        };
        let value = to_usj_value(&doc).unwrap();
        assert_eq!(value["content"][0]["content"][0], "hello world");
    }

    #[test]
    fn test_usj_note() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Note {
                    marker: "f".into(),
                    caller: "+".into(),
                    category: None,
                    content: vec![
                        Node::Char {
                            marker: "fr".into(),
                            content: vec![Node::text("1.1")],
                            attributes: vec![],
                            spans: NodeSpans::node(5..10),
                        },
                        Node::Char {
                            marker: "ft".into(),
                            content: vec![Node::text("A footnote")],
                            attributes: vec![],
                            spans: NodeSpans::node(10..25),
                        },
                    ],
                    spans: NodeSpans::node(0..30),
                }],
                spans: NodeSpans::node(0..35),
            }],
        };
        let value = to_usj_value(&doc).unwrap();
        let note = &value["content"][0]["content"][0];
        assert_eq!(note["type"], "note");
        assert_eq!(note["marker"], "f");
        assert_eq!(note["caller"], "+");
    }
}
