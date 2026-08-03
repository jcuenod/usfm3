// Node entry point: synchronously initialize the WebAssembly module from the
// bundled bytes, then re-export the public API. Browser/bundler consumers use the
// default `init` export from `usfm3_wasm.js` instead (see package "exports").
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { initSync } from "./usfm3_wasm.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmBytes = readFileSync(join(__dirname, "usfm3_wasm_bg.wasm"));
initSync({ module: wasmBytes });

export { parse, parseCst, ParsedDocument } from "./usfm3_wasm.js";
