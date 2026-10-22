// Browser/bundler entry point. Callers must `await init()` (the default export) or
// `initSync({ module })` before using anything else; Node consumers load `node.js`,
// which does that for them. Both entries expose an identical namespace.
export { default, initSync, parse, parseCst, ParsedDocument } from "./usfm3_wasm.js";
export { tapeSchema, tapeView, parseTape } from "./api.js";
export { Tape, TapeCursor, tape } from "./tape.js";
