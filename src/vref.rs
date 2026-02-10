use crate::ast::{Document, Node};

/// Returns true for paragraph markers that carry verse-level body text
/// (as opposed to section headings, introductions, etc.).
fn is_verse_paragraph(marker: &str) -> bool {
    if matches!(
        marker,
        // body paragraphs
        "p" | "m" | "po" | "pr" | "cls"
            | "pmo" | "pm" | "pmc" | "pmr"
            | "pi" | "pi1" | "pi2" | "pi3"
            | "mi" | "nb" | "pc"
            | "ph" | "ph1" | "ph2" | "ph3"
            | "pb"
            // poetry
            | "q" | "q1" | "q2" | "q3" | "q4"
            | "qr" | "qc" | "qa"
            | "qm" | "qm1" | "qm2" | "qm3"
            | "qd"
            // lists
            | "lh"
            | "li" | "li1" | "li2" | "li3" | "li4"
            | "lf"
            | "lim" | "lim1" | "lim2" | "lim3"
    ) {
        return true;
    }
    // Fallback: strip trailing digits to handle higher-numbered variants
    // (e.g., q5, li5) that are valid via dynamic marker lookup.
    let base = marker.trim_end_matches(|c: char| c.is_ascii_digit());
    if !base.is_empty() && base != marker {
        return is_verse_paragraph(base);
    }
    false
}

/// Recursively collect plain text from a node, skipping notes and milestones.
fn collect_text(node: &Node, buf: &mut String) {
    match node {
        Node::Text(s) => buf.push_str(s),
        Node::Char { content, .. } | Node::Ref { content, .. } | Node::Unknown { content, .. } => {
            for child in content {
                collect_text(child, buf);
            }
        }
        // Skip notes, milestones, and everything else.
        _ => {}
    }
}

/// Serialize a [`Document`] to a JSON object mapping verse references to plain text.
///
/// The output is a flat `{ "GEN 1:1": "In the beginning ...", ... }` dictionary.
/// Footnotes, cross-references, headings, and all formatting are stripped;
/// only the running body text of each verse is kept.
pub fn to_vref_json_string(doc: &Document) -> String {
    let map = to_vref_map(doc);
    serde_json::to_string_pretty(&map).expect("vref JSON serialization should not fail")
}

