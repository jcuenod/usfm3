# usfm3

An error-tolerant, fast USFM 3.x parser. One parse produces a flat *tape* of 16-byte
nodes; USJ, USX, normalized USFM, the CST, verse text and diagnostics are all views
over that tape and the borrowed source.

```toml
[dependencies]
usfm3 = "0.3"
```

> **Coming from 0.2?** 0.3 is a rewrite with a different API. See the
> [changelog](https://github.com/jcuenod/usfm3/blob/main/CHANGELOG.md#migrating-from-02)
> for the old → new mapping.

## Usage

```rust
use usfm3::{ParseOptions, Version};

let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning God created the heavens and the earth.\n";

// Diagnostics off: the fast path.
let doc = usfm3::parse(src, Version::V3_1);

let usj: String = doc.to_usj();     // JSON text
let usx: String = doc.to_usx();     // XML text
let usfm: String = doc.to_usfm();   // normalized USFM
let cst: String = doc.to_cst_json();

// Verse text: notes excluded, character markers dropped (inner text kept),
// whitespace collapsed.
for (reference, text) in doc.vref_dict() {
    println!("{reference}\t{text}");
}

// Opt into diagnostics (structural checks plus a validation pass).
let doc = usfm3::parse_with(src, Version::V3_1, ParseOptions { diagnostics: true });
for d in doc.diagnostics() {
    let at = d.span.line_col(src);
    println!("{}:{} {} {}: {}", at.line, at.col, d.severity.as_str(), d.code.as_str(), d.message);
}
```

`parse` never fails: malformed input is recovered from, and with diagnostics on, every
recovery is reported with a byte span into the source.

### Verse text

| Method | Returns |
| --- | --- |
| `vref_dict()` | `Vec<(String, String)>` — empty verses dropped, duplicate references merged |
| `to_vref_json()` | the same, as a JSON object |
| `vref_text()` | `VrefText`, keyed by the packed `VerseRef` |
| `vrefs_raw()` | every verse in order, uncleaned |
| `for_each_vref(f)` | streaming, no allocation per verse |

### Low-level access

`usfm3::low_level` exposes the tape (`Tape`, `Node`, `NodeKind`), the CST `Cursor`,
and the version-specific `MarkerTable`, for editor tooling that wants to walk the parse
directly. `Document::to_packed_tape(offsets)` and `schema_json(version)` are the
host-facing packed form used by the JavaScript and Python bindings.

## Bindings

- npm: [`usfm3`](https://www.npmjs.com/package/usfm3) (WebAssembly, browser and Node)
- PyPI: [`usfm3`](https://pypi.org/project/usfm3/)

## License

MIT
