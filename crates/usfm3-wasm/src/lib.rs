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

use js_sys::JSON;
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

#[wasm_bindgen(typescript_custom_section)]
const TYPESCRIPT_API: &str = r#"
/** USFM specification version. Selects the marker table and lint rules. */
export type Version = "3.1" | "3.2";

export interface ParseOptions {
  /** Collect diagnostics (lexical/structural + a semantic validation pass).
   *  Off by default so the hot path pays nothing. */
  diagnostics?: boolean;
  /** USFM version. Defaults to "3.1". */
  version?: Version;
}

// ── Primitives ───────────────────────────────────────────────────────────────

export interface Span {
  start: number;
  end: number;
  /** Present on synthetic close tokens (implicit/recovered closes). */
  synthetic?: boolean;
}

// ── Diagnostics ──────────────────────────────────────────────────────────────

export type Severity = "error" | "warning" | "info";

export type DiagnosticCode =
  | "UnknownMarker"
  | "UnclosedMarker"
  | "StrayCloseMarker"
  | "StrayBackslash"
  | "MissingIdMarker"
  | "InvalidBookCode"
  | "DuplicateId"
  | "TextBeforeId"
  | "MissingChapterMarker"
  | "InvalidChapterSequence"
  | "DuplicateChapter"
  | "InvalidVerseSequence"
  | "VerseOutsideParagraph"
  | "NoteSubmarkerOutsideNote"
  | "NonEmptyBlankLine"
  | "LeadingZeros"
  | "MilestoneMismatch"
  | "InvalidAttributes"
  | "MissingChapterNumber"
  | "MissingVerseNumber"
  | "CharCrossesVerseBoundary"
  | "EmptyFigure"
  | "UnquotedAttributeValue"
  | "MissingRequiredAttribute"
  | "BodyParagraphBeforeChapter"
  | "EmptyWordMarker"
  | "MissingMilestoneSelfClose"
  | "InvalidTableColumnSequence"
  | "DeprecatedMarker"
  | "ImplicitClose"
  | "UnclosedNote"
  | "UnclosedAtEof"
  | "DefaultAttributeNotDefined"
  | "MisnestedMarker"
  | "MissingNestingPrefix"
  | "HeaderAfterBody";

/** A diagnostic. Spans are UTF-8 byte offsets into the source; `line`/`column`
 *  are 1-based for editors working in line/column space. */
export interface Diagnostic {
  severity: Severity;
  code: DiagnosticCode;
  message: string;
  start: number;
  end: number;
  line: number;
  column: number;
}

// ── CST (concrete syntax tree, for editor decoration) ──────────────────────────

export interface CstClose {
  start: number;
  end: number;
  synthetic?: boolean;
}

export interface CstNode {
  type: string;
  marker?: string;
  category?: string;
  role?: string;
  closing?: string;
  number?: number;
  /** UTF-8 byte offsets into the source. */
  start: number;
  end: number;
  synthetic?: boolean;
  close?: CstClose;
  children?: CstNode[];
}

// ── USJ ────────────────────────────────────────────────────────────────────────

export interface UsjAttribute {
  key: string;
  value: string;
}

export interface UsjBook { type: "book"; marker: string; code: string; content?: UsjContentNode[]; }
export interface UsjChapter { type: "chapter"; marker: string; number: string; sid?: string; altnumber?: string; pubnumber?: string; }
export interface UsjVerse { type: "verse"; marker: string; number: string; sid?: string; altnumber?: string; pubnumber?: string; }
export interface UsjPara { type: "para"; marker: string; content?: UsjContentNode[]; }
export interface UsjChar { type: "char"; marker: string; content?: UsjContentNode[]; attributes?: UsjAttribute[]; }
export interface UsjNote { type: "note"; marker: string; caller: string; category?: string; content?: UsjContentNode[]; }
export interface UsjMilestone { type: "ms"; marker: string; attributes?: UsjAttribute[]; }
export interface UsjFigure { type: "figure"; marker: string; content?: UsjContentNode[]; attributes?: UsjAttribute[]; }
export interface UsjSidebar { type: "sidebar"; marker: string; category?: string; content?: UsjContentNode[]; }
export interface UsjPeriph { type: "periph"; alt?: string; id?: string; content?: UsjContentNode[]; }
export interface UsjTable { type: "table"; content?: UsjContentNode[]; }
export interface UsjTableRow { type: "table:row"; marker: string; content?: UsjContentNode[]; }
export interface UsjTableCell { type: "table:cell"; marker: string; align: string; content?: UsjContentNode[]; }
export interface UsjRef { type: "ref"; loc?: string; content?: UsjContentNode[]; }
export interface UsjUnknown { type: "unknown"; marker: string; content?: UsjContentNode[]; }
export interface UsjOptBreak { type: "optbreak"; }

export type UsjContentNode =
  | string
  | UsjBook
  | UsjChapter
  | UsjVerse
  | UsjPara
  | UsjChar
  | UsjNote
  | UsjMilestone
  | UsjFigure
  | UsjSidebar
  | UsjPeriph
  | UsjTable
  | UsjTableRow
  | UsjTableCell
  | UsjRef
  | UsjUnknown
  | UsjOptBreak;

export interface UsjDocument {
  type: "USJ";
  version: string;
  content: UsjContentNode[];
}

// ── ParsedDocument handle ──────────────────────────────────────────────────────

export class ParsedDocument {
  /** Free the WebAssembly-side handle. Call when done. */
  free(): void;
  /** Concrete syntax tree (for editor decoration / folding). */
  cst(): CstNode;
  /** Diagnostics as a flat array. Empty unless `diagnostics: true` was passed. */
  diagnostics(): Diagnostic[];
  /** USJ document object. */
  toUsj(): UsjDocument;
  /** USX serialization (XML string). */
  toUsx(): string;
  /** Normalized USFM (string). */
  toUsfm(): string;
  /** Verse-reference → cleaned-text dictionary, in document order. */
  toVref(): Record<string, string>;
}

/** Parse USFM into a lazy handle. Never throws on malformed input — the parser
 *  recovers and (when `diagnostics` is on) reports. */
export function parse(usfm: string, options?: ParseOptions): ParsedDocument;

/** One-shot convenience: parse and return the concrete syntax tree. */
export function parseCst(usfm: string, options?: ParseOptions): CstNode;
"#;

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

    #[wasm_bindgen(skip_typescript)]
    pub fn diagnostics(&self) -> Result<JsValue, JsError> {
        parse_json(&self.doc().to_diagnostics_json())
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
