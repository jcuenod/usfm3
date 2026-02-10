use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

use ::usfm3 as usfm3_lib;

// ---------------------------------------------------------------------------
// TypeScript wrapper types (auto-generate .d.ts via tsify)
// ---------------------------------------------------------------------------

/// Severity level for diagnostics.
#[derive(Tsify, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Machine-readable diagnostic code.
#[derive(Tsify, Serialize, Clone)]
pub enum DiagnosticCode {
    UnknownMarker,
    UnclosedMarker,
    StrayCloseMarker,
    MisnestedMarker,
    MissingNestingPrefix,
    ImplicitClose,
    UnclosedNote,
    UnclosedAtEof,
    InvalidChapterSequence,
    InvalidVerseSequence,
    DuplicateChapter,
    DuplicateId,
    MissingIdMarker,
    InvalidBookCode,
    NoteSubmarkerOutsideNote,
    TextBeforeId,
    HeaderAfterBody,
    MilestoneMismatch,
    InvalidAttributes,
    MissingChapterNumber,
    MissingVerseNumber,
    MissingChapterMarker,
    CharCrossesVerseBoundary,
    EmptyFigure,
    UnquotedAttributeValue,
    MissingRequiredAttribute,
    DefaultAttributeNotDefined,
    BodyParagraphBeforeChapter,
    NonEmptyBlankLine,
    LeadingZeros,
    EmptyWordMarker,
    MissingMilestoneSelfClose,
}

/// A diagnostic message with source location.
#[derive(Tsify, Serialize, Clone)]
#[tsify(into_wasm_abi)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub code: DiagnosticCode,
    pub start: usize,
    pub end: usize,
}

