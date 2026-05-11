use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::io::Cursor;

use crate::ast::{ChapterData, CharData, Document, Node, VerseData};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during USX serialization.
#[derive(Debug)]
pub enum UsxError {
    /// An error from the underlying XML writer.
    Xml(quick_xml::Error),
    /// An I/O error from the underlying writer.
    Io(std::io::Error),
}

impl From<quick_xml::Error> for UsxError {
    fn from(e: quick_xml::Error) -> Self {
        UsxError::Xml(e)
    }
}

impl From<std::io::Error> for UsxError {
    fn from(e: std::io::Error) -> Self {
        UsxError::Io(e)
    }
}

impl std::fmt::Display for UsxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsxError::Xml(e) => write!(f, "XML serialization error: {}", e),
            UsxError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for UsxError {}

// ---------------------------------------------------------------------------
// Serializer state
// ---------------------------------------------------------------------------

/// Tracks the current open chapter/verse milestones so that end milestones
/// (`eid`) can be emitted automatically during serialization.
struct SerializerState {
    current_chapter_sid: Option<String>,
    current_verse_sid: Option<String>,
}

impl SerializerState {
    fn new() -> Self {
        Self {
            current_chapter_sid: None,
            current_verse_sid: None,
        }
    }

    /// Emit a `<verse eid="..."/>` for the currently-open verse, if any.
    fn close_verse<W: std::io::Write>(
        &mut self,
        writer: &mut Writer<W>,
    ) -> Result<(), quick_xml::Error> {
        if let Some(sid) = self.current_verse_sid.take() {
            let mut elem = BytesStart::new("verse");
            elem.push_attribute(("eid", sid.as_str()));
            writer.write_event(Event::Empty(elem))?;
        }
        Ok(())
    }

    /// Emit a `<chapter eid="..."/>` for the currently-open chapter, if any.
    fn close_chapter<W: std::io::Write>(
        &mut self,
        writer: &mut Writer<W>,
    ) -> Result<(), quick_xml::Error> {
        if let Some(sid) = self.current_chapter_sid.take() {
            let mut elem = BytesStart::new("chapter");
            elem.push_attribute(("eid", sid.as_str()));
            writer.write_event(Event::Empty(elem))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Serialize a [`Document`] to a USX 3.0 XML string.
///
/// The output includes an XML declaration (`<?xml ... ?>`) followed by a
/// `<usx version="3.0">` root element containing the serialized document
/// nodes. Chapter and verse end milestones (`eid`) are generated
/// automatically where required by the USX specification.
pub fn to_usx_string(doc: &Document) -> Result<String, UsxError> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // XML declaration: <?xml version="1.0" encoding="utf-8"?>
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    // Root element: <usx version="3.0">
    let mut root = BytesStart::new("usx");
    root.push_attribute(("version", "3.0"));
    writer.write_event(Event::Start(root))?;

    let mut state = SerializerState::new();

    for node in &doc.content {
        serialize_node(&mut writer, node, &mut state)?;
    }

    // Close any open verse/chapter at the end of the document.
    state.close_verse(&mut writer)?;
    state.close_chapter(&mut writer)?;

    writer.write_event(Event::End(BytesEnd::new("usx")))?;

    let result = writer.into_inner().into_inner();
    Ok(String::from_utf8(result).expect("quick-xml should always produce valid UTF-8"))
}

// ---------------------------------------------------------------------------
// Node serialization
// ---------------------------------------------------------------------------

fn serialize_node<W: std::io::Write>(
    writer: &mut Writer<W>,
    node: &Node,
    state: &mut SerializerState,
) -> Result<(), UsxError> {
    match node {
        Node::Book {
            marker,
            code,
            content,
            ..
        } => {
            let mut elem = BytesStart::new("book");
            elem.push_attribute(("code", code.as_ref()));
            elem.push_attribute(("style", marker.as_str()));
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("book")))?;
            }
        }

        Node::Chapter(data) => {
            let ChapterData {
                number,
                sid,
                altnumber,
                pubnumber,
                ..
            } = data.as_ref();
            // Close previous verse and chapter before opening a new chapter.
            state.close_verse(writer)?;
            state.close_chapter(writer)?;

            let sid_str = sid.as_deref().unwrap_or("");
            let mut elem = BytesStart::new("chapter");
            elem.push_attribute(("number", number.as_ref()));
            elem.push_attribute(("style", "c"));
            elem.push_attribute(("sid", sid_str));
            if let Some(alt) = altnumber {
                elem.push_attribute(("altnumber", alt.as_ref()));
            }
            if let Some(pub_) = pubnumber {
                elem.push_attribute(("pubnumber", pub_.as_ref()));
            }
            writer.write_event(Event::Empty(elem))?;

            state.current_chapter_sid = Some(sid_str.to_string());
        }

