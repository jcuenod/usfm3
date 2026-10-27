# Changelog

This file covers the Rust crate (`usfm3` on crates.io) and the CLI. The npm and PyPI
packages have their own changelogs, which carry their own migration notes:

- [npm `usfm3`](crates/usfm3-wasm/CHANGELOG.md)
- [PyPI `usfm3`](crates/usfm3-python/CHANGELOG.md)

Until 1.0, a minor version bump (0.2 → 0.3) may break the API. Cargo and npm treat it
as breaking, so `usfm3 = "0.2"` and `"usfm3": "^0.2.1"` will not pick it up. pip has
no such rule: pin `usfm3<0.3` if you aren't ready to upgrade.

## Unreleased

### Alternate and published numbers

`\ca`/`\cp` attach to a chapter and `\va`/`\vp` to a verse only when they come
directly after that `\c` or `\v`, at most one of each. Anything else stays a `char`
element, as before, and now raises a `MisplacedNumberMarker` warning. That covers a
`\cp` in the middle of a chapter, a `\cp` after a verse (0.3.1 attached it to the
verse), and a second `\vp` on the same verse (0.3.1 kept only the last; the first
now wins and the second stays a `char`).

### Verse and chapter numbers in diagnostics

- A number glued to its marker (`\v10`, `\c2`) is read the way the serializers
  already read it, with a new `MissingSpaceBeforeNumber` warning. 0.3.1 reported a
  `MissingVerseNumber`/`MissingChapterNumber` error and lost its place in the
  verse sequence, so the next verse was flagged too.

## 0.3.1

### Performance

No API or output changes. Measured on the BSB and the door43 Hindi IRV, against
0.3.0:

| | change |
| --- | --- |
| parse | −25% |
| `to_usj` | −27% |
| `to_usx` | −32% |
| `to_usfm` | −19% |
| `to_vref_json` | −12% |
| `to_packed_tape` (utf16 / chars) | −24% / −26% |

Marker names are now looked up with a generated `match` rather than a `phf` map, so
`phf` is no longer a dependency: `memchr` is the only one.

## 0.3.0

A rewrite. 0.2 lexed into tokens, built a CST, then lowered that into an AST and a
source map, and serialized from the AST. 0.3 parses once into a flat *tape* of 16-byte
nodes, and every output is a view over that tape and the borrowed source. There is no
token stream, AST or source map any more.

On a 27 MB input (synthetic, so treat as indicative), relative to 0.2.1:

| | 0.2.1 | 0.3.0 |
| --- | --- | --- |
| vref | 3.13 s, 2.6 GB | 0.23 s, 135 MB |
| USJ | 13.3 s, 8.3 GB | 0.62 s, 482 MB |
| USX | 2.66 s, 2.8 GB | 0.69 s, 509 MB |

Runtime dependencies are now `memchr` and `phf` only; `serde`, `serde_json`,
`quick-xml`, `logos` and `unicode-general-category` are gone.

### Migrating from 0.2

#### Parsing

```rust
// 0.2
let parsed = usfm3::parse(text, usfm3::ParseOptions { diagnostics: true });

// 0.3
use usfm3::{ParseOptions, Version};
let doc = usfm3::parse(text, Version::V3_1);                       // diagnostics off
let doc = usfm3::parse_with(text, Version::V3_1, ParseOptions { diagnostics: true });
```

