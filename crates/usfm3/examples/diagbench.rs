//! Isolate `to_diagnostics_json` cost from the FFI boundary. Fixed source length,
//! varying diagnostic count.
use std::time::Instant;
use usfm3::{ParseOptions, Version};

fn diag_doc(n: usize) -> String {
    const SLOTS: usize = 5000;
    let mut s = String::from("\\id GEN\n\\c 1\n\\p");
    for i in 0..SLOTS {
        if i % 25 == 0 {
            s.push_str(&format!("\n\\v {} ", 1 + i / 25));
        }
        s.push_str(if i < n { "\\qqbogus" } else { "filler01" });
        s.push(' ');
    }
    s.push('\n');
    s
}

fn med(mut f: impl FnMut() -> usize) -> f64 {
    for _ in 0..3 { f(); }
    let mut runs: Vec<f64> = (0..9).map(|_| { let t = Instant::now(); f(); t.elapsed().as_secs_f64() * 1e3 }).collect();
    runs.sort_by(f64::total_cmp);
    runs[4]
}

/// What the serializer did before: resolve line/column per diagnostic with
/// `Span::line_col`, which counts newlines from byte 0 every time. Reproduced here so
/// the improvement can be measured rather than asserted.
fn line_col_per_diagnostic(doc: &usfm3::Document<'_>) -> usize {
    let mut acc = 0usize;
    for d in doc.diagnostics() {
        let lc = d.span.line_col(doc.source());
        acc += lc.line as usize + lc.col as usize;
    }
    acc
}

/// The shared forward-only scanner the serializer uses now.
fn offsetmap_for_all(doc: &usfm3::Document<'_>) -> usize {
    use usfm3::Offsets;
    use usfm3::low_level::OffsetMap;
    let mut map = OffsetMap::new(doc.source(), Offsets::Utf8);
    let mut acc = 0usize;
    for d in doc.diagnostics() {
        let p = map.point_at(d.span.start);
        acc += p.line as usize + p.col as usize;
    }
    acc
}

fn main() {
    let opts = ParseOptions { diagnostics: true };

    println!("Source length held CONSTANT; only the diagnostic count varies.\n");
    println!(
        "{:>7} {:>10} {:>11} {:>11} {:>11} {:>8}",
        "diags", "parse+val", "serialize", "jsonBytes", "us/diag", ""
    );
    for n in [0usize, 5, 50, 500, 5000] {
        let src = diag_doc(n);
        let count = usfm3::parse_with(&src, Version::V3_1, opts).diagnostics().len();
        let bytes = usfm3::parse_with(&src, Version::V3_1, opts).to_diagnostics_json().len();
        let base = med(|| usfm3::parse_with(&src, Version::V3_1, opts).diagnostics().len());
        let full = med(|| usfm3::parse_with(&src, Version::V3_1, opts).to_diagnostics_json().len());
        let per = if count > 0 { (full - base) * 1000.0 / count as f64 } else { 0.0 };
        println!(
            "{:>7} {:>7.2}ms {:>9.2}ms {:>11} {:>7.2}us",
            count, base, full - base, bytes, per
        );
    }

    println!("\nline/column resolution in isolation — the part Phase 1 changed:\n");
    println!("{:>7} {:>14} {:>14} {:>10}", "diags", "line_col each", "one OffsetMap", "speedup");
    for n in [50usize, 500, 5000] {
        let src = diag_doc(n);
        let doc = usfm3::parse_with(&src, Version::V3_1, opts);
        let count = doc.diagnostics().len();
        let old = med(|| line_col_per_diagnostic(&doc));
        let new = med(|| offsetmap_for_all(&doc));
        // Sanity: both must agree, or the speedup is meaningless.
        assert_eq!(line_col_per_diagnostic(&doc), offsetmap_for_all(&doc));
        println!(
            "{:>7} {:>11.3}ms {:>11.3}ms {:>9.0}x",
            count, old, new, old / new.max(f64::MIN_POSITIVE)
        );
    }
}