/// Options for the `parse` function.
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
pub struct ParseOptions {
    #[serde(default = "default_true")]
    pub validate: bool,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Hand-written TypeScript declarations for ParseResult and parse()
// ---------------------------------------------------------------------------

#[wasm_bindgen(typescript_custom_section)]
const PARSE_RESULT_TS: &str = r#"
export class ParseResult {
  free(): void;
  readonly diagnostics: Diagnostic[];
  hasErrors(): boolean;
  toUsj(): any;
  toUsx(): string;
  toUsfm(): string;
}
export function parse(usfm: string, options?: ParseOptions): ParseResult;
"#;

// ---------------------------------------------------------------------------
// ParseResult
// ---------------------------------------------------------------------------

#[wasm_bindgen(skip_typescript)]
pub struct ParseResult {
    document: usfm3_lib::ast::Document,
    diagnostics_js: JsValue,
    has_errors: bool,
}

#[wasm_bindgen]
impl ParseResult {
    /// Serialize the parsed document to USJ (Unified Scripture JSON).
    #[wasm_bindgen(js_name = "toUsj")]
    pub fn to_usj(&self) -> Result<JsValue, JsError> {
        let usj = usfm3_lib::usj::UsjDocument::from_document(&self.document);
        serde_wasm_bindgen::to_value(&usj).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Serialize the parsed document to USX XML.
    #[wasm_bindgen(js_name = "toUsx")]
    pub fn to_usx(&self) -> Result<String, JsError> {
        usfm3_lib::usx::to_usx_string(&self.document).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Serialize the parsed document to normalized USFM.
    #[wasm_bindgen(js_name = "toUsfm")]
    pub fn to_usfm(&self) -> String {
        usfm3_lib::usfm::to_usfm_string(&self.document)
    }

    /// Get the diagnostics array.
    #[wasm_bindgen(getter)]
    pub fn diagnostics(&self) -> JsValue {
        self.diagnostics_js.clone()
    }

    /// True if any diagnostics have Error severity.
    #[wasm_bindgen(js_name = "hasErrors")]
    pub fn has_errors(&self) -> bool {
        self.has_errors
    }
}

// ---------------------------------------------------------------------------
// parse() entry point
// ---------------------------------------------------------------------------

#[wasm_bindgen(skip_typescript)]
pub fn parse(usfm: &str, options: Option<ParseOptions>) -> Result<ParseResult, JsError> {
    let validate = options.is_none_or(|o| o.validate);

    let result = usfm3_lib::builder::parse(usfm);

    let validation_diags = if validate {
        usfm3_lib::validation::validate(&result.document)
    } else {
        usfm3_lib::diagnostics::DiagnosticList::new()
    };

    let diagnostics: Vec<Diagnostic> = result
        .diagnostics
        .iter()
        .chain(validation_diags.iter())
        .map(convert_diagnostic)
        .collect();

    let has_errors = diagnostics
        .iter()
        .any(|d| matches!(d.severity, Severity::Error));

    let diagnostics_js =
        serde_wasm_bindgen::to_value(&diagnostics).map_err(|e| JsError::new(&e.to_string()))?;

    Ok(ParseResult {
        document: result.document,
        diagnostics_js,
        has_errors,
    })
}

// ---------------------------------------------------------------------------
// Conversion helpers (core types → wasm wrapper types)
// ---------------------------------------------------------------------------

fn convert_diagnostic(d: &usfm3_lib::diagnostics::Diagnostic) -> Diagnostic {
    Diagnostic {
        severity: convert_severity(d.severity),
        message: d.message.clone(),
        code: convert_code(d.code),
        start: d.span.start,
        end: d.span.end,
    }
}

fn convert_severity(s: usfm3_lib::diagnostics::Severity) -> Severity {
    match s {
        usfm3_lib::diagnostics::Severity::Error => Severity::Error,
        usfm3_lib::diagnostics::Severity::Warning => Severity::Warning,
        usfm3_lib::diagnostics::Severity::Info => Severity::Info,
    }
}

fn convert_code(c: usfm3_lib::diagnostics::DiagnosticCode) -> DiagnosticCode {
    use usfm3_lib::diagnostics::DiagnosticCode as DC;
    match c {
        DC::UnknownMarker => DiagnosticCode::UnknownMarker,
        DC::UnclosedMarker => DiagnosticCode::UnclosedMarker,
        DC::StrayCloseMarker => DiagnosticCode::StrayCloseMarker,
        DC::MisnestedMarker => DiagnosticCode::MisnestedMarker,
        DC::MissingNestingPrefix => DiagnosticCode::MissingNestingPrefix,
        DC::ImplicitClose => DiagnosticCode::ImplicitClose,
        DC::UnclosedNote => DiagnosticCode::UnclosedNote,
        DC::UnclosedAtEof => DiagnosticCode::UnclosedAtEof,
        DC::InvalidChapterSequence => DiagnosticCode::InvalidChapterSequence,
        DC::InvalidVerseSequence => DiagnosticCode::InvalidVerseSequence,
        DC::DuplicateChapter => DiagnosticCode::DuplicateChapter,
        DC::DuplicateId => DiagnosticCode::DuplicateId,
        DC::MissingIdMarker => DiagnosticCode::MissingIdMarker,
        DC::InvalidBookCode => DiagnosticCode::InvalidBookCode,
        DC::NoteSubmarkerOutsideNote => DiagnosticCode::NoteSubmarkerOutsideNote,
        DC::TextBeforeId => DiagnosticCode::TextBeforeId,
        DC::HeaderAfterBody => DiagnosticCode::HeaderAfterBody,
        DC::MilestoneMismatch => DiagnosticCode::MilestoneMismatch,
        DC::InvalidAttributes => DiagnosticCode::InvalidAttributes,
        DC::MissingChapterNumber => DiagnosticCode::MissingChapterNumber,
        DC::MissingVerseNumber => DiagnosticCode::MissingVerseNumber,
        DC::MissingChapterMarker => DiagnosticCode::MissingChapterMarker,
        DC::CharCrossesVerseBoundary => DiagnosticCode::CharCrossesVerseBoundary,
        DC::EmptyFigure => DiagnosticCode::EmptyFigure,
        DC::UnquotedAttributeValue => DiagnosticCode::UnquotedAttributeValue,
        DC::MissingRequiredAttribute => DiagnosticCode::MissingRequiredAttribute,
        DC::DefaultAttributeNotDefined => DiagnosticCode::DefaultAttributeNotDefined,
        DC::BodyParagraphBeforeChapter => DiagnosticCode::BodyParagraphBeforeChapter,
        DC::NonEmptyBlankLine => DiagnosticCode::NonEmptyBlankLine,
        DC::LeadingZeros => DiagnosticCode::LeadingZeros,
        DC::EmptyWordMarker => DiagnosticCode::EmptyWordMarker,
        DC::MissingMilestoneSelfClose => DiagnosticCode::MissingMilestoneSelfClose,
    }
}
