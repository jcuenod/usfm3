use serde_json::Value;
use usfm3::low_level::{Cursor, NodeKind};
use usfm3::{DiagnosticCode, ParseOptions, Severity, Version};

fn parse(src: &str) -> usfm3::Document<'_> {
    usfm3::parse(src, Version::V3_1)
}

fn parse_with_diagnostics(src: &str) -> usfm3::Document<'_> {
    usfm3::parse_with(src, Version::V3_1, ParseOptions { diagnostics: true })
}

fn parse_usj(src: &str) -> Value {
    serde_json::from_str(&parse(src).to_usj()).expect("USJ should be valid JSON")
}

fn parse_cst_json(src: &str) -> Value {
    serde_json::from_str(&parse(src).to_cst_json()).expect("CST JSON should be valid JSON")
}

fn parse_vref(src: &str) -> Vec<(String, String)> {
    parse(src).vref_dict()
}

fn vref_text<'a>(rows: &'a [(String, String)], key: &str) -> Option<&'a str> {
    rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn collect_by_type<'a>(value: &'a Value, ty: &str) -> Vec<&'a Value> {
    let mut out = Vec::new();
    collect_by_type_inner(value, ty, &mut out);
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
    if let Some(children) = value.get("children").and_then(Value::as_array) {
        for child in children {
            collect_by_type_inner(child, ty, out);
        }
    }
}

fn normalize_json(v: &mut Value) {
    match v {
        Value::String(s) => *s = s.split_whitespace().collect::<Vec<_>>().join(" "),
        Value::Array(items) => {
            for item in items.iter_mut() {
                normalize_json(item);
            }
            items.retain(|item| !matches!(item, Value::String(s) if s.is_empty()));
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                normalize_json(value);
            }
        }
        _ => {}
    }
}

fn visit_cursor(cur: Cursor<'_>, f: &mut impl FnMut(Cursor<'_>)) {
    f(cur);
    for child in cur.children() {
        visit_cursor(child, f);
    }
}

fn direct_children(cur: Cursor<'_>) -> Vec<Cursor<'_>> {
    cur.children().collect()
}

fn descendant_markers<'a>(cur: Cursor<'a>, marker: &str) -> Vec<Cursor<'a>> {
    let mut out = Vec::new();
    fn walk<'a>(cur: Cursor<'a>, marker: &str, out: &mut Vec<Cursor<'a>>) {
        if cur.kind() == NodeKind::MarkerOpen && cur.marker_name() == marker {
            out.push(cur);
        }
        for child in cur.children() {
            walk(child, marker, out);
        }
    }
    walk(cur, marker, &mut out);
    out
}

#[test]
fn tape_is_lossless_and_cursor_exposes_explicit_and_synthetic_closes() {
    let src = "\\id GEN\r\n\\c 1\r\n\\p  \\v 1  In the beginning\\nd Lord\\nd*\r\n";
    let doc = parse(src);

    let reconstructed = doc
        .tape()
        .nodes
        .iter()
        .filter(|node| !node.is_synthetic())
        .map(|node| node.span().text(doc.source()))
        .collect::<String>();
    assert_eq!(reconstructed, src, "non-synthetic tape spans should be lossless");

    let mut saw_explicit_close = false;
    let mut saw_synthetic_close = false;
    visit_cursor(doc.cursor(), &mut |cur| {
        if cur.kind() != NodeKind::MarkerOpen {
            return;
        }
        if cur.marker_name() == "nd" {
            let close = cur.matching_close().expect("\\nd should have a close");
            assert_eq!(close.text(), "\\nd*");
            saw_explicit_close = true;
        }
        if cur.marker_name() == "p" {
            let close = cur.matching_close().expect("\\p should close synthetically");
            assert!(close.is_synthetic());
            saw_synthetic_close = true;
        }
    });

    assert!(saw_explicit_close, "expected to see an explicit close marker");
    assert!(saw_synthetic_close, "expected to see an implicit block close");
}

#[test]
fn cst_json_reports_close_ranges_and_nested_markers_are_flagged() {
    let cst = parse_cst_json("\\p \\+nd Lord\\+nd*");
    let markers = collect_by_type(&cst, "marker");
    let nested = markers
        .iter()
        .find(|node| node["marker"] == "nd" && node["start"] == 3)
        .expect("nested +nd marker should be present");

    assert_eq!(nested["close"]["start"], Value::from(12_u64));
    assert_eq!(nested["close"]["end"], Value::from(17_u64));

    let mut saw_nested = false;
    visit_cursor(parse("\\p \\+nd Lord\\+nd*").cursor(), &mut |cur| {
        if cur.kind() == NodeKind::MarkerOpen && cur.marker_name() == "nd" && cur.is_nested() {
            saw_nested = true;
        }
    });
    assert!(saw_nested, "nested +nd marker should be preserved as nested");
}

