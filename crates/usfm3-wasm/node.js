import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { initSync } from "./usfm3_wasm.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmBytes = readFileSync(join(__dirname, "usfm3_wasm_bg.wasm"));
initSync({ module: wasmBytes });

export { parse, ParseResult } from "./usfm3_wasm.js";
