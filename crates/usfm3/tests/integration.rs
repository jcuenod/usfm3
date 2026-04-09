//! Integration tests covering end-to-end USFM parsing, USJ export, and verse
//! reference extraction.  Ported (and modernised) from the old throwaway
//! `main.rs` test suite that used a previous AST shape.

use serde_json::Value;
use usfm3::ast::Node;
use usfm3::builder;
use usfm3::cst::{self, CstKind, MarkerTokenKind};
use usfm3::diagnostics::{DiagnosticCode, Severity};
use usfm3::usj;
use usfm3::vref;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse USFM and return the document, panicking on unexpected errors.
fn get_ast(usfm: &str) -> usfm3::ast::Document<'_> {
    builder::parse(usfm).ast
}

fn get_cst(usfm: &str) -> usfm3::cst::CstDocument {
    cst::parse(usfm)
}

fn get_full(usfm: &str) -> usfm3::ParsedDocument {
    usfm3::parse(usfm, usfm3::ParseOptions { diagnostics: true })
}

fn diagnostics<'a>(result: &'a usfm3::AstDocument<'a>) -> &'a [usfm3::diagnostics::Diagnostic] {
    result
        .diagnostics
        .as_deref()
        .expect("builder::parse() should collect diagnostics")
}

/// Parse USFM and return the USJ JSON value.
fn parse_to_usj(usfm: &str) -> Value {
    let doc = get_ast(usfm);
    usj::to_usj_value(&doc).expect("USJ serialization failed")
}

/// Parse USFM and return the vref map (verse-ref → plain text).
fn parse_to_vref(usfm: &str) -> serde_json::Map<String, Value> {
    let doc = get_ast(usfm);
    vref::to_vref_map(&doc)
}

/// Parse USFM and return the USX XML string.
fn parse_to_usx(usfm: &str) -> String {
    let doc = get_ast(usfm);
    usfm3::usx::to_usx_string(&doc).expect("USX serialization failed")
}

#[test]
fn cst_round_trip_preserves_source_exactly() {
    let usfm = "\\id GEN\r\n\\c 1\r\n\\p  \\v 1  In the beginning\\nd Lord\\nd*\r\n";
    let cst = get_cst(usfm);
    assert_eq!(cst.to_source_string(), usfm);
}

#[test]
fn cst_leaf_spans_are_gap_free_and_ordered() {
    let usfm = "\\id GEN\r\n\\c 1\r\n\\p  \\v 1  In the beginning\r\n";
    let cst = get_cst(usfm);

    let mut expected_start = 0;
    for &leaf_id in cst.leaf_ids() {
        let span = cst.node(leaf_id).span.clone();
        assert_eq!(
            span.start, expected_start,
            "leaf spans should be contiguous"
        );
        assert!(span.end >= span.start, "leaf spans should be ordered");
        expected_start = span.end;
    }

    assert_eq!(
        expected_start,
        usfm.len(),
        "leaf spans should cover the full source"
    );
}

#[test]
fn cst_cursor_mapping_finds_preserved_trivia() {
    let usfm = "\\p  \\v 1  In the beginning";
    let cst = get_cst(usfm);

    let first_gap = cst.leaf_at_offset(2).unwrap();
    let second_gap = cst.leaf_at_offset(8).unwrap();

    assert!(matches!(cst.node(first_gap).kind, CstKind::WhitespaceToken));
    assert!(matches!(
        cst.node(second_gap).kind,
        CstKind::WhitespaceToken
    ));

    let range = cst.covering_node_range(4, 8).unwrap();
    assert!(matches!(cst.node(range).kind, CstKind::Verse { .. }));
}

#[test]
fn cst_preserves_explicit_close_markers_as_leaves() {
    let usfm = "\\p \\nd Lord\\nd*";
    let cst = get_cst(usfm);
    let close_leaf = cst
        .leaf_ids()
        .iter()
        .copied()
        .find(|&id| matches!(cst.node(id).kind, CstKind::ClosingMarkerToken { .. }))
        .expect("expected explicit close-marker leaf");

    assert_eq!(cst.source_text(close_leaf), "\\nd*");
}

