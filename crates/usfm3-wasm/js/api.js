/**
 * Ergonomic entry points over the packed tape.
 *
 * Nothing here may touch wasm at module scope: ESM hoisting evaluates this body
 * *before* `node.js` calls `initSync`, so every wasm access sits inside a function.
 */

import { parse as wasmParse, tapeSchema as wasmTapeSchema } from "./usfm3_wasm.js";
import { Tape } from "./tape.js";

/**
 * The marker/kind schema for a USFM version, memoized.
 *
 * A packed tape carries `kind` and `marker` as integers, which are only meaningful
 * against this table. It is small and version-stable, so it is fetched once per version
 * and shared by every `Tape` — the returned object is frozen because it is shared.
 *
 * @param {"3.1" | "3.2"} [version]
 */
const schemas = new Map();
export function tapeSchema(version = "3.1") {
  let schema = schemas.get(version);
  if (schema === undefined) {
    schema = Object.freeze(wasmTapeSchema(version));
    schemas.set(version, schema);
  }
  return schema;
}

/**
 * A `Tape` view over an already-parsed handle.
 *
 * `source` must be the exact string passed to `parse()` — the tape holds offsets into
 * it, not a copy of it. The constructor validates the length against the header and
 * throws rather than producing silently wrong spans.
 *
 * The returned view holds no wasm resources, so `doc` may be freed immediately after.
 *
 * @param {import("./usfm3_wasm.js").ParsedDocument} doc
 * @param {string} source
 * @param {{ offsets?: "utf8" | "utf16" | "chars", version?: "3.1" | "3.2" }} [options]
 */
export function tapeView(doc, source, options) {
  return new Tape(doc.tape(options), source, tapeSchema(options?.version ?? "3.1"));
}

/**
 * Parse and return a walkable `Tape` — the one-shot form, and the fast path this whole
 * mechanism exists for.
 *
 * The handle is freed before returning, which is only safe because `tape()` copies the
 * buffer onto the JS heap. Compare `parseCst()`, which materializes the same
 * information as a JSON object graph: on a book-sized document that is megabytes of
 * text through `JSON.parse` and tens of thousands of objects for the GC.
 *
 * @param {string} usfm
 * @param {{ diagnostics?: boolean, version?: "3.1" | "3.2", offsets?: "utf8" | "utf16" | "chars" }} [options]
 */
export function parseTape(usfm, options) {
  const doc = wasmParse(usfm, options);
  try {
    return tapeView(doc, usfm, options);
  } finally {
    doc.free();
  }
}
