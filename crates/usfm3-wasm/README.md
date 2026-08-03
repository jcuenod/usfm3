# usfm3

`usfm3` is the JavaScript / TypeScript package for the Rust `usfm3` parser — an
error-tolerant, blazingly fast USFM 3.x parser compiled to WebAssembly. It runs in
browsers and in Node (the same `.wasm` powers both).

A single `parse()` produces a flat tape; every output (USJ, USX, USFM, CST, vref,
diagnostics) is a cheap view over it. There is no separate token/AST/source-map
stage — the handle below is all you need.

## Installation

```sh
npm install usfm3
```

### Node

```ts
import { parse } from "usfm3"; // wasm is initialized for you
```

### Browser / bundler

```ts
import init, { parse } from "usfm3";
await init();
```

## Quick Start

```ts
import { parse } from "usfm3";

const doc = parse(usfmText);                          // diagnostics off (hot path)
const dbg = parse(usfmText, { diagnostics: true });   // opt into validation
const v32 = parse(usfmText, { version: "3.2" });      // select USFM version

const usj = doc.toUsj();         // UsjDocument (object)
const usx = doc.toUsx();         // string (XML)
const usfm = doc.toUsfm();       // string (normalized USFM)
const vref = doc.toVref();       // { "GEN 1:1": "In the beginning…", … }
const cst = doc.cst();           // CstNode (for editors)
const diags = doc.diagnostics(); // Diagnostic[] (empty unless requested)

doc.free();                      // release the wasm-side handle
```

## API

### `parse(usfm: string, options?: ParseOptions): ParsedDocument`

Parse into a lazy handle. Never throws on malformed input — the parser recovers,
and (with `diagnostics: true`) reports what it recovered from.

```ts
interface ParseOptions {
  diagnostics?: boolean; // default false
  version?: "3.1" | "3.2"; // default "3.1"
}
```

### `parseCst(usfm: string, options?: ParseOptions): CstNode`

One-shot: parse and return the concrete syntax tree.

### `ParsedDocument`

- `cst(): CstNode` — concrete syntax tree (byte spans, marker
  category/role/closing, matching close token), for editor decoration and folding.
- `toUsj(): UsjDocument` — USJ document.
- `toUsx(): string` — USX (XML).
- `toUsfm(): string` — normalized USFM.
- `toVref(): Record<string, string>` — verse-reference → cleaned text, in document
  order (empty verses dropped, duplicate references merged).
- `diagnostics(): Diagnostic[]` — flat list; empty unless `diagnostics: true`.
- `free(): void` — release the handle.

## Notes

- `toUsx()`/`toUsfm()` return strings; the JSON-shaped views are parsed natively at
  the wasm boundary, so you get plain objects.
- Diagnostics carry `severity` (`"error" | "warning" | "lint"`), a stable `code`, a
  human `message`, byte `start`/`end`, and 1-based `line`/`column`.
- vref text is version-correct: notes excluded, character-marker tags dropped
  (inner text kept), whitespace collapsed.
- Call `free()` when you are done with a handle (WebAssembly has no GC).

## License

MIT
