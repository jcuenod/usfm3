//! WebAssembly bindings for the `usfm3` parser.
//!
//! The core is "one tape, many views": a single parse produces a flat tape, and
//! every output (USJ, USX, USFM, CST, vref, diagnostics) is a cheap view over it.
//! So this binding exposes a single [`parse`] entry point returning a lazy
//! [`ParsedDocument`] handle — there is no separate token / AST / source-map stage
//! to special-case (unlike older staged parsers).
//!
//! The core serializers emit JSON as strings; the JSON-shaped views
//! (`cst`/`toUsj`/`toVref`/`diagnostics`) are parsed at the boundary with the
//! engine's native `JSON.parse`, so JS callers get plain objects. `toUsx` and
//! `toUsfm` are returned as strings verbatim.
//!
//! # `tape()` and offset spaces
//!
//! `cst()` costs a full JSON materialization plus a `JSON.parse` into an object graph
//! — on a book-sized document that is megabytes of text and tens of thousands of
//! objects, per call. For an editor reparsing on every keystroke that dominates, so
//! [`ParsedDocument::tape`] hands over the tape as a packed `Uint32Array` instead and
//! `js/tape.js` walks it with shifts. `cst()` is unchanged and still available.
//!
//! Offsets crossing this boundary are **UTF-16 code units** by default, because that
//! is what indexes a JS string. Byte offsets would make `source.slice(start, end)`
//! silently wrong on any non-ASCII document, which is most real translation text
//! (103 of the 242 fixtures in the conformance corpus). Pass `{ offsets: "utf8" }` for
//! byte offsets. Both `tape()` and `diagnostics()` honour the option and share the
//! same default, so their numbers always agree.
//!
//! Deliberately absent: per-node accessors (`kindAt(i)`, `textAt(i)`). A boundary
//! crossing per node is *slower* than the JSON this replaces, and every string return
//! allocates. Decode the buffer in JS instead.

use js_sys::{JSON, Uint32Array};
use wasm_bindgen::prelude::*;

use ::usfm3 as usfm3_lib;

// `Document<'src>` borrows the source buffer, so the handle owns the source
// String and the borrowing Document together via a self-referential cell.
self_cell::self_cell!(
    struct Parsed {
        owner: String,
        #[covariant]
        dependent: ParsedView,
    }
);

/// The borrowing half of the cell: the parsed [`usfm3_lib::Document`].
type ParsedView<'src> = usfm3_lib::Document<'src>;

#[wasm_bindgen(skip_typescript)]
pub struct ParsedDocument {
    inner: Parsed,
}

impl ParsedDocument {
    fn build(usfm: String, version: usfm3_lib::Version, diagnostics: bool) -> Self {
        let inner = Parsed::new(usfm, |source| {
            usfm3_lib::parse_with(source, version, usfm3_lib::ParseOptions { diagnostics })
        });
        ParsedDocument { inner }
    }

    fn doc(&self) -> &usfm3_lib::Document<'_> {
        self.inner.borrow_dependent()
    }
}

#[wasm_bindgen]
impl ParsedDocument {
    #[wasm_bindgen(skip_typescript)]
    pub fn cst(&self) -> Result<JsValue, JsError> {
        parse_json(&self.doc().to_cst_json())
    }

    /// Diagnostics as a flat array, with offsets in the requested space.
    ///
    /// UTF-16 by default. The previous byte offsets were already wrong for the editors
    /// the `line`/`column` fields exist to serve; `{ offsets: "utf8" }` restores them.
    #[wasm_bindgen(skip_typescript)]
    pub fn diagnostics(&self, options: JsValue) -> Result<JsValue, JsError> {
        let offsets = read_offsets(&options, JS_OFFSETS);
        parse_json(&self.doc().to_diagnostics_json_with(offsets))
    }