#[test]
fn cst_preserves_raw_marker_spelling() {
    let usfm = "\\p \\+nd Lord\\+nd*";
    let cst = get_cst(usfm);
    let nested_open = cst
        .leaf_ids()
        .iter()
        .copied()
        .find(|&id| {
            matches!(
                cst.node(id).kind,
                CstKind::MarkerToken {
                    token_kind: MarkerTokenKind::Nested,
                    ..
                }
            )
        })
        .expect("expected nested marker leaf");

    assert_eq!(cst.source_text(nested_open), "\\+nd");
}

#[test]
fn lowering_from_cst_matches_direct_parse() {
    let usfm = "\\id GEN Genesis\n\\c 1\n\\p \\v 1 In the beginning\\nd Lord\\nd*\n";
    let cst = get_cst(usfm);
    let lowered = builder::lower(&cst, usfm3::ParseOptions { diagnostics: true });
    let parsed = builder::parse(usfm);

    assert_eq!(lowered.ast, parsed.ast);
    assert_eq!(lowered.diagnostics, parsed.diagnostics);
}

#[test]
fn parser_diagnostics_capture_cst_anchor() {
    let result = builder::parse("\\p \\notreal foo");
    let diag = diagnostics(&result)
        .iter()
        .find(|d| d.code == DiagnosticCode::UnknownMarker)
        .expect("expected unknown marker diagnostic");

    assert!(
        diag.anchor_cst.is_some(),
        "parser diagnostic should be CST-anchored"
    );
}

#[test]
fn validation_diagnostics_resolve_cst_anchor_lazily() {
    let result = get_full("\\id BAD Bad Book\n\\c 1\n\\p \\v 1 Text");
    let diag = result
        .diagnostics()
        .expect("diagnostics should be available")
        .iter()
        .find(|d| d.code == DiagnosticCode::InvalidBookCode)
        .expect("expected invalid book code diagnostic");

    assert!(
        diag.anchor_cst.is_none(),
        "validation diagnostics should stay lazy"
    );
    assert!(
        diag.resolved_anchor_cst(result.cst()).is_some(),
        "validation diagnostics should resolve back to a CST node"
    );
}

/// Collect all nodes of a given USJ `type` from a JSON array, recursively.
fn collect_by_type<'a>(value: &'a Value, ty: &str) -> Vec<&'a Value> {
    let mut out = Vec::new();
    if let Some(arr) = value.as_array() {
        for item in arr {
            collect_by_type_inner(item, ty, &mut out);
        }
    } else {
        collect_by_type_inner(value, ty, &mut out);
    }
    out
}

