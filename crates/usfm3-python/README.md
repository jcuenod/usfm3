# usfm3

`usfm3` is a Python parser for [USFM 3.x](https://docs.usfm.bible/usfm/3.1.1/index.html).
It turns USFM into Python-friendly outputs:

- `to_usj(spans=False)`: [USJ](https://docs.usfm.bible/usfm/3.1.1/usj/index.html) as a `dict`
- `to_usx()`: [USX](https://docs.usfm.bible/usfm/3.1.1/usx/index.html) as an XML `str`
- `to_usfm()`: normalized USFM as a `str`
- `to_vref()`: a verse-text map like `{"GEN 1:1": "In the beginning..."}`

The parser is error-tolerant, so malformed input still produces a parse result with
structured diagnostics.

Built in Rust for speed, with native Python bindings via [PyO3](https://pyo3.rs).

## Installation

```bash
pip install usfm3
```

Requires Python 3.9+.

## Quick Start

```python
import usfm3

text = r"""\id GEN
\c 1
\p
\v 1 In the beginning God created the heavens and the earth.
"""

result = usfm3.parse(text)

print(result.to_vref()["GEN 1:1"])

for diagnostic in result.diagnostics:
    print(
        f"[{diagnostic.severity}] {diagnostic.code}: "
        f"{diagnostic.message} ({diagnostic.start}..{diagnostic.end})"
    )

usj = result.to_usj()
usj_with_spans = result.to_usj(spans=True)
usx = result.to_usx()
normalized_usfm = result.to_usfm()
```

## Validation

`parse()` runs semantic validation by default, so diagnostics can include issues such as
chapter and verse sequencing, invalid attributes, or mismatched milestones.

If you only want parsing, disable validation:

```python
result = usfm3.parse(text, validate=False)
```

## API Summary

### `usfm3.parse(usfm: str, validate: bool = True) -> ParseResult`

Parses a USFM string and returns a `ParseResult`.

### `ParseResult`

- `to_usj(spans: bool = False) -> dict`
- `to_usx() -> str`
- `to_usfm() -> str`
- `to_vref() -> dict[str, str]`
- `has_errors() -> bool`
- `diagnostics -> list[Diagnostic]`

### `Diagnostic`

Each diagnostic has:

- `severity`: `"error"`, `"warning"`, or `"info"`
- `code`: machine-readable code such as `"UnknownMarker"`
- `message`: human-readable message
- `start`
- `end`

`start` and `end` are byte offsets into the original source.

## Notes

- `to_vref()` returns plain verse text keyed by references such as `"GEN 1:1"`.
- `to_usfm()` returns normalized USFM, so whitespace may be regularized.
- When `spans=True`, structural USJ nodes include a nested `spans` object with `node` and optional `code`, `number`, and `close` ranges.
- Invalid USFM is reported through `diagnostics`; `parse()` still returns a result.

## Related Packages

- Rust crate: [crates.io/crates/usfm3](https://crates.io/crates/usfm3)
- JavaScript/TypeScript package: [npmjs.com/package/usfm3](https://www.npmjs.com/package/usfm3)
- Source code: [github.com/jcuenod/usfm3](https://github.com/jcuenod/usfm3)

## License

MIT