    /// The tape as a packed `Uint32Array`: an 8-word header, then 4 words per node.
    ///
    /// The returned array is a **copy owned by the JS heap**, not a view into wasm
    /// memory. That is what makes it safe: a view would be detached by any later
    /// allocation that grows `memory`, and would dangle after `free()`. It also makes
    /// the buffer `postMessage`-transferable, so a worker can parse and the main
    /// thread can walk. There is no copy to save anyway — the offset translation means
    /// the words have to be built rather than memcpy'd.
    ///
    /// Pair it with `tapeSchema()` and the source string; see `js/tape.js`.
    #[wasm_bindgen(skip_typescript)]
    pub fn tape(&self, options: JsValue) -> Uint32Array {
        let offsets = read_offsets(&options, JS_OFFSETS);
        Uint32Array::from(&self.doc().to_packed_tape(offsets)[..])
    }

    #[wasm_bindgen(js_name = "toUsj", skip_typescript)]
    pub fn to_usj(&self) -> Result<JsValue, JsError> {
        parse_json(&self.doc().to_usj())
    }

    #[wasm_bindgen(js_name = "toUsx", skip_typescript)]
    pub fn to_usx(&self) -> String {
        self.doc().to_usx()
    }

    #[wasm_bindgen(js_name = "toUsfm", skip_typescript)]
    pub fn to_usfm(&self) -> String {
        self.doc().to_usfm()
    }

    #[wasm_bindgen(js_name = "toVref", skip_typescript)]
    pub fn to_vref(&self) -> Result<JsValue, JsError> {
        parse_json(&self.doc().to_vref_json())
    }
}

#[wasm_bindgen(skip_typescript)]
pub fn parse(usfm: String, options: JsValue) -> ParsedDocument {
    let (diagnostics, version) = read_options(&options);
    ParsedDocument::build(usfm, version, diagnostics)
}

#[wasm_bindgen(js_name = "parseCst", skip_typescript)]
pub fn parse_cst(usfm: String, options: JsValue) -> Result<JsValue, JsError> {
    let (diagnostics, version) = read_options(&options);
    let doc = ParsedDocument::build(usfm, version, diagnostics);
    doc.cst()
}

/// The marker/kind schema needed to interpret a packed tape from [`ParsedDocument::tape`].
///
/// A free function rather than a method on purpose: it needs no handle, so a worker can
/// hydrate a view with no `ParsedDocument` alive. Small and version-stable — the JS
/// layer memoizes it per version.
#[wasm_bindgen(js_name = "tapeSchema", skip_typescript)]
pub fn tape_schema(version: Option<String>) -> Result<JsValue, JsError> {
    parse_json(&usfm3_lib::schema_json(version_from(version.as_deref())))
}

/// Offsets handed to JS index JS strings, which are UTF-16 code units.
const JS_OFFSETS: usfm3_lib::Offsets = usfm3_lib::Offsets::Utf16;

/// Read `{ offsets? }` off a JS options object (or `undefined`/`null`), falling back to
/// `default`. Unrecognized values fall back too, so JS never has to pre-validate.
fn read_offsets(options: &JsValue, default: usfm3_lib::Offsets) -> usfm3_lib::Offsets {
    let name = js_sys::Reflect::get(options, &JsValue::from_str("offsets"))
        .ok()
        .and_then(|v| v.as_string());
    usfm3_lib::Offsets::from_name(name.as_deref(), default)
}

/// Read `{ diagnostics?, version? }` off a JS options object (or `undefined`).
fn read_options(options: &JsValue) -> (bool, usfm3_lib::Version) {
    let diagnostics = js_sys::Reflect::get(options, &JsValue::from_str("diagnostics"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let version = js_sys::Reflect::get(options, &JsValue::from_str("version"))
        .ok()
        .and_then(|v| v.as_string());
    (diagnostics, version_from(version.as_deref()))
}

fn version_from(s: Option<&str>) -> usfm3_lib::Version {
    match s {
        Some("3.2") => usfm3_lib::Version::V3_2,
        _ => usfm3_lib::Version::V3_1,
    }
}

/// Parse a JSON string emitted by the core serializer into a JS value via the
/// engine's native parser. The input is always well-formed JSON (we produced it),
/// so an error here is an internal bug, surfaced rather than swallowed.
fn parse_json(s: &str) -> Result<JsValue, JsError> {
    JSON::parse(s).map_err(|e| {
        JsError::new(&format!(
            "internal: serializer produced invalid JSON ({e:?})"
        ))
    })
}
