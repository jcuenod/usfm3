# usfm3

`usfm3` is a Python binding for the Rust `usfm3` parser.

It exposes a staged API so live-editor code can stay on tokens/CST until it actually needs AST-backed output.

## Installation

```bash
pip install usfm3
```

## Quick Start

```python
import usfm3

parsed = usfm3.parse(text)

tokens = usfm3.tokenize(text)
cst = usfm3.parse_cst(text)
ast_document = usfm3.parse_ast(text, diagnostics=True)

ast = parsed.ast()
source_map = parsed.source_map()
diagnostics = parsed.diagnostics

usj = parsed.to_usj()
usj_with_spans = parsed.to_usj(spans=True)
usx = parsed.to_usx()
usfm = parsed.to_usfm()
vref = parsed.to_vref()
```

## API

### `usfm3.parse(usfm: str, diagnostics: bool = False) -> ParsedDocument`

Returns a lazy parsed handle.

### `usfm3.parse_cst(usfm: str) -> dict`

Returns a JSON-friendly CST tree.

### `usfm3.parse_ast(usfm: str, diagnostics: bool = False) -> dict`

Returns:

```python
{
    "ast": ...,
    "source_map": ...,
    "diagnostics": list | None,
}
```

### `usfm3.tokenize(usfm: str) -> list[dict]`

Returns token spans suitable for editor tooling.

### `ParsedDocument`

- `cst() -> dict`
- `ast() -> dict`
- `source_map() -> dict`
- `to_usj(spans: bool = False) -> dict`
- `to_usx() -> str`
- `to_usfm() -> str`
- `to_vref() -> dict[str, str]`
- `diagnostics -> list[dict] | None`

## Notes

- Diagnostics are only computed when `diagnostics=True`.
- Diagnostics are a flat list with `severity`, `code`, `message`, `span`, and optional `anchor_cst`.
- AST nodes do not include spans.
- `spans=True` on `to_usj()` derives inline span data from the source map.

## License

MIT