#[test]
fn metadata_and_empty_document_usj_envelopes_are_valid() {
    let usj = parse_usj(
        "\\id GEN Genesis\n\\ide UTF-8\n\\mt1 The First Book of Moses\n\\h Genesis\n\\c 1\n\\v 1 In the beginning",
    );
    assert_eq!(usj["type"], "USJ");
    assert_eq!(usj["version"], "3.1");

    let content = usj["content"].as_array().expect("USJ content should be an array");
    assert_eq!(content[0]["type"], "book");
    assert_eq!(content[0]["code"], "GEN");

    let meta_markers = content
        .iter()
        .filter(|node| node["type"] == "para")
        .filter_map(|node| node["marker"].as_str())
        .collect::<Vec<_>>();
    assert!(meta_markers.contains(&"ide"));
    assert!(meta_markers.contains(&"mt1"));
    assert!(meta_markers.contains(&"h"));

    let empty = parse_usj("");
    assert_eq!(empty["type"], "USJ");
    assert!(empty["content"].as_array().is_some_and(|items| items.is_empty()));
}

#[test]
fn bare_verse_recovery_warns_once_and_serializes_with_implicit_paragraph() {
    let src = "\\id GEN Genesis\n\\c 1\n\\v 1 In the beginning\n\\v 2 The earth was formless";
    let doc = parse_with_diagnostics(src);

    let warnings = doc
        .diagnostics()
        .iter()
        .filter(|d| d.code == DiagnosticCode::VerseOutsideParagraph)
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "bare verses should warn only once");
    assert_eq!(warnings[0].severity, Severity::Warning);

    let normalized = doc.to_usfm();
    assert!(normalized.contains("\\c 1 \\v 1 In the beginning \\v 2 The earth was formless"));

    let vref = doc.vref_dict();
    assert_eq!(vref[0], ("GEN 1:1".into(), "In the beginning".into()));
    assert_eq!(vref[1], ("GEN 1:2".into(), "The earth was formless".into()));
}

#[test]
fn cst_root_starts_with_id_marker_and_tracks_its_text_child() {
    let doc = parse("\\id GEN Genesis");
    let root = doc.cursor();
    let children = direct_children(root);
    let first = children.first().copied().expect("root should have a child");

    assert_eq!(first.kind(), NodeKind::MarkerOpen);
    assert_eq!(first.marker_name(), "id");

    let id_children = direct_children(first);
    let text = id_children
        .iter()
        .find(|child| child.kind() == NodeKind::Text)
        .expect("\\id should have a text child");
    assert_eq!(text.text(), " GEN Genesis");
}

#[test]
fn cst_chapter_nodes_appear_at_the_top_level() {
    let doc = parse("\\id GEN\n\\c 1\n\\c 2");
    let root = doc.cursor();
    let chapters = direct_children(root)
        .into_iter()
        .filter(|child| child.kind() == NodeKind::Chapter)
        .collect::<Vec<_>>();

    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].number(), Some(1));
    assert_eq!(chapters[1].number(), Some(2));
}

#[test]
fn cst_verse_is_nested_inside_paragraph() {
    let doc = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 Text");
    let root = doc.cursor();
    let para = direct_children(root)
        .into_iter()
        .find(|child| child.kind() == NodeKind::MarkerOpen && child.marker_name() == "p")
        .expect("expected a \\p paragraph");

    let verses = direct_children(para)
        .into_iter()
        .filter(|child| child.kind() == NodeKind::Verse)
        .collect::<Vec<_>>();
    assert_eq!(verses.len(), 1);
    assert_eq!(verses[0].number(), Some(1));
}

#[test]
fn cst_note_marker_preserves_caller_and_submarker_structure() {
    let doc = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 Word \\f + \\fr 1:1 \\ft A note.\\f* more.");
    let root = doc.cursor();
    let note = descendant_markers(root, "f")
        .into_iter()
        .next()
        .expect("expected a footnote marker");

    let note_children = direct_children(note);
    let first_text = note_children
        .iter()
        .find(|child| child.kind() == NodeKind::Text)
        .expect("note should have text containing the caller");
    assert!(first_text.text().starts_with(" +"));
    assert!(note_children.iter().any(|child| child.kind() == NodeKind::MarkerOpen && child.marker_name() == "fr"));
    assert!(note_children.iter().any(|child| child.kind() == NodeKind::MarkerOpen && child.marker_name() == "ft"));
}

#[test]
fn cst_table_rows_and_cells_preserve_the_same_structure_the_ast_grouped() {
    let doc = parse("\\id GEN\n\\c 1\n\\tr \\th1 A \\th2 B\n\\tr \\tc1 1 \\tc2 2\n");
    let root = doc.cursor();
    let rows = direct_children(root)
        .into_iter()
        .filter(|child| child.kind() == NodeKind::MarkerOpen && child.marker_name() == "tr")
        .collect::<Vec<_>>();

    assert_eq!(rows.len(), 2, "CST should preserve both consecutive table rows");

    let first_cells = direct_children(rows[0])
        .into_iter()
        .filter(|child| child.kind() == NodeKind::MarkerOpen)
        .map(|child| child.marker_name().to_string())
        .collect::<Vec<_>>();
    let second_cells = direct_children(rows[1])
        .into_iter()
        .filter(|child| child.kind() == NodeKind::MarkerOpen)
        .map(|child| child.marker_name().to_string())
        .collect::<Vec<_>>();

    assert_eq!(first_cells, vec!["th1".to_string(), "th2".to_string()]);
    assert_eq!(second_cells, vec!["tc1".to_string(), "tc2".to_string()]);
}

