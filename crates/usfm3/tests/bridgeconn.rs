use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use usfm3::builder;
use usfm3::usj;
use usfm3::validation;

const FIXTURE_ROOT: &str = "tests/fixtures/usfm-grammar";
const EXPECTED_FAILURES_FILE: &str = "tests/bridgeconn_expected_failures.txt";

// ---------------------------------------------------------------------------
// Test case discovery
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct TestCase {
    /// Relative path from FIXTURE_ROOT, e.g. "basic/minimal"
    name: String,
    /// Whether BridgeConn marks this as "pass" or "fail"
    validated_pass: bool,
    /// Full path to origin.usfm
    usfm_path: PathBuf,
    /// Full path to origin.json
    json_path: PathBuf,
}

fn discover_test_cases() -> Vec<TestCase> {
    let root = Path::new(FIXTURE_ROOT);
    if !root.exists() {
        panic!(
            "Fixture directory not found: {FIXTURE_ROOT}\n\
             Run: npx degit Bridgeconn/usfm-grammar/tests {FIXTURE_ROOT}"
        );
    }

    let mut cases = Vec::new();
    walk_dir(root, root, &mut cases);
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}

fn walk_dir(dir: &Path, root: &Path, cases: &mut Vec<TestCase>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let usfm_path = path.join("origin.usfm");
            let json_path = path.join("origin.json");

            if usfm_path.exists() && json_path.exists() {
                let rel = path.strip_prefix(root).unwrap();
                let name = rel.to_string_lossy().to_string();
                let metadata_path = path.join("metadata.xml");
                let validated_pass = parse_metadata(&metadata_path);

                cases.push(TestCase {
                    name,
                    validated_pass,
                    usfm_path,
                    json_path,
                });
            }

            walk_dir(&path, root, cases);
        }
    }
}

fn parse_metadata(path: &Path) -> bool {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    // Simple string check - avoids full XML parsing
    !content.contains("<validated>fail</validated>")
}

// ---------------------------------------------------------------------------
// Expected failures
// ---------------------------------------------------------------------------

fn load_expected_failures() -> HashSet<String> {
    let content = std::fs::read_to_string(EXPECTED_FAILURES_FILE).unwrap_or_default();
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// USJ normalization
// ---------------------------------------------------------------------------

/// Standard keys per node type that are NOT attributes.
fn standard_keys(node_type: &str) -> &'static [&'static str] {
    match node_type {
        "ms" => &["type", "marker"],
        "char" => &["type", "marker", "content"],
        "figure" => &["type", "marker", "content"],
        "chapter" => &[
            "type",
            "marker",
            "number",
            "sid",
            "altnumber",
            "pubnumber",
            "content",
        ],
        "verse" => &[
            "type",
            "marker",
            "number",
            "sid",
            "altnumber",
            "pubnumber",
            "content",
        ],
        "book" => &["type", "marker", "code", "content"],
        "para" => &["type", "marker", "content"],
        "note" => &["type", "marker", "caller", "category", "content"],
        "table" => &["type", "content"],
        "table:row" => &["type", "marker", "content"],
        "table:cell" => &["type", "marker", "align", "content"],
        "sidebar" => &["type", "marker", "category", "content"],
        _ => &["type", "marker", "content"],
    }
}

/// Normalize a USJ JSON value for comparison.
///
/// Transforms both our output and BridgeConn's expected output into a
/// common format so structural differences don't cause spurious failures.
fn normalize_for_comparison(value: &mut Value) {
    normalize_for_comparison_inner(value, None);
}

fn preserve_trailing_content_space(node_type: Option<&str>) -> bool {
    matches!(node_type, Some("char" | "ref" | "table:cell"))
}

