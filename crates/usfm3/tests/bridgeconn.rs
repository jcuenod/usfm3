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
const EXPECTED_FAILURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/bridgeconn_expected_failures.txt"
);

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
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let usfm = path.join("origin.usfm");
        let json = path.join("origin.json");
        if usfm.exists() && json.exists() {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
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

    let (pass_cases, fail_cases): (Vec<_>, Vec<_>) = cases.iter().partition(|c| c.validated_pass);

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
            if doc
                .diagnostics()
                .iter()
                .any(|d| d.severity == usfm3::Severity::Error)
            {
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
        eprintln!(
            "\n{} cases now pass — remove from expected-failures:",
            fixed.len()
        );
        for name in &fixed {
            eprintln!("  FIXED {name}");
        }
    }

    assert!(
        unexpected.is_empty(),
        "{} unexpected USJ failures",
        unexpected.len()
    );
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

    eprintln!(
        "\nUSFM round-trip: {}/{} matched",
        checked - broken.len(),
        checked
    );
    for name in &broken {
        eprintln!("  ROUND-TRIP DIFF {name}");
    }
    assert!(broken.is_empty(), "{} round-trip mismatches", broken.len());
}

/// Every diagnostic's `line`/`column`, as produced by the shared forward-only
/// `OffsetMap`, must equal what per-call `Span::line_col` would have produced.
///
/// This is the compatibility proof for making `to_diagnostics_json` linear: the
/// scanner is amortized across the whole array instead of rescanning the prefix per
/// diagnostic, so the risk is not that it is slow but that it *drifts* — and it walks
/// every fixture in the corpus here, non-ASCII ones included.
#[test]
fn diagnostic_line_col_matches_span_line_col() {
    use usfm3::Offsets;
    use usfm3::low_level::OffsetMap;

    let mut checked = 0usize;
    for case in discover() {
        let usfm = std::fs::read_to_string(&case.usfm).unwrap();
        let doc = usfm3::parse_with(
            &usfm,
            usfm3::Version::V3_1,
            usfm3::ParseOptions { diagnostics: true },
        );
        // One map for the whole array, exactly as the serializer uses it.
        let mut map = OffsetMap::new(doc.source(), Offsets::Utf8);
        for d in doc.diagnostics() {
            let point = map.point_at(d.span.start);
            let lc = d.span.line_col(doc.source());
            assert_eq!(
                (point.offset, point.line, point.col),
                (d.span.start, lc.line, lc.col),
                "{}: diagnostic {:?} at byte {}",
                case.name,
                d.code,
                d.span.start
            );
            assert_eq!(
                map.span_len(d.span.start, d.span.end()),
                d.span.len,
                "{}: utf8 span_len must be the byte length",
                case.name
            );
            checked += 1;
        }
    }
    eprintln!("\nline/col parity: {checked} diagnostics across the corpus");
    assert!(checked > 0, "corpus produced no diagnostics to check");
}

mod common;

/// Rebuild the CST JSON from a packed tape and compare it against `to_cst_json()` for
/// every fixture in the corpus.
///
/// This is the structural proof for the packed wire format. It exercises all four
/// `link` overloads, the `Root.link`-is-not-a-close quirk, the marker-info
/// availability rule, close-folding, and the child/sibling traversal contract —
/// against 266 real documents rather than a handful of hand-written cases. Both sides
/// use `utf8`, so non-ASCII fixtures are included rather than skipped.
#[test]
fn packed_tape_matches_cst_json_across_the_corpus() {
    use common::Packed;
    use usfm3::Offsets;

    let schema: Value = serde_json::from_str(&usfm3::schema_json(usfm3::Version::V3_1)).unwrap();
    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for case in discover() {
        let usfm = std::fs::read_to_string(&case.usfm).unwrap();
        let doc = usfm3::parse(&usfm, usfm3::Version::V3_1);
        let packed = Packed::new(doc.to_packed_tape(Offsets::Utf8));
        let rebuilt = packed.to_cst_json(doc.source(), &schema);
        let expected: Value = serde_json::from_str(&doc.to_cst_json()).unwrap();
        if rebuilt == expected {
            checked += 1;
        } else {
            mismatches.push(case.name.clone());
        }
    }

    eprintln!(
        "\npacked CST parity: {checked}/{} fixtures",
        checked + mismatches.len()
    );
    for name in mismatches.iter().take(10) {
        eprintln!("  PACKED CST DIFF {name}");
    }
    assert!(
        mismatches.is_empty(),
        "{} packed/JSON CST mismatches",
        mismatches.len()
    );
}

/// The packed offsets must slice the host's own encoding exactly, for every fixture and
/// both host spaces.
///
/// Non-ASCII is the norm rather than the exception here: 103 of the 242 fixtures this
/// walks contain non-ASCII bytes (119 of the 266 `origin.usfm` files overall). Real
/// translation text is never plain ASCII, so an offset bug is not an edge case — it is
/// the default outcome. Running the whole corpus in both host spaces makes that
/// unmissable.
#[test]
fn packed_offsets_are_slice_exact_across_the_corpus() {
    use common::Packed;
    use usfm3::Offsets;

    let mut non_ascii = 0usize;
    for case in discover() {
        let usfm = std::fs::read_to_string(&case.usfm).unwrap();
        if !usfm.is_ascii() {
            non_ascii += 1;
        }
        let doc = usfm3::parse(&usfm, usfm3::Version::V3_1);
        let units: Vec<u16> = usfm.encode_utf16().collect();
        let scalars: Vec<char> = usfm.chars().collect();

        let utf16 = Packed::new(doc.to_packed_tape(Offsets::Utf16));
        let chars = Packed::new(doc.to_packed_tape(Offsets::Chars));
        assert_eq!(
            utf16.lengths().1,
            units.len() as u32,
            "{}: utf16 length",
            case.name
        );
        assert_eq!(
            chars.lengths().2,
            scalars.len() as u32,
            "{}: char length",
            case.name
        );

        for (i, node) in doc.tape().nodes.iter().enumerate() {
            let expected = node.span().text(doc.source());

            let p = utf16.node(i);
            let got =
                String::from_utf16(&units[p.start as usize..(p.start + p.len) as usize]).unwrap();
            assert_eq!(got, expected, "{}: utf16 node {i}", case.name);

            let p = chars.node(i);
            let got: String = scalars[p.start as usize..(p.start + p.len) as usize]
                .iter()
                .collect();
            assert_eq!(got, expected, "{}: chars node {i}", case.name);
        }
    }
    eprintln!("\nslice-exactness: {non_ascii} non-ASCII fixtures covered");
    assert!(non_ascii > 0, "corpus should contain non-ASCII fixtures");
}
