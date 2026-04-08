# usfm3

> ⚠️ **Beta Notice**: This project is in beta and the API may change before reaching stability. I will, of course, try to minimize breaking changes and follow `semver`. But until there's a 1.0.0, I will break things at my own discretion. If you want to be notified beforehand, let me know. Otherwise, I will assume no one is using this in anything mission critical.

A fast, flexible, error-tolerant USFM 3.x parser written in Rust, with CLI, Python, and WebAssembly bindings.

The public API is staged:

```
tokenize -> parse_cst -> parse_ast / lower_cst -> serialize
```

That lets editor-style integrations stay on the cheap token/CST path, while AST-dependent work like USJ, USX, USFM, vref, and diagnostics is only paid for when requested.

## Packages

| Package | Description |
| --- | --- |
| [`usfm3`](crates/usfm3/) | Core Rust crate |
| [`usfm3-cli`](crates/usfm3-cli/) | CLI |
| [`usfm3-python`](crates/usfm3-python/) | PyPI package |
| [`usfm3-wasm`](crates/usfm3-wasm/) | npm package / WASM bindings |

## CLI

```sh
usfm3 tokens path/to/file.usfm
usfm3 cst path/to/file.usfm
usfm3 ast path/to/file.usfm
usfm3 diagnostics path/to/file.usfm
usfm3 usj path/to/file.usfm
usfm3 usj path/to/file.usfm --inline-spans
usfm3 usx path/to/file.usfm
usfm3 usfm path/to/file.usfm
usfm3 vref path/to/file.usfm
```

If no input path is given, the CLI reads from stdin.

## Rust

```rust
let parsed = usfm3::parse(
    r#"\id GEN
\c 1
\p
\v 1 In the beginning God created the heavens and the earth.
"#,
    usfm3::ParseOptions::default(),
);

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

For eager AST work:

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

For CST-first lowering:

```rust
let cst = usfm3::parse_cst(text);
let ast_document = usfm3::lower_cst(
    &cst,
    usfm3::ParseOptions {
        diagnostics: true,
    },
);
```

## Python

```python
import usfm3

parsed = usfm3.parse(text)

tokens = usfm3.tokenize(text)
cst = usfm3.parse_cst(text)
ast_document = usfm3.parse_ast(text, diagnostics=True)

ast = parsed.ast()
source_map = parsed.source_map()
diagnostics = parsed.diagnostics

usj = parsed.to_usj()
usj_with_spans = parsed.to_usj(spans=True)
usx = parsed.to_usx()
usfm = parsed.to_usfm()
vref = parsed.to_vref()
```

## JavaScript / TypeScript

```ts
import { parse, parseAst, parseCst, tokenize } from "usfm3";

const parsed = parse(usfmText);

const tokens = tokenize(usfmText);
const cst = parseCst(usfmText);
const astDocument = parseAst(usfmText, { diagnostics: true });

const ast = parsed.ast();
const sourceMap = parsed.sourceMap();
const diagnostics = parsed.diagnostics();

const usj = parsed.toUsj();
const usjWithSpans = parsed.toUsj({ spans: true });
const usx = parsed.toUsx();
const usfm = parsed.toUsfm();
const vref = parsed.toVref();

parsed.free();
```

## Data Model

- AST nodes do not carry spans.
- Source locations live in a parallel `source_map` tree.
- Diagnostics are a single flat list, sorted in document order.
- Diagnostics are only computed when `diagnostics: true` is requested.
- Inline USJ spans are derived from the source map.

## Performance Notes

- `tokenize()` is the lightest editor-facing path.
- `parse_cst()` preserves full source-backed structure and is the preferred CST/LSP path.
- `parse()` is lazy and diagnostics-off by default.
- `parse_ast(..., diagnostics: true)` or `lower_cst(..., diagnostics: true)` opt into the full semantic + diagnostic pass.

## License

MIT
