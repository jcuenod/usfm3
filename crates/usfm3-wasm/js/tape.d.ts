// Types for the `usfm3/tape.js` subpath.
//
// This entry exists because `tape.js` imports nothing — no wasm glue, no init. A worker
// that received a transferred buffer, a source string and a schema can decode the tape
// with no WebAssembly instance at all, and importing the package root would pull the
// glue in for nothing. Only the decoder is re-exported here; `parse`, `parseTape` and
// friends need wasm and live at the package root.
export { Tape, TapeCursor, tape } from "./index.js";
export type {
  CstClose,
  CstNode,
  MarkerMeta,
  OffsetSpace,
  TapeKind,
  TapeSchema,
  Version,
} from "./index.js";
