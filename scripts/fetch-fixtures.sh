#!/usr/bin/env bash
# Fetch the BridgeConn usfm-grammar conformance corpus into
# crates/usfm3/tests/fixtures/usfm-grammar, at a pinned commit so upstream
# changes can't break CI unannounced. To move to a newer corpus, bump REV, run
# the tests, and update crates/usfm3/tests/bridgeconn_expected_failures.txt.
set -euo pipefail

REV=eca9e9f7b3476791b4d5fd711714fa15b55534bf # 2026-10-13
DEST="$(cd "$(dirname "$0")/.." && pwd)/crates/usfm3/tests/fixtures/usfm-grammar"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git -C "$tmp" init -q
git -C "$tmp" fetch -q --depth 1 https://github.com/Bridgeconn/usfm-grammar.git "$REV"
git -C "$tmp" checkout -q FETCH_HEAD

rm -rf "$DEST"
mkdir -p "$DEST"
cp -r "$tmp/tests/." "$DEST/"
echo "usfm-grammar @ ${REV:0:7} -> $DEST"
