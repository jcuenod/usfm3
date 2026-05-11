use crate::ast::{Attribute, Document, Node};

/// Serialize a [`Document`] to a normalized USFM 3.x string.
///
/// The output is *normalized* -- it produces valid, well-formed USFM but does
/// not attempt round-trip fidelity with the original source text.  Whitespace
/// and formatting are regularized according to common USFM conventions:
///
/// * Paragraph-level markers (`\p`, `\q1`, `\m`, ...) start on a new line.
/// * `\c N` gets its own line.
/// * `\v N` is inline within paragraph content.
/// * Character markers (`\nd`, `\bk`, ...) are inline with closing markers.
/// * Notes (`\f`, `\x`) are inline with closing markers.
/// * Milestones are inline.
/// * The `\id` line is always first.
/// * Sidebars use `\esb` / `\esbe` each on their own line.
pub fn to_usfm_string(doc: &Document) -> String {
    let mut output = String::new();
    let mut ser = UsfmSerializer::new(&mut output);
    ser.serialize_document(doc);
    output
}

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

struct UsfmSerializer<'a> {
    output: &'a mut String,
    /// True when the write cursor is at column 0 of a fresh line.
    at_line_start: bool,
}

impl<'a> UsfmSerializer<'a> {
    fn new(output: &'a mut String) -> Self {
        Self {
            output,
            at_line_start: true,
        }
    }

    // -- Document -----------------------------------------------------------

    fn serialize_document(&mut self, doc: &Document) {
        for node in &doc.content {
            self.serialize_node(node);
        }
        // Ensure the file ends with exactly one trailing newline.
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    // -- Helpers ------------------------------------------------------------

    /// Ensure we are at the start of a new line.  If we are not, emit a
    /// newline character first.
    fn ensure_newline(&mut self) {
        if !self.at_line_start {
            self.output.push('\n');
            self.at_line_start = true;
        }
    }

    /// Ensure a single space separator exists before the next token, unless
    /// we are already at line-start or the output already ends with a space.
    fn ensure_space(&mut self) {
        if !self.at_line_start && !self.output.ends_with(' ') && !self.output.ends_with('\n') {
            self.output.push(' ');
        }
    }

    // -- Node dispatch ------------------------------------------------------

    fn serialize_node(&mut self, node: &Node) {
        match node {
            Node::Book { code, content, .. } => self.serialize_book(code, content),

            Node::Chapter(data) => {
                self.serialize_chapter(&data.number);
                if let Some(alt) = &data.altnumber {
                    self.output.push_str(" \\ca ");
                    self.output.push_str(alt);
                    self.output.push_str("\\ca*");
                }
                if let Some(pub_) = &data.pubnumber {
                    self.output.push_str(" \\cp ");
                    self.output.push_str(pub_);
                }
            }

            Node::Verse(data) => {
                self.serialize_verse(&data.number);
                if let Some(alt) = &data.altnumber {
                    self.output.push_str("\\va ");
                    self.output.push_str(alt);
                    self.output.push_str("\\va*");
                }
                if let Some(pub_) = &data.pubnumber {
                    self.output.push_str("\\vp ");
                    self.output.push_str(pub_);
                    self.output.push_str("\\vp*");
                }
            }

            Node::Para {
                marker, content, ..
            } => self.serialize_para(marker, content),

            Node::Char(data) => self.serialize_char(&data.marker, &data.content, &data.attributes),

            Node::Note {
                marker,
                caller,
                content,
                ..
            } => self.serialize_note(marker, caller, content),

            Node::Milestone {
                marker, attributes, ..
            } => self.serialize_milestone(marker, attributes),

            Node::Figure {
                marker,
                content,
                attributes,
                ..
            } => self.serialize_figure(marker, content, attributes),

            Node::Sidebar { content, .. } => self.serialize_sidebar(content),

            Node::Periph { content, .. } => {
                self.ensure_newline();
                self.output.push_str("\\periph");
                self.at_line_start = false;
                for child in content {
                    self.serialize_node(child);
                }
            }

            Node::Table { content, .. } => {
                for child in content {
                    self.serialize_node(child);
                }
            }

            Node::TableRow {
                marker, content, ..
            } => self.serialize_para(marker, content),

            Node::TableCell {
                marker, content, ..
            } => self.serialize_char(marker, content, &[]),

            Node::Text(s) => self.serialize_text(s),

            Node::Ref {
                content,
                attributes,
                ..
            } => self.serialize_char("ref", content, attributes),

            Node::Unknown {
                marker, content, ..
            } => self.serialize_unknown(marker, content),

            Node::OptBreak => self.output.push_str("//"),
        }
    }

    // -- Individual node types ----------------------------------------------

    /// `\id CODE text`
    fn serialize_book(&mut self, code: &str, content: &[Node]) {
        self.ensure_newline();
        self.output.push_str("\\id ");
        self.output.push_str(code);
        // The Book's content children are typically a single Text node with
        // the book description (e.g. "Genesis").  We emit each child after a
        // separating space.
        for child in content {
            match child {
                Node::Text(s) => {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        self.output.push(' ');
                        self.output.push_str(trimmed);
                    }
                }
                other => {
                    // Unexpected child type inside Book -- serialize it
                    // generically so no data is lost.
                    self.output.push(' ');
                    self.at_line_start = false;
                    self.serialize_node(other);
                }
            }
        }
        self.output.push('\n');
        self.at_line_start = true;
    }