#[test]
fn character_attributes_note_submarkers_and_vref_cleaning_work_together() {
    let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 \\w beginning|lemma=\"H7225\" strong=\"H7225\"\\w*. Text \\f + \\fr 1:1 \\ft note text \\f* rest.";
    let usj = parse_usj(src);

    let chars = collect_by_type(&usj, "char");
    let w = chars
        .iter()
        .find(|node| node["marker"] == "w")
        .expect("\\w char should exist");
    assert_eq!(w["lemma"], "H7225");
    assert_eq!(w["strong"], "H7225");
    assert_eq!(w["content"][0], "beginning");

    let note = collect_by_type(&usj, "note")
        .into_iter()
        .next()
        .expect("footnote should exist");
    assert_eq!(note["marker"], "f");
    assert_eq!(note["caller"], "+");
    let note_chars = collect_by_type(note, "char");
    assert!(note_chars.iter().any(|node| node["marker"] == "fr"));
    assert!(note_chars.iter().any(|node| node["marker"] == "ft"));

    let dict = parse(src).vref_dict();
    assert_eq!(
        dict[0],
        ("GEN 1:1".into(), "beginning. Text rest.".into()),
        "vref text should drop note text but keep inline char content",
    );
}

#[test]
fn note_submarker_opening_whitespace_is_structural() {
    let usj = parse_usj("\\id GEN\n\\c 1\n\\p\n\\v 1 Text \\f + \\fr   1:1 \\ft   note text \\f* rest.");
    let note = collect_by_type(&usj, "note")
        .into_iter()
        .next()
        .expect("footnote should exist");
    let note_chars = collect_by_type(note, "char");

    let fr = note_chars
        .iter()
        .find(|node| node["marker"] == "fr")
        .expect("\\fr should exist");
    let ft = note_chars
        .iter()
        .find(|node| node["marker"] == "ft")
        .expect("\\ft should exist");

    assert_eq!(fr["content"][0].as_str(), Some("1:1 "));
    assert_eq!(ft["content"][0].as_str(), Some("note text "));
}

#[test]
fn milestones_are_preserved_and_self_closed_cleanly() {
    let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 \\qt-s |who=\"Jesus\"\\*In truth \\qt-e\\*";
    let doc = parse_with_diagnostics(src);
    assert!(
        !doc.diagnostics().iter().any(|d| d.severity == Severity::Error),
        "well-formed milestones should not emit errors: {:?}",
        doc.diagnostics()
    );

    let usj = parse_usj(src);
    let milestones = collect_by_type(&usj, "ms");
    assert_eq!(milestones.len(), 2);
    let start = milestones
        .iter()
        .find(|node| node["marker"] == "qt-s")
        .expect("qt-s should be present");
    assert_eq!(start["who"], "Jesus");
    assert!(milestones.iter().any(|node| node["marker"] == "qt-e"));
}

#[test]
fn verse_sids_cover_basic_ranges_and_subverse_labels() {
    let usj = parse_usj("\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning \\v 2 The earth");
    let verses = collect_by_type(&usj, "verse");
    assert!(verses.iter().any(|node| node["number"] == "1" && node["sid"] == "GEN 1:1"));
    assert!(verses.iter().any(|node| node["number"] == "2" && node["sid"] == "GEN 1:2"));

    for (num, sid) in [("3-4", "GEN 1:3-4"), ("12-83", "GEN 1:12-83"), ("3b", "GEN 1:3b")] {
        let usj = parse(&format!("\\id GEN\n\\c 1\n\\p\n\\v {num} text")).to_usj();
        assert!(usj.contains(&format!("\"number\":\"{num}\"")), "{num}");
        assert!(usj.contains(&format!("\"sid\":\"{sid}\"")), "{sid}");
    }
}

