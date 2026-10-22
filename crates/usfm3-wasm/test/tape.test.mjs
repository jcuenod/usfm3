/**
 * Tests for the packed-tape JS layer. Dependency-free: `node --test 'test/*.test.mjs'`.
 *
 * The load-bearing test is `traversal parity`: `js/tape.js` reimplements
 * `Cursor::first_child` / `next_sibling` / `marker_info` in JavaScript, so those rules
 * now live in two languages. Comparing `root().toJSON()` against `cst()` over the whole
 * conformance corpus is what keeps them from drifting.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { parse, tapeView, parseTape, tapeSchema, Tape } from "../pkg/node.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORPUS = join(HERE, "../../usfm3/tests/fixtures/usfm-grammar");

/** Fixtures with both inputs, matching the Rust harness's `discover()`. */
function fixtures() {
  const out = [];
  (function walk(dir) {
    for (const entry of readdirSync(dir)) {
      const path = join(dir, entry);
      if (!statSync(path).isDirectory()) continue;
      try {
        statSync(join(path, "origin.usfm"));
        statSync(join(path, "origin.json"));
        out.push({ name: path.slice(CORPUS.length + 1), usfm: join(path, "origin.usfm") });
      } catch {
        /* not a case dir */
      }
      walk(path);
    }
  })(CORPUS);
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Drop the `marker` key (the name *as written*) throughout a CST tree.
 *
 * Everything else in the tree is either an integer or a schema lookup, but `marker`
 * comes from slicing the source with the node's span — which only lines up when the
 * offsets are in the host's own space. Under `{offsets:"utf8"}` a non-ASCII document's
 * byte offsets index the UTF-16 string at the wrong place, so `marker` is compared
 * separately (see "marker names under the utf16 default") rather than here.
 */
function stripMarkerNames(node) {
  const { marker, children, ...rest } = node;
  if (children) rest.children = children.map(stripMarkerNames);
  return rest;
}

/**
 * `toJSON()` minus the source-slicing field, built straight from cursors.
 *
 * Needed because `toJSON()` now *throws* on a non-sliceable tape rather than emitting a
 * wrong `marker`. Everything below reads integers and schema lookups only, so it is
 * valid in any offset space — which is exactly what lets the traversal rules be checked
 * against `cst()` on non-ASCII fixtures.
 */
function structureOf(cursor) {
  const out = { type: cursor.kind };
  const marker = cursor.marker;
  if (marker) {
    out.category = marker.category;
    out.role = marker.role;
    out.closing = marker.closing;
  }
  if (cursor.number !== undefined) out.number = cursor.number;
  out.start = cursor.start;
  out.end = cursor.end;
  if (cursor.synthetic) out.synthetic = true;
  const close = cursor.matchingClose();
  if (close) {
    out.close = { start: close.start, end: close.end };
    if (close.synthetic) out.close.synthetic = true;
  }
  const children = [...cursor.children()].map(structureOf);
  if (children.length > 0) out.children = children;
  return out;
}

/** Marker names in document order, from a `cst()` object graph. */
function cstMarkerNames(node, out = []) {
  if (node.marker !== undefined) out.push(node.marker);
  for (const child of node.children ?? []) cstMarkerNames(child, out);
  return out;
}

/** Marker names in document order, from a tape cursor. */
function tapeMarkerNames(cursor, out = []) {
  if (cursor.marker) out.push(cursor.markerName);
  for (const child of cursor.children()) tapeMarkerNames(child, out);
  return out;
}

test("traversal parity: packed tape rebuilds cst() across the corpus", () => {
  const cases = fixtures();
  assert.ok(cases.length > 200, `expected the full corpus, found ${cases.length}`);

  let exact = 0;
  const nonAscii = [];
  for (const c of cases) {
    const src = readFileSync(c.usfm, "utf8");
    if (!/^[\x00-\x7F]*$/.test(src)) {
      nonAscii.push(c);
      continue;
    }
    const doc = parse(src);
    try {
      // utf8 on both sides, so the start/end integers are directly comparable, and on an
      // ASCII source the utf8 tape is still sliceable so `marker` is checked too.
      const rebuilt = tapeView(doc, src, { offsets: "utf8" }).root().toJSON();
      const expected = doc.cst();
      assert.deepStrictEqual(rebuilt, expected, c.name);
      exact++;
    } finally {
      doc.free();
    }
  }
  // Non-ASCII fixtures: a utf8 tape is not sliceable, so compare the traversal shape
  // using integer/schema fields only. Marker names for these are covered by the next
  // test, under the utf16 default where they are meaningful.
  let structural = 0;
  for (const c of nonAscii) {
    const src = readFileSync(c.usfm, "utf8");
    const doc = parse(src);
    try {
      const t = tapeView(doc, src, { offsets: "utf8" });
      assert.equal(t.sliceable, false, `${c.name} should not be sliceable`);
      assert.deepStrictEqual(structureOf(t.root()), stripMarkerNames(doc.cst()), c.name);
      structural++;
    } finally {
      doc.free();
    }
  }

  assert.ok(exact > 0 && structural > 0, "corpus should cover both ASCII and non-ASCII");
  console.log(`    ${exact} fixtures exact, ${structural} structural (non-ASCII)`);
});

test("text and markerName refuse to guess in the wrong offset space", () => {
  // The regression this guards: under {offsets:"utf8"} on a non-ASCII source, markerName
  // used to return "" for \\s1 and "1" for \\q2 — plausible-looking values that read as
  // real data downstream. A marker name is structural metadata, and asking for byte
  // offsets does not imply you do not need it, so this must fail loudly.
  const src = "\\id GEN\n\\c 1\n\\p\n\\v 1 יהוה\n\\q1 poetry\n";

  const ok = parseTape(src); // utf16 default
  assert.equal(ok.sliceable, true);
  const q1 = [...ok.nodes()].find((n) => n.kind === "marker" && n.markerName === "q1");
  assert.ok(q1, "utf16 must resolve marker names");
  assert.equal(q1.text.startsWith("\\q1"), true);
  assert.ok(ok.root().toJSON());

  for (const offsets of ["utf8", "chars"]) {
    const t = parseTape(src, { offsets });
    // "chars" is unsliceable here only if the source has astral scalars; this one does
    // not, so chars coincides with utf16 and stays usable. utf8 does not.
    if (offsets === "utf8") {
      assert.equal(t.sliceable, false, "utf8 over non-ASCII is not sliceable");
      const marker = [...t.nodes()].find((n) => n.kind === "marker");
      assert.throws(() => marker.text, /host's own space/);
      assert.throws(() => marker.markerName, /host's own space/);
      assert.throws(() => t.root().toJSON(), /host's own space/);
      // The integer and schema fields stay available — that is the whole point of
      // asking for byte offsets.
      assert.equal(typeof marker.start, "number");
      assert.equal(marker.marker.category, "identification");
      assert.equal(marker.kind, "marker");
    } else {
      assert.equal(t.sliceable, true, "chars == utf16 with no astral scalars");
    }
  }

  // An astral scalar makes "chars" diverge from utf16 too, and it must be caught.
  const astral = parseTape("\\id GEN\n\\p \u{1F600}\n\\q1 x\n", { offsets: "chars" });
  assert.equal(astral.sliceable, false, "an astral scalar breaks chars-as-code-units");
  assert.throws(() => astral.root().toJSON(), /host's own space/);

  // Pure ASCII: every space coincides, so nothing is restricted.
  const ascii = parseTape("\\id GEN\n\\q1 x\n", { offsets: "utf8" });
  assert.equal(ascii.sliceable, true);
  assert.ok(ascii.root().toJSON());
});

test("marker names under the utf16 default match cst() across the corpus", () => {
  // The half of `toJSON()` the utf8 parity test cannot check: `markerName` slices the
  // source, so it is only correct in the host's space. Names are ASCII, but their
  // *positions* are not, which is exactly the bug this catches.
  let nonAscii = 0;
  for (const c of fixtures()) {
    const src = readFileSync(c.usfm, "utf8");
    if (!/^[\x00-\x7F]*$/.test(src)) nonAscii++;
    const doc = parse(src);
    try {
      assert.deepStrictEqual(
        tapeMarkerNames(tapeView(doc, src).root()),
        cstMarkerNames(doc.cst()),
        c.name,
      );
    } finally {
      doc.free();
    }
  }
  assert.ok(nonAscii > 50, `expected many non-ASCII fixtures, saw ${nonAscii}`);
  console.log(`    marker names verified, ${nonAscii} non-ASCII fixtures included`);
});

test("cursor text is lossless over non-ASCII sources", () => {
  // The JS-side restatement of the tape's losslessness invariant, and the end-to-end
  // proof of the UTF-16 remap: concatenating every non-synthetic node's text in tape
  // order must reproduce the source exactly.
  let checked = 0;
  for (const c of fixtures()) {
    const src = readFileSync(c.usfm, "utf8");
    if (/^[\x00-\x7F]*$/.test(src)) continue;
    const t = parseTape(src);
    let joined = "";
    for (const cursor of t.scan()) if (!cursor.synthetic) joined += cursor.text;
    assert.equal(joined, src, c.name);
    checked++;
  }
  assert.ok(checked > 50, `expected many non-ASCII fixtures, saw ${checked}`);
  console.log(`    ${checked} non-ASCII fixtures reproduced byte-for-byte`);
});

test("diagnostics and tape offsets agree at the defaults", () => {
  // The point of moving diagnostics into the host's space: a caller can use a
  // diagnostic's offsets and a node's offsets against the same string without passing
  // `offsets` to either call.
  const src = "\\id GEN\n\\c 1\n\\p\n\\v 1 יהוה \u{1F600} \\add unclosed\n";
  const doc = parse(src, { diagnostics: true });
  try {
    const diags = doc.diagnostics();
    assert.ok(diags.length > 0, "fixture should produce diagnostics");
    const t = tapeView(doc, src);

    for (const d of diags) {
      assert.ok(d.end <= src.length, `${d.code} end ${d.end} exceeds ${src.length}`);
      assert.ok(d.column >= 1, "columns are 1-based");
      // The diagnostic's span must be sliceable out of the very same string.
      assert.equal(typeof src.slice(d.start, d.end), "string");
      // And it must land inside some node's span, in the same coordinate space.
      const covering = [...t.nodes()].some((n) => n.start <= d.start && d.start <= n.end);
      assert.ok(covering, `${d.code} at ${d.start} is outside every node span`);
    }

    // Explicit utf8 must come back smaller-or-equal (bytes >= code units for these).
    const bytes = doc.diagnostics({ offsets: "utf8" });
    assert.equal(bytes.length, diags.length);
    assert.ok(bytes.some((b, i) => b.start !== diags[i].start), "spaces should differ here");
  } finally {
    doc.free();
  }
});

test("a Tape outlives its handle and survives wasm memory growth", () => {
  const src = readFileSync(join(CORPUS, "basic/multiple-paragraphs/origin.usfm"), "utf8");
  // parseTape() frees the handle internally before returning.
  const t = parseTape(src);
  const before = JSON.stringify(t.root().toJSON());

  // Force allocation churn: a view into wasm memory would be detached by now.
  const big = src.repeat(400);
  for (let i = 0; i < 6; i++) {
    const doc = parse(big);
    doc.toUsj();
    doc.free();
  }

  assert.equal(JSON.stringify(t.root().toJSON()), before, "walk changed after memory growth");
  assert.equal(t.words[0], t.schema.magic, "buffer was detached");
});

test("a Tape survives transfer to another thread", () => {
  const src = readFileSync(join(CORPUS, "basic/multiple-paragraphs/origin.usfm"), "utf8");
  const t = parseTape(src);
  const expected = JSON.stringify(t.root().toJSON());

  const moved = structuredClone(
    { words: t.words, source: t.source },
    { transfer: [t.words.buffer] },
  );
  assert.equal(t.words.byteLength, 0, "the original buffer should be transferred away");

  const rebuilt = new Tape(moved.words, moved.source, tapeSchema("3.1"));
  assert.equal(JSON.stringify(rebuilt.root().toJSON()), expected);
});

test("the schema is memoized per version", () => {
  assert.equal(tapeSchema("3.1"), tapeSchema("3.1"), "same version should be one object");
  assert.equal(tapeSchema(), tapeSchema("3.1"), "3.1 is the default");
  assert.notEqual(tapeSchema("3.2"), tapeSchema("3.1"));
  assert.ok(Object.isFrozen(tapeSchema("3.1")), "shared, so it must be frozen");
  assert.equal(tapeSchema("3.1").kinds[0], "root");
  assert.equal(tapeSchema("3.1").markers[65535], undefined, "UNKNOWN resolves to undefined");
});

test("the constructor rejects buffers it cannot trust", () => {
  const src = "\\id GEN\n\\c 1\n\\p\n\\v 1 text\n";
  const schema = tapeSchema("3.1");
  const doc = parse(src);
  const words = doc.tape();
  doc.free();

  assert.throws(() => new Tape(words.slice(0, 4), src, schema), /shorter than its header/);

  const badMagic = words.slice();
  badMagic[0] = 0xdeadbeef;
  assert.throws(() => new Tape(badMagic, src, schema), /bad magic/);

  const badFormat = words.slice();
  badFormat[1] = 99;
  assert.throws(() => new Tape(badFormat, src, schema), /wire format/);

  const truncated = words.slice(0, words.length - 4);
  assert.throws(() => new Tape(truncated, src, schema), /expected \d+ .*for \d+ nodes/);

  const badVersion = words.slice();
  badVersion[4] = 2;
  assert.throws(() => new Tape(badVersion, src, schema), /USFM version rank/);

  assert.throws(() => new Tape(words, src + " ", schema), /wrong string/);
  assert.throws(() => new Tape(words, src.slice(1), schema), /wrong string/);

  assert.throws(() => new Tape([...words], src, schema), /expected a Uint32Array/);
  assert.throws(() => new Tape(words, null, schema), /expected the source string/);
  assert.throws(() => new Tape(words, src, {}), /expected a schema/);
});

test("the source-length check is O(1) in every offset space", () => {
  // All three lengths ride in the header precisely so this check never has to encode
  // the source. Exercise each space so a regression to "encode to count" is caught.
  const src = "\\id GEN\n\\c 1\n\\p\n\\v 1 יהוה \u{1F600}\n";
  for (const offsets of ["utf8", "utf16", "chars"]) {
    const t = parseTape(src, { offsets });
    assert.equal(t.offsets, offsets);
    assert.throws(() => new Tape(t.words, src + "x", tapeSchema("3.1")), /wrong string/);
  }
});

test("node kinds, flags and link overloads decode correctly", () => {
  const t = parseTape("\\id GEN\n\\c 3\n\\p\n\\v 5 a \\nd b\\nd* c\n");

  const root = t.root();
  assert.equal(root.kind, "root");
  assert.equal(root.index, 0);
  // Root's link is the last node index, NOT a close node — firstChild must cope.
  assert.equal(root.link, t.length - 1);
  assert.ok(root.firstChild() !== null);

  const chapter = [...t.nodes()].find((n) => n.kind === "chapter");
  assert.equal(chapter.number, 3, "chapter link carries the number");
  const verse = [...t.nodes()].find((n) => n.kind === "verse");
  assert.equal(verse.number, 5, "verse link carries the number");

  const nd = [...t.nodes()].find((n) => n.kind === "marker" && n.markerName === "nd");
  assert.equal(nd.marker.name, "nd", "schema stores the base name");
  assert.equal(nd.marker.category, "character");
  const close = nd.matchingClose();
  assert.equal(close.kind, "close");
  assert.equal(t.source.slice(close.start, close.end), "\\nd*");
  assert.equal(nd.text.startsWith("\\nd"), true);

  // Closes are never children — they terminate the run.
  assert.ok([...root.walk()].every((n) => n.kind !== "close" || n.index === close.index));
  assert.equal([...t.nodes()].filter((n) => n.kind === "text").length > 0, true);
});

test("nodes() is spreadable; scan() reuses one cursor", () => {
  const t = parseTape("\\id GEN\n\\c 1\n\\p\n\\v 1 text\n");
  // scan() is the allocation-free walk: every yield is the same object, advanced in
  // place. Spreading it is therefore meaningless, which is why it is not the default.
  const reused = [...t.scan()];
  assert.ok(reused.every((c) => c === reused[0]), "scan() should yield one reused cursor");
  assert.equal(reused[0].index, t.length - 1, "the shared cursor ends at the last node");

  // nodes() yields independent cursors, so the collection idioms work.
  const fresh = [...t.nodes()];
  assert.equal(fresh.length, t.length);
  assert.ok(fresh.every((c, i) => c.index === i), "nodes() must be spreadable");
  assert.notEqual(t.at(1), t.at(1), "at() always makes a new cursor");
});

test("out-of-range access throws rather than reading garbage", () => {
  const t = parseTape("\\id GEN\n");
  assert.throws(() => t.at(-1), /out of range/);
  assert.throws(() => t.at(t.length), /out of range/);
  assert.throws(() => t.at(1.5), /out of range/);
});
