# usfm3

An error-tolerant, fast USFM 3.x parser written in Rust, with WebAssembly (npm) and
Python bindings.

One parse produces a flat tape of 16-byte nodes. Every output — USJ, USX, normalized
USFM, a CST for editors, cleaned verse text, and diagnostics — is a view over that tape
and the original source, so you pay only for the views you ask for.

> **0.3 is a rewrite.** The API changed in every package. See
> [CHANGELOG.md](CHANGELOG.md#migrating-from-02) for what moved where. The 0.2 code is
> in this repository's history (tag `v0.2.1`).
>
> Until 1.0, minor versions may break the API; pin `0.3.*` if that matters to you.

## Packages

| Package | Registry | Docs |
| --- | --- | --- |
| [`crates/usfm3`](crates/usfm3/) | [crates.io](https://crates.io/crates/usfm3) | [README](crates/usfm3/README.md) |
| [`crates/usfm3-wasm`](crates/usfm3-wasm/) | [npm `usfm3`](https://www.npmjs.com/package/usfm3) | [README](crates/usfm3-wasm/README.md), [CHANGELOG](crates/usfm3-wasm/CHANGELOG.md) |
| [`crates/usfm3-python`](crates/usfm3-python/) | [PyPI `usfm3`](https://pypi.org/project/usfm3/) | [README](crates/usfm3-python/README.md), [CHANGELOG](crates/usfm3-python/CHANGELOG.md) |
| [`crates/usfm3-cli`](crates/usfm3-cli/) | — (build from source) | `usfm3 --help` |

## Quick look

```rust
let doc = usfm3::parse(src, usfm3::Version::V3_1);
let usj = doc.to_usj();
let verses = doc.vref_dict(); // [("GEN 1:1", "In the beginning…"), …]
```

```js
import { parse } from "usfm3";
const doc = parse(src);
const usj = doc.toUsj();
doc.free();
```

```python
import usfm3
doc = usfm3.parse(src)
usj = doc.to_usj()
```

## CLI

```sh
cargo install --path crates/usfm3-cli

usfm3 vref  file.usfm               # verse reference -> text, as JSON
usfm3 usj   file.usfm
usfm3 usx   file.usfm
usfm3 usfm  file.usfm               # normalized USFM
usfm3 cst   file.usfm [--diagnostics]
usfm3 diagnostics file.usfm
```

## Development

The conformance tests use the [BridgeConn usfm-grammar](https://github.com/Bridgeconn/usfm-grammar)
corpus, which is not vendored:

```sh
git clone --depth 1 https://github.com/Bridgeconn/usfm-grammar.git /tmp/usfm-grammar
mkdir -p crates/usfm3/tests/fixtures/usfm-grammar
cp -r /tmp/usfm-grammar/tests/* crates/usfm3/tests/fixtures/usfm-grammar/

cargo test --workspace
crates/usfm3-wasm/build.sh && node --test 'crates/usfm3-wasm/test/*.test.mjs'
```

CI enforces `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`.

### Releasing

The version lives once, in the root `Cargo.toml` (`[workspace.package]`); the npm and
PyPI packages derive theirs from it. Bump it, update the changelogs, then push a `v*`
tag matching it. That tag publishes to crates.io, npm and PyPI.

## License

MIT
