# usfm3

An error-tolerant [USFM 3.x](https://docs.usfm.bible/usfm/3.1.1/index.html) parser for Python. Outputs [USJ](https://docs.usfm.bible/usfm/3.1.1/usj/index.html) (JSON), [USX](https://docs.usfm.bible/usfm/3.1.1/usx/index.html) (XML), normalized USFM, and verse-reference maps.

Built in Rust for speed, with native Python bindings via [PyO3](https://pyo3.rs).

Also available as a [Rust crate](https://crates.io/crates/usfm3) and [npm package](https://www.npmjs.com/package/usfm3) (WebAssembly).

## Installation

```sh
pip install usfm3
```

Requires Python 3.9+.

## Usage

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

# Skip semantic validation
result = usfm3.parse(text, validate=False)
```

## API

### `usfm3.parse(usfm: str, validate: bool = True) -> ParseResult`

Parse a USFM string. Returns a `ParseResult` with lazy output methods and diagnostics.

### `ParseResult`

| Method / Property | Returns | Description |
|---|---|---|
| `to_usj()` | `dict` | USJ (Unified Scripture JSON) |
| `to_usx()` | `str` | USX (Unified Scripture XML) |
| `to_usfm()` | `str` | Normalized USFM |
| `to_vref()` | `dict` | Verse reference to plain text map |
| `has_errors()` | `bool` | True if any error-severity diagnostics |
| `diagnostics` | `list[Diagnostic]` | Parser and validation diagnostics |

### `Diagnostic`

| Property | Type | Description |
|---|---|---|
| `severity` | `str` | `"error"`, `"warning"`, or `"info"` |
| `code` | `str` | Machine-readable code (e.g. `"UnknownMarker"`) |
| `message` | `str` | Human-readable message |
| `start` | `int` | Start byte offset in source |
| `end` | `int` | End byte offset in source |

## License

MIT
