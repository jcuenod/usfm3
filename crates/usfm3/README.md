# usfm3

`usfm3` is the core Rust crate for parsing USFM 3.x.

The public pipeline is:

`tokenize -> parse_cst -> parse_ast / lower_cst -> serialize`

## Installation

```toml
[dependencies]
usfm3 = "0.1"
```

## Lazy Parse Handle

```rust
let parsed = usfm3::parse(text, usfm3::ParseOptions::default());

let tokens = parsed.tokens();
let cst = parsed.cst();
let ast = parsed.ast();
let source_map = parsed.source_map();
let diagnostics = parsed.diagnostics();

let usj = parsed
    .to_usj(usfm3::usj::UsjOptions { include_spans: false })
    .unwrap();
let usx = parsed.to_usx().unwrap();
let usfm = parsed.to_usfm();
let vref = parsed.to_vref();
```

`parse()` is lazy and diagnostics-off by default, so token/CST/editor consumers do not pay for validation unless they opt in.

## Eager AST

```rust
let ast_document = usfm3::parse_ast(
    text,
    usfm3::ParseOptions {
        diagnostics: true,
    },
);

let ast = ast_document.ast;
let source_map = ast_document.source_map;
let diagnostics = ast_document.diagnostics;
```

## CST-First Lowering

```rust
let cst = usfm3::parse_cst(text);
let ast_document = usfm3::lower_cst(
    &cst,
    usfm3::ParseOptions {
        diagnostics: true,
    },
);
```

## Notes

- AST nodes are semantic-only and do not own spans.
- `source_map` mirrors the AST and holds source ranges plus CST anchors.
- Diagnostics are a single flat list when requested, otherwise `None`.
- USJ inline spans come from `source_map`, not from the AST.

## License

MIT