    /// `\c N`
    fn serialize_chapter(&mut self, number: &str) {
        self.ensure_newline();
        self.output.push_str("\\c ");
        self.output.push_str(number);
        self.output.push('\n');
        self.at_line_start = true;
    }

    /// `\v N ` -- inline within a paragraph.
    fn serialize_verse(&mut self, number: &str) {
        if !self.at_line_start {
            self.ensure_space();
        }
        self.output.push_str("\\v ");
        self.output.push_str(number);
        self.output.push(' ');
        self.at_line_start = false;
    }

    /// Paragraph-level container.
    ///
    /// Produces output like:
    /// ```text
    /// \p \v 1 In the beginning ...
    /// ```
    /// or, for empty paragraphs such as `\b`:
    /// ```text
    /// \b
    /// ```
    fn serialize_para(&mut self, marker: &str, content: &[Node]) {
        self.ensure_newline();
        self.output.push('\\');
        self.output.push_str(marker);
        self.at_line_start = false;

        if content.is_empty() {
            // Marker-only paragraph (e.g. \b).  The next node will call
            // ensure_newline() and add the line break.
            return;
        }

        // For paragraphs with content, we place children on the same line.
        // A space is needed between the marker and the first child unless
        // the first child is a verse (which will handle its own spacing).
        let first_is_verse = matches!(content.first(), Some(Node::Verse(_)));
        if !first_is_verse {
            self.output.push(' ');
        }

        for child in content {
            self.serialize_node(child);
        }

        while self.output.ends_with(' ') {
            self.output.pop();
        }
    }

    /// Character-level inline marker: `\nd Lord\nd*` or `\w word|lemma="grace"\w*`
    fn serialize_char(&mut self, marker: &str, content: &[Node], attributes: &[Attribute]) {
        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push(' ');
        self.at_line_start = false;

        for child in content {
            self.serialize_node(child);
        }

        if !attributes.is_empty() {
            self.serialize_attributes(attributes);
        }

        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push('*');
    }

    /// Note (footnote / cross-reference): `\f + \fr 1.1 \ft text\f*`
    fn serialize_note(&mut self, marker: &str, caller: &str, content: &[Node]) {
        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push(' ');
        self.output.push_str(caller);
        self.output.push(' ');
        self.at_line_start = false;

        for child in content {
            self.serialize_node(child);
        }

        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push('*');
    }

