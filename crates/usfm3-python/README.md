# usfm3

`usfm3` is a Python binding for the Rust `usfm3` parser — an error-tolerant,
blazingly fast USFM 3.x parser.

A single `parse()` produces a flat tape; every output (USJ, USX, USFM, CST, vref,
diagnostics) is a cheap view over it. There is no separate token/AST/source-map
stage — the handle below is all you need.

## Installation

```bash
pip install usfm3
```

## Quick Start

```python
import usfm3

doc = usfm3.parse(text)                       # diagnostics off (hot path)
doc = usfm3.parse(text, diagnostics=True)     # opt into validation
doc = usfm3.parse(text, version="3.2")        # select USFM version

usj = doc.to_usj()       # dict
usx = doc.to_usx()       # str (XML)
usfm = doc.to_usfm()     # str (normalized USFM)
vref = doc.to_vref()     # { "GEN 1:1": "In the beginning…", ... }
cst = doc.cst()          # dict (concrete syntax tree, for editors)
diags = doc.diagnostics  # list (empty unless diagnostics=True)

# One-shot convenience:
cst = usfm3.parse_cst(text)
```

## API

### `usfm3.parse(usfm: str, *, diagnostics: bool = False, version: str = "3.1") -> ParsedDocument`

Parse into a lazy handle. Never raises on malformed input — the parser recovers,
and (when `diagnostics=True`) reports what it had to recover from. `version` is
`"3.1"` or `"3.2"`.

### `usfm3.parse_cst(usfm: str, *, diagnostics: bool = False, version: str = "3.1") -> dict`

One-shot: parse and return the concrete syntax tree.

### `ParsedDocument`

- `cst() -> dict` — concrete syntax tree (byte spans, marker category/role/closing,
  matching close token), for editor decoration and folding.
- `to_usj() -> dict` — USJ document.
- `to_usx() -> str` — USX (XML).
- `to_usfm() -> str` — normalized USFM.
- `to_vref() -> dict[str, str]` — verse-reference → cleaned text, in document
  order. Empty verses are dropped and duplicate references merged, so it is a
  faithful dictionary.
- `diagnostics(*, offsets="chars") -> list[dict]` — flat list of diagnostics. Empty
  unless `diagnostics=True` was passed. **In 0.3.0 this became a method** (to carry
  `offsets`) and its offsets became code points; `offsets="utf8"` restores the old
  byte values. See CHANGELOG.md.
- `tape(*, offsets="chars") -> bytes` — the flat tape packed as little-endian
  `uint32`s: an 8-word header, then 4 words per node. The numeric/vectorized view
  (`numpy.frombuffer(doc.tape(), dtype="<u4")`); for ordinary traversal use `cst()`.

Module-level: `parse`, `parse_cst`, and `tape_schema(*, version="3.1")` — the kind and
marker tables needed to resolve the ids in a packed tape. The array index *is* the id.

## Notes

- Diagnostics carry `severity` (`"error" | "warning" | "info"`), a stable `code`,
  a human `message`, `start`/`end`, and 1-based `line`/`column`.
- **Offsets are code points** for `diagnostics()` and `tape()`, so
  `src[d["start"]:d["end"]]` is correct. `cst()` is unchanged and reports bytes.
- vref text is version-correct: notes are excluded, character-marker tags are
  dropped (inner text kept), whitespace is collapsed.

## License

MIT
