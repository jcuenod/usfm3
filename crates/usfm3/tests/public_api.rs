use usfm3::{ParseOptions, parse, parse_ast, parse_cst, tokenize};

const SAMPLE: &str = "\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning";

#[test]
fn lazy_parse_handle_supports_core_workflow() {
    let parsed = parse(SAMPLE, ParseOptions::default());

    assert!(!parsed.tokens().is_empty());
    assert!(!parsed.cst().leaf_ids().is_empty());
    assert!(!parsed.ast().content.is_empty());
    assert!(parsed.source_map().content.len() == parsed.ast().content.len());
    assert!(parsed.diagnostics().is_none());
    assert!(
        parsed
            .to_usj(usfm3::usj::UsjOptions {
                include_spans: true,
            })
            .is_ok()
    );
}

#[test]
fn eager_ast_can_include_diagnostics() {
    let ast_document = parse_ast(
        "\\id BAD\n\\c 1\n\\v 1 Text",
        ParseOptions { diagnostics: true },
    );

    assert!(ast_document.diagnostics.is_some());
    assert_eq!(
        ast_document.ast.content.len(),
        ast_document.source_map.content.len()
    );
}

#[test]
fn tokenize_and_parse_cst_are_first_class_entry_points() {
    let tokens = tokenize(SAMPLE);
    let cst = parse_cst(SAMPLE);

    assert!(!tokens.is_empty());
    assert!(!cst.leaf_ids().is_empty());
}