`Document<'src>` **borrows** the source instead of owning a copy, so keep the `String`
alive alongside it. `parse_owned`, `parse_cst_owned` and `parse_ast_owned` are gone. If
you need a self-contained value, store the `String` and the `Document` together, for
example with [`self_cell`](https://crates.io/crates/self_cell) (the bindings do this).

`Version` is new: it picks the marker table (`V3_1`, the default, or `V3_2`).

#### Outputs

| 0.2.1 | 0.3.0 |
| --- | --- |
| `to_usj(UsjOptions) -> Result<serde_json::Value, UsjError>` | `to_usj() -> String` (JSON text; infallible) |
| `to_usj(UsjOptions { include_spans: true })` | removed. Positions come from the CST (`to_cst_json`, `cursor()`) |
| `to_usx() -> Result<String, UsxError>` | `to_usx() -> String` |
| `to_usfm() -> String` | unchanged |
| `to_vref() -> serde_json::Map<String, Value>` | `vref_dict() -> Vec<(String, String)>`, in document order; or `to_vref_json() -> String` |

If you relied on `serde_json::Value`, parse the string:
`serde_json::from_str::<Value>(&doc.to_usj())`.

New verse-text primitives: `vref_text()` (keyed by a packed `VerseRef`), `vrefs_raw()`
(uncleaned) and `for_each_vref(f)` (streaming).

#### Tokens, CST, AST, source map

| 0.2.1 | 0.3.0 |
| --- | --- |
| `tokenize`, `ParsedDocument::tokens()` | removed |
| `parse_cst`, `ParsedDocument::cst()` | `doc.cursor()` (`low_level::Cursor`) to walk; `doc.to_cst_json()` for JSON |
| `parse_ast`, `lower_cst`, `ast()`, `ast_document()` | removed. Walk the CST, or read `to_usj()` |
| `source_map()` | removed. Every CST node has a `span()` |

The modules `ast`, `builder`, `cst`, `diagnostics`, `lexer`, `markers`, `source_map`,
`usfm`, `usj`, `usx`, `validation` and `vref` are no longer public. Use the crate root,
or `usfm3::low_level` for the tape (`Tape`, `Node`, `NodeKind`), `Cursor`,
`MarkerTable` and `Span`.

#### Diagnostics

| 0.2.1 | 0.3.0 |
| --- | --- |
| `diagnostics() -> Option<&[Diagnostic]>` (`None` when off) | `diagnostics() -> &[Diagnostic]` (empty when off) |
| `usfm3::diagnostics::{Diagnostic, Severity, DiagnosticCode}` | `usfm3::{Diagnostic, Severity, DiagnosticCode}` |
| `d.span: Range<usize>` | `d.span: Span { start: u32, len: u32 }`. Use `d.span.range()` for the old `Range<usize>`, or `d.span.end()` |
| `d.anchor_cst` | removed |
| — | `d.span.line_col(source)` → 1-based `{ line, col }` |

Offsets are still UTF-8 bytes. `Severity` is still `Error`/`Warning`/`Info`, and the
`DiagnosticCode` names are the same, with one addition: `StrayBackslash`.

#### CLI

The CLI is not published; this only matters if you build it from source.

- `tokens` and `ast` were removed, and so was `usj --inline-spans`.
- `cst --diagnostics` is new.
- A file path is now required; the CLI no longer reads stdin.

### Output changes

Formats changed even where the content didn't, so expect diffs against 0.2.1 output.
These apply to every package.

**USJ**
- Attributes are flat properties (`"lemma": "…"`) instead of
  `"attributes": [{ "key", "value" }]`, as in the USJ reference.
- Empty containers carry `"content": []`.
- Text strings no longer keep the space after a marker (`" the first verse "` is now
  `"the first verse"`), and whitespace-only strings between inline elements are dropped.
- The `\xt` link attribute is `href` (the USFM 3.1 name) instead of `link-href`.
- An unknown marker is a `para` (or a `char`, when it has an explicit `\tag*` close)
  carrying its own marker name, instead of a `type: "unknown"` node.

**All formats**
- Verses before any `\c` belong to chapter 1 when the book has no `\c` at all (a
  one-chapter book missing `\c 1`), and to chapter 0 when a real `\c` follows, so
  they can't merge into it. 0.2.1 wrote `BOOK :N` in vref and `BOOK 0:N` in sids.
- Malformed attributes are parsed leniently instead of kept as text:
  `|lemma strong="l"` yields `strong="l"`, dropping the dangling word.
- A `|` that doesn't read as an attribute list is text: in a table cell or footnote
  text (no close of its own), or a bare value on a marker with no default attribute
  (`\add one| two\add*`). 0.2.1 silently dropped the text after it. Where an
  attribute list was attempted, DefaultAttributeNotDefined or InvalidAttributes
  reports it.

**USX**
- No `<?xml?>` declaration; one top-level element per line with a two-space indent.
- Implicitly closed character and note markers carry `closed="false"`.
- Unknown markers carry `status="unknown"`, as in the reference USX.

**vref**
- Duplicate references are concatenated; 0.2.1 kept only the last one.
- A verse straight after a heading or blank line (`\s`, `\b`, `\lit`), or inside a table
  cell, is no longer dropped. Its text used to be glued onto the previous verse.
- Table cell text is included.
- Doubled spaces left behind by removed notes are collapsed.
- `\qa`, `\rq` and `\vp` text, and the attributes of milestones that span several
  lines, no longer leak into verse text.
- In aligned USFM (one word per line), a line break before trailing punctuation is
  layout, not a space: `Paul, a servant` rather than 0.2.1's `Paul,a servant`.
- `//` (an optional line break) is dropped rather than kept verbatim.
- Glued `\v1` and `\c2` (no space) are recovered as verse 1 and chapter 2.

### Added

- `Version`, which selects the USFM 3.1 or 3.2 marker table.
- `BOOK_CODES` covers the deuterocanon/apocrypha, peripherals and `XXA`–`XXG` as well
  as the 66 books. vref keys use the `\id` code as written (`VrefRow::book_code`),
  even for codes outside the list. InvalidBookCode says when a code is a lowercase
  known one (`php` → `PHP`).
- `Document::to_packed_tape(offsets)` and `schema_json(version)`. These give the tape as
  a flat `u32` buffer, with offsets in UTF-8, UTF-16 or code points, plus the tables
  needed to decode it. The JS and Python packages use them to hand the parse to a host
  without building JSON.
- `low_level::OffsetMap`, which converts byte offsets to UTF-16 or code points and
  resolves line/column in a single forward pass.
