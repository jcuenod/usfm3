# usfm3

An error-tolerant [USFM 3.x](https://docs.usfm.bible/usfm/3.1.1/index.html) parser for JavaScript and TypeScript. Outputs [USJ](https://docs.usfm.bible/usfm/3.1.1/usj/index.html) (JSON), [USX](https://docs.usfm.bible/usfm/3.1.1/usx/index.html) (XML), normalized USFM, and vref format (a key-value map of verse references to text).

Built in Rust and compiled to WebAssembly. Works in browsers, Node.js, Deno, and Bun.

Also available as a [Rust crate](https://crates.io/crates/usfm3) and [Python package](https://pypi.org/project/usfm3/).

## Installation

```sh
npm install usfm3
```

## Usage

WASM is automatically initialized in Node.js, Deno, and Bun. In a browser, call `init()` first:

```typescript
import init from "usfm3";
await init(); // browser only
```

Then parse USFM text:

```typescript
import { parse } from "usfm3";

const result = parse(usfmText);

if (result.hasErrors()) {
    console.error("Document has errors");
}

// Output formats (lazy — only serialized when called)
const usj = result.toUsj();    // USJ object
const usx = result.toUsx();    // USX XML string
const usfm = result.toUsfm();  // Normalized USFM string
const vref = result.toVref();  // Vref pairs like { "GEN 1:1": "In the beginning...", ... }

// Diagnostics
for (const d of result.diagnostics) {
    console.log(`[${d.severity}] ${d.message} (${d.start}..${d.end})`);
    // d.code is a machine-readable string like "UnknownMarker", "ImplicitClose", etc.
}

// Skip semantic validation
const result2 = parse(usfmText, { validate: false });

// Free WASM memory when done
result.free();
```

## API

### `parse(usfm: string, options?: ParseOptions): ParseResult`

Parse a USFM string. Returns a `ParseResult` with lazy output methods and diagnostics.

**ParseOptions:**

| Field | Type | Default | Description |
|---|---|---|---|
| `validate` | `boolean` | `true` | Run semantic validation after parsing |

### `ParseResult`

| Method / Property | Returns | Description |
|---|---|---|
| `toUsj()` | `object` | USJ (Unified Scripture JSON) |
| `toUsx()` | `string` | USX (Unified Scripture XML) |
| `toUsfm()` | `string` | Normalized USFM |
| `toVref()` | `Record<string, string>` | Verse reference to plain text map |
| `hasErrors()` | `boolean` | True if any error-severity diagnostics |
| `diagnostics` | `Diagnostic[]` | Parser and validation diagnostics |
| `free()` | `void` | Free WASM memory |

### `Diagnostic`

| Property | Type | Description |
|---|---|---|
| `severity` | `string` | `"error"`, `"warning"`, or `"info"` |
| `code` | `string` | Machine-readable code (e.g. `"UnknownMarker"`) |
| `message` | `string` | Human-readable message |
| `start` | `number` | Start byte offset in source |
| `end` | `number` | End byte offset in source |

## License

MIT
