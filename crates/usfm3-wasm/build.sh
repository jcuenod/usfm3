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

wasm-pack build --release --target web --out-dir "$OUT" --out-name usfm3_wasm

# Node wrapper + merge our package metadata (name, exports, files, …) into the
# generated package.json.
cp node.js "$OUT/node.js"
node -e '
  const fs = require("fs");
  const out = process.argv[1];
  const pkg = JSON.parse(fs.readFileSync(out + "/package.json", "utf8"));
  const ovr = JSON.parse(fs.readFileSync("package-overrides.json", "utf8"));
  fs.writeFileSync(out + "/package.json", JSON.stringify({ ...pkg, ...ovr }, null, 2) + "\n");
' "$OUT"

echo "built $OUT/ (npm package \"usfm3\")"
