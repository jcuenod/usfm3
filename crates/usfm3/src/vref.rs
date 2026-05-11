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
        Node::Char(data) => {
            for child in &data.content {
                collect_text(child, buf);
            }
        }
        Node::Ref { content, .. } | Node::Unknown { content, .. } => {
            for child in content {
                collect_text(child, buf);
            }
        }
        // Skip notes, milestones, and everything else.
        _ => {}
    }
}

fn is_opening_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\'' | '(' | '[' | '{' | '<' | '“' | '‘' | '«' | '‹'
    )
}

fn starts_boundary_separated_content(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.peek().copied() {
        if ch.is_whitespace() || is_opening_punctuation(ch) {
            chars.next();
            continue;
        }
        if !is_opening_punctuation(ch) {
            break;
        }
    }
    chars.peek().is_some_and(|ch| ch.is_alphanumeric())
}

fn ends_with_tight_joiner(text: &str) -> bool {
    let mut chars = text.chars().rev().skip_while(|ch| ch.is_whitespace());
    let Some(last) = chars.next() else {
        return false;
    };
    if !matches!(
        last,
        '-' | '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
    ) {
        return false;
    }

    chars.next().is_some_and(|prev| !prev.is_whitespace())
}

fn append_vref_text(buf: &mut String, text: &str, needs_boundary_space: bool) {
    if text.is_empty() {
        return;
    }

    if needs_boundary_space
        && !buf.is_empty()
        && !buf.ends_with(char::is_whitespace)
        && !text.starts_with(char::is_whitespace)
    {
        buf.push(' ');
    }

    buf.push_str(text);
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
                book = code.to_string();
            }
            Node::Chapter(data) => {
                chapter = data.number.to_string();
            }
            Node::Para { marker, content } => {
                if !is_verse_paragraph(marker) {
                    continue;
                }
                let mut verse_opened_in_para = false;
                let mut appended_visible_text = false;
                for child in content {
                    if let Node::Verse(data) = child {
                        // Flush the previous verse.
                        let trimmed = current_text.trim();
                        if !current_ref.is_empty() && !trimmed.is_empty() {
                            map.insert(
                                current_ref.clone(),
                                serde_json::Value::String(trimmed.to_string()),
                            );
                        }
                        current_ref = format!("{} {}:{}", book, chapter, data.number);
                        current_text = String::new();
                        verse_opened_in_para = true;
                        appended_visible_text = false;
                    } else if !current_ref.is_empty() {
                        let mut fragment = String::new();
                        collect_text(child, &mut fragment);
                        if !fragment.is_empty() {
                            let needs_boundary_space = !verse_opened_in_para
                                && !appended_visible_text
                                && starts_boundary_separated_content(&fragment)
                                && !ends_with_tight_joiner(&current_text);
                            append_vref_text(&mut current_text, &fragment, needs_boundary_space);
                            appended_visible_text = true;
                        }
                    }
                }
            }
            // Handle root-level verses (valid per USFM 3.1 — verses can be
            // siblings of paragraphs in chapter content).
            Node::Verse(data) => {
                // Flush the previous verse.
                let trimmed = current_text.trim();
                if !current_ref.is_empty() && !trimmed.is_empty() {
                    map.insert(
                        current_ref.clone(),
                        serde_json::Value::String(trimmed.to_string()),
                    );
                }
                current_ref = format!("{} {}:{}", book, chapter, data.number);
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

    fn sample_doc() -> Document<'static> {
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
                        Node::Verse(Box::new(VerseData {
                            marker: "v".into(),
                            number: "2".into(),
                            sid: Some("GEN 1:2".into()),
                            altnumber: None,
                            pubnumber: None,
                        })),
                        Node::text("The earth was without form and void."),
                    ],
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
                        Node::text("In the beginning"),
                        Node::Note {
                            marker: "f".into(),
                            caller: "+".into(),
                            category: None,
                            content: vec![Node::text("A footnote")],
                        },
                        Node::text(" God created."),
                    ],
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
                },
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                // Section heading -- should be ignored.
                Node::Para {
                    marker: "s1".into(),
                    content: vec![Node::text("The Creation")],
                },
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
                        Node::text("In the beginning."),
                    ],
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
                        Node::text("The "),
                        Node::Char(Box::new(CharData {
                            marker: "nd".into(),
                            content: vec![Node::text("Lord")],
                            attributes: vec![],
                        })),
                        Node::text(" said."),
                    ],
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
                        Node::text("First part."),
                    ],
                },
                // Continuation paragraph (no verse marker, same verse).
                Node::Para {
                    marker: "q1".into(),
                    content: vec![Node::text("Second part.")],
                },
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("GEN 1:1").and_then(|v| v.as_str()),
            Some("First part. Second part.")
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
                },
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                Node::Verse(Box::new(VerseData {
                    marker: "v".into(),
                    number: "1".into(),
                    sid: Some("GEN 1:1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                Node::text("In the beginning."),
                Node::Verse(Box::new(VerseData {
                    marker: "v".into(),
                    number: "2".into(),
                    sid: Some("GEN 1:2".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
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
                },
                Node::Chapter(Box::new(ChapterData {
                    marker: "c".into(),
                    number: "1".into(),
                    sid: Some("GEN 1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                // Root-level verse.
                Node::Verse(Box::new(VerseData {
                    marker: "v".into(),
                    number: "1".into(),
                    sid: Some("GEN 1:1".into()),
                    altnumber: None,
                    pubnumber: None,
                })),
                Node::text("First."),
                // Then a paragraph with verse 2.
                Node::Para {
                    marker: "p".into(),
                    content: vec![
                        Node::Verse(Box::new(VerseData {
                            marker: "v".into(),
                            number: "2".into(),
                            sid: Some("GEN 1:2".into()),
                            altnumber: None,
                            pubnumber: None,
                        })),
                        Node::text("Second."),
                    ],
                },
            ],
        };
        let map = to_vref_map(&doc);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("GEN 1:1").and_then(|v| v.as_str()), Some("First."));
        assert_eq!(map.get("GEN 1:2").and_then(|v| v.as_str()), Some("Second."));
    }
}
