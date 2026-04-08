use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

use ::usfm3 as usfm3_lib;

#[derive(Tsify, Deserialize, Clone, Copy, Default)]
#[tsify(from_wasm_abi)]
pub struct ParseOptions {
    #[serde(default)]
    pub diagnostics: bool,
}

#[derive(Tsify, Deserialize, Clone, Copy, Default)]
#[tsify(from_wasm_abi)]
pub struct UsjOptions {
    #[serde(default)]
    pub spans: bool,
}

#[wasm_bindgen(typescript_custom_section)]
const TYPESCRIPT_API: &str = r#"
export interface ParseOptions {
  diagnostics?: boolean;
}

export interface UsjOptions {
  spans?: boolean;
}

// ── Primitives ───────────────────────────────────────────────────────────────

export interface Span {
  start: number;
  end: number;
}

// ── Diagnostics ──────────────────────────────────────────────────────────────

export type Severity = "info" | "warning" | "error";

export type DiagnosticCode =
  | "UnknownMarker"
  | "DeprecatedMarker"
  | "UnclosedMarker"
  | "StrayCloseMarker"
  | "MisnestedMarker"
  | "MissingNestingPrefix"
  | "ImplicitClose"
  | "UnclosedNote"
  | "UnclosedAtEof"
  | "InvalidChapterSequence"
  | "InvalidVerseSequence"
  | "DuplicateChapter"
  | "DuplicateId"
  | "MissingIdMarker"
  | "InvalidBookCode"
  | "NoteSubmarkerOutsideNote"
  | "TextBeforeId"
  | "HeaderAfterBody"
  | "MilestoneMismatch"
  | "InvalidAttributes"
  | "MissingChapterNumber"
  | "MissingVerseNumber"
  | "VerseOutsideParagraph"
  | "MissingChapterMarker"
  | "CharCrossesVerseBoundary"
  | "EmptyFigure"
  | "UnquotedAttributeValue"
  | "MissingRequiredAttribute"
  | "DefaultAttributeNotDefined"
  | "BodyParagraphBeforeChapter"
  | "NonEmptyBlankLine"
  | "LeadingZeros"
  | "EmptyWordMarker"
  | "MissingMilestoneSelfClose"
  | "InvalidTableColumnSequence";

export interface Diagnostic {
  severity: Severity;
  span: Span;
  message: string;
  code: DiagnosticCode;
  anchor_cst?: number;
}

// ── Tokens ───────────────────────────────────────────────────────────────────

export interface TokenSpan {
  kind: "whitespace" | "marker" | "closing_marker" | "milestone_end" | "attributes" | "text" | "newline";
  text: string;
  start: number;
  end: number;
  normalized_marker?: string;
  token_kind?: "chapter" | "verse" | "milestone" | "nested" | "regular";
}

// ── CST ──────────────────────────────────────────────────────────────────────

export interface ExportedCstNode {
  type: string;
  span: Span;
  marker?: string;
  token_kind?: string;
  text?: string;
  children?: ExportedCstNode[];
}

// ── Source Map ────────────────────────────────────────────────────────────────

export interface SourceSpans {
  node: Span;
  code?: Span;
  number?: Span;
  close?: Span;
}

export interface SourceNode {
  spans?: SourceSpans;
  children?: SourceNode[];
  anchor_cst?: number;
}

export interface SourceMap {
  content: SourceNode[];
}

// ── AST ───────────────────────────────────────────────────────────────────────

export interface AstAttribute {
  key: string;
  value: string;
}

export type AstNode =
  | { Book: { marker: string; code: string; content: AstNode[] } }
  | { Chapter: { marker: string; number: string; sid?: string; altnumber?: string; pubnumber?: string } }
  | { Verse: { marker: string; number: string; sid?: string; altnumber?: string; pubnumber?: string } }
  | { Para: { marker: string; content: AstNode[] } }
  | { Char: { marker: string; content: AstNode[]; attributes: AstAttribute[] } }
  | { Note: { marker: string; caller: string; category?: string; content: AstNode[] } }
  | { Milestone: { marker: string; attributes: AstAttribute[] } }
  | { Figure: { marker: string; content: AstNode[]; attributes: AstAttribute[] } }
  | { Sidebar: { marker: string; category?: string; content: AstNode[] } }
  | { Periph: { alt?: string; content: AstNode[]; attributes: AstAttribute[] } }
  | { Table: { content: AstNode[] } }
  | { TableRow: { marker: string; content: AstNode[] } }
  | { TableCell: { marker: string; align: string; content: AstNode[] } }
  | { Ref: { content: AstNode[]; attributes: AstAttribute[] } }
  | { Unknown: { marker: string; content: AstNode[] } }
  | { Text: string }
  | "OptBreak";

export interface AstDocument {
  content: AstNode[];
}

export interface ParsedAstDocument {
  ast: AstDocument;
  source_map: SourceMap;
  diagnostics?: Diagnostic[];
}

// ── USJ ───────────────────────────────────────────────────────────────────────

export interface UsjAttribute {
  key: string;
  value: string;
}

/** Byte-offset spans; only present when `UsjOptions.spans` is `true`. */
export interface UsjSpans {
  node: Span;
  code?: Span;
  number?: Span;
  close?: Span;
}

