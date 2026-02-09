use crate::ast::Document;
use serde::Serialize;

/// USJ document wrapper with version metadata.
///
/// Wraps the parsed [`Document`] content with the required USJ envelope
/// fields (`type` and `version`), producing JSON that conforms to the
/// Unified Scripture JSON specification.
#[derive(Debug, Clone, Serialize)]
pub struct UsjDocument {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub version: String,
    pub content: Vec<crate::ast::Node>,
}

impl UsjDocument {
    /// Create a USJ document from a parsed Document.
    pub fn from_document(doc: &Document) -> Self {
        UsjDocument {
            doc_type: "USJ".to_string(),
            version: "3.1".to_string(),
            content: doc.content.clone(),
        }
    }
}

/// Serialize a Document to a USJ JSON string.
pub fn to_usj_string(doc: &Document) -> Result<String, serde_json::Error> {
    let usj = UsjDocument::from_document(doc);
    serde_json::to_string(&usj)
}

/// Serialize a Document to a pretty-printed USJ JSON string.
pub fn to_usj_string_pretty(doc: &Document) -> Result<String, serde_json::Error> {
    let usj = UsjDocument::from_document(doc);
    serde_json::to_string_pretty(&usj)
}

/// Serialize a Document to a serde_json::Value.
pub fn to_usj_value(doc: &Document) -> Result<serde_json::Value, serde_json::Error> {
    let usj = UsjDocument::from_document(doc);
    serde_json::to_value(&usj)
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
                    span: 0..20,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    span: 20..25,
                },
                Node::Para {
                    marker: "p".into(),
                    content: vec![
                        Node::Verse {
                            marker: "v".into(),
                            number: "1".into(),
                            sid: Some("GEN 1:1".into()),
                            span: 30..33,
                        },
                        Node::text("In the beginning God created the heavens and the earth."),
                    ],
                    span: 25..90,
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
        assert!(json_str.contains('\n')); // pretty-printed has newlines
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
        assert_eq!(value["content"][2]["content"][1], "In the beginning God created the heavens and the earth.");
    }

    #[test]
    fn test_usj_no_spans() {
        let doc = sample_document();
        let json_str = to_usj_string(&doc).unwrap();
        assert!(!json_str.contains("span"));
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
            content: vec![
                Node::Para {
                    marker: "p".into(),
                    content: vec![Node::text("hello world")],
                    span: 0..20,
                },
            ],
        };
        let value = to_usj_value(&doc).unwrap();
        // Text nodes should be bare strings, not objects
        assert_eq!(value["content"][0]["content"][0], "hello world");
    }

    #[test]
    fn test_usj_note() {
        let doc = Document {
            content: vec![
                Node::Para {
                    marker: "p".into(),
                    content: vec![
                        Node::Note {
                            marker: "f".into(),
                            caller: "+".into(),
                            content: vec![
                                Node::Char {
                                    marker: "fr".into(),
                                    content: vec![Node::text("1.1")],
                                    span: 5..10,
                                },
                                Node::Char {
                                    marker: "ft".into(),
                                    content: vec![Node::text("A footnote")],
                                    span: 10..25,
                                },
                            ],
                            span: 0..30,
                        },
                    ],
                    span: 0..35,
                },
            ],
        };
        let value = to_usj_value(&doc).unwrap();
        let note = &value["content"][0]["content"][0];
        assert_eq!(note["type"], "note");
        assert_eq!(note["marker"], "f");
        assert_eq!(note["caller"], "+");
    }

    #[test]
    fn test_from_document() {
        let doc = sample_document();
        let usj = UsjDocument::from_document(&doc);
        assert_eq!(usj.doc_type, "USJ");
        assert_eq!(usj.version, "3.1");
        assert_eq!(usj.content.len(), 3);
    }
}