#[test]
fn table_rows_group_and_cell_spacing_matches_spec_examples() {
    let src = "\\id NUM\n\\c 7\n\\tr \\th1 Day \\th2 Tribe \\th3 Leader\n\\tr \\tcr1 1st \\tc2 Judah \\tc3 Nahshon son of Amminadab";
    let usj = parse_usj(src);

    let tables = collect_by_type(&usj, "table");
    assert_eq!(tables.len(), 1, "consecutive rows should group into one table");
    assert_eq!(collect_by_type(&usj, "table:row").len(), 2);

    let cells = collect_by_type(&usj, "table:cell");
    let text = cells
        .iter()
        .map(|cell| cell["content"][0].as_str().expect("cell should start with text"))
        .collect::<Vec<_>>();
    assert_eq!(
        text,
        vec![
            "Day ",
            "Tribe ",
            "Leader",
            "1st ",
            "Judah ",
            "Nahshon son of Amminadab",
        ]
    );

    let usx = parse(src).to_usx();
    assert!(usx.contains("<para style=\"tr\"><char style=\"th1\" closed=\"false\">Day </char>"));
    assert!(usx.contains("<char style=\"th3\" closed=\"false\">Leader</char>"));
    assert!(usx.contains("<char style=\"tc2\" closed=\"false\">Judah </char>"));
    assert!(usx.contains("<char style=\"tc3\" closed=\"false\">Nahshon son of Amminadab</char>"));

    let diag_doc = parse_with_diagnostics(src);
    assert!(
        !diag_doc.diagnostics().iter().any(|d| {
            matches!(d.code, DiagnosticCode::ImplicitClose | DiagnosticCode::UnclosedAtEof)
        }),
        "table rows/cells should close structurally without close diagnostics",
    );
}

#[test]
fn table_rows_close_cleanly_at_row_eof_and_following_block_boundaries() {
    for src in [
        "\\id GEN\n\\c 1\n\\tr \\th1 Day \\th2 Tribe \\th3 Leader\n\\tr \\tcr1 1st \\tc2 Judah \\tc3 Nahshon son of Amminadab",
        "\\id GEN\n\\c 1\n\\tr \\tc1 A \\tc2 B\n\\p\n\\v 1 After the table.\n\\c 2\n\\p\n\\v 1 Next chapter.",
    ] {
        let doc = parse_with_diagnostics(src);
        assert!(
            !doc.diagnostics().iter().any(|d| {
                matches!(d.code, DiagnosticCode::ImplicitClose | DiagnosticCode::UnclosedAtEof)
            }),
            "table structure should close cleanly for {src:?}: {:?}",
            doc.diagnostics(),
        );
    }
}

#[test]
fn table_row_newlines_do_not_become_trailing_cell_space() {
    let eof_usj = parse_usj("\\id GEN\n\\c 1\n\\tr \\tc1 A \\tc2 B\n");
    let eof_cells = collect_by_type(&eof_usj, "table:cell");
    assert_eq!(eof_cells[0]["content"][0].as_str(), Some("A "));
    assert_eq!(eof_cells[1]["content"][0].as_str(), Some("B"));

    let boundary_usj = parse_usj("\\id GEN\n\\c 1\n\\tr \\tc1 A \\tc2 B\n\\p\n\\v 1 After.");
    let boundary_cells = collect_by_type(&boundary_usj, "table:cell");
    assert_eq!(boundary_cells[0]["content"][0].as_str(), Some("A "));
    assert_eq!(boundary_cells[1]["content"][0].as_str(), Some("B"));
}

#[test]
fn table_cell_opening_whitespace_is_structural() {
    let usj = parse_usj("\\id GEN\n\\c 1\n\\tr \\tc1   Judah \\tc2   Issachar");
    let cells = collect_by_type(&usj, "table:cell");

    assert_eq!(cells[0]["content"][0].as_str(), Some("Judah "));
    assert_eq!(cells[1]["content"][0].as_str(), Some("Issachar"));
}

#[test]
fn marker_opening_whitespace_is_structural_and_closing_boundary_restores_space() {
    let usj = parse_usj("\\id GEN\n\\c 1\n\\p\n\\v 1 Start \\bd   bold\\bd*\ntail.");
    let chars = collect_by_type(&usj, "char");
    let bd = chars
        .iter()
        .find(|node| node["marker"] == "bd")
        .expect("\\bd should exist");
    assert_eq!(bd["content"][0].as_str(), Some("bold"));

    let dict = parse_vref("\\id GEN\n\\c 1\n\\p\n\\v 1 Start \\bd   bold\\bd*\ntail.");
    assert_eq!(vref_text(&dict, "GEN 1:1"), Some("Start bold tail."));
}

#[test]
fn paragraph_text_whitespace_is_normalized_to_single_spaces() {
    let dict = parse_vref("\\id GEN\n\\c 1\n\\p\n\\v 1 Alpha   beta\n   gamma");
    assert_eq!(vref_text(&dict, "GEN 1:1"), Some("Alpha beta gamma"));
}

