/**
 * Benchmarks for the two per-keystroke wins. Run: `node test/bench.mjs`.
 *
 * Run this in a browser too for the headline figures — V8 in Chromium parses JSON
 * noticeably faster than Node's build, so Node-only numbers overstate the tape's
 * advantage. Conversely Chromium clamps `performance.now()` to 100 us, so the small
 * deltas (walk time, packing cost) are only resolvable here in Node.
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { parse, parseCst, parseTape, tapeView, tapeSchema, Tape } from "../pkg/node.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const SAMPLE = readFileSync(join(HERE, "../../../44JHNjdt.SFM"), "utf8");

/** A book-sized document. Synthetic (the sample repeated), but real markup and real
 *  non-ASCII content, which is what the offset translation cares about. */
function bookSized() {
  let out = "";
  for (let i = 0; out.length < 280_000; i++) out += SAMPLE;
  return out;
}

const time = (label, fn, iters = 20) => {
  for (let i = 0; i < Math.max(3, iters >> 2); i++) fn(); // warm up
  const runs = [];
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    fn();
    runs.push(performance.now() - t0);
  }
  runs.sort((a, b) => a - b);
  const median = runs[runs.length >> 1];
  console.log(`  ${label.padEnd(46)} ${median.toFixed(3).padStart(9)} ms`);
  return median;
};

const kib = (n) => `${(n / 1024).toFixed(1)} KiB`;

function scale(name, src) {
  console.log(`\n${name} — ${kib(src.length)}, ${[...src].length} chars, non-ASCII: ${!/^[\x00-\x7F]*$/.test(src)}`);

  const doc = parse(src);
  const json = JSON.stringify(doc.cst());
  const words = doc.tape();
  const tapeNodes = words[2];
  const jsonNodes = (function count(n) {
    return 1 + (n.children ?? []).reduce((a, c) => a + count(c), 0);
  })(doc.cst());
  doc.free();

  console.log(
    `  tape nodes ${tapeNodes} vs cst() JSON nodes ${jsonNodes}` +
      `   (cst() folds ${tapeNodes - jsonNodes} close tokens into parents)`,
  );
  console.log(`  cst() JSON ${kib(json.length)}   packed ${kib(words.byteLength)}` +
    `   ratio ${(json.length / words.byteLength).toFixed(2)}x smaller`);

  console.log("  end-to-end:");
  const floor = time("parse() alone (the floor)", () => parse(src).free());
  const cst = time("parse() + cst()", () => {
    const d = parse(src);
    d.cst();
    d.free();
  });
  const tape = time("parseTape()", () => parseTape(src));
  console.log(
    `  → ${(cst / tape).toFixed(1)}x faster end-to-end;` +
      ` serialization ${(cst - floor).toFixed(2)} ms → ${(tape - floor).toFixed(2)} ms`,
  );

  console.log("  walk (traverse every node, read kind + span):");
  const tree = parseCst(src);
  time("object tree, recursive", () => {
    let n = 0;
    (function walk(node) {
      n += node.start + node.end;
      for (const c of node.children ?? []) walk(c);
    })(tree);
    return n;
  }, 50);
  const t = parseTape(src);
  time("cursor, scan() (allocation-free)", () => {
    let n = 0;
    for (const c of t.scan()) n += c.start + c.end;
    return n;
  }, 50);

  console.log("  worker handoff:");
  time("structuredClone the object tree", () => structuredClone(tree), 10);
  time("transfer the packed buffer", () => {
    const copy = parse(src);
    const w = copy.tape();
    copy.free();
    structuredClone({ words: w, source: src }, { transfer: [w.buffer] });
  }, 10);

  console.log("  offset-space cost (the price of the utf16 default):");
  const d2 = parse(src);
  time('tape({offsets:"utf8"}) — no scan', () => d2.tape({ offsets: "utf8" }), 30);
  time('tape({offsets:"utf16"}) — one pass', () => d2.tape({ offsets: "utf16" }), 30);
  d2.free();

  console.log("  guardrail (must be unchanged — not on this path):");
  time("toUsj()", () => {
    const d = parse(src);
    d.toUsj();
    d.free();
  }, 10);
  time("toVref()", () => {
    const d = parse(src);
    d.toVref();
    d.free();
  }, 10);
}

console.log("=".repeat(72));
console.log("Cost 2 — packed tape vs cst() JSON");
console.log("=".repeat(72));
scale("chapter-sized", SAMPLE);
scale("book-sized", bookSized());

console.log("\n" + "=".repeat(72));
console.log("Cost 1 — diagnostics(): O(N x len) → O(len + N)");
console.log("=".repeat(72));
console.log("  `Span::line_col` counted newlines from byte 0 on every call, so serializing");
console.log("  N diagnostics rescanned the prefix N times. One shared forward-only");
console.log("  OffsetMap makes it a single pass.");
console.log("");
console.log("  The document length is held CONSTANT below and only N varies. An editor");
console.log("  asks on every debounce, and a mid-edit document is exactly when N spikes —");
console.log("  every half-typed marker is a diagnostic.");
console.log("");
console.log("  NOTE: the delta here also includes the FFI boundary — a JSON string copy");
console.log("  plus JSON.parse into N objects — which dominates at large N and is NOT");
console.log("  what Phase 1 changed. For the line/column fix in isolation (quadratic ->");
console.log("  linear, 272x at N=5000) run:  cargo run --release -p usfm3 --example diagbench\n");

/**
 * A fixed-length document with exactly `n` unknown-marker diagnostics.
 *
 * `\qqbogus` and `filler01` are both 8 characters, so swapping one for the other keeps
 * the source byte-for-byte the same length — the only variable is N.
 */
function diagDoc(n) {
  const SLOTS = 5000;
  const body = [];
  for (let i = 0; i < SLOTS; i++) {
    if (i % 25 === 0) body.push(`\n\\v ${1 + (i / 25) | 0} `);
    body.push(i < n ? "\\qqbogus" : "filler01", " ");
  }
  return `\\id GEN\n\\c 1\n\\p${body.join("")}\n`;
}

const median = (fn, iters = 9) => {
  for (let i = 0; i < 3; i++) fn();
  const runs = [];
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    fn();
    runs.push(performance.now() - t0);
  }
  runs.sort((a, b) => a - b);
  return runs[iters >> 1];
};

console.log("     source   diagnostics   parse+validate   + diagnostics()        delta");
for (const n of [0, 5, 50, 500, 5000]) {
  const src = diagDoc(n);
  const probe = parse(src, { diagnostics: true });
  const count = probe.diagnostics().length;
  probe.free();

  const validate = median(() => parse(src, { diagnostics: true }).free());
  const serialize = median(() => {
    const d = parse(src, { diagnostics: true });
    d.diagnostics();
    d.free();
  });

  console.log(
    `  ${kib(src.length).padStart(9)}   ${String(count).padStart(11)}   ` +
      `${validate.toFixed(2).padStart(11)} ms   ${serialize.toFixed(2).padStart(12)} ms   ` +
      `${(serialize - validate).toFixed(2).padStart(8)} ms`,
  );
}