fn collect_by_type_inner<'a>(value: &'a Value, ty: &str, out: &mut Vec<&'a Value>) {
    if value.get("type").and_then(Value::as_str) == Some(ty) {
        out.push(value);
    }
    if let Some(content) = value.get("content").and_then(Value::as_array) {
        for child in content {
            collect_by_type_inner(child, ty, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Book identification and metadata
// ---------------------------------------------------------------------------

#[test]
fn parse_id_and_meta() {
    let usfm = r#"\id GEN Genesis
\ide UTF-8
\mt1 The First Book of Moses
\h Genesis
\c 1
\v 1 In the beginning God created the heavens and the earth."#;

    let usj = parse_to_usj(usfm);
    let content = usj["content"].as_array().unwrap();

    // Book node
    let book = &content[0];
    assert_eq!(book["type"], "book");
    assert_eq!(book["code"], "GEN");

    // Meta paragraphs: ide, mt1, h should all appear before the chapter.
    let meta_markers: Vec<&str> = content
        .iter()
        .filter(|n| n["type"] == "para")
        .filter_map(|n| n["marker"].as_str())
        .collect();
    assert!(meta_markers.contains(&"ide"), "should have \\ide");
    assert!(meta_markers.contains(&"mt1"), "should have \\mt1");
    assert!(meta_markers.contains(&"h"), "should have \\h");

    // Chapter should be present.
    let chapters = collect_by_type(&usj, "chapter");
    assert!(!chapters.is_empty(), "should have at least one chapter");
    assert_eq!(chapters[0]["number"], "1");
}

// ---------------------------------------------------------------------------
// Implicit paragraph and verses
// ---------------------------------------------------------------------------

#[test]
fn implicit_paragraph_and_verses() {
    let usfm = r#"\id GEN Genesis
\c 1
\v 1 In the beginning
\v 2 The earth was formless"#;

    let result = builder::parse(usfm);
    let doc = result.ast;
    let warnings: Vec<_> = result
        .diagnostics
        .as_deref()
        .expect("diagnostics should be available")
        .iter()
        .filter(|d| d.code == DiagnosticCode::VerseOutsideParagraph)
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "should warn once for the first bare verse"
    );
    assert_eq!(warnings[0].severity, Severity::Warning);

    let para = doc
        .content
        .iter()
        .find(|n| matches!(n, Node::Para { marker, .. } if marker == "p"))
        .expect("bare verses should be recovered into an implicit \\p");

    let para_children = para.children();
    assert!(
        matches!(para_children.first(), Some(Node::Verse(data)) if data.number == "1"),
        "implicit paragraph should start with verse 1"
    );
    assert!(
        para_children
            .iter()
            .any(|n| matches!(n, Node::Verse(data) if data.number == "2")),
        "implicit paragraph should also contain verse 2"
    );

    let usj = usj::to_usj_value(&doc).expect("USJ serialization failed");

    let content = usj["content"].as_array().unwrap();
    assert_eq!(content[2]["type"], "para");
    assert_eq!(content[2]["marker"], "p");

    let verses = collect_by_type(&usj, "verse");
    assert!(verses.len() >= 2, "should have at least two verse markers");

    let v1 = verses.iter().find(|v| v["number"] == "1").unwrap();
    assert_eq!(v1["sid"], "GEN 1:1");

    let v2 = verses.iter().find(|v| v["number"] == "2").unwrap();
    assert_eq!(v2["sid"], "GEN 1:2");

    // Plain verse text via vref.
    let vref = parse_to_vref(usfm);
    assert_eq!(
        vref.get("GEN 1:1").and_then(|v| v.as_str()),
        Some("In the beginning")
    );
    assert_eq!(
        vref.get("GEN 1:2").and_then(|v| v.as_str()),
        Some("The earth was formless")
    );
}

#[test]
fn bare_verse_recovery_serializes_with_implicit_paragraph() {
    let usfm = r#"\id GEN Genesis
\c 1
\v 1 In the beginning
\v 2 The earth was formless"#;

    let doc = get_ast(usfm);
    let normalized = usfm3::usfm::to_usfm_string(&doc);

    assert!(normalized.contains("\\c 1\n\\p \\v 1 In the beginning"));
    assert!(normalized.contains("\\v 2 The earth was formless"));
}

// ---------------------------------------------------------------------------
// Character marker attributes (\w with lemma and strong)
// ---------------------------------------------------------------------------

#[test]
fn char_marker_attributes() {
    let usfm = r#"\id GEN
\c 1
\v 1 \w beginning|lemma="H7225" strong="H7225"\w*."#;

    let usj = parse_to_usj(usfm);

    // Find the \w char node.
    let chars = collect_by_type(&usj, "char");
    let w_node = chars
        .iter()
        .find(|c| c["marker"] == "w")
        .expect("should find a \\w char node");

    // Check attributes.
    let attrs = w_node["attributes"].as_array().unwrap();
    let lemma = attrs.iter().find(|a| a["key"] == "lemma").unwrap();
    assert_eq!(lemma["value"], "H7225");
    let strong = attrs.iter().find(|a| a["key"] == "strong").unwrap();
    assert_eq!(strong["value"], "H7225");

    // The word text should be "beginning".
    let text_content: String = w_node["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n.as_str())
        .collect();
    assert!(text_content.contains("beginning"));
}

// ---------------------------------------------------------------------------
// Milestone markers (\qt-s / \qt-e)
// ---------------------------------------------------------------------------

#[test]
fn milestone_markers() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 \qt-s |who="Jesus"\*In truth \qt-e\*"#;

    let usj = parse_to_usj(usfm);

    let milestones = collect_by_type(&usj, "ms");
    assert!(
        milestones.len() >= 2,
        "should have qt-s and qt-e milestones"
    );

    let qt_s = milestones
        .iter()
        .find(|m| m["marker"].as_str().unwrap().starts_with("qt-s"))
        .expect("should find qt-s");
    let who = qt_s["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["key"] == "who")
        .expect("qt-s should have a 'who' attribute");
    assert_eq!(who["value"], "Jesus");

    let qt_e = milestones
        .iter()
        .find(|m| m["marker"].as_str().unwrap().starts_with("qt-e"))
        .expect("should find qt-e");
    assert_eq!(qt_e["type"], "ms");
}

// ---------------------------------------------------------------------------
// Footnotes with content markers
// ---------------------------------------------------------------------------

#[test]
fn footnote_with_content_markers() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 Text \f + \fr 1:1 \ft note text \f* rest."#;

    let usj = parse_to_usj(usfm);

    // Find the note.
    let notes = collect_by_type(&usj, "note");
    assert!(!notes.is_empty(), "should have a footnote");
    let note = notes[0];
    assert_eq!(note["marker"], "f");
    assert_eq!(note["caller"], "+");

    // Note children should include \fr and \ft char markers.
    let note_chars = collect_by_type(note, "char");
    let fr = note_chars.iter().find(|c| c["marker"] == "fr");
    assert!(fr.is_some(), "note should have \\fr");

    let ft = note_chars.iter().find(|c| c["marker"] == "ft");
    assert!(ft.is_some(), "note should have \\ft");

    // Vref should include "Text" and "rest." but NOT "note text".
    let vref = parse_to_vref(usfm);
    let v1 = vref.get("GEN 1:1").and_then(|v| v.as_str()).unwrap();
    assert!(v1.contains("Text"), "verse text should contain 'Text'");
    assert!(v1.contains("rest."), "verse text should contain 'rest.'");
    assert!(
        !v1.contains("note text"),
        "verse text should NOT contain footnote content"
    );
}

// https://ubsicap.github.io/usfm/usfm3.0/about/syntax.html
// "Significant whitespace": "The space after ... the end of the opening marker within a character or note marker pair."
#[test]
fn first_note_submarker_opening_whitespace_is_structural() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 Text \f + \fr   1:1 \ft   note text \f* rest."#;

    let usj = parse_to_usj(usfm);
    let notes = collect_by_type(&usj, "note");
    let note = notes[0];
    let note_chars = collect_by_type(note, "char");

    let fr = note_chars
        .iter()
        .find(|c| c["marker"] == "fr")
        .expect("note should have \\fr");
    let ft = note_chars
        .iter()
        .find(|c| c["marker"] == "ft")
        .expect("note should have \\ft");

    assert_eq!(fr["content"][0].as_str(), Some("1:1 "));
    assert!(
        ft["content"][0]
            .as_str()
            .is_some_and(|s| s.contains("note text")),
        "\\ft content should retain the note text"
    );
}

// ---------------------------------------------------------------------------
// USJ export: verse SIDs
// ---------------------------------------------------------------------------

#[test]
fn usj_verse_sids() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 In the beginning \v 2 The earth"#;

    let usj = parse_to_usj(usfm);

    let verses = collect_by_type(&usj, "verse");
    assert!(verses.len() >= 2);

    let v1 = verses.iter().find(|v| v["number"] == "1").unwrap();
    assert_eq!(v1["sid"], "GEN 1:1");

    let v2 = verses.iter().find(|v| v["number"] == "2").unwrap();
    assert_eq!(v2["sid"], "GEN 1:2");
}