#[test]
fn vref_handles_continuations_root_level_verses_and_boundary_spacing() {
    let poetry = parse(
        "\\id LUK\n\\c 1\n\\p\n\\v 48 for he has been mindful\n\\q2 of the humble state of his servant.\n\\q1 From now on all generations will call me blessed,\n\\q2\n\\v 49 for the Mighty One has done great things for me—\n\\q2 holy is his name.\n",
    )
    .vref_dict();
    assert_eq!(
        poetry[0],
        (
            "LUK 1:48".into(),
            "for he has been mindful of the humble state of his servant. From now on all generations will call me blessed,".into(),
        )
    );
    assert_eq!(
        poetry[1],
        (
            "LUK 1:49".into(),
            "for the Mighty One has done great things for me— holy is his name.".into(),
        )
    );

    let punctuation = parse(
        "\\id ACT\n\\c 1\n\\p\n\\v 17 He was counted among us.”\n\\p (\\v 18 Then he acquired a field.)\n\\p\n\\v 28 They will listen.”\n\\p [\\v 29 Then they departed.]",
    )
    .vref_dict();
    assert_eq!(punctuation[0].1, "He was counted among us.” (");
    assert_eq!(punctuation[2].1, "They will listen.” [");

    let root = parse(
        "\\id MRK\n\\c 9\n\\v 1 Truly I say to you, some standing here will not taste death.\n\\s The Transfiguration\n\\p\n\\v 2 After six days Jesus took Peter with him.",
    )
    .vref_dict();
    assert_eq!(root[0].1, "Truly I say to you, some standing here will not taste death.");
    assert_eq!(root[1].1, "After six days Jesus took Peter with him.");
}

#[test]
fn quoted_and_poetry_continuations_preserve_word_boundaries_and_quotes() {
    let luk = parse_vref(
        "\\id LUK\n\\c 4\n\\p\n\\v 10 For it is written:\n\\q1 “‘He will command his angels concerning you\n\\q2 to guard you carefully;\n",
    );
    assert_eq!(
        vref_text(&luk, "LUK 4:10"),
        Some("For it is written: “‘He will command his angels concerning you to guard you carefully;"),
    );

    let mrk = parse_vref(
        "\\id MRK\n\\c 1\n\\p\n\\v 2 As it is written in Isaiah the prophet,\n\\q1 “Behold, I send my messenger before your face.\n\\q1 He will prepare your way.\n\\q1\n\\v 3 A voice cries in the wilderness,\n\\q1 ‘Prepare the way of the Lord,\n\\q1 make his paths straight.’”",
    );
    assert_eq!(
        vref_text(&mrk, "MRK 1:2"),
        Some("As it is written in Isaiah the prophet, “Behold, I send my messenger before your face. He will prepare your way."),
    );
    assert_eq!(
        vref_text(&mrk, "MRK 1:3"),
        Some("A voice cries in the wilderness, ‘Prepare the way of the Lord, make his paths straight.’”"),
    );
}

#[test]
fn punctuation_boundaries_restore_spaces_in_vref() {
    let dash = parse_vref(
        "\\id DEU\n\\c 32\n\\q1 \\v 31 Because their rock is not like our Rock;\n\\q2 even our enemies themselves -\n\\q2 they agree that it is so.\n",
    );
    assert_eq!(
        vref_text(&dash, "DEU 32:31"),
        Some("Because their rock is not like our Rock; even our enemies themselves - they agree that it is so."),
    );

    let paragraph = parse_vref(
        "\\id MRK\n\\c 5\n\\p\n\\v 40 And they laughed at him.\n\\p He put them all outside and took the child's father and mother and those who were with him into the room.",
    );
    assert_eq!(
        vref_text(&paragraph, "MRK 5:40"),
        Some("And they laughed at him. He put them all outside and took the child's father and mother and those who were with him into the room."),
    );
}

#[test]
fn repeated_same_verse_markers_merge_and_exports_keep_normalized_labels() {
    let src = "\\id JOB\n\\c 2\n\\p\n\\v 9 How long will you hold fast your integrity?\n\\v 9‘A’ I will wait a little longer.\n\\v 9‘B’ My name has vanished from the earth.\n\\v 9‘C’ You sit in decay through the whole night.\n\\v 9‘D’ Curse God and die.\n\\v 10 But he said to her, “You speak as one of the foolish women speaks.”";
    let dict = parse(src).vref_dict();
    assert_eq!(
        dict[0],
        (
            "JOB 2:9".into(),
            "How long will you hold fast your integrity? ‘A’ I will wait a little longer. ‘B’ My name has vanished from the earth. ‘C’ You sit in decay through the whole night. ‘D’ Curse God and die.".into(),
        )
    );
    assert_eq!(
        dict[1],
        (
            "JOB 2:10".into(),
            "But he said to her, “You speak as one of the foolish women speaks.”".into(),
        )
    );

    let usfm = parse(src).to_usfm();
    assert!(usfm.contains("\\p \\v 9 How long will you hold fast your integrity?"));
    assert!(usfm.contains("\\v 9 ‘A’ I will wait a little longer."));
    assert!(!usfm.contains("\\v 9‘A’"));

    let usx = parse(src).to_usx();
    assert!(usx.contains("<verse number=\"9\" style=\"v\" sid=\"JOB 2:9\" />"));
    assert!(usx.contains("‘A’ I will wait a little longer."));
    assert!(!usx.contains("number=\"9‘A’\""));
}