/// Build the ordered map of verse-ref → plain text.
pub fn to_vref_map(doc: &Document) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();

    let mut book = String::new();
    let mut chapter = String::new();
    let mut current_ref = String::new();
    let mut current_text = String::new();

    for node in &doc.content {
        match node {
            Node::Book { code, .. } => {
                book = code.clone();
            }
            Node::Chapter { number, .. } => {
                chapter = number.clone();
            }
            Node::Para {
                marker, content, ..
            } => {
                if !is_verse_paragraph(marker) {
                    continue;
                }
                for child in content {
                    if let Node::Verse { number, .. } = child {
                        // Flush the previous verse.
                        let trimmed = current_text.trim();
                        if !current_ref.is_empty() && !trimmed.is_empty() {
                            map.insert(
                                current_ref.clone(),
                                serde_json::Value::String(trimmed.to_string()),
                            );
                        }
                        current_ref = format!("{} {}:{}", book, chapter, number);
                        current_text = String::new();
                    } else if !current_ref.is_empty() {
                        collect_text(child, &mut current_text);
                    }
                }
            }
            // Handle root-level verses (valid per USFM 3.1 — verses can be
            // siblings of paragraphs in chapter content).
            Node::Verse { number, .. } => {
                // Flush the previous verse.
                let trimmed = current_text.trim();
                if !current_ref.is_empty() && !trimmed.is_empty() {
                    map.insert(
                        current_ref.clone(),
                        serde_json::Value::String(trimmed.to_string()),
                    );
                }
                current_ref = format!("{} {}:{}", book, chapter, number);
                current_text = String::new();
            }
            // Collect root-level text into the current verse.
            node if !current_ref.is_empty() => {
                collect_text(node, &mut current_text);
            }
            _ => {}
        }
    }

    // Flush the last verse.
    let trimmed = current_text.trim();
    if !current_ref.is_empty() && !trimmed.is_empty() {
        map.insert(current_ref, serde_json::Value::String(trimmed.to_string()));
    }

    map
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    fn sample_doc() -> Document {
        Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![Node::text("Genesis")],
                    span: 0..10,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 10..15,
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
                            span: 15..20,
                        },
                        Node::text("In the beginning God created the heavens and the earth."),
                        Node::Verse {
                            marker: "v".into(),
                            number: "2".into(),
                            sid: Some("GEN 1:2".into()),
                            altnumber: None,
                            pubnumber: None,
                            span: 80..85,
                        },
                        Node::text("The earth was without form and void."),
                    ],
                    span: 15..120,
                },
            ],
        }
    }

    #[test]
    fn test_basic_vref() {
        let map = to_vref_map(&sample_doc());
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get("GEN 1:1").and_then(|v| v.as_str()),
            Some("In the beginning God created the heavens and the earth.")
        );
        assert_eq!(
            map.get("GEN 1:2").and_then(|v| v.as_str()),
            Some("The earth was without form and void.")
        );
    }

    #[test]
    fn test_insertion_order_preserved() {
        let map = to_vref_map(&sample_doc());
        let keys: Vec<&String> = map.keys().collect();
        assert_eq!(keys, vec!["GEN 1:1", "GEN 1:2"]);
    }

    #[test]
    fn test_footnotes_stripped() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                    span: 0..5,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 5..10,
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
                            span: 10..14,
                        },
                        Node::text("In the beginning"),
                        Node::Note {
                            marker: "f".into(),
                            caller: "+".into(),
                            category: None,
                            content: vec![Node::text("A footnote")],
                            span: 30..45,
                        },
                        Node::text(" God created."),
                    ],
                    span: 10..60,
                },
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(
            map.get("GEN 1:1").and_then(|v| v.as_str()),
            Some("In the beginning God created.")
        );
    }

    #[test]
    fn test_section_headings_skipped() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                    span: 0..5,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 5..10,
                },
                // Section heading -- should be ignored.
                Node::Para {
                    marker: "s1".into(),
                    content: vec![Node::text("The Creation")],
                    span: 10..25,
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
                            span: 25..29,
                        },
                        Node::text("In the beginning."),
                    ],
                    span: 25..50,
                },
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("GEN 1:1").and_then(|v| v.as_str()),
            Some("In the beginning.")
        );
    }

    #[test]
    fn test_char_markers_text_included() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                    span: 0..5,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 5..10,
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
                            span: 10..14,
                        },
                        Node::text("The "),
                        Node::Char {
                            marker: "nd".into(),
                            content: vec![Node::text("Lord")],
                            attributes: vec![],
                            span: 18..28,
                        },
                        Node::text(" said."),
                    ],
                    span: 10..40,
                },
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(
            map.get("GEN 1:1").and_then(|v| v.as_str()),
            Some("The Lord said.")
        );
    }

    #[test]
    fn test_empty_document() {
        let map = to_vref_map(&Document::new());
        assert!(map.is_empty());
    }

    #[test]
    fn test_verse_spanning_paragraphs() {
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                    span: 0..5,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 5..10,
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
                            span: 10..14,
                        },
                        Node::text("First part."),
                    ],
                    span: 10..30,
                },
                // Continuation paragraph (no verse marker, same verse).
                Node::Para {
                    marker: "q1".into(),
                    content: vec![Node::text("Second part.")],
                    span: 30..50,
                },
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("GEN 1:1").and_then(|v| v.as_str()),
            Some("First part.Second part.")
        );
    }

    #[test]
    fn test_json_output() {
        let json = to_vref_json_string(&sample_doc());
        assert!(json.contains("\"GEN 1:1\""));
        assert!(json.contains("\"GEN 1:2\""));
        assert!(json.contains("In the beginning"));
    }

    #[test]
    fn test_root_level_verses_collected() {
        // Verses at root level (no Para wrapper) — valid per USFM 3.1.
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                    span: 0..5,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 5..10,
                },
                Node::Verse {
                    marker: "v".into(),
                    number: "1".into(),
                    sid: Some("GEN 1:1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 10..14,
                },
                Node::text("In the beginning."),
                Node::Verse {
                    marker: "v".into(),
                    number: "2".into(),
                    sid: Some("GEN 1:2".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 30..34,
                },
                Node::text("And God said."),
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get("GEN 1:1").and_then(|v| v.as_str()),
            Some("In the beginning.")
        );
        assert_eq!(
            map.get("GEN 1:2").and_then(|v| v.as_str()),
            Some("And God said.")
        );
    }

    #[test]
    fn test_root_level_verses_then_paragraph() {
        // Root-level verses followed by a paragraph with more verses.
        let doc = Document {
            content: vec![
                Node::Book {
                    marker: "id".into(),
                    code: "GEN".into(),
                    content: vec![],
                    span: 0..5,
                },
                Node::Chapter {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 5..10,
                },
                // Root-level verse.
                Node::Verse {
                    marker: "v".into(),
                    number: "1".into(),
                    sid: Some("GEN 1:1".into()),
                    altnumber: None,
                    pubnumber: None,
                    span: 10..14,
                },
                Node::text("First."),
                // Then a paragraph with verse 2.
                Node::Para {
                    marker: "p".into(),
                    content: vec![
                        Node::Verse {
                            marker: "v".into(),
                            number: "2".into(),
                            sid: Some("GEN 1:2".into()),
                            altnumber: None,
                            pubnumber: None,
                            span: 20..24,
                        },
                        Node::text("Second."),
                    ],
                    span: 18..30,
                },
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("GEN 1:1").and_then(|v| v.as_str()), Some("First."));
        assert_eq!(map.get("GEN 1:2").and_then(|v| v.as_str()), Some("Second."));
    }
}