// ---------------------------------------------------------------------------
// Table rows
// ---------------------------------------------------------------------------

#[test]
fn tables_minimal() {
    let usfm = r#"\id GEN
\c 1
\tr \th1 Head1 \th2 Head2
\tr \tc1 A \tc2 B
"#;

    let usj = parse_to_usj(usfm);

    // Table rows should be inside a table container.
    let tables = collect_by_type(&usj, "table");
    assert!(!tables.is_empty(), "should have a table node");

    let rows = collect_by_type(&usj, "table:row");
    assert_eq!(rows.len(), 2, "should have two table rows");

    // Cells inside rows.
    let cells = collect_by_type(&usj, "table:cell");
    assert!(cells.len() >= 4, "should have at least 4 table cells");
}

#[test]
fn tables_do_not_emit_close_diagnostics_at_row_or_eof_boundaries() {
    let usfm = r#"\id GEN
\c 1
\tr \th1 Day \th2 Tribe \th3 Leader
\tr \tcr1 1st \tc2 Judah \tc3 Nahshon son of Amminadab"#;

    let result = builder::parse(usfm);

    assert!(
        !diagnostics(&result).iter().any(|d| matches!(
            d.code,
            DiagnosticCode::ImplicitClose | DiagnosticCode::UnclosedAtEof
        )),
        "table rows/cells should close structurally without implicit/unclosed diagnostics: {:?}",
        diagnostics(&result)
            .iter()
            .map(|d| format!("{:?}", d.code))
            .collect::<Vec<_>>()
    );
}

