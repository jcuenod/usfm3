//! `usfm3` — command-line front-end for the `usfm3` parser library.
//!
//! Subcommands:
//!   usfm3 vref <file.usfm>            { "GEN 1:1": "text", … } JSON dictionary
//!   usfm3 cst <file.usfm>             concrete syntax tree as JSON (for editors)
//!       [--diagnostics]               …wrapped together with a diagnostics array
//!   usfm3 diagnostics <file.usfm>     errors + warnings as a JSON array

use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use usfm3::{parse, BookId, Document, Version};

const USAGE: &str = "\
usfm3 — USFM tooling

USAGE:
    usfm3 vref <file.usfm>              Emit a verse-reference -> text JSON dictionary
    usfm3 cst <file.usfm> [--diagnostics]
                                       Emit the concrete syntax tree as JSON
    usfm3 diagnostics <file.usfm>      Emit errors + warnings as a JSON array

";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("vref") => with_path(&args[1..], "vref", run_vref),
        Some("cst") => {
            let diagnostics = args.iter().any(|a| a == "--diagnostics");
            with_path(&args[1..], "cst", |path| run_cst(path, diagnostics))
        }
        Some("usj") => with_path(&args[1..], "usj", |p| render(p, |d| d.to_usj())),
        Some("usx") => with_path(&args[1..], "usx", |p| render(p, |d| d.to_usx())),
        Some("diagnostics") => with_path(&args[1..], "diagnostics", run_diagnostics),
        Some("-h") | Some("--help") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("error: unknown subcommand `{other}`\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Resolve the first positional (non-`-`) argument as a file path; dispatch to `f`.
fn with_path(args: &[String], cmd: &str, f: impl FnOnce(&str) -> ExitCode) -> ExitCode {
    match args.iter().find(|a| !a.starts_with('-')) {
        Some(path) => f(path),
        None => {
            eprintln!("error: `{cmd}` requires a file path\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Read + parse a file (reporting on failure), then hand a borrowed [`Document`]
/// to `f`.
fn read_and<R>(path: &str, f: impl FnOnce(&Document<'_>) -> R) -> Result<R, ExitCode> {
    let source = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read {path}: {e}");
        ExitCode::FAILURE
    })?;
    let doc = parse(&source, Version::V3_1);
    Ok(f(&doc))
}

/// Write a string (plus trailing newline) to stdout via a buffered lock.
fn emit(s: &str) -> ExitCode {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    match out
        .write_all(s.as_bytes())
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: writing output: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_vref(path: &str) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let doc = parse(&source, Version::V3_1);
    // `vref_dict` yields unique, non-empty keys in document order — so the result
    // is a valid JSON object (no duplicate keys, no empty verses).
    let mut dict = doc.vref_dict();

    // If the document had no `\id` (book reported as "???"), fall back to a book
    // code inferred from the (Paratext-style) filename, e.g. `21ECCBSB.usfm`.
    if dict.iter().any(|(k, _)| k.starts_with("??? ")) {
        if let Some(code) = infer_book_code(path) {
            for (k, _) in dict.iter_mut() {
                if let Some(rest) = k.strip_prefix("??? ") {
                    *k = format!("{code} {rest}");
                }
            }
        }
    }

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let result = write_json_object(&mut out, &dict).and_then(|_| out.flush());
    if let Err(e) = result {
        eprintln!("error: writing output: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run_cst(path: &str, with_diagnostics: bool) -> ExitCode {
    let rendered = read_and(path, |doc| {
        if with_diagnostics {
            let mut s = String::from("{\"cst\":");
            let _ = doc.write_cst_json(&mut s);
            s.push_str(",\"diagnostics\":");
            let _ = doc.write_diagnostics_json(&mut s);
            s.push('}');
            s
        } else {
            doc.to_cst_json()
        }
    });
    match rendered {
        Ok(s) => emit(&s),
        Err(code) => code,
    }
}

fn run_diagnostics(path: &str) -> ExitCode {
    match read_and(path, |doc| doc.to_diagnostics_json()) {
        Ok(s) => emit(&s),
        Err(code) => code,
    }
}

fn render(path: &str, f: impl FnOnce(&Document<'_>) -> String) -> ExitCode {
    match read_and(path, f) {
        Ok(s) => emit(&s),
        Err(code) => code,
    }
}

/// Best-effort book code from a Paratext-style filename (e.g. `21ECCBSB.usfm`
/// → `ECC`): the first 3-letter window of the uppercased stem that is a valid
/// USFM book code.
fn infer_book_code(path: &str) -> Option<&'static str> {
    let stem = std::path::Path::new(path).file_stem()?.to_str()?.to_ascii_uppercase();
    stem.as_bytes()
        .windows(3)
        .filter_map(|w| std::str::from_utf8(w).ok())
        .find_map(|s| BookId::from_code(s).filter(|b| b.is_known()).map(|b| b.code()))
}

/// Write `entries` as a compact JSON object, preserving order.
fn write_json_object<W: Write>(out: &mut W, entries: &[(String, String)]) -> io::Result<()> {
    out.write_all(b"{\n")?;
    for (i, (key, value)) in entries.iter().enumerate() {
        if i > 0 {
            out.write_all(b",\n")?;
        }
        out.write_all(b"  ")?;
        write_json_string(out, key)?;
        out.write_all(b": ")?;
        write_json_string(out, value)?;
    }
    out.write_all(b"\n}\n")
}

fn write_json_string<W: Write>(out: &mut W, s: &str) -> io::Result<()> {
    out.write_all(b"\"")?;
    let bytes = s.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let escaped: &[u8] = match b {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0x00..=0x1F => {
                out.write_all(&bytes[start..i])?;
                write!(out, "\\u{:04x}", b)?;
                start = i + 1;
                continue;
            }
            _ => continue,
        };
        out.write_all(&bytes[start..i])?;
        out.write_all(escaped)?;
        start = i + 1;
    }
    out.write_all(&bytes[start..])?;
    out.write_all(b"\"")
}