fn normalize_for_comparison_inner(value: &mut Value, _parent_type: Option<&str>) {
    match value {
        Value::Object(map) => {
            let node_type = map.get("type").and_then(|v| v.as_str()).map(String::from);

            if let Some(ref typ) = node_type {
                let std_keys = standard_keys(typ);

                // Normalize "href" top-level key to "link-href" before
                // collecting non-standard keys (BridgeConn uses both names).
                if let Some(val) = map.remove("href") {
                    map.entry("link-href".to_string()).or_insert(val);
                }

                // Collect non-standard keys as attributes
                let extra_keys: Vec<String> = map
                    .keys()
                    .filter(|k| !std_keys.contains(&k.as_str()) && k.as_str() != "attributes")
                    .cloned()
                    .collect();

                if !extra_keys.is_empty() {
                    // Extract existing attributes array if present
                    let mut attrs: Vec<Value> = map
                        .remove("attributes")
                        .and_then(|v| match v {
                            Value::Array(a) => Some(a),
                            _ => None,
                        })
                        .unwrap_or_default();

                    // Move extra keys into attributes array
                    for key in &extra_keys {
                        if let Some(val) = map.remove(key) {
                            let val_str = match &val {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            attrs.push(serde_json::json!({"key": key, "value": val_str}));
                        }
                    }

                    if !attrs.is_empty() {
                        // Sort attributes by key for stable comparison
                        attrs.sort_by(|a, b| {
                            let ak = a.get("key").and_then(|v| v.as_str()).unwrap_or("");
                            let bk = b.get("key").and_then(|v| v.as_str()).unwrap_or("");
                            ak.cmp(bk)
                        });
                        map.insert("attributes".to_string(), Value::Array(attrs));
                    }
                } else if let Some(Value::Array(attrs)) = map.get_mut("attributes") {
                    // Sort existing attributes by key
                    attrs.sort_by(|a, b| {
                        let ak = a.get("key").and_then(|v| v.as_str()).unwrap_or("");
                        let bk = b.get("key").and_then(|v| v.as_str()).unwrap_or("");
                        ak.cmp(bk)
                    });
                }

                // Normalize attribute key aliases: BridgeConn uses both
                // "href" and "link-href" for \xt default attribute across
                // different fixtures. Canonicalize to "link-href".
                if let Some(Value::Array(attrs)) = map.get_mut("attributes") {
                    for attr in attrs.iter_mut() {
                        if let Value::Object(attr_map) = attr
                            && attr_map.get("key").and_then(|v| v.as_str()) == Some("href")
                        {
                            attr_map
                                .insert("key".to_string(), Value::String("link-href".to_string()));
                        }
                    }
                }

                // Remove empty content arrays (we omit them, BridgeConn includes them)
                if let Some(Value::Array(arr)) = map.get("content")
                    && arr.is_empty()
                {
                    map.remove("content");
                }
            }

            // Recurse into content arrays
            if let Some(Value::Array(arr)) = map.get_mut("content") {
                for item in &mut *arr {
                    normalize_for_comparison_inner(item, node_type.as_deref());
                }

                // ── Note sub-marker leading-whitespace normalization ──
                // USFM spec: the space after an opening marker is structural.
                // Our parser preserves it for non-first note sub-markers (as
                // a word boundary), while BridgeConn does so inconsistently —
                // some transitions preserve it, others with identical USFM
                // structure don't.  Normalize both sides by stripping leading
                // whitespace from the first text element of each char child
                // within a note.
                if node_type.as_deref() == Some("note") {
                    for item in arr.iter_mut() {
                        if let Value::Object(child_map) = item
                            && child_map.get("type").and_then(|v| v.as_str()) == Some("char")
                        {
                            if let Some(Value::Array(cc)) = child_map.get_mut("content") {
                                if let Some(Value::String(s)) = cc.first_mut() {
                                    *s = s.trim_start().to_string();
                                }
                                // Remove empty strings left over from trimming
                                cc.retain(|v| !matches!(v, Value::String(s) if s.is_empty()));
                            }
                            // Remove empty content arrays
                            if child_map
                                .get("content")
                                .is_some_and(|v| matches!(v, Value::Array(a) if a.is_empty()))
                            {
                                child_map.remove("content");
                            }
                        }
                    }
                }

                // ── General whitespace normalization ──
                // USFM spec: "Multiple whitespace between words are normalized
                // to a single space."  Our parser enforces this, but BridgeConn
                // sometimes preserves extra whitespace.  Normalize both sides.
                for item in arr.iter_mut() {
                    if let Value::String(s) = item {
                        let had_newline = s.contains('\n');
                        // Replace literal newlines with space
                        if had_newline {
                            *s = s.replace('\n', " ");
                        }
                        // Collapse multiple spaces to single
                        while s.contains("  ") {
                            *s = s.replace("  ", " ");
                        }
                        if had_newline || !preserve_trailing_content_space(node_type.as_deref()) {
                            let trimmed = s.trim_end();
                            if trimmed.len() != s.len() {
                                *s = trimmed.to_string();
                            }
                        }
                    }
                }
                // Strip leading space from text nodes that follow non-text
                // elements — our parser skips whitespace after opening markers,
                // while BridgeConn may preserve it.
                {
                    let mut prev_was_nontext = false;
                    for item in arr.iter_mut() {
                        match item {
                            Value::String(s) => {
                                if prev_was_nontext && s.starts_with(' ') {
                                    *s = s[1..].to_string();
                                }
                                prev_was_nontext = false;
                            }
                            _ => {
                                prev_was_nontext = true;
                            }
                        }
                    }
                }
                // Remove empty and whitespace-only string entries.
                arr.retain(|item| !matches!(item, Value::String(s) if s.trim().is_empty()));
            }
        }
        Value::Array(arr) => {
            for item in arr {
                normalize_for_comparison_inner(item, _parent_type);
            }
        }
        Value::String(_) => {}
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// JSON diff
// ---------------------------------------------------------------------------

fn diff_values(path: &str, ours: &Value, expected: &Value) -> Vec<String> {
    let mut diffs = Vec::new();
    match (ours, expected) {
        (Value::Object(a), Value::Object(b)) => {
            let all_keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in all_keys {
                let child_path = format!("{path}.{key}");
                match (a.get(key), b.get(key)) {
                    (Some(va), Some(vb)) => {
                        diffs.extend(diff_values(&child_path, va, vb));
                    }
                    (Some(va), None) => {
                        diffs.push(format!("{child_path}: extra in ours = {va}"));
                    }
                    (None, Some(vb)) => {
                        diffs.push(format!("{child_path}: missing in ours (expected {vb})"));
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                diffs.push(format!("{path}: array length {} vs {}", a.len(), b.len()));
            }
            for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
                diffs.extend(diff_values(&format!("{path}[{i}]"), va, vb));
            }
        }
        _ => {
            if ours != expected {
                diffs.push(format!("{path}: {ours} vs {expected}"));
            }
        }
    }
    diffs
}

// ---------------------------------------------------------------------------
// Run a single test case
// ---------------------------------------------------------------------------

enum TestResult {
    Pass,
    Fail(Vec<String>),
    Skipped(String),
}

fn run_test_case(case: &TestCase) -> TestResult {
    // Read input USFM
    let usfm = match std::fs::read_to_string(&case.usfm_path) {
        Ok(s) => s,
        Err(e) => return TestResult::Skipped(format!("cannot read usfm: {e}")),
    };

    // Parse with our parser
    let result = builder::parse(&usfm);

    // For "fail" tests, check that we produce diagnostics (from parsing or validation).
    if !case.validated_pass {
        let has_parse_errors = result.diagnostics.has_errors();
        let validation_diags = validation::validate(&result.document);
        let has_validation_errors = validation_diags.has_errors();
        if has_parse_errors || has_validation_errors {
            return TestResult::Pass;
        } else {
            return TestResult::Fail(vec![
                "BridgeConn marks as 'fail' but we produced no error diagnostics".to_string(),
            ]);
        }
    }

    // For "pass" tests, compare USJ output
    let mut our_usj = match usj::to_usj_value(&result.document) {
        Ok(v) => v,
        Err(e) => return TestResult::Fail(vec![format!("USJ serialization error: {e}")]),
    };

    let expected_str = match std::fs::read_to_string(&case.json_path) {
        Ok(s) => s,
        Err(e) => return TestResult::Skipped(format!("cannot read json: {e}")),
    };

    let mut expected_usj: Value = match serde_json::from_str(&expected_str) {
        Ok(v) => v,
        Err(e) => return TestResult::Fail(vec![format!("cannot parse expected json: {e}")]),
    };

    // Normalize both sides
    normalize_for_comparison(&mut our_usj);
    normalize_for_comparison(&mut expected_usj);

    // Compare
    let diffs = diff_values("$", &our_usj, &expected_usj);
    if diffs.is_empty() {
        TestResult::Pass
    } else {
        TestResult::Fail(diffs)
    }
}

// ---------------------------------------------------------------------------
// Summary reporting
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CategoryStats {
    total: usize,
    pass: usize,
    expected_fail: usize,
    unexpected_fail: usize,
    unexpected_pass: usize,
    skipped: usize,
}

fn print_summary(results: &BTreeMap<String, (TestResult, bool)>) -> (Vec<String>, Vec<String>) {
    let mut by_category: BTreeMap<String, CategoryStats> = BTreeMap::new();
    let mut unexpected_failures = Vec::new();
    let mut unexpected_passes = Vec::new();

    for (name, (result, is_expected_failure)) in results {
        let category = name.split('/').next().unwrap_or("unknown").to_string();
        let stats = by_category.entry(category).or_default();
        stats.total += 1;

        match (result, *is_expected_failure) {
            (TestResult::Pass, false) => stats.pass += 1,
            (TestResult::Pass, true) => {
                stats.unexpected_pass += 1;
                unexpected_passes.push(name.clone());
            }
            (TestResult::Fail(_), true) => stats.expected_fail += 1,
            (TestResult::Fail(diffs), false) => {
                stats.unexpected_fail += 1;
                unexpected_failures.push(name.clone());
                eprintln!("\n--- UNEXPECTED FAIL: {name} ---");
                for d in diffs.iter().take(5) {
                    eprintln!("  {d}");
                }
                if diffs.len() > 5 {
                    eprintln!("  ... and {} more diffs", diffs.len() - 5);
                }
            }
            (TestResult::Skipped(reason), _) => {
                stats.skipped += 1;
                eprintln!("SKIP: {name}: {reason}");
            }
        }
    }

    // Print summary table
    eprintln!("\n{:=<78}", "= BridgeConn Test Suite Results ");
    eprintln!(
        "{:<22} {:>5} {:>5} {:>7} {:>9} {:>9} {:>5}",
        "Category", "Total", "Pass", "ExpFail", "UnexpFail", "UnexpPass", "Skip"
    );
    eprintln!("{:-<78}", "");

    let mut totals = CategoryStats::default();
    for (cat, stats) in &by_category {
        eprintln!(
            "{:<22} {:>5} {:>5} {:>7} {:>9} {:>9} {:>5}",
            cat,
            stats.total,
            stats.pass,
            stats.expected_fail,
            stats.unexpected_fail,
            stats.unexpected_pass,
            stats.skipped
        );
        totals.total += stats.total;
        totals.pass += stats.pass;
        totals.expected_fail += stats.expected_fail;
        totals.unexpected_fail += stats.unexpected_fail;
        totals.unexpected_pass += stats.unexpected_pass;
        totals.skipped += stats.skipped;
    }

    eprintln!("{:-<78}", "");
    eprintln!(
        "{:<22} {:>5} {:>5} {:>7} {:>9} {:>9} {:>5}",
        "TOTAL",
        totals.total,
        totals.pass,
        totals.expected_fail,
        totals.unexpected_fail,
        totals.unexpected_pass,
        totals.skipped
    );
    eprintln!("{:=<78}", "");

    (unexpected_failures, unexpected_passes)
}

// ---------------------------------------------------------------------------
// Main test
// ---------------------------------------------------------------------------

#[test]
fn bridgeconn_test_suite() {
    let cases = discover_test_cases();
    let expected_failures = load_expected_failures();

    eprintln!("\nDiscovered {} test cases", cases.len());

    let mut results: BTreeMap<String, (TestResult, bool)> = BTreeMap::new();

    for case in &cases {
        let is_expected_failure = expected_failures.contains(&case.name);
        let result = run_test_case(case);
        results.insert(case.name.clone(), (result, is_expected_failure));
    }

    let (unexpected_failures, unexpected_passes) = print_summary(&results);

    if !unexpected_passes.is_empty() {
        eprintln!("\n=== UNEXPECTED PASSES (remove from expected_failures.txt) ===");
        for name in &unexpected_passes {
            eprintln!("  {name}");
        }
    }

    if !unexpected_failures.is_empty() {
        eprintln!("\n=== UNEXPECTED FAILURES (add to expected_failures.txt or fix) ===");
        for name in &unexpected_failures {
            eprintln!("  {name}");
        }
    }

    assert!(
        unexpected_failures.is_empty(),
        "{} unexpected failure(s) - see details above.\n\
         To mark as expected, add to {EXPECTED_FAILURES_FILE}",
        unexpected_failures.len()
    );
}