#[test]
fn repeated_verse_markers_merge_in_vref_across_paragraph_breaks() {
    let dict = parse_vref(
        "\\id 2CH\n\\c 35\n\\p\n\\v 19 The Passover was kept in the eighteenth year of the king.\n\\p\n\\v 19 ‘A’ The king burned the idols.\n\\v 19 ‘B’ No king before him turned to the Lord like this.\n\\v 19 ‘C’ Yet the fierce anger of the Lord did not turn away.\n\\v 19 ‘D’ The Lord said, “I will remove Judah from before me.”\n\\v 20 After all this, the king went out to battle.",
    );
    assert_eq!(
        vref_text(&dict, "2CH 35:19"),
        Some("The Passover was kept in the eighteenth year of the king. ‘A’ The king burned the idols. ‘B’ No king before him turned to the Lord like this. ‘C’ Yet the fierce anger of the Lord did not turn away. ‘D’ The Lord said, “I will remove Judah from before me.”"),
    );
    assert_eq!(
        vref_text(&dict, "2CH 35:20"),
        Some("After all this, the king went out to battle."),
    );
}

#[test]
fn usfm_serialization_normalizes_spacing_for_continuation_cases() {
    let poetry = parse(
        "\\id MRK\n\\c 1\n\\p\n\\v 2 As it is written in Isaiah the prophet,\n\\q1 “Behold, I send my messenger before your face.\n\\q1 He will prepare your way.\n\\q1\n\\v 3 A voice cries in the wilderness,\n\\q1 ‘Prepare the way of the Lord,\n\\q1 make his paths straight.’”",
    )
    .to_usfm();
    assert!(poetry.contains("\\p \\v 2 As it is written in Isaiah the prophet,"));
    assert!(poetry.contains("\\q1 \\v 3 A voice cries in the wilderness,"));
    assert!(!poetry.contains("\\v 2  As it is written"));
    assert!(!poetry.contains("\\v 3  A voice cries"));

    let paragraph = parse(
        "\\id MRK\n\\c 5\n\\p\n\\v 40 And they laughed at him.\n\\p He put them all outside and took the child's father and mother and those who were with him into the room.",
    )
    .to_usfm();
    assert!(paragraph.contains("\\p \\v 40 And they laughed at him."));
    assert!(!paragraph.contains("\\v 40  And they laughed at him."));

    let root = parse(
        "\\id MRK\n\\c 9\n\\v 1 Truly I say to you, some standing here will not taste death.\n\\s The Transfiguration\n\\p\n\\v 2 After six days Jesus took Peter with him.",
    )
    .to_usfm();
    assert!(root.contains("\\c 9 \\v 1 Truly I say to you, some standing here will not taste death."));
    assert!(root.contains("\\s The Transfiguration"));
    assert!(root.contains("\\p \\v 2 After six days Jesus took Peter with him."));
    assert!(!root.contains("death. \n\\s"));
    assert!(!root.contains("\\v 2  After six days Jesus took Peter with him."));
}

#[test]
fn usx_serialization_preserves_structure_for_continuation_cases() {
    let poetry = parse(
        "\\id MRK\n\\c 1\n\\p\n\\v 2 As it is written in Isaiah the prophet,\n\\q1 “Behold, I send my messenger before your face.\n\\q1 He will prepare your way.\n\\q1\n\\v 3 A voice cries in the wilderness,\n\\q1 ‘Prepare the way of the Lord,\n\\q1 make his paths straight.’”",
    )
    .to_usx();
    assert!(poetry.contains("<para style=\"p\"><verse number=\"2\" style=\"v\" sid=\"MRK 1:2\" />As it is written in Isaiah the prophet,</para>"));
    assert!(poetry.contains("<para style=\"q1\"><verse eid=\"MRK 1:2\" /><verse number=\"3\" style=\"v\" sid=\"MRK 1:3\" />A voice cries in the wilderness,</para>"));
    assert!(poetry.contains("<para style=\"q1\">make his paths straight.’”</para>"));

    let root = parse(
        "\\id MRK\n\\c 9\n\\v 1 Truly I say to you, some standing here will not taste death.\n\\s The Transfiguration\n\\p\n\\v 2 After six days Jesus took Peter with him.",
    )
    .to_usx();
    assert!(root.contains("<para style=\"p\"><verse number=\"1\" style=\"v\" sid=\"MRK 9:1\" />Truly I say to you, some standing here will not taste death.</para>"));
    assert!(root.contains("<para style=\"s\">The Transfiguration</para>"));
    assert!(root.contains("<para style=\"p\"><verse eid=\"MRK 9:1\" /><verse number=\"2\" style=\"v\" sid=\"MRK 9:2\" />After six days Jesus took Peter with him.</para>"));
}

