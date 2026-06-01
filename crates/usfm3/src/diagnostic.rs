//! Diagnostics: errors, warnings, and lints produced during parsing.
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
    Lint,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Lint => "lint",
        }
    }
}

/// A single diagnostic. `code` is a stable, machine-readable identifier
/// (e.g. `"unknown-marker"`, `"unclosed-char-marker"`) suitable for editor
/// rule configuration; `message` is the human-facing string.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(code: &'static str, span: Span, message: impl Into<String>) -> Self {
        Self { severity: Severity::Error, code, span, message: message.into() }
    }
    pub fn warning(code: &'static str, span: Span, message: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, code, span, message: message.into() }
    }
    pub fn lint(code: &'static str, span: Span, message: impl Into<String>) -> Self {
        Self { severity: Severity::Lint, code, span, message: message.into() }
    }
}