        Node::Verse(data) => {
            let VerseData {
                number,
                sid,
                altnumber,
                pubnumber,
                ..
            } = data.as_ref();
            // Close the previous verse before opening a new one.
            state.close_verse(writer)?;

            let sid_str = sid.as_deref().unwrap_or("");
            let mut elem = BytesStart::new("verse");
            elem.push_attribute(("number", number.as_ref()));
            elem.push_attribute(("style", "v"));
            elem.push_attribute(("sid", sid_str));
            if let Some(alt) = altnumber {
                elem.push_attribute(("altnumber", alt.as_ref()));
            }
            if let Some(pub_) = pubnumber {
                elem.push_attribute(("pubnumber", pub_.as_ref()));
            }
            writer.write_event(Event::Empty(elem))?;

            state.current_verse_sid = Some(sid_str.to_string());
        }

        Node::Para {
            marker, content, ..
        } => {
            let mut elem = BytesStart::new("para");
            elem.push_attribute(("style", marker.as_str()));
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for (index, child) in content.iter().enumerate() {
                    if index + 1 == content.len()
                        && let Node::Text(text) = child
                    {
                        let trimmed = text.trim_end_matches(' ');
                        if !trimmed.is_empty() {
                            writer.write_event(Event::Text(BytesText::new(trimmed)))?;
                        }
                    } else {
                        serialize_node(writer, child, state)?;
                    }
                }
                writer.write_event(Event::End(BytesEnd::new("para")))?;
            }
        }

        Node::Char(data) => {
            let CharData {
                marker,
                content,
                attributes,
                ..
            } = data.as_ref();
            let mut elem = BytesStart::new("char");
            elem.push_attribute(("style", marker.as_str()));
            for attr in attributes {
                elem.push_attribute((attr.key.as_ref(), attr.value.as_ref()));
            }
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("char")))?;
            }
        }

        Node::Note {
            marker,
            caller,
            content,
            ..
        } => {
            let mut elem = BytesStart::new("note");
            elem.push_attribute(("caller", caller.as_ref()));
            elem.push_attribute(("style", marker.as_str()));
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("note")))?;
            }
        }

        Node::Milestone {
            marker, attributes, ..
        } => {
            let mut elem = BytesStart::new("ms");
            elem.push_attribute(("style", marker.as_str()));
            for attr in attributes {
                elem.push_attribute((attr.key.as_ref(), attr.value.as_ref()));
            }
            writer.write_event(Event::Empty(elem))?;
        }

        Node::Figure {
            marker,
            content,
            attributes,
            ..
        } => {
            let mut elem = BytesStart::new("figure");
            elem.push_attribute(("style", marker.as_str()));
            for attr in attributes {
                elem.push_attribute((attr.key.as_ref(), attr.value.as_ref()));
            }
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("figure")))?;
            }
        }

        Node::Sidebar {
            marker, content, ..
        } => {
            let mut elem = BytesStart::new("sidebar");
            elem.push_attribute(("style", marker.as_str()));
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("sidebar")))?;
            }
        }

        Node::Text(s) => {
            writer.write_event(Event::Text(BytesText::new(s)))?;
        }

        Node::OptBreak => {
            writer.write_event(Event::Empty(BytesStart::new("optbreak")))?;
        }

        Node::Periph { content, .. } => {
            let elem = BytesStart::new("periph");
            writer.write_event(Event::Start(elem))?;
            for child in content {
                serialize_node(writer, child, state)?;
            }
            writer.write_event(Event::End(BytesEnd::new("periph")))?;
        }

        Node::Table { content, .. } => {
            let elem = BytesStart::new("table");
            writer.write_event(Event::Start(elem))?;
            for child in content {
                serialize_node(writer, child, state)?;
            }
            writer.write_event(Event::End(BytesEnd::new("table")))?;
        }

        Node::TableRow { content, .. } => {
            let elem = BytesStart::new("row");
            writer.write_event(Event::Start(elem.to_owned()))?;
            for child in content {
                serialize_node(writer, child, state)?;
            }
            writer.write_event(Event::End(BytesEnd::new("row")))?;
        }

        Node::TableCell {
            marker,
            align,
            content,
            ..
        } => {
            let mut elem = BytesStart::new("cell");
            elem.push_attribute(("style", marker.as_str()));
            elem.push_attribute(("align", align.as_ref()));
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("cell")))?;
            }
        }

        Node::Ref {
            content,
            attributes,
            ..
        } => {
            let mut elem = BytesStart::new("ref");
            for attr in attributes {
                elem.push_attribute((attr.key.as_ref(), attr.value.as_ref()));
            }
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("ref")))?;
            }
        }

        Node::Unknown {
            marker, content, ..
        } => {
            // Best effort: render unknown markers as <char> elements.
            let mut elem = BytesStart::new("char");
            elem.push_attribute(("style", marker.as_str()));
            if content.is_empty() {
                writer.write_event(Event::Empty(elem))?;
            } else {
                writer.write_event(Event::Start(elem))?;
                for child in content {
                    serialize_node(writer, child, state)?;
                }
                writer.write_event(Event::End(BytesEnd::new("char")))?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    fn sample_document() -> Document<'static> {
        Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![Node::text("Genesis")],
                },
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                Node::Para {
                    marker: "p".into(),
                    content: vec![
                        Node::Verse(Box::new(VerseData {
                            marker: "v".into(),
                            number: "1".into(),
                            sid: Some("GEN 1:1".into()),
                            altnumber: None,
                            pubnumber: None,
                        })),
                        Node::text("In the beginning God created the heavens and the earth."),
                    ],
                },
            ],
        }
    }

    #[test]
    fn test_usx_basic() {
        let xml = to_usx_string(&sample_document()).unwrap();
        assert!(xml.contains("<usx version=\"3.0\">"));
        assert!(xml.contains("</usx>"));
        assert!(xml.contains("<book code=\"GEN\" style=\"id\">Genesis</book>"));
        assert!(xml.contains("<chapter number=\"1\" style=\"c\" sid=\"GEN 1\"/>"));
        assert!(xml.contains("<verse number=\"1\" style=\"v\" sid=\"GEN 1:1\"/>"));
        assert!(xml.contains("<para style=\"p\">"));
        assert!(xml.contains("In the beginning"));
    }

    #[test]
    fn test_usx_chapter_eid() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                },
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "2".into(),
                    sid: Some("GEN 2".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
            ],
        };
        let xml = to_usx_string(&doc).unwrap();
        // Should have eid for chapter 1 before chapter 2
        assert!(xml.contains("<chapter eid=\"GEN 1\"/>"));
        // Should have eid for chapter 2 at end
        assert!(xml.contains("<chapter eid=\"GEN 2\"/>"));
    }

    #[test]
    fn test_usx_verse_eid() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                },
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                Node::Para {
                    marker: "p".into(),
                    content: vec![
                        Node::Verse(Box::new(VerseData {
                            marker: "v".into(),
                            number: "1".into(),
                            sid: Some("GEN 1:1".into()),
                            altnumber: None,
                            pubnumber: None,
                        })),
                        Node::text("First verse. "),
                        Node::Verse(Box::new(VerseData {
                            marker: "v".into(),
                            number: "2".into(),
                            sid: Some("GEN 1:2".into()),
                            altnumber: None,
                            pubnumber: None,
                        })),
                        Node::text("Second verse."),
                    ],
                },
            ],
        };
        let xml = to_usx_string(&doc).unwrap();
        assert!(xml.contains("<verse eid=\"GEN 1:1\"/>"));
        assert!(xml.contains("<verse eid=\"GEN 1:2\"/>"));
    }

    #[test]
    fn test_usx_note() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Note {
                    marker: "f".into(),
                    caller: "+".into(),
                    category: None,
                    content: vec![Node::Char(Box::new(CharData {
                        marker: "ft".into(),
                        content: vec![Node::text("A footnote")],
                        attributes: vec![],
                    }))],
                }],
            }],
        };
        let xml = to_usx_string(&doc).unwrap();
        assert!(xml.contains("<note caller=\"+\" style=\"f\">"));
        assert!(xml.contains("<char style=\"ft\">A footnote</char>"));
        assert!(xml.contains("</note>"));
    }

    #[test]
    fn test_usx_milestone() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Milestone {
                    marker: "qt1-s".into(),
                    attributes: vec![Attribute {
                        key: "who".into(),
                        value: "Jesus".into(),
                    }],
                }],
            }],
        };
        let xml = to_usx_string(&doc).unwrap();
        assert!(xml.contains("<ms style=\"qt1-s\" who=\"Jesus\"/>"));
    }

    #[test]
    fn test_usx_empty_document() {
        let doc = Document::new();
        let xml = to_usx_string(&doc).unwrap();
        assert!(xml.contains("<usx version=\"3.0\""));
        assert!(xml.contains("</usx>"));
    }

    #[test]
    fn test_usx_xml_escaping() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("He said \"hello\" & <goodbye>")],
            }],
        };
        let xml = to_usx_string(&doc).unwrap();
        // XML special characters should be escaped
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&lt;"));
        assert!(xml.contains("&gt;"));
    }
}
