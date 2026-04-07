use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use usfm3::ParseOptions;

fn main() {
    let scenarios = load_scenarios();
    println!("stage,scenario,iters,total_bytes,elapsed_ms,mb_per_s");

    for (name, docs) in scenarios {
        let total_bytes: usize = docs.iter().map(|doc| doc.len()).sum();
        let iters = iterations_for_bytes(total_bytes.max(1));

        bench_stage("tokenize", &name, iters, total_bytes, || {
            for doc in &docs {
                black_box(usfm3::tokenize(doc));
            }
        });

        bench_stage("parse_cst", &name, iters, total_bytes, || {
            for doc in &docs {
                black_box(usfm3::parse_cst(doc));
            }
        });

        let csts: Vec<_> = docs.iter().map(|doc| usfm3::parse_cst(doc)).collect();
        bench_stage("lower_cst", &name, iters, total_bytes, || {
            for cst in &csts {
                black_box(usfm3::lower_cst(cst, ParseOptions::default()));
            }
        });

        bench_stage("parse_ast", &name, iters, total_bytes, || {
            for doc in &docs {
                black_box(usfm3::parse_ast(doc, ParseOptions::default()));
            }
        });

        bench_stage(
            "parse_ast_with_diagnostics",
            &name,
            iters,
            total_bytes,
            || {
                for doc in &docs {
                    black_box(usfm3::parse_ast(doc, ParseOptions { diagnostics: true }));
                }
            },
        );

        bench_stage("parse_lazy", &name, iters, total_bytes, || {
            for doc in &docs {
                black_box(usfm3::parse(doc, ParseOptions::default()));
            }
        });
    }
}

fn bench_stage<F>(stage: &str, scenario: &str, iters: usize, total_bytes: usize, mut f: F)
where
    F: FnMut(),
{
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let total_megabytes = (total_bytes * iters) as f64 / (1024.0 * 1024.0);
    let throughput = total_megabytes / elapsed.as_secs_f64();
    println!(
        "{stage},{scenario},{iters},{total_bytes},{:.3},{:.3}",
        elapsed.as_secs_f64() * 1000.0,
        throughput
    );
}

fn iterations_for_bytes(total_bytes: usize) -> usize {
    if total_bytes < 8 * 1024 {
        200
    } else if total_bytes < 256 * 1024 {
        50
    } else if total_bytes < 2 * 1024 * 1024 {
        10
    } else {
        3
    }
}

fn load_scenarios() -> Vec<(String, Vec<String>)> {
    let mut scenarios = vec![
        ("verse_dense".to_string(), vec![synthetic_verse_dense()]),
        (
            "note_table_heavy".to_string(),
            vec![synthetic_note_table_heavy()],
        ),
        ("malformed".to_string(), vec![synthetic_malformed()]),
    ];

    let bridgeconn_root = Path::new("tests/fixtures/usfm-grammar");
    if bridgeconn_root.exists() {
        let mut docs = Vec::new();
        collect_bridgeconn_docs(bridgeconn_root, &mut docs);
        if !docs.is_empty() {
            scenarios.push(("bridgeconn_corpus".to_string(), docs));
        }
    }

    scenarios
}

fn collect_bridgeconn_docs(root: &Path, docs: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_bridgeconn_docs(&path, docs);
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("origin.usfm")
            && let Ok(doc) = fs::read_to_string(&path)
        {
            docs.push(doc);
        }
    }
}

fn synthetic_verse_dense() -> String {
    let mut out = String::from("\\id GEN Genesis\n\\c 1\n\\p\n");
    for verse in 1..=400 {
        out.push_str(&format!(
            "\\v {verse} In the beginning verse {verse} contains words and words and words.\n"
        ));
    }
    out
}

fn synthetic_note_table_heavy() -> String {
    let mut out = String::from("\\id GEN Genesis\n\\c 1\n");
    for row in 0..120 {
        out.push_str("\\p \\v 1 Text \\f + \\fr 1:1 \\ft note text with \\xt cross ref\\xt*\\f*\n");
        out.push_str(&format!(
            "\\tr \\th1 Head {row} \\th2 Head {} \\tc1 Cell {} \\tc2 Cell {}\n",
            row + 1,
            row,
            row + 1
        ));
    }
    out
}

fn synthetic_malformed() -> String {
    let mut out = String::from("\\id BAD Broken\n\\p \\zz custom\n");
    for verse in 0..250 {
        out.push_str(&format!(
            "\\v 0{verse} Broken \\w |bad-default \\nd nested without close \\f + \\fr ref\n"
        ));
    }
    out.push_str("\\esbe\n\\qt1-s |who=\"speaker\"\n");
    out
}