#[test]
fn parse_and_export_basic_document_covers_book_structure_and_note_stripping() {
    let src = "\\id GEN\n\\c 1\n\\s1 The Beginning\n\\p\n\\v 1 In the beginning \\w God|strong=\"H430\"\\w* created the heavens and the earth.\n\\v 2 Now the earth was formless and empty, \\f + \\ft Or possibly \"without form and void\"\\f* darkness was over the surface of the deep, and the Spirit of God was hovering over the waters.\n";
    let usj = parse_usj(src);
    assert_eq!(usj["content"][0]["code"], "GEN");
    assert!(usj["content"].as_array().is_some_and(|content| content.len() >= 3));

    let dict = parse_vref(src);
    let v1 = vref_text(&dict, "GEN 1:1").expect("GEN 1:1 should exist");
    assert!(v1.contains("In the beginning"));
    assert!(v1.contains("God"));
    assert!(v1.contains("the earth."));

    let v2 = vref_text(&dict, "GEN 1:2").expect("GEN 1:2 should exist");
    assert!(v2.contains("formless and empty,"));
    assert!(v2.contains("hovering over the waters."));
    assert!(!v2.contains("without form and void"));
}

#[test]
fn usfm_roundtrip_preserves_usj_structure_for_basic_document() {
    let src = "\\id GEN\n\\c 1\n\\s1 The Beginning\n\\p\n\\v 1 In the beginning \\w God|strong=\"H430\"\\w* created the heavens and the earth.\n\\v 2 Now the earth was formless and empty, \\f + \\ft Or possibly \"without form and void\"\\f* darkness was over the surface of the deep.";
    let first = parse(src).to_usj();
    let roundtripped = parse(&parse(src).to_usfm()).to_usj();

    let mut first: Value = serde_json::from_str(&first).expect("direct USJ should parse");
    let mut roundtripped: Value =
        serde_json::from_str(&roundtripped).expect("round-tripped USJ should parse");
    normalize_json(&mut first);
    normalize_json(&mut roundtripped);
    assert_eq!(first, roundtripped, "USFM export should round-trip through USJ");
}

#[test]
fn usx_output_has_root_element_and_closing_tag() {
    let usx = parse("\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning.").to_usx();
    assert!(usx.starts_with("<usx version=\"3.0\">"));
    assert!(usx.contains("<usx"));
    assert!(usx.contains("</usx>"));
}

#[test]
fn diagnostics_json_reports_unclosed_character_marker_with_location() {
    let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 \\nd Lord is missing close marker";
    let doc = parse_with_diagnostics(src);
    let diag = doc
        .diagnostics()
        .iter()
        .find(|d| d.code == DiagnosticCode::UnclosedAtEof || d.code == DiagnosticCode::ImplicitClose)
        .expect("unclosed character marker should emit a diagnostic");

    assert!(matches!(diag.severity, Severity::Error | Severity::Warning));

    let diagnostics_json: Value =
        serde_json::from_str(&doc.to_diagnostics_json()).expect("diagnostics JSON should parse");
    let items = diagnostics_json
        .as_array()
        .expect("diagnostics JSON should be an array");
    let item = items
        .iter()
        .find(|item| item["code"] == diag.code.as_str())
        .expect("matching diagnostic JSON entry should exist");
    assert_eq!(item["line"], 4);
    assert!(item["column"].as_u64().is_some_and(|col| col >= 6));
}

/// Malformed USFM: `\v11` (number fused to marker without a space) must parse
/// as verse 11 in USJ, USX, USFM export, and vref — not as verse 0 / empty string.
#[test]
fn verse_number_fused_to_marker_parses_as_verse_eleven() {
    let src = "\\id GEN\n\\c 1\n\\p \\v11 fused verse text";
    let usj = parse_usj(src);
    let verses = collect_by_type(&usj, "verse");
    assert_eq!(verses.len(), 1);
    assert_eq!(verses[0]["number"], "11");
    assert_eq!(verses[0]["sid"], "GEN 1:11");

    let vref = parse_vref(src);
    assert_eq!(vref[0].0, "GEN 1:11");
    assert!(vref[0].1.contains("fused verse text"), "vref: {:?}", vref[0]);

    let usx = parse(src).to_usx();
    assert!(usx.contains("number=\"11\""), "USX: {usx}");
    assert!(usx.contains("sid=\"GEN 1:11\""), "USX: {usx}");

    let usfm = parse(src).to_usfm();
    assert!(usfm.contains("\\v 11 fused verse text"), "USFM: {usfm}");
}

/// Old-style `\c N Title` (chapter title on the same line as the chapter number)
/// must be silently absorbed — the title text must not appear as a floating
/// root-level string in USJ/USX, and the following paragraph must parse normally.
#[test]
fn chapter_inline_title_is_absorbed_not_emitted_as_root_text() {
    let src = "\\id 2SA\n\\c 4 Ish-bosheth Murdered\n\\p\n\\v 1 When Abner's son Ish-bosheth heard";
    let usj = parse_usj(src);
    let content = usj["content"].as_array().expect("content should be an array");

    // Root-level strings are not valid USJ; none should be present.
    assert!(
        !content.iter().any(|node| node.is_string()),
        "inline chapter title must not leak as a root-level string: {content:?}"
    );
    // The chapter is still present with the correct number.
    let chapters = collect_by_type(&usj, "chapter");
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0]["number"], "4");
    assert_eq!(chapters[0]["sid"], "2SA 4");

    // The following paragraph and verse parse normally.
    let verses = collect_by_type(&usj, "verse");
    assert_eq!(verses.len(), 1);
    assert_eq!(verses[0]["number"], "1");
}

