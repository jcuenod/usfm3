# Changelog

## 0.3.1

### Requires WebAssembly SIMD

The `.wasm` is now built with SIMD, which the parser's scanner uses. It needs Chrome
91, Firefox 89, Safari 16.4, Node 16.4 or later (every engine released since 2023).

### Performance

No API or output changes. Measured in Node: SIMD makes `parse()` about 23% faster
and `parseTape()` about 20% faster; the new marker lookup takes a further 9% off
`parse()`; walking a tape with `children()` is about 22% faster. The Rust
serializer changes listed in the crate changelog apply here too.

## 0.3.0

A rewrite of the underlying parser. 0.2 built tokens, then a CST, then an AST plus a
source map. 0.3 parses once into a flat tape, and every output is a view over it. The
package is still `usfm3` and the entry point is still `parse()`, but several methods
are gone and the diagnostic and CST shapes changed. The parser is also much faster
(around 13× for `toVref()` and 20× for `toUsj()` in native benchmarks, with far less
memory).

`"usfm3": "^0.2.1"` will not pick up 0.3.0. Upgrade deliberately.

### Migrating from 0.2

#### Functions and methods

| 0.2.1 | 0.3.0 |
| --- | --- |
| `parse(usfm, { diagnostics })` | unchanged; also accepts `version: "3.1" \| "3.2"` (default `"3.1"`) |
| `parseCst(usfm)` | `parseCst(usfm, options?)`. The node shape changed (below) |
| `parseAst(usfm, options)` | removed. Use `toUsj()` for a semantic tree |
| `tokenize(usfm)` | removed. Use `cst()`, or `parseTape()` for a flat walk |
| `doc.ast()` | removed. Use `toUsj()` |
| `doc.sourceMap()` | removed. Positions are on `cst()` nodes and on the tape |
| `doc.toUsj({ spans: true })` | `toUsj()` takes no options; inline spans are gone |
| `doc.toUsx()`, `doc.toUsfm()`, `doc.toVref()` | unchanged signatures (see output changes below) |
| `doc.diagnostics()` → `null`/`undefined` when off | → `[]` when off |
| `doc.free()` | unchanged |

#### Diagnostics

```ts
// 0.2.1: UTF-8 byte offsets, nested
{ severity, code, message, span: { start, end }, anchor_cst? }

// 0.3.0: UTF-16 offsets by default, flat, with line/column
{ severity, code, message, start, end, line, column }
```

- `d.span.start` becomes `d.start`, and `d.span.end` becomes `d.end`.
- **Offsets changed space as well as place.** 0.2.1 gave UTF-8 bytes; 0.3.0 gives UTF-16
  code units, so `source.slice(d.start, d.end)` is now correct as-is. If you converted
  bytes to UTF-16 yourself, **remove that conversion**: on non-ASCII text, which is
  most translation text, it would now misplace every position. To keep byte offsets,
  call `doc.diagnostics({ offsets: "utf8" })`.
- `line` and `column` are new and 1-based. `column` uses the same unit as `start`.
- `anchor_cst` is gone.
- `severity` (`"error" | "warning" | "info"`) and the `code` names are unchanged. One
  new code was added: `StrayBackslash`.

#### CST

`cst()` and `parseCst()` return a new node shape:

```ts
// 0.2.1
{ type, span: { start, end }, marker?, token_kind?, text?, children? }

// 0.3.0
{ type, marker?, category?, role?, closing?, number?, start, end,
  synthetic?, close?: { start, end, synthetic? }, children? }
```

Text is no longer inlined: slice the source with `start`/`end`. **CST offsets are UTF-8
bytes**, unlike diagnostics, so on non-ASCII sources use `parseTape()`, whose
`TapeCursor.text` slices for you. Each marker now carries its `category`, `role` and
`closing` rule, and the position of its matching close (`close`). The full types are
in `index.d.ts`.

#### Package layout

- `main` and `types` now point at `node.js` and `index.d.ts`; they used to point at the
  generated wasm-bindgen files. Importing the package root works as before: Node
  resolves to `node.js` (wasm initialized for you), and bundlers resolve to `index.js`
  (call the default `init()` first).
- New subpath: `usfm3/tape.js`. It imports no wasm, so it can decode a transferred tape
  on a worker.

### Output changes

`toUsj()`, `toUsx()` and `toVref()` changed format, so expect diffs against 0.2.1
output. In USJ, attributes are flat properties and empty containers carry
`"content": []`. In USX there is no XML declaration and the indentation changed. In
vref, duplicate references are concatenated and verses after headings are no longer
dropped. The full list is in the
[repository changelog](https://github.com/jcuenod/usfm3/blob/main/CHANGELOG.md#output-changes).

### Added: `tape()`, the CST as a packed typed array

`cst()` materializes a JSON string and re-parses it into an object graph. On a
book-sized document that is ~4.7 MiB of text and ~58,000 objects, on every call. That
cost dominates when an editor reparses on each keystroke.

```js
import { parseTape } from "usfm3";

const tape = parseTape(usfm);          // walkable, holds no wasm resources
for (const node of tape.scan()) {      // allocation-free
  if (node.kind === "marker") console.log(node.markerName, node.start, node.end);
}
```

Measured on a 278 KiB document (Node 24):

| | `parse()` + `cst()` | `parseTape()` |
| --- | --- | --- |
| Time | 27.1 ms | 2.5 ms |
| Payload | 4,725 KiB | 917 KiB |
| Worker handoff | 35.1 ms (`structuredClone`) | 2.9 ms (transfer the buffer) |

`toUsj()` and `toVref()` are unaffected.

New exports:

- `ParsedDocument.tape(options?)` → `Uint32Array`: an 8-word header, then 4 words per
  node. It is a copy owned by the JS heap, so it survives `free()` and wasm memory
  growth, and it can be transferred to a worker.
- `tapeSchema(version?)` → the marker and kind tables needed to interpret the ids.
  Memoized and frozen.
- `tapeView(doc, source, options?)` and `parseTape(usfm, options?)` → a walkable `Tape`.
- `Tape`, `TapeCursor`, `tape()`. These are also available from `usfm3/tape.js`.

`Tape#nodes()` yields independent cursors (safe to spread, filter or retain).
`Tape#scan()` is the allocation-free walk and yields **one reused cursor**.

### Added: `offsets` option

`tape()` and `diagnostics()` both accept `{ offsets: "utf8" | "utf16" | "chars" }`,
defaulting to `"utf16"`.

`TapeCursor.text`, `.markerName` and `.toJSON()` **throw** on a tape whose offsets are
not the host's own, rather than returning wrong characters (`source.slice()` with byte
offsets silently misreads any non-ASCII source). `Tape#sliceable` reports this
without needing a `try`/`catch`; the check is O(1) because the header carries the
source length in all three spaces. `start`, `end`, `kind` and `marker` are safe to
read in any space.

### Note on subpath imports

`exports` intentionally does not expose `./node.js` or `./api.js`. Import the package
root: both entry points expose the same namespace.

```js
import { parse, parseCst, parseTape, tapeView, tapeSchema, Tape, TapeCursor } from "usfm3";
```