#[test]
fn tables_close_cleanly_before_following_block_markers() {
    let usfm = r#"\id GEN
\c 1
\tr \tc1 A \tc2 B
\p
\v 1 After the table.
\c 2
\p
\v 1 Next chapter."#;

    let result = builder::parse(usfm);

    assert!(
        !diagnostics(&result).iter().any(|d| matches!(
            d.code,
            DiagnosticCode::ImplicitClose | DiagnosticCode::UnclosedAtEof
        )),
        "table rows/cells should close cleanly before later block markers: {:?}",
        diagnostics(&result)
            .iter()
            .map(|d| format!("{:?}", d.code))
            .collect::<Vec<_>>()
    );
}

#[test]
fn table_cell_whitespace_matches_spec_examples() {
    let usfm = r#"\id NUM
\c 7
\tr \th1 Day \th2 Tribe \th3 Leader
\tr \tcr1 1st \tc2 Judah \tc3 Nahshon son of Amminadab"#;

    let usj = parse_to_usj(usfm);
    let cells = collect_by_type(&usj, "table:cell");
    let cell_text: Vec<&str> = cells
        .iter()
        .map(|cell| cell["content"][0].as_str().unwrap())
        .collect();

    assert_eq!(
        cell_text,
        vec![
            "Day ",
            "Tribe ",
            "Leader",
            "1st ",
            "Judah ",
            "Nahshon son of Amminadab",
        ]
    );

    let usx = parse_to_usx(usfm);
    assert!(
        usx.contains(r#"<cell style="th1" align="start">Day </cell>"#),
        "first header cell should preserve trailing space in USX: {usx}"
    );
    assert!(
        usx.contains(r#"<cell style="th3" align="start">Leader</cell>"#),
        "final header cell should not carry a trailing space in USX: {usx}"
    );
    assert!(
        usx.contains(r#"<cell style="tc2" align="start">Judah </cell>"#),
        "non-final body cell should preserve trailing space in USX: {usx}"
    );
    assert!(
        usx.contains(r#"<cell style="tc3" align="start">Nahshon son of Amminadab</cell>"#),
        "final body cell should not carry a trailing space in USX: {usx}"
    );
}

#[test]
fn table_row_newlines_do_not_become_trailing_cell_space() {
    let eof_usj = parse_to_usj(
        r#"\id GEN
\c 1
\tr \tc1 A \tc2 B
"#,
    );
    let eof_cells = collect_by_type(&eof_usj, "table:cell");
    assert_eq!(eof_cells[0]["content"][0].as_str(), Some("A "));
    assert_eq!(eof_cells[1]["content"][0].as_str(), Some("B"));

    let boundary_usj = parse_to_usj(
        r#"\id GEN
\c 1
\tr \tc1 A \tc2 B
\p
\v 1 After."#,
    );
    let boundary_cells = collect_by_type(&boundary_usj, "table:cell");
    assert_eq!(boundary_cells[0]["content"][0].as_str(), Some("A "));
    assert_eq!(boundary_cells[1]["content"][0].as_str(), Some("B"));
}

// https://ubsicap.github.io/usfm/usfm3.0/about/syntax.html
// "Significant whitespace": "The space after ... the end of the opening marker within a character or note marker pair."
#[test]
fn table_cell_opening_whitespace_is_structural() {
    let usfm = r#"\id GEN
\c 1
\tr \tc1   Judah \tc2   Issachar"#;

    let usj = parse_to_usj(usfm);
    let cells = collect_by_type(&usj, "table:cell");

    assert_eq!(cells[0]["content"][0].as_str(), Some("Judah "));
    assert_eq!(cells[1]["content"][0].as_str(), Some("Issachar"));
}

// https://ubsicap.github.io/usfm/usfm3.0/about/syntax.html
// "Normalized whitespace preceding the closing marker of a character or note marker pair is preserved."
#[test]
fn opening_marker_whitespace_is_structural_and_closing_marker_whitespace_restores_word_boundary() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 Start \bd   bold\bd*
tail."#;

    let usj = parse_to_usj(usfm);
    let chars = collect_by_type(&usj, "char");
    let bd = chars
        .iter()
        .find(|c| c["marker"] == "bd")
        .expect("should find \\bd char node");

    assert_eq!(bd["content"][0].as_str(), Some("bold"));

    let vref = parse_to_vref(usfm);
    assert_eq!(
        vref.get("GEN 1:1").and_then(|v| v.as_str()),
        Some("Start bold tail.")
    );
}

// https://ubsicap.github.io/usfm/usfm3.0/about/syntax.html
// "Multiple whitespace between words are normalized to a single space (U+0020)."
#[test]
fn paragraph_text_whitespace_is_normalized_to_single_spaces() {
    let usfm = "\\id GEN\n\\c 1\n\\p\n\\v 1 Alpha   beta\n   gamma";

    let vref = parse_to_vref(usfm);
    assert_eq!(
        vref.get("GEN 1:1").and_then(|v| v.as_str()),
        Some("Alpha beta gamma")
    );
}

#[test]
fn verse_continuation_paragraphs_restore_word_boundaries_in_vref() {
    let usfm = r#"\id LUK
\c 1
\p
\v 48 for he has been mindful
\q2 of the humble state of his servant.
\q1 From now on all generations will call me blessed,
\q2
\v 49 for the Mighty One has done great things for me—
\q2 holy is his name.
"#;

    let vref = parse_to_vref(usfm);
    assert_eq!(
        vref.get("LUK 1:48").and_then(|v| v.as_str()),
        Some("for he has been mindful of the humble state of his servant. From now on all generations will call me blessed,")
    );
    assert_eq!(
        vref.get("LUK 1:49").and_then(|v| v.as_str()),
        Some("for the Mighty One has done great things for me—holy is his name.")
    );
}

#[test]
fn quoted_poetry_continuations_restore_word_boundaries_in_vref() {
    let usfm = r#"\id LUK
\c 4
\p
\v 10 For it is written:
\q1 “‘He will command his angels concerning you
\q2 to guard you carefully;
"#;

    let vref = parse_to_vref(usfm);
    assert_eq!(
        vref.get("LUK 4:10").and_then(|v| v.as_str()),
        Some("For it is written: “‘He will command his angels concerning you to guard you carefully;")
    );
}

// ---------------------------------------------------------------------------
// Combined parse and export
// ---------------------------------------------------------------------------

#[test]
fn parse_and_export_basic() {
    let usfm = r#"\id GEN
\c 1
\s1 The Beginning
\p
\v 1 In the beginning \w God|strong="H430"\w* created the heavens and the earth.
\v 2 Now the earth was formless and empty, \f + \ft Or possibly "without form and void"\f* darkness was over the surface of the deep, and the Spirit of God was hovering over the waters.
"#;

    let usj = parse_to_usj(usfm);

    // Book code.
    assert_eq!(usj["content"][0]["code"], "GEN");

    // USJ structure: book, chapter, section heading, paragraph.
    let content = usj["content"].as_array().unwrap();
    assert!(content.len() >= 3);

    // Vref: footnotes stripped, word-level markup flattened.
    let vref = parse_to_vref(usfm);

    let v1 = vref.get("GEN 1:1").and_then(|v| v.as_str()).unwrap();
    assert!(
        v1.contains("In the beginning") && v1.contains("God") && v1.contains("the earth."),
        "verse 1 text: {v1}"
    );

    let v2 = vref.get("GEN 1:2").and_then(|v| v.as_str()).unwrap();
    assert!(
        v2.contains("formless and empty,") && v2.contains("hovering over the waters."),
        "verse 2 text: {v2}"
    );
    // Footnote text should be stripped.
    assert!(
        !v2.contains("without form and void"),
        "footnote content should be stripped from vref"
    );
}

// ---------------------------------------------------------------------------
// USJ envelope format
// ---------------------------------------------------------------------------

#[test]
fn usj_envelope() {
    let usfm = r#"\id GEN
\c 1
\v 1 Text"#;

    let usj = parse_to_usj(usfm);
    assert_eq!(usj["type"], "USJ");
    assert_eq!(usj["version"], "3.1");
    assert!(usj["content"].is_array());
}

// ---------------------------------------------------------------------------
// Empty document
// ---------------------------------------------------------------------------

#[test]
fn empty_document_produces_valid_usj() {
    let doc = usfm3::ast::Document::new();
    let usj = usj::to_usj_value(&doc).unwrap();
    assert_eq!(usj["type"], "USJ");
    assert_eq!(usj["content"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Diagnostics are produced (not empty)
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_for_unclosed_char_marker() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 \nd Lord is missing close marker"#;

    let result = builder::parse(usfm);
    assert!(
        !diagnostics(&result).is_empty(),
        "should produce diagnostics for unclosed \\nd"
    );
}

// ---------------------------------------------------------------------------
// USFM round-trip serialization
// ---------------------------------------------------------------------------

#[test]
fn usfm_round_trip_preserves_structure() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 In the beginning God created the heavens and the earth.
\v 2 The earth was without form and void.
"#;

    let doc = get_ast(usfm);
    let output = usfm3::usfm::to_usfm_string(&doc);

    // Re-parse the output and compare USJ.
    let doc2 = get_ast(&output);
    let usj1 = usj::to_usj_value(&doc).unwrap();
    let usj2 = usj::to_usj_value(&doc2).unwrap();
    assert_eq!(usj1, usj2, "USFM round-trip should produce identical USJ");
}

// ---------------------------------------------------------------------------
// USX output is well-formed XML
// ---------------------------------------------------------------------------

#[test]
fn usx_output_is_xml() {
    let usfm = r#"\id GEN
\c 1
\p
\v 1 In the beginning."#;

    let doc = get_ast(usfm);
    let xml = usfm3::usx::to_usx_string(&doc).expect("USX serialization failed");
    assert!(
        xml.contains("<?xml"),
        "USX should start with XML declaration"
    );
    assert!(
        xml.contains("<usx"),
        "USX should contain <usx> root element"
    );
    assert!(xml.contains("</usx>"), "USX should contain closing </usx>");
}

// ---------------------------------------------------------------------------
// AST-level structural checks
// ---------------------------------------------------------------------------

#[test]
fn ast_book_node_is_first() {
    let doc = get_ast(r#"\id GEN Genesis"#);
    assert!(
        matches!(&doc.content[0], Node::Book { code, .. } if code == "GEN"),
        "first node should be a Book with code GEN"
    );
}

#[test]
fn ast_chapter_produces_chapter_node() {
    let doc = get_ast(
        r#"\id GEN
\c 1
\c 2"#,
    );

    let chapters: Vec<_> = doc
        .content
        .iter()
        .filter(|n| matches!(n, Node::Chapter { .. }))
        .collect();
    assert_eq!(chapters.len(), 2, "should have two chapter nodes");
}

#[test]
fn ast_verse_inside_para() {
    let doc = get_ast(
        r#"\id GEN
\c 1
\p
\v 1 Text"#,
    );

    let para = doc
        .content
        .iter()
        .find(|n| matches!(n, Node::Para { marker, .. } if marker == "p"))
        .expect("should have a \\p paragraph");

    let has_verse = para
        .children()
        .iter()
        .any(|n| matches!(n, Node::Verse(data) if data.number == "1"));
    assert!(has_verse, "paragraph should contain verse 1");
}

#[test]
fn ast_note_node_structure() {
    let doc = get_ast(
        r#"\id GEN
\c 1
\p
\v 1 Word \f + \fr 1:1 \ft A note.\f* more."#,
    );

    let notes: Vec<_> = doc
        .content
        .iter()
        .flat_map(|n| n.children())
        .filter(|n| matches!(n, Node::Note { .. }))
        .collect();

    assert!(!notes.is_empty(), "should have a Note node");
    if let Node::Note {
        marker,
        caller,
        content,
        ..
    } = &notes[0]
    {
        assert_eq!(marker, "f");
        assert_eq!(caller, "+");
        assert!(!content.is_empty(), "note should have content children");
    }
}

#[test]
fn ast_table_grouping() {
    let doc = get_ast(
        r#"\id GEN
\c 1
\tr \th1 A \th2 B
\tr \tc1 1 \tc2 2
"#,
    );

    let tables: Vec<_> = doc
        .content
        .iter()
        .filter(|n| matches!(n, Node::Table { .. }))
        .collect();
    assert_eq!(
        tables.len(),
        1,
        "consecutive \\tr rows should be grouped into one Table"
    );

    let rows = tables[0].children();
    assert_eq!(rows.len(), 2, "table should have two rows");
    for row in rows {
        assert!(
            matches!(row, Node::TableRow { .. }),
            "table children should be TableRow"
        );
    }
}
