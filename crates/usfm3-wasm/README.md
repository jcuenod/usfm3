# usfm3

`usfm3` is the JavaScript / TypeScript package for the Rust `usfm3` parser.

It exposes a staged API so browser, Node, and editor integrations can stay on the cheapest representation they need.

## Installation

```sh
npm install usfm3
```

In browsers:

```ts
import init from "usfm3";
await init();
```

## Quick Start

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

## API

### `parse(usfm: string, options?: { diagnostics?: boolean }): ParsedDocument`

Returns a lazy parsed handle.

### `parseCst(usfm: string): any`

Returns a JSON-friendly CST tree.

### `parseAst(usfm: string, options?: { diagnostics?: boolean }): any`

Returns:

```ts
{
  ast: ...,
  source_map: ...,
  diagnostics?: Diagnostic[]
}
```

### `tokenize(usfm: string): any[]`

Returns token spans suitable for editor tooling.

### `ParsedDocument`

- `cst(): any`
- `ast(): any`
- `sourceMap(): any`
- `diagnostics(): Diagnostic[] | undefined`
- `toUsj(options?: { spans?: boolean }): any`
- `toUsx(): string`
- `toUsfm(): string`
- `toVref(): Record<string, string>`
- `free(): void`

## Notes

- `tokenize()` and `parseCst()` are cheaper than materializing the AST.
- Diagnostics are only computed when `diagnostics: true` is requested.
- Diagnostics are a flat list.
- AST nodes do not carry spans.
- `toUsj({ spans: true })` derives inline span data from the source map.

## License

MIT
