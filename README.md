# usfm3

An error-tolerant [USFM 3.x](https://docs.usfm.bible/usfm/3.1.1/index.html) parser written in Rust. Outputs [USJ](https://docs.usfm.bible/usfm/3.1.1/usj/index.html) (JSON), [USX](https://docs.usfm.bible/usfm/3.1.1/usx/index.html) (XML), normalized USFM, and vref format (a key-value map of verse references to text).

Available as a Rust library, CLI tool, Python package, and WebAssembly module.

## Features

- Parses all USFM 3.x markers including tables, milestones, sidebars, figures, and nested character styles
- Error-tolerant: always produces a document tree, even from malformed input
- Structured diagnostics with source locations, severity levels, and machine-readable codes
- Semantic validation (chapter/verse sequence, attribute rules, milestone pairing, etc.)
- Multiple output formats: USJ, USX, USFM, and verse-reference maps

## Packages

| Crate | Description |
|---|---|
| [`usfm3`](crates/usfm3/) | Core Rust library |
| [`usfm3-cli`](crates/usfm3-cli/) | Command-line tool |
| [`usfm3-python`](crates/usfm3-python/) | Python bindings (PyO3) |
| [`usfm3-wasm`](crates/usfm3-wasm/) | WebAssembly bindings (works in browsers and Node.js) |

## Quick Start

### CLI

```sh
# From a file (defaults to USJ output)
usfm3 path/to/file.usfm

# Choose output format
usfm3 path/to/file.usfm usx
usfm3 path/to/file.usfm usfm
usfm3 path/to/file.usfm vref

# From stdin
cat file.usfm | usfm3

# Skip validation
usfm3 path/to/file.usfm --no-validate
```

Diagnostics are printed to stderr; document output goes to stdout.

### Rust

Crate available on [crates.io](https://crates.io/crates/usfm3):

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
println!("{usj}");

// Output as USX (XML)
let usx = usfm3::usx::to_usx_string(&result.document).unwrap();

// Output as normalized USFM
let usfm = usfm3::usfm::to_usfm_string(&result.document);

// Run semantic validation
let validation_diags = usfm3::validation::validate(&result.document);
```

### Python

Python bindings available at: [PyPI](https://pypi.org/project/usfm3/)

```python
import usfm3

result = usfm3.parse(open("GEN.usfm").read())

# Output formats
usj = result.to_usj()       # dict
usx = result.to_usx()       # XML string
usfm = result.to_usfm()     # USFM string
vref = result.to_vref()     # {"GEN 1:1": "In the beginning...", ...}

# Diagnostics
for d in result.diagnostics:
    print(f"[{d.severity}] {d.message} ({d.start}..{d.end})")

if result.has_errors():
    print("Document has errors")

# Skip validation
result = usfm3.parse(text, validate=False)
```

Build with [maturin](https://www.maturin.rs):

```sh
cd crates/usfm3-python
maturin develop  # install into current venv
```

### JavaScript / TypeScript (WebAssembly)

Works in browsers, Node.js, Deno, and Bun. [NPM](https://www.npmjs.com/package/usfm3)

WASM is automatically initialized in Node.js, Deno, and Bun. In a browser, call `init()` first:

```typescript
import init from "usfm3";
await init(); // browser only
```

```typescript
import { parse } from "usfm3";

const result = parse(usfmText);

// Output formats (lazy -- only serialized when called)
const usj = result.toUsj();    // USJ object
const usx = result.toUsx();    // USX XML string
const usfm = result.toUsfm();  // Normalized USFM string
const vref = result.toVref();  // Vref pairs like { "GEN 1:1": "In the beginning...", ... }

// Diagnostics
for (const d of result.diagnostics) {
    console.log(`[${d.severity}] ${d.message} (${d.start}..${d.end})`);
    // d.code is a machine-readable enum like "UnknownMarker", "ImplicitClose", etc.
}

// Skip validation
const result2 = parse(usfmText, { validate: false });

// Free wasm memory when done
result.free();
```

Build with [wasm-pack](https://rustwasm.github.io/wasm-pack/):

```sh
wasm-pack build crates/usfm3-wasm --target web     # for browsers
wasm-pack build crates/usfm3-wasm --target nodejs  # for Node.js
```

## Building from Source

```sh
# Build everything
cargo build

# Build individual crates
cargo build -p usfm3          # core library
cargo build -p usfm3-cli      # CLI

# Run tests
cargo test -p usfm3
```

## Architecture

The parser uses a two-phase architecture:

1. **Lexer** (`logos`-based tokenizer) -- splits USFM source into tokens with byte-offset spans
2. **Builder** (stack-based tree builder) -- converts the token stream into a `Document` AST

This design makes the parser error-tolerant: the lexer always succeeds, and the builder recovers from structural errors by emitting diagnostics and applying heuristics (implicit closes, etc.).

Validation is a separate pass over the AST that checks semantic rules without modifying the tree.

## Output Formats

| Format | Function | Description |
|--------|----------|-------------|
| USJ | `usj::to_usj_string()` | Unified Scripture JSON -- the standard JSON representation |
| USX | `usx::to_usx_string()` | Unified Scripture XML -- the standard XML representation |
| USFM | `usfm::to_usfm_string()` | Normalized USFM with regularized whitespace |
| VRef | `vref::to_vref_json_string()` | Verse reference to plain text map (strips formatting/notes) |

## License

MIT
