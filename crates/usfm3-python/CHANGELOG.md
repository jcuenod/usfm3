# Changelog

## 0.3.0

A rewrite of the underlying parser. 0.2 built tokens, then a CST, then an AST plus a
source map. 0.3 parses once into a flat tape, and every output is a view over it. The
module is still `usfm3` and the entry point is still `parse()`, but several functions
are gone and the diagnostic and CST shapes changed. The parser is also much faster
(around 13× for `to_vref()` and 20× for `to_usj()` in native benchmarks, with far less
memory).

**pip does not treat 0.2 → 0.3 as breaking.** An unpinned `usfm3` dependency will
upgrade. Pin `usfm3<0.3` until you have migrated.

### Migrating from 0.2

#### Functions and methods

| 0.2.1 | 0.3.0 |
| --- | --- |
| `parse(usfm, diagnostics=False)` | `parse(usfm, *, diagnostics=False, version="3.1")`. **`diagnostics` is now keyword-only**: `parse(text, True)` raises `TypeError` |
| `parse_cst(usfm)` | `parse_cst(usfm, *, diagnostics=False, version="3.1")`. The node shape changed (below) |
| `parse_ast(usfm, diagnostics=False)` | removed. Use `to_usj()` for a semantic tree |
| `tokenize(usfm)` | removed. Use `cst()`, or `tape()` for a flat numeric view |
| `doc.ast()` | removed. Use `to_usj()` |
| `doc.source_map()` | removed. Positions are on `cst()` nodes |
| `doc.to_usj(spans=True)` | `to_usj()` takes no arguments; inline spans are gone |
| `doc.diagnostics` (property, `None` when off) | `doc.diagnostics()` (method, `[]` when off) |
| `doc.to_usx()` raised `RuntimeError` on failure | never raises |
| `doc.to_usfm()`, `doc.to_vref()` | unchanged signatures (see output changes below) |

#### Diagnostics

```python
# 0.2.1: a property; UTF-8 byte offsets, nested
doc.diagnostics  # [{"severity", "code", "message", "span": {"start", "end"}, "anchor_cst"?}]

# 0.3.0: a method; code-point offsets by default, flat, with line/column
doc.diagnostics()  # [{"severity", "code", "message", "start", "end", "line", "column"}]
```

- `doc.diagnostics` without parentheses is now a bound method. Iterating over it raises
  `TypeError`, and a truthiness check is always true. Add the `()`.
- `d["span"]["start"]` becomes `d["start"]`, and `d["span"]["end"]` becomes `d["end"]`.
- **Offsets changed space as well as place.** 0.2.1 gave UTF-8 bytes; 0.3.0 gives code
  points, so `src[d["start"]:d["end"]]` is now correct as-is. If you converted bytes
  yourself, **remove that conversion**: on non-ASCII text it would now misplace every
  position. `doc.diagnostics(offsets="utf8")` gives byte offsets; `"utf16"` is also
  accepted.
- `line` and `column` are new and 1-based. `column` uses the same unit as `start`.
- `anchor_cst` is gone.
- `severity` (`"error" | "warning" | "info"`) and the `code` names are unchanged. One
  new code was added: `StrayBackslash`.

#### CST

`cst()` and `parse_cst()` return a new node shape:

```python
# 0.2.1
{"type", "span": {"start", "end"}, "marker"?, "token_kind"?, "text"?, "children"?}

# 0.3.0
{"type", "marker"?, "category"?, "role"?, "closing"?, "number"?, "start", "end",
 "synthetic"?, "close"?: {"start", "end", "synthetic"?}, "children"?}
```

Text is no longer inlined; slice the source with `start`/`end`. **CST offsets are UTF-8
bytes**, unlike diagnostics, so slice the encoded source:
`src.encode()[n["start"]:n["end"]].decode()`. Each marker now carries its `category`,
`role` and `closing` rule, and the position of its matching close (`close`).

### Output changes

`to_usj()`, `to_usx()` and `to_vref()` changed format, so expect diffs against 0.2.1
output. In USJ, attributes are flat properties and empty containers carry
`"content": []`. In USX there is no XML declaration and the indentation changed. In
vref, duplicate references are concatenated and verses after headings are no longer
dropped. The full list is in the
[repository changelog](https://github.com/jcuenod/usfm3/blob/main/CHANGELOG.md#output-changes).

### Added: `tape()` and `tape_schema()`

```python
import numpy as np, usfm3

doc = usfm3.parse(text)
words = np.frombuffer(doc.tape(), dtype="<u4")     # 8-word header, then 4 per node
schema = usfm3.tape_schema()                        # kind + marker tables
```

`tape(*, offsets="chars")` returns the flat tape as `bytes`: an 8-word header followed
by four little-endian `uint32`s per node (`kind | flags << 8 | marker << 16`, `start`,
`len`, `link`). This is the numeric, vectorized view. For ordinary traversal, keep using
`cst()`: a Python cursor would be an in-process PyO3 object with no serialization to
avoid, so packing gains nothing there.

`tape_schema(*, version="3.1")` returns the tables needed to resolve the `kind` and
`marker` ids; the array index *is* the id. It is small and stable for a given version,
so cache it.

### Added: `version` option

`parse()` and `parse_cst()` accept `version="3.1"` (the default) or `"3.2"`, which
selects the marker table.
