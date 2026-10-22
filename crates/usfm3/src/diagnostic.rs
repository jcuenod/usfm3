//! Diagnostics: errors, warnings, and info notices produced during parsing.
//!
//! The parser is *recovering* — it never bails. Anything it cannot interpret
//! becomes a [`Diagnostic`] plus an error/text node on the tape, and parsing
//! continues. Every diagnostic points at a real [`Span`] so an editor can
//! underline the exact bytes.

use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    /// Malformed input we recovered from (e.g. a stray backslash, a marker that
    /// is not valid in the active USFM version).
    Error,
    /// Well-formed-enough but suspicious (e.g. an unclosed `\add` that we closed
    /// implicitly at a paragraph boundary).
    Warning,
    /// Style/usage guidance that does not affect interpretation.
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// Machine-readable diagnostic code. The CamelCase variant names are the stable
/// identifiers (matching the rsusfm3 vocabulary) and serialize as their name,
/// e.g. `"InvalidBookCode"`. Variants not yet emitted are reserved for upcoming
/// rules so the vocabulary stays aligned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiagnosticCode {
    // Parser (lexical/structural)
    UnknownMarker,
    UnclosedMarker,
    StrayCloseMarker,
    StrayBackslash,
    // Validation (semantic)
    MissingIdMarker,
    InvalidBookCode,
    DuplicateId,
    TextBeforeId,
    MissingChapterMarker,
    InvalidChapterSequence,
    DuplicateChapter,
    InvalidVerseSequence,
    VerseOutsideParagraph,
    NoteSubmarkerOutsideNote,
    NonEmptyBlankLine,
    LeadingZeros,
    MilestoneMismatch,
    InvalidAttributes,
    MissingChapterNumber,
    MissingVerseNumber,
    CharCrossesVerseBoundary,
    EmptyFigure,
    UnquotedAttributeValue,
    MissingRequiredAttribute,
    BodyParagraphBeforeChapter,
    EmptyWordMarker,
    MissingMilestoneSelfClose,
    InvalidTableColumnSequence,
    DeprecatedMarker,
    ImplicitClose,
    UnclosedNote,
    UnclosedAtEof,
    DefaultAttributeNotDefined,
    // Reserved — these conditions are currently reported via a more general code
    // (a misnested close surfaces as StrayCloseMarker) or are niche advisories not
    // yet split out.
    MisnestedMarker,
    MissingNestingPrefix,
    HeaderAfterBody,
}

impl DiagnosticCode {
    /// The stable string identifier (the CamelCase variant name).
    pub fn as_str(self) -> &'static str {
        // `{:?}` on a fieldless variant is exactly its name; cheap and exhaustive.
        // Mapped through a tiny lookup so the wire format is explicit, not Debug.
        DIAGNOSTIC_CODE_NAMES[self as usize]
    }
}

/// Parallel to the `DiagnosticCode` declaration order.
static DIAGNOSTIC_CODE_NAMES: &[&str] = &[
    "UnknownMarker",
    "UnclosedMarker",
    "StrayCloseMarker",
    "StrayBackslash",
    "MissingIdMarker",
    "InvalidBookCode",
    "DuplicateId",
    "TextBeforeId",
    "MissingChapterMarker",
    "InvalidChapterSequence",
    "DuplicateChapter",
    "InvalidVerseSequence",
    "VerseOutsideParagraph",
    "NoteSubmarkerOutsideNote",
    "NonEmptyBlankLine",
    "LeadingZeros",
    "MilestoneMismatch",
    "InvalidAttributes",
    "MissingChapterNumber",
    "MissingVerseNumber",
    "CharCrossesVerseBoundary",
    "EmptyFigure",
    "UnquotedAttributeValue",
    "MissingRequiredAttribute",
    "BodyParagraphBeforeChapter",
    "EmptyWordMarker",
    "MissingMilestoneSelfClose",
    "InvalidTableColumnSequence",
    "DeprecatedMarker",
    "ImplicitClose",
    "UnclosedNote",
    "UnclosedAtEof",
    "DefaultAttributeNotDefined",
    "MisnestedMarker",
    "MissingNestingPrefix",
    "HeaderAfterBody",
];

/// A single diagnostic. `code` is the machine-readable [`DiagnosticCode`];
/// `message` is the human-facing string; `span` points at the offending bytes.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(code: DiagnosticCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            span,
            message: message.into(),
        }
    }
    pub fn warning(code: DiagnosticCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            span,
            message: message.into(),
        }
    }
    pub fn info(code: DiagnosticCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            code,
            span,
            message: message.into(),
        }
    }
}
