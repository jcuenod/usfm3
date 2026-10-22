# usfm3

`usfm3` is the JavaScript / TypeScript package for the Rust `usfm3` parser — an
error-tolerant, blazingly fast USFM 3.x parser compiled to WebAssembly. It runs in
browsers and in Node (the same `.wasm` powers both).

A single `parse()` produces a flat tape; every output (USJ, USX, USFM, CST, vref,
diagnostics) is a cheap view over it. There is no separate token/AST/source-map
stage — the handle below is all you need.

> **Coming from 0.2?** 0.3 is a rewrite: `tokenize`, `parseAst`, `ast()`,
> `sourceMap()` and `toUsj({ spans })` are gone, diagnostics are flat (`start`/`end`
> instead of `span`), and the CST has a new shape. See
> [CHANGELOG.md](CHANGELOG.md#migrating-from-02).

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
- `diagnostics(options?): Diagnostic[]` — flat list; empty unless `diagnostics: true`.
  Offsets are UTF-16 code units by default; pass `{ offsets: "utf8" }` for bytes.
- `tape(options?): Uint32Array` — the tape packed as `u32`s. Prefer `parseTape()` /
  `tapeView()`, which wrap it in a walkable `Tape`.
- `free(): void` — release the handle.

Module-level: `parse`, `parseCst`, `parseTape`, `tapeView`, `tapeSchema`, and the
`Tape` / `TapeCursor` classes. Import the package root — `exports` routes Node to
`node.js` and bundlers to `index.js` with an identical namespace. The one subpath is
`usfm3/tape.js`, which imports no wasm and can decode a transferred buffer on a worker.

## The packed tape

`cst()` builds a JSON string and re-parses it into an object graph — ~4.7 MiB and
~58,000 objects for a book-sized document, on every call. `parseTape()` hands over the
same information as a `Uint32Array` you walk with shifts:

```js
import { parseTape } from "usfm3";

const tape = parseTape(usfm);
for (const node of tape.scan()) {          // allocation-free; reuses one cursor
  if (node.kind === "marker") console.log(node.markerName, node.start, node.end);
}
```

278 KiB document, Node 24: `parse()` + `cst()` 27.1 ms → `parseTape()` 2.5 ms; payload
4,725 KiB → 917 KiB; worker handoff 35.1 ms (`structuredClone`) → 2.9 ms (transfer the
buffer). `nodes()` yields independent cursors when you need to spread or retain them.

## Notes

- `toUsx()`/`toUsfm()` return strings; the JSON-shaped views are parsed natively at
  the wasm boundary, so you get plain objects.
- Diagnostics carry `severity` (`"error" | "warning" | "info"`), a stable `code`, a
  human `message`, `start`/`end`, and 1-based `line`/`column`.
- **Offsets are UTF-16 code units** for `diagnostics()` and `tape()`, so they index a
  JS string directly. Pass `{ offsets: "utf8" }` for bytes or `"chars"` for code
  points. `cst()` reports UTF-8 byte offsets.
- `TapeCursor.text` / `.markerName` / `.toJSON()` throw on a tape built with
  non-host offsets over a non-ASCII source, rather than returning wrong characters.
  `Tape#sliceable` reports that up front; `start`/`end`/`kind`/`marker` are always safe.
- vref text is version-correct: notes excluded, character-marker tags dropped
  (inner text kept), whitespace collapsed.
- Call `free()` when you are done with a handle (WebAssembly has no GC).

## License

MIT
