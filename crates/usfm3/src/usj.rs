use crate::ast::{Document, Node};
use crate::source_map::{SourceMap, SourceNode, SourceSpans};
use serde_json::{Map, Value, json};

/// Options controlling USJ serialization.
#[derive(Debug, Clone, Copy, Default)]
pub struct UsjOptions {
    pub include_spans: bool,
}

#[derive(Debug)]
pub enum UsjError {
    MissingSourceMapForSpans,
    SourceMapShapeMismatch,
    Serialization(serde_json::Error),
}

impl std::fmt::Display for UsjError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsjError::MissingSourceMapForSpans => {
                write!(f, "USJ span output requires a source map")
            }
            UsjError::SourceMapShapeMismatch => {
                write!(f, "source map does not match AST shape")
            }
            UsjError::Serialization(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for UsjError {}

impl From<serde_json::Error> for UsjError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value)
    }
}

pub fn to_usj_string(doc: &Document) -> Result<String, UsjError> {
    to_usj_string_with_options(doc, None, UsjOptions::default())
}

pub fn to_usj_string_with_options(
    doc: &Document,
    source_map: Option<&SourceMap>,
    options: UsjOptions,
) -> Result<String, UsjError> {
    Ok(serde_json::to_string(&to_usj_value_with_options(doc, source_map, options)?)?)
}

pub fn to_usj_string_pretty(doc: &Document) -> Result<String, UsjError> {
    to_usj_string_pretty_with_options(doc, None, UsjOptions::default())
}

pub fn to_usj_string_pretty_with_options(
    doc: &Document,
    source_map: Option<&SourceMap>,
    options: UsjOptions,
) -> Result<String, UsjError> {
    Ok(serde_json::to_string_pretty(&to_usj_value_with_options(
        doc, source_map, options,
    )?)?)
}

pub fn to_usj_value(doc: &Document) -> Result<Value, UsjError> {
    to_usj_value_with_options(doc, None, UsjOptions::default())
}

pub fn to_usj_value_with_options(
    doc: &Document,
    source_map: Option<&SourceMap>,
    options: UsjOptions,
) -> Result<Value, UsjError> {
    if options.include_spans && source_map.is_none() {
        return Err(UsjError::MissingSourceMapForSpans);
    }
    let content = serialize_nodes(
        &doc.content,
        source_map.map(|map| map.content.as_slice()),
        options,
    )?;
    Ok(json!({
        "type": "USJ",
        "version": "3.1",
        "content": content,
    }))
}

fn serialize_nodes(
    nodes: &[Node],
    source_nodes: Option<&[SourceNode]>,
    options: UsjOptions,
) -> Result<Vec<Value>, UsjError> {
    if let Some(source_nodes) = source_nodes
        && source_nodes.len() != nodes.len()
    {
        return Err(UsjError::SourceMapShapeMismatch);
    }

    nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| {
            let source_node = source_nodes.and_then(|nodes| nodes.get(idx));
            serialize_node(node, source_node, options)
        })
        .collect()
}