/// Deprecated USFM 2.x positional figure syntax
/// (`\fig DESC|FILE|SIZE|LOC|COPY|CAP|REF\fig*`) must be mapped to the USFM 3.0
/// named attributes (alt/file/size/loc/copy/ref) with the CAP field as the caption
/// content — NOT emitted as a single `lemma` field holding the raw bar-list.
/// Spec: deprecated illustration syntax at ubsicap.github.io/usfm/characters.
#[test]
fn deprecated_positional_figure_maps_fields_to_named_attributes() {
    let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 Text \\fig Sea of Galilee|avnt016.tif|span|||At once they left their nets.|1.18\\fig* more.";
    let usj = parse_usj(src);
    let fig = collect_by_type(&usj, "figure")
        .into_iter()
        .next()
        .expect("figure should exist");

    // The raw bar-list must NOT survive as a `lemma` (the old buggy behavior).
    assert!(fig.get("lemma").is_none(), "figure should not carry a lemma: {fig}");
    // Positional fields are mapped to their named attributes; empties dropped.
    assert_eq!(fig["alt"], "Sea of Galilee");
    assert_eq!(fig["file"], "avnt016.tif");
    assert_eq!(fig["size"], "span");
    assert!(fig.get("loc").is_none(), "empty LOC field must be dropped");
    assert!(fig.get("copy").is_none(), "empty COPY field must be dropped");
    assert_eq!(fig["ref"], "1.18");
    // CAP is the caption (figure element content).
    assert_eq!(fig["content"][0], "At once they left their nets.");

    // USX carries the same named attributes with CAP as element text.
    let usx = parse(src).to_usx();
    assert!(usx.contains("alt=\"Sea of Galilee\""), "USX: {usx}");
    assert!(usx.contains("file=\"avnt016.tif\""), "USX: {usx}");
    assert!(usx.contains("ref=\"1.18\""), "USX: {usx}");
    assert!(usx.contains(">At once they left their nets.</figure>"), "USX: {usx}");
    assert!(!usx.contains("lemma="), "USX must not emit a lemma attribute: {usx}");

    // USFM round-trips the deprecated positional form verbatim (no lemma corruption).
    let usfm = parse(src).to_usfm();
    assert!(
        usfm.contains("\\fig Sea of Galilee|avnt016.tif|span|||At once they left their nets.|1.18\\fig*"),
        "USFM round-trip must preserve the positional figure: {usfm}"
    );
    assert!(!usfm.contains("lemma="), "USFM must not introduce a lemma attribute: {usfm}");
}

/// Modern `\fig caption|src="…" …\fig*` is unaffected by the deprecated-form
/// handling: the leading text is the caption, named attributes inline, and `src`
/// maps to the `file` USJ/USX key.
#[test]
fn modern_figure_attributes_are_unaffected_by_legacy_handling() {
    let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 Text \\fig At once they left their nets.|src=\"avnt016.tif\" size=\"span\" ref=\"1.18\"\\fig* more.";
    let usj = parse_usj(src);
    let fig = collect_by_type(&usj, "figure")
        .into_iter()
        .next()
        .expect("figure should exist");
    assert_eq!(fig["file"], "avnt016.tif");
    assert_eq!(fig["size"], "span");
    assert_eq!(fig["ref"], "1.18");
    assert!(fig.get("alt").is_none(), "modern form has no alt here: {fig}");
    assert_eq!(fig["content"][0], "At once they left their nets.");
}

/// USJ schema marks `content` optional (markerObject requires only `type`). We emit
/// `content:[]` for empty paragraph containers (uniform shape) but omit it entirely
/// for an empty `\b`, which the spec says takes no content ("No text should follow
/// this marker" — ubsicap.github.io/usfm/paragraphs).
#[test]
fn empty_paragraph_emits_content_array_but_empty_b_omits_it() {
    let src = "\\id GEN\n\\c 1\n\\p\n\\b\n\\p\n\\v 1 Verse.";
    let usj = parse_usj(src);
    let paras = collect_by_type(&usj, "para");

    let b = paras.iter().find(|p| p["marker"] == "b").expect("\\b should exist");
    assert!(b.get("content").is_none(), "empty \\b must omit content: {b}");

    // The empty \p preceding \b must carry an explicit empty content array.
    let empty_p = paras
        .iter()
        .find(|p| p["marker"] == "p" && p["content"].as_array().is_some_and(|a| a.is_empty()))
        .expect("an empty \\p with content:[] should exist");
    assert!(empty_p["content"].as_array().unwrap().is_empty());
}
