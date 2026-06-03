//! BridgeConn / usfm-grammar conformance corpus (copied from the usfm-grammar
//! project). For each `<case>/origin.usfm` we parse and compare our USJ output to
//! the expected `<case>/origin.json`.
//!
//! Whitespace policy: USFM↔USJ round-tripping is lossy on whitespace, so per
//! project policy whitespace is NOT authoritative — both sides are normalized
//! (collapse runs, trim, drop empty strings) before comparison.
//!
//! Cases marked `<validated>fail</validated>` are reported separately: full
//! validation diagnostics are a later milestone, so they don't gate the suite.
//! Pass-cases gate against `bridgeconn_expected_failures.txt`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/usfm-grammar");
const EXPECTED_FAILURES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/bridgeconn_expected_failures.txt");

struct Case {
    name: String,
    validated_pass: bool,
    usfm: PathBuf,
    json: PathBuf,
}

fn discover() -> Vec<Case> {
    let root = Path::new(ROOT);
    assert!(root.exists(), "fixture corpus missing at {ROOT}");
    let mut cases = Vec::new();
    walk(root, root, &mut cases);
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}

fn walk(dir: &Path, root: &Path, cases: &mut Vec<Case>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let usfm = path.join("origin.usfm");
        let json = path.join("origin.json");
        if usfm.exists() && json.exists() {
            let name = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            let meta = std::fs::read_to_string(path.join("metadata.xml")).unwrap_or_default();
            cases.push(Case {
                name,
                validated_pass: meta.contains("<validated>pass</validated>"),
                usfm,
                json,
            });
        }
        walk(&path, root, cases);
    }
}

fn load_expected_failures() -> HashSet<String> {
    std::fs::read_to_string(EXPECTED_FAILURES)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

/// Whitespace-insensitive normalization: collapse internal whitespace + trim every
/// string, and drop now-empty strings from arrays.
fn normalize(v: &mut Value) {
    match v {
        Value::String(s) => *s = collapse(s),
        Value::Array(items) => {
            for item in items.iter_mut() {
                normalize(item);
            }
            items.retain(|i| !matches!(i, Value::String(s) if s.is_empty()));
        }
        Value::Object(map) => {
            for (_, val) in map.iter_mut() {
                normalize(val);
            }
        }
        _ => {}
    }
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn compare_usj(case: &Case) -> Result<(), String> {
    let usfm = std::fs::read_to_string(&case.usfm).map_err(|e| e.to_string())?;
    let doc = usfm3::parse(&usfm, usfm3::Version::V3_1);
    let ours = doc.to_usj();
    let mut ours: Value =
        serde_json::from_str(&ours).map_err(|e| format!("our USJ is invalid JSON: {e}\n{ours}"))?;
    let expected = std::fs::read_to_string(&case.json).map_err(|e| e.to_string())?;
    let mut expected: Value = serde_json::from_str(&expected).map_err(|e| e.to_string())?;
    normalize(&mut ours);
    normalize(&mut expected);
    if ours == expected {
        Ok(())
    } else {
        Err(format!(
            "USJ mismatch\n  ours:     {}\n  expected: {}",
            ours, expected
        ))
    }
}

#[test]
fn bridgeconn_usj() {
    let cases = discover();
    let expected_failures = load_expected_failures();

    let (pass_cases, fail_cases): (Vec<_>, Vec<_>) =
        cases.iter().partition(|c| c.validated_pass);

    let mut passed = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();
    for c in &pass_cases {
        match compare_usj(c) {
            Ok(()) => passed += 1,
            Err(reason) => failures.push((c.name.clone(), reason)),
        }
    }

    // Fail-cases: do we currently emit an error diagnostic? (Reported only.)
    let mut fail_detected = 0usize;
    for c in &fail_cases {
        if let Ok(usfm) = std::fs::read_to_string(&c.usfm) {
            let opts = usfm3::ParseOptions { diagnostics: true };
            let doc = usfm3::parse_with(&usfm, usfm3::Version::V3_1, opts);
            if doc.diagnostics().iter().any(|d| d.severity == usfm3::Severity::Error) {
                fail_detected += 1;
            }
        }
    }

    eprintln!(
        "\nbridgeconn USJ: {}/{} pass-cases matched; fail-cases detected {}/{}",
        passed,
        pass_cases.len(),
        fail_detected,
        fail_cases.len()
    );

    let failed_names: HashSet<&str> = failures.iter().map(|(n, _)| n.as_str()).collect();
    let unexpected: Vec<&(String, String)> = failures
        .iter()
        .filter(|(n, _)| !expected_failures.contains(n))
        .collect();
    let fixed: Vec<&String> = expected_failures
        .iter()
        .filter(|n| !failed_names.contains(n.as_str()))
        .collect();

    if !unexpected.is_empty() {
        eprintln!("\n{} unexpected pass-case failures:", unexpected.len());
        for (name, reason) in &unexpected {
            eprintln!("  FAIL {name}: {}", reason.lines().next().unwrap_or(""));
        }
    }
    if !fixed.is_empty() {
        eprintln!("\n{} cases now pass — remove from expected-failures:", fixed.len());
        for name in &fixed {
            eprintln!("  FIXED {name}");
        }
    }

    assert!(unexpected.is_empty(), "{} unexpected USJ failures", unexpected.len());
}

/// Exporting to USFM and re-parsing must yield the same USJ (whitespace-normalized)
/// for every pass-case we already serialize correctly — i.e. the exporter is
/// faithful wherever the serializer is.
#[test]
fn bridgeconn_usfm_roundtrip() {
    let expected_failures = load_expected_failures();
    let mut checked = 0usize;
    let mut broken: Vec<String> = Vec::new();

    for case in discover().iter().filter(|c| c.validated_pass) {
        // Only round-trip cases whose USJ we already match (skip known gaps).
        if expected_failures.contains(&case.name) || compare_usj(case).is_err() {
            continue;
        }
        checked += 1;
        let usfm = std::fs::read_to_string(&case.usfm).unwrap();
        let direct = usfm3::parse(&usfm, usfm3::Version::V3_1).to_usj();
        let exported = usfm3::parse(&usfm, usfm3::Version::V3_1).to_usfm();
        let round = usfm3::parse(&exported, usfm3::Version::V3_1).to_usj();

        let mut a: Value = serde_json::from_str(&direct).unwrap();
        let mut b: Value = serde_json::from_str(&round).unwrap();
        normalize(&mut a);
        normalize(&mut b);
        if a != b {
            broken.push(case.name.clone());
        }
    }

    eprintln!("\nUSFM round-trip: {}/{} matched", checked - broken.len(), checked);
    for name in &broken {
        eprintln!("  ROUND-TRIP DIFF {name}");
    }
    assert!(broken.is_empty(), "{} round-trip mismatches", broken.len());
}
