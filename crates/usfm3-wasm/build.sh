#!/usr/bin/env bash
# Build the npm package into ./pkg.
#
# Uses wasm-pack's `web` target (emits both an async default `init` and a sync
# `initSync`). Browser/bundler consumers call the default `init`; Node consumers
# load `node.js`, which calls `initSync` with the bundled bytes. The `exports` map
# in package-overrides.json routes each environment to the right entry.
set -euo pipefail
cd "$(dirname "$0")"

OUT="${1:-pkg}"

# WebAssembly SIMD lets memchr, the parser's structural scanner, use vector
# instructions instead of its scalar fallback. Every engine since 2023 supports it
# (Chrome 91, Firefox 89, Safari 16.4, Node 16.4).
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-feature=+simd128"

wasm-pack build --release --target web --out-dir "$OUT" --out-name usfm3_wasm

# Hand-written JS/TS layer (the packed-tape view, the ergonomic entries, and the
# authoritative types) + the Node wrapper, then merge our package metadata (name,
# exports, files, …) into the generated package.json.
cp node.js js/index.js js/api.js js/tape.js js/index.d.ts js/tape.d.ts CHANGELOG.md "$OUT/"
node -e '
  const fs = require("fs");
  const out = process.argv[1];
  const pkg = JSON.parse(fs.readFileSync(out + "/package.json", "utf8"));
  const ovr = JSON.parse(fs.readFileSync("package-overrides.json", "utf8"));
  fs.writeFileSync(out + "/package.json", JSON.stringify({ ...pkg, ...ovr }, null, 2) + "\n");
' "$OUT"

echo "built $OUT/ (npm package \"usfm3\")"