    /// Milestone: `\qt1-s\|who="Jesus"\*` or just `\qt1-s\*`.
    fn serialize_milestone(&mut self, marker: &str, attributes: &[Attribute]) {
        self.output.push('\\');
        self.output.push_str(marker);
        if !attributes.is_empty() {
            self.serialize_attributes(attributes);
        }
        self.output.push_str("\\*");
        self.at_line_start = false;
    }

    /// Figure: `\fig caption|src="img.jpg"\fig*`
    fn serialize_figure(&mut self, marker: &str, content: &[Node], attributes: &[Attribute]) {
        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push(' ');
        self.at_line_start = false;

        for child in content {
            self.serialize_node(child);
        }

        if !attributes.is_empty() {
            self.serialize_attributes(attributes);
        }

        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push('*');
    }

    /// Sidebar: `\esb` ... `\esbe` each on their own line.
    fn serialize_sidebar(&mut self, content: &[Node]) {
        self.ensure_newline();
        self.output.push_str("\\esb");
        self.output.push('\n');
        self.at_line_start = true;

        for child in content {
            self.serialize_node(child);
        }

        self.ensure_newline();
        self.output.push_str("\\esbe");
        self.output.push('\n');
        self.at_line_start = true;
    }

    /// Plain text -- emitted verbatim.
    fn serialize_text(&mut self, s: &str) {
        let text = if self.output.ends_with(' ') {
            s.trim_start_matches(' ')
        } else {
            s
        };
        self.output.push_str(text);
        self.at_line_start = text.ends_with('\n');
    }

    /// Unknown / custom marker: `\marker content\marker*`
    fn serialize_unknown(&mut self, marker: &str, content: &[Node]) {
        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push(' ');
        self.at_line_start = false;

        for child in content {
            self.serialize_node(child);
        }

        self.output.push('\\');
        self.output.push_str(marker);
        self.output.push('*');
    }

    // -- Attributes ---------------------------------------------------------