export interface UsjBook {
  type: "book";
  marker: string;
  code: string;
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjChapter {
  type: "chapter";
  marker: string;
  number: string;
  sid?: string;
  altnumber?: string;
  pubnumber?: string;
  spans?: UsjSpans;
}

export interface UsjVerse {
  type: "verse";
  marker: string;
  number: string;
  sid?: string;
  altnumber?: string;
  pubnumber?: string;
  spans?: UsjSpans;
}

export interface UsjPara {
  type: "para";
  marker: string;
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjChar {
  type: "char";
  marker: string;
  content?: UsjContentNode[];
  attributes?: UsjAttribute[];
  spans?: UsjSpans;
}

export interface UsjNote {
  type: "note";
  marker: string;
  caller: string;
  category?: string;
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjMilestone {
  type: "ms";
  marker: string;
  attributes?: UsjAttribute[];
  spans?: UsjSpans;
}

export interface UsjFigure {
  type: "figure";
  marker: string;
  content?: UsjContentNode[];
  attributes?: UsjAttribute[];
  spans?: UsjSpans;
}

export interface UsjSidebar {
  type: "sidebar";
  marker: string;
  category?: string;
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjPeriph {
  type: "periph";
  alt?: string;
  content?: UsjContentNode[];
  attributes?: UsjAttribute[];
  spans?: UsjSpans;
}

export interface UsjTable {
  type: "table";
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjTableRow {
  type: "table:row";
  marker: string;
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjTableCell {
  type: "table:cell";
  marker: string;
  align: string;
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjRef {
  type: "ref";
  content?: UsjContentNode[];
  attributes?: UsjAttribute[];
  spans?: UsjSpans;
}

export interface UsjUnknown {
  type: "unknown";
  marker: string;
  content?: UsjContentNode[];
  spans?: UsjSpans;
}

export interface UsjOptBreak {
  type: "optbreak";
}

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

// ── ParsedDocument class ─────────────────────────────────────────────────────

export class ParsedDocument {
  free(): void;
  cst(): ExportedCstNode;
  ast(): AstDocument;
  sourceMap(): SourceMap;
  diagnostics(): Diagnostic[] | undefined;
  toUsj(options?: UsjOptions): UsjDocument;
  toUsx(): string;
  toUsfm(): string;
  toVref(): Record<string, string>;
}

export function parse(usfm: string, options?: ParseOptions): ParsedDocument;
export function parseCst(usfm: string): ExportedCstNode;
export function parseAst(usfm: string, options?: ParseOptions): ParsedAstDocument;
export function tokenize(usfm: string): TokenSpan[];
"#;

#[wasm_bindgen(skip_typescript)]
pub struct ParsedDocument {
    inner: usfm3_lib::ParsedDocument,
}

#[wasm_bindgen]
impl ParsedDocument {
    #[wasm_bindgen]
    pub fn cst(&self) -> Result<JsValue, JsError> {
        to_js_value(&usfm3_lib::cst::export(self.inner.cst()))
    }

    #[wasm_bindgen]
    pub fn ast(&self) -> Result<JsValue, JsError> {
        to_js_value(self.inner.ast())
    }

    #[wasm_bindgen(js_name = "sourceMap")]
    pub fn source_map(&self) -> Result<JsValue, JsError> {
        to_js_value(self.inner.source_map())
    }

    #[wasm_bindgen]
    pub fn diagnostics(&self) -> Result<JsValue, JsError> {
        to_js_value(&self.inner.diagnostics())
    }

    #[wasm_bindgen(js_name = "toUsj")]
    pub fn to_usj(&self, options: Option<UsjOptions>) -> Result<JsValue, JsError> {
        let options = options.unwrap_or_default();
        let value = self
            .inner
            .to_usj(usfm3_lib::usj::UsjOptions {
                include_spans: options.spans,
            })
            .map_err(|error| JsError::new(&error.to_string()))?;
        to_js_value(&value)
    }

    #[wasm_bindgen(js_name = "toUsx")]
    pub fn to_usx(&self) -> Result<String, JsError> {
        self.inner
            .to_usx()
            .map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = "toUsfm")]
    pub fn to_usfm(&self) -> String {
        self.inner.to_usfm()
    }

    #[wasm_bindgen(js_name = "toVref")]
    pub fn to_vref(&self) -> Result<JsValue, JsError> {
        to_js_value(&self.inner.to_vref())
    }
}

#[wasm_bindgen(skip_typescript)]
pub fn parse(usfm: &str, options: Option<ParseOptions>) -> Result<ParsedDocument, JsError> {
    let options = options.unwrap_or_default();
    Ok(ParsedDocument {
        inner: usfm3_lib::parse(
            usfm,
            usfm3_lib::ParseOptions {
                diagnostics: options.diagnostics,
            },
        ),
    })
}

#[wasm_bindgen(skip_typescript, js_name = "parseCst")]
pub fn parse_cst(usfm: &str) -> Result<JsValue, JsError> {
    let cst = usfm3_lib::parse_cst(usfm);
    to_js_value(&usfm3_lib::cst::export(&cst))
}

#[wasm_bindgen(skip_typescript, js_name = "parseAst")]
pub fn parse_ast(usfm: &str, options: Option<ParseOptions>) -> Result<JsValue, JsError> {
    let options = options.unwrap_or_default();
    let ast_document = usfm3_lib::parse_ast(
        usfm,
        usfm3_lib::ParseOptions {
            diagnostics: options.diagnostics,
        },
    );
    to_js_value(&ast_document)
}

#[wasm_bindgen(skip_typescript)]
pub fn tokenize(usfm: &str) -> Result<JsValue, JsError> {
    to_js_value(&usfm3_lib::tokenize(usfm))
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    value
        .serialize(&serializer)
        .map_err(|error| JsError::new(&error.to_string()))
}