fn serialize_node(
    node: &Node,
    source: Option<&SourceNode>,
    options: UsjOptions,
) -> Result<Value, UsjError> {
    let mut map = Map::new();
    match node {
        Node::Text(text) => return Ok(Value::String(text.clone())),
        Node::OptBreak => {
            map.insert("type".into(), Value::String("optbreak".into()));
        }
        Node::Book {
            marker,
            code,
            content,
        } => {
            map.insert("type".into(), Value::String("book".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            map.insert("code".into(), Value::String(code.clone()));
            insert_content(&mut map, content, source, options)?;
        }
        Node::Chapter {
            marker,
            number,
            sid,
            altnumber,
            pubnumber,
        } => {
            map.insert("type".into(), Value::String("chapter".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            map.insert("number".into(), Value::String(number.clone()));
            insert_optional_string(&mut map, "sid", sid.as_ref());
            insert_optional_string(&mut map, "altnumber", altnumber.as_ref());
            insert_optional_string(&mut map, "pubnumber", pubnumber.as_ref());
        }
        Node::Verse {
            marker,
            number,
            sid,
            altnumber,
            pubnumber,
        } => {
            map.insert("type".into(), Value::String("verse".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            map.insert("number".into(), Value::String(number.clone()));
            insert_optional_string(&mut map, "sid", sid.as_ref());
            insert_optional_string(&mut map, "altnumber", altnumber.as_ref());
            insert_optional_string(&mut map, "pubnumber", pubnumber.as_ref());
        }
        Node::Para { marker, content } => {
            map.insert("type".into(), Value::String("para".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            insert_content(&mut map, content, source, options)?;
        }
        Node::Char {
            marker,
            content,
            attributes,
        } => {
            map.insert("type".into(), Value::String("char".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            insert_content(&mut map, content, source, options)?;
            insert_attributes(&mut map, attributes)?;
        }
        Node::Note {
            marker,
            caller,
            category,
            content,
        } => {
            map.insert("type".into(), Value::String("note".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            map.insert("caller".into(), Value::String(caller.clone()));
            insert_optional_string(&mut map, "category", category.as_ref());
            insert_content(&mut map, content, source, options)?;
        }
        Node::Milestone { marker, attributes } => {
            map.insert("type".into(), Value::String("ms".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            insert_attributes(&mut map, attributes)?;
        }
        Node::Figure {
            marker,
            content,
            attributes,
        } => {
            map.insert("type".into(), Value::String("figure".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            insert_content(&mut map, content, source, options)?;
            insert_attributes(&mut map, attributes)?;
        }
        Node::Sidebar {
            marker,
            category,
            content,
        } => {
            map.insert("type".into(), Value::String("sidebar".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            insert_optional_string(&mut map, "category", category.as_ref());
            insert_content(&mut map, content, source, options)?;
        }
        Node::Periph {
            alt,
            content,
            attributes,
        } => {
            map.insert("type".into(), Value::String("periph".into()));
            insert_optional_string(&mut map, "alt", alt.as_ref());
            insert_content(&mut map, content, source, options)?;
            insert_attributes(&mut map, attributes)?;
        }
        Node::Table { content } => {
            map.insert("type".into(), Value::String("table".into()));
            insert_content(&mut map, content, source, options)?;
        }
        Node::TableRow { marker, content } => {
            map.insert("type".into(), Value::String("table:row".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            insert_content(&mut map, content, source, options)?;
        }
        Node::TableCell {
            marker,
            align,
            content,
        } => {
            map.insert("type".into(), Value::String("table:cell".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            map.insert("align".into(), Value::String(align.clone()));
            insert_content(&mut map, content, source, options)?;
        }
        Node::Ref {
            content,
            attributes,
        } => {
            map.insert("type".into(), Value::String("ref".into()));
            insert_content(&mut map, content, source, options)?;
            insert_attributes(&mut map, attributes)?;
        }
        Node::Unknown { marker, content } => {
            map.insert("type".into(), Value::String("unknown".into()));
            map.insert("marker".into(), Value::String(marker.to_string()));
            insert_content(&mut map, content, source, options)?;
        }
    }

    if options.include_spans {
        let Some(source) = source else {
            return Err(UsjError::MissingSourceMapForSpans);
        };
        if let Some(spans) = &source.spans {
            map.insert("spans".into(), serialize_spans(spans));
        }
    }

    Ok(Value::Object(map))
}

fn insert_content(
    map: &mut Map<String, Value>,
    content: &[Node],
    source: Option<&SourceNode>,
    options: UsjOptions,
) -> Result<(), UsjError> {
    if !content.is_empty() {
        map.insert(
            "content".into(),
            Value::Array(serialize_nodes(
                content,
                source.map(|source| source.children.as_slice()),
                options,
            )?),
        );
    }
    Ok(())
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<&String>) {
    if let Some(value) = value {
        map.insert(key.into(), Value::String(value.clone()));
    }
}

fn insert_attributes(
    map: &mut Map<String, Value>,
    attributes: &[crate::ast::Attribute],
) -> Result<(), UsjError> {
    if !attributes.is_empty() {
        map.insert("attributes".into(), serde_json::to_value(attributes)?);
    }
    Ok(())
}

fn serialize_spans(spans: &SourceSpans) -> Value {
    let mut map = Map::new();
    map.insert("node".into(), json!([spans.node.start, spans.node.end]));
    if let Some(code) = &spans.code {
        map.insert("code".into(), json!([code.start, code.end]));
    }
    if let Some(number) = &spans.number {
        map.insert("number".into(), json!([number.start, number.end]));
    }
    if let Some(close) = &spans.close {
        map.insert("close".into(), json!([close.start, close.end]));
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Attribute, Document, Node};
    use crate::source_map::{SourceMap, SourceNode, SourceSpans};
    use serde_json::json;

    fn sample_document() -> Document {
        Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![Node::text("Genesis")],
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
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
                        },
                        Node::text("In the beginning God created the heavens and the earth."),
                    ],
                },
            ],
        }
    }

    fn sample_source_map() -> SourceMap {
        SourceMap {
            content: vec![
                SourceNode::structural(
                    SourceSpans::node(0..20).with_code(4..7),
                    vec![SourceNode::leaf()],
                    Some(0),
                ),
                SourceNode::structural(
                    SourceSpans::node(21..25).with_number(24..25),
                    Vec::new(),
                    Some(1),
                ),
                SourceNode::structural(
                    SourceSpans::node(26..90),
                    vec![
                        SourceNode::structural(
                            SourceSpans::node(29..33).with_number(32..33),
                            Vec::new(),
                            Some(2),
                        ),
                        SourceNode::leaf(),
                    ],
                    Some(3),
                ),
            ],
        }
    }

    #[test]
    fn usj_string_contains_envelope_and_book_data() {
        let doc = sample_document();
        let json = to_usj_string(&doc).unwrap();

        assert!(json.contains("\"type\":\"USJ\""));
        assert!(json.contains("\"version\":\"3.1\""));
        assert!(json.contains("\"type\":\"book\""));
        assert!(json.contains("\"code\":\"GEN\""));
    }

    #[test]
    fn usj_pretty_prints() {
        let doc = sample_document();
        let json = to_usj_string_pretty(&doc).unwrap();

        assert!(json.contains('\n'));
        assert!(json.contains("\"type\": \"USJ\""));
    }

    #[test]
    fn usj_value_preserves_structure() {
        let doc = sample_document();
        let value = to_usj_value(&doc).unwrap();

        assert_eq!(value["type"], "USJ");
        assert_eq!(value["version"], "3.1");
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
    fn usj_omits_spans_by_default() {
        let value = to_usj_value(&sample_document()).unwrap();

        assert!(value["content"][0].get("spans").is_none());
    }

    #[test]
    fn usj_with_spans_uses_source_map() {
        let doc = sample_document();
        let source_map = sample_source_map();
        let value = to_usj_value_with_options(
            &doc,
            Some(&source_map),
            UsjOptions {
                include_spans: true,
            },
        )
        .unwrap();

        assert_eq!(value["content"][0]["spans"]["node"], json!([0, 20]));
        assert_eq!(value["content"][0]["spans"]["code"], json!([4, 7]));
        assert_eq!(value["content"][1]["spans"]["number"], json!([24, 25]));
        assert!(value["content"][2]["content"][1].is_string());
    }

    #[test]
    fn usj_with_spans_requires_source_map() {
        let error = to_usj_value_with_options(
            &sample_document(),
            None,
            UsjOptions {
                include_spans: true,
            },
        )
        .unwrap_err();

        assert!(matches!(error, UsjError::MissingSourceMapForSpans));
    }

    #[test]
    fn usj_rejects_shape_mismatches_between_ast_and_source_map() {
        let doc = sample_document();
        let source_map = SourceMap {
            content: vec![SourceNode::structural(
                SourceSpans::node(0..1),
                Vec::new(),
                None,
            )],
        };

        let error = to_usj_value_with_options(
            &doc,
            Some(&source_map),
            UsjOptions {
                include_spans: true,
            },
        )
        .unwrap_err();

        assert!(matches!(error, UsjError::SourceMapShapeMismatch));
    }

    #[test]
    fn usj_empty_document_is_valid() {
        let value = to_usj_value(&Document::new()).unwrap();
        assert_eq!(value["type"], "USJ");
        assert_eq!(value["content"], json!([]));
    }

    #[test]
    fn usj_text_nodes_serialize_as_bare_strings() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("hello world")],
            }],
        };

        let value = to_usj_value(&doc).unwrap();
        assert_eq!(value["content"][0]["content"][0], "hello world");
    }

    #[test]
    fn usj_serializes_note_and_attributes() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Note {
                    marker: "f".into(),
                    caller: "+".into(),
                    category: Some("ex".into()),
                    content: vec![
                        Node::Char {
                            marker: "fr".into(),
                            content: vec![Node::text("1.1")],
                            attributes: vec![],
                        },
                        Node::Char {
                            marker: "ft".into(),
                            content: vec![Node::text("A footnote")],
                            attributes: vec![Attribute {
                                key: "style".into(),
                                value: "plain".into(),
                            }],
                        },
                    ],
                }],
            }],
        };

        let value = to_usj_value(&doc).unwrap();
        let note = &value["content"][0]["content"][0];
        assert_eq!(note["type"], "note");
        assert_eq!(note["marker"], "f");
        assert_eq!(note["caller"], "+");
        assert_eq!(note["category"], "ex");
        assert_eq!(note["content"][1]["attributes"][0]["key"], "style");
    }

    #[test]
    fn usj_omits_empty_content_arrays() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "b".into(),
                content: vec![],
            }],
        };

        let value = to_usj_value(&doc).unwrap();
        assert_eq!(value["content"][0]["type"], "para");
        assert_eq!(value["content"][0]["marker"], "b");
        assert!(value["content"][0].get("content").is_none());
    }
}
