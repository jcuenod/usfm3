# usfm3

An error-tolerant [USFM 3.x](https://docs.usfm.bible/usfm/3.1.1/index.html) parser written in Rust. Outputs [USJ](https://docs.usfm.bible/usfm/3.1.1/usj/index.html) (JSON), [USX](https://docs.usfm.bible/usfm/3.1.1/usx/index.html) (XML), normalized USFM, and verse-reference maps.

Also available as a [Python package](https://pypi.org/project/usfm3/) and [npm package](https://www.npmjs.com/package/usfm3) (WebAssembly).

## Features

- Parses all USFM 3.x markers including tables, milestones, sidebars, figures, and nested character styles
- Error-tolerant: always produces a document tree, even from malformed input
- Structured diagnostics with source locations, severity levels, and machine-readable codes
- Semantic validation (chapter/verse sequence, attribute rules, milestone pairing, etc.)
- Multiple output formats: USJ, USX, USFM, and verse-reference maps

## Installation

```toml
[dependencies]
usfm3 = "0.1"
```

## Usage

```rust
let result = usfm3::builder::parse(r#"\id GEN
\c 1
\p
\v 1 In the beginning God created the heavens and the earth.
"#);

// Check for errors
for diag in result.diagnostics.iter() {
    eprintln!("{diag}");
}

// Output as USJ (JSON)
let usj = usfm3::usj::to_usj_string_pretty(&result.document).unwrap();

// Include source spans for editor tooling
let usj_with_spans = usfm3::usj::to_usj_string_pretty_with_options(
    &result.document,
    usfm3::usj::UsjOptions {
        include_spans: true,
    },
)
.unwrap();

// Output as USX (XML)
let usx = usfm3::usx::to_usx_string(&result.document).unwrap();

// Output as normalized USFM
let usfm = usfm3::usfm::to_usfm_string(&result.document);

// Output as verse-reference map
let vref = usfm3::vref::to_vref_json_string(&result.document);

// Run semantic validation
let validation_diags = usfm3::validation::validate(&result.document);
```

## Output Formats

| Format | Function | Description |
|--------|----------|-------------|
| USJ | `usj::to_usj_string()` / `usj::to_usj_string_with_options()` | Unified Scripture JSON; optional nested source spans |
| USX | `usx::to_usx_string()` | Unified Scripture XML |
| USFM | `usfm::to_usfm_string()` | Normalized USFM with regularized whitespace |
| VRef | `vref::to_vref_json_string()` | Verse reference to plain text map |

When `include_spans` is enabled, each structural USJ node includes a `spans` object with a required `node` range and optional `code`, `number`, and `close` ranges.

## License

MIT