    /// Serialize a `|key="value" ...` attribute list.
    fn serialize_attributes(&mut self, attributes: &[Attribute]) {
        self.output.push('|');
        for (i, attr) in attributes.iter().enumerate() {
            if i > 0 {
                self.output.push(' ');
            }
            self.output.push_str(&attr.key);
            self.output.push_str("=\"");
            self.output.push_str(&attr.value);
            self.output.push('"');
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    #[test]
    fn test_simple_document() {
        let doc = Document {
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
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\id GEN Genesis"));
        assert!(usfm.contains("\\c 1"));
        assert!(usfm.contains("\\p"));
        assert!(usfm.contains("\\v 1 "));
        assert!(usfm.contains("In the beginning"));
    }

    #[test]
    fn test_character_markers() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::text("The "),
                    Node::Char(Box::new(CharData {
                        marker: "nd".into(),
                        content: vec![Node::text("Lord")],
                        attributes: vec![],
                    })),
                    Node::text(" spoke."),
                ],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\nd Lord\\nd*"));
    }

    #[test]
    fn test_footnote() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::text("text"),
                    Node::Note {
                        marker: "f".into(),
                        caller: "+".into(),
                        category: None,
                        content: vec![
                            Node::Char(Box::new(CharData {
                                marker: "fr".into(),
                                content: vec![Node::text("1.1")],
                                attributes: vec![],
                            })),
                            Node::Char(Box::new(CharData {
                                marker: "ft".into(),
                                content: vec![Node::text("A note")],
                                attributes: vec![],
                            })),
                        ],
                    },
                    Node::text(" more text"),
                ],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\f + "));
        assert!(usfm.contains("\\f*"));
        assert!(usfm.contains("\\fr 1.1\\fr*"));
    }

    #[test]
    fn test_poetry() {
        let doc = Document {
            content: vec![
                Node::Para {
                    marker: "q1".into(),
                    content: vec![
                        Node::Verse(Box::new(VerseData {
                            marker: "v".into(),
                            number: "1".into(),
                            sid: None,
                            altnumber: None,
                            pubnumber: None,
                        })),
                        Node::text("O Lord, I have heard of what you have done,"),
                    ],
                },
                Node::Para {
                    marker: "q2".into(),
                    content: vec![Node::text("and I am filled with awe.")],
                },
            ],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\q1"));
        assert!(usfm.contains("\\q2"));
    }

    #[test]
    fn test_sidebar() {
        let doc = Document {
            content: vec![Node::Sidebar {
                marker: "esb".into(),
                category: None,
                content: vec![Node::Para {
                    marker: "p".into(),
                    content: vec![Node::text("Sidebar content")],
                }],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\esb"));
        assert!(usfm.contains("\\esbe"));
        assert!(usfm.contains("Sidebar content"));
    }

    #[test]
    fn test_empty_document() {
        let doc = Document::new();
        let usfm = to_usfm_string(&doc);
        assert!(usfm.is_empty() || usfm.trim().is_empty());
    }

    #[test]
    fn test_blank_line_paragraph() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "b".into(),
                content: vec![],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\b"));
    }

    #[test]
    fn test_milestone_with_attributes() {
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
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\qt1-s|who=\"Jesus\"\\*"));
    }

    #[test]
    fn test_milestone_without_attributes() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Milestone {
                    marker: "qt1-e".into(),
                    attributes: vec![],
                }],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\qt1-e\\*"));
    }

    #[test]
    fn test_figure() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Figure {
                    marker: "fig".into(),
                    content: vec![Node::text("A caption")],
                    attributes: vec![Attribute {
                        key: "src".into(),
                        value: "image.jpg".into(),
                    }],
                }],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\fig A caption|src=\"image.jpg\"\\fig*"));
    }

    #[test]
    fn test_unknown_marker() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Unknown {
                    marker: "zcustom".into(),
                    content: vec![Node::text("data")],
                }],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\zcustom data\\zcustom*"));
    }

    #[test]
    fn test_multiple_verses_in_paragraph() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "1".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::text("First verse text."),
                    Node::Verse(Box::new(VerseData {
                        marker: "v".into(),
                        number: "2".into(),
                        sid: None,
                        altnumber: None,
                        pubnumber: None,
                    })),
                    Node::text("Second verse text."),
                ],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\v 1 First verse text."));
        assert!(usfm.contains("\\v 2 Second verse text."));
    }

    #[test]
    fn test_trailing_newline() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::text("hello")],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.ends_with('\n'));
    }

    #[test]
    fn test_header_markers() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![Node::text("Genesis")],
                },
                Node::Para {
                    marker: "h".into(),
                    content: vec![Node::text("Genesis")],
                },
                Node::Para {
                    marker: "toc1".into(),
                    content: vec![Node::text("Genesis")],
                },
            ],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\id GEN Genesis\n"));
        assert!(usfm.contains("\\h Genesis"));
        assert!(usfm.contains("\\toc1 Genesis"));
    }

    #[test]
    fn test_nested_char_markers() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![Node::Char(Box::new(CharData {
                    marker: "add".into(),
                    content: vec![
                        Node::text("added "),
                        Node::Char(Box::new(CharData {
                            marker: "nd".into(),
                            content: vec![Node::text("Lord")],
                            attributes: vec![],
                        })),
                    ],
                    attributes: vec![],
                }))],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\add added \\nd Lord\\nd*\\add*"));
    }

    #[test]
    fn test_cross_reference() {
        let doc = Document {
            content: vec![Node::Para {
                marker: "p".into(),
                content: vec![
                    Node::text("text"),
                    Node::Note {
                        marker: "x".into(),
                        caller: "-".into(),
                        category: None,
                        content: vec![Node::Char(Box::new(CharData {
                            marker: "xt".into(),
                            content: vec![Node::text("Gen 1:1")],
                            attributes: vec![],
                        }))],
                    },
                ],
            }],
        };
        let usfm = to_usfm_string(&doc);
        assert!(usfm.contains("\\x - \\xt Gen 1:1\\xt*\\x*"));
    }
}
