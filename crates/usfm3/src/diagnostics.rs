/// Diagnostics and error reporting for the USFM 3.x parser.
///
/// This module provides structured diagnostic messages with source locations,
/// severity levels, and machine-readable codes. Diagnostics are generated both
/// during parsing (structural issues) and during validation (semantic issues).
///
/// A byte-offset range into the source text.
pub type Span = std::ops::Range<usize>;

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// Machine-readable diagnostic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    // Parser-generated diagnostics
    UnknownMarker,
    DeprecatedMarker,
    UnclosedMarker,
    StrayCloseMarker,
    MisnestedMarker,
    MissingNestingPrefix,
    ImplicitClose,
    UnclosedNote,
    UnclosedAtEof,

    // Validation-generated diagnostics
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
    VerseOutsideParagraph,
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
    InvalidTableColumnSequence,
}

/// A diagnostic message with source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub code: DiagnosticCode,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.severity, self.message)
    }
}

impl Diagnostic {
    /// Unknown/unrecognized marker encountered.
    pub fn unknown_marker(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("unknown marker \\{marker}"),
            code: DiagnosticCode::UnknownMarker,
        }
    }

    /// A recognized marker is deprecated and should be replaced.
    pub fn deprecated_marker(marker: &str, replacement: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            message: format!("\\{marker} is deprecated; prefer {replacement}"),
            code: DiagnosticCode::DeprecatedMarker,
        }
    }

    /// A marker was implicitly closed (e.g., character marker closed by next
    /// paragraph).  In USFM 3.1, character styles must be explicitly closed,
    /// so this is an error.
    ///
    /// The diagnostic highlights the opener that remained unclosed, while the
    /// message still reports where it was opened.
    pub fn implicitly_closed(marker: &str, open_span: Span, close_marker: &str) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span: open_span.clone(),
            message: format!(
                "\\{marker} is missing explicit close (\\{marker}*). It was closed implicitly by \\{close_marker}.",
            ),
            code: DiagnosticCode::ImplicitClose,
        }
    }

    /// A closing marker was found that doesn't match the expected nesting.
    ///
    /// For example, `\add` was closed by `\nd*` instead of `\add*`.
    pub fn misnested_close(open_marker: &str, close_marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("expected \\{open_marker}* but found \\{close_marker}*"),
            code: DiagnosticCode::MisnestedMarker,
        }
    }

    /// A closing marker with no matching opener (e.g., stray `\nd*` with no `\nd`).
    pub fn stray_close(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("closing marker \\{marker}* has no matching opener"),
            code: DiagnosticCode::StrayCloseMarker,
        }
    }

    /// A character marker used inside another character context without the `\+` prefix.
    pub fn missing_nesting_prefix(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            message: format!(
                "\\{marker} is nested inside another character marker without \\+ prefix"
            ),
            code: DiagnosticCode::MissingNestingPrefix,
        }
    }

    /// Malformed attribute string (e.g. unquoted values, missing `=`).
    pub fn malformed_attributes(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "malformed attribute string (values must be quoted)".to_string(),
            code: DiagnosticCode::InvalidAttributes,
        }
    }

    /// Leading zeros in a chapter or verse number (e.g. `\c 091`, `\v 01`).
    pub fn leading_zeros(number: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("leading zeros in number '{number}'"),
            code: DiagnosticCode::LeadingZeros,
        }
    }

    /// A note (`\f` or `\x`) was not properly closed before a paragraph/chapter boundary.
    pub fn unclosed_note(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("note \\{marker} was not closed before paragraph or chapter boundary"),
            code: DiagnosticCode::UnclosedNote,
        }
    }

    /// A marker was still open at end of file.
    pub fn unclosed_at_eof(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("\\{marker} was still open at end of file"),
            code: DiagnosticCode::UnclosedAtEof,
        }
    }

    // ── Validation diagnostics ──────────────────────────────────────────

    /// The required `\id` marker is missing from the document.
    pub fn missing_id_marker() -> Self {
        Diagnostic {
            severity: Severity::Error,
            span: 0..0,
            message: "document is missing required \\id marker".to_string(),
            code: DiagnosticCode::MissingIdMarker,
        }
    }

    /// The book code provided in the `\id` marker is invalid.
    pub fn invalid_book_code(code: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("invalid book code \"{code}\""),
            code: DiagnosticCode::InvalidBookCode,
        }
    }

    /// Chapter numbers are not in the expected sequential order.
    pub fn invalid_chapter_sequence(expected: u32, got: u32, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            message: format!("expected chapter {expected} but found chapter {got}"),
            code: DiagnosticCode::InvalidChapterSequence,
        }
    }

    /// Verse numbers are not in the expected sequential order.
    pub fn invalid_verse_sequence(expected: &str, got: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            message: format!("expected verse {expected} but found verse {got}"),
            code: DiagnosticCode::InvalidVerseSequence,
        }
    }

    /// A chapter number appears more than once in the document.
    pub fn duplicate_chapter(number: u32, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("duplicate chapter {number}"),
            code: DiagnosticCode::DuplicateChapter,
        }
    }

    /// A footnote/cross-reference sub-marker appears outside of a note.
    pub fn note_submarker_outside_note(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("\\{marker} can only appear inside a footnote or cross-reference"),
            code: DiagnosticCode::NoteSubmarkerOutsideNote,
        }
    }

    /// Text content appears before the `\id` marker.
    pub fn text_before_id(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "text content is not allowed before \\id marker".to_string(),
            code: DiagnosticCode::TextBeforeId,
        }
    }

    /// A header marker appears after body content has begun.
    pub fn header_after_body(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            message: format!("header marker \\{marker} appears after body content has started"),
            code: DiagnosticCode::HeaderAfterBody,
        }
    }

    /// A milestone marker has no matching start or end counterpart.
    pub fn milestone_mismatch(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("milestone \\{marker} has no matching counterpart"),
            code: DiagnosticCode::MilestoneMismatch,
        }
    }

    /// A duplicate `\id` marker was found in the document.
    pub fn duplicate_id(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "duplicate \\id marker found".to_string(),
            code: DiagnosticCode::DuplicateId,
        }
    }

    /// A `\c` marker is missing its chapter number.
    pub fn missing_chapter_number(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "\\c marker is missing a chapter number".to_string(),
            code: DiagnosticCode::MissingChapterNumber,
        }
    }

    /// A `\v` marker is missing its verse number.
    pub fn missing_verse_number(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "\\v marker is missing a verse number".to_string(),
            code: DiagnosticCode::MissingVerseNumber,
        }
    }

    /// A `\v` marker appeared at document root instead of inside a paragraph.
    pub fn verse_outside_paragraph(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            span,
            message: "\\v must appear inside a paragraph; inserted implicit \\p for recovery"
                .to_string(),
            code: DiagnosticCode::VerseOutsideParagraph,
        }
    }

    /// The document has no `\c` chapter marker.
    pub fn missing_chapter_marker() -> Self {
        Diagnostic {
            severity: Severity::Error,
            span: 0..0,
            message: "document has no \\c chapter marker".to_string(),
            code: DiagnosticCode::MissingChapterMarker,
        }
    }

    /// A character marker spans across a verse boundary.
    pub fn char_crosses_verse(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("\\{marker} spans across a verse boundary"),
            code: DiagnosticCode::CharCrossesVerseBoundary,
        }
    }

    /// A `\fig` marker has no content or attributes.
    pub fn empty_figure(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "\\fig has no content or attributes".to_string(),
            code: DiagnosticCode::EmptyFigure,
        }
    }

    /// An attribute value is not properly quoted.
    pub fn unquoted_attribute_value(attr: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("attribute value for \"{attr}\" must be quoted"),
            code: DiagnosticCode::UnquotedAttributeValue,
        }
    }

    /// A required attribute is missing.
    pub fn missing_required_attribute(marker: &str, attr: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("\\{marker} is missing required attribute \"{attr}\""),
            code: DiagnosticCode::MissingRequiredAttribute,
        }
    }

    /// A default attribute was used on a marker that has no default attribute defined.
    pub fn default_attribute_not_defined(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!(
                "\\{marker} does not define a default attribute; value after | is invalid"
            ),
            code: DiagnosticCode::DefaultAttributeNotDefined,
        }
    }

    /// A `\b` blank line marker contains content when it should be empty.
    pub fn non_empty_blank_line(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "\\b marker should not contain content".to_string(),
            code: DiagnosticCode::NonEmptyBlankLine,
        }
    }

    /// A body paragraph marker appears before the first chapter.
    pub fn body_paragraph_before_chapter(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!(
                "body paragraph \\{marker} is not allowed before the first \\c marker"
            ),
            code: DiagnosticCode::BodyParagraphBeforeChapter,
        }
    }

    /// A `\w` word marker with no content and no attributes.
    pub fn empty_word_marker(span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: "\\w marker has no content or attributes".to_string(),
            code: DiagnosticCode::EmptyWordMarker,
        }
    }

    /// A milestone marker (e.g., `\k-s`) is missing its self-closing `\*`.
    pub fn missing_milestone_self_close(marker: &str, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("milestone \\{marker} is missing closing \\*"),
            code: DiagnosticCode::MissingMilestoneSelfClose,
        }
    }

    /// A table row skips or reorders numbered columns.
    pub fn invalid_table_column_sequence(expected: u32, found: u32, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            span,
            message: format!("expected table column {expected} but found column {found}"),
            code: DiagnosticCode::InvalidTableColumnSequence,
        }
    }
}

/// A collection of diagnostics produced during parsing and validation.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticList {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticList {
    /// Create an empty diagnostic list.
    pub fn new() -> Self {
        DiagnosticList {
            diagnostics: Vec::new(),
        }
    }

    /// Append a single diagnostic.
    pub fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    /// Merge all diagnostics from another list into this one.
    pub fn extend(&mut self, other: DiagnosticList) {
        self.diagnostics.extend(other.diagnostics);
    }

    /// Returns `true` if no diagnostics have been recorded.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Returns the number of diagnostics.
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Iterate over all diagnostics.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter()
    }

    /// Iterate over only error-severity diagnostics.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    /// Iterate over only warning-severity diagnostics.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Returns `true` if any diagnostic has error severity.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Consume the list and return the underlying `Vec<Diagnostic>`.
    pub fn into_inner(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Constructor tests ───────────────────────────────────────────────

    #[test]
    fn test_unknown_marker() {
        let d = Diagnostic::unknown_marker("zzz", 10..15);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::UnknownMarker);
        assert_eq!(d.span, 10..15);
        assert!(d.message.contains("\\zzz"));
    }

    #[test]
    fn test_deprecated_marker() {
        let d = Diagnostic::deprecated_marker(
            "addpn",
            "nested \\pn ...\\pn* within \\add ...\\add*",
            15..21,
        );
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.code, DiagnosticCode::DeprecatedMarker);
        assert_eq!(d.span, 15..21);
        assert!(d.message.contains("\\addpn"));
        assert!(d.message.contains("\\pn"));
        assert!(d.message.contains("\\add"));
        assert!(d.message.contains("deprecated"));
    }

    #[test]
    fn test_implicitly_closed() {
        let d = Diagnostic::implicitly_closed("p", 0..2, "q");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::ImplicitClose);
        assert_eq!(d.span, 0..2);
        assert!(d.message.contains("\\p"));
        assert!(d.message.contains("\\p*"));
        assert!(d.message.contains("\\q"));
        assert!(d.message.contains("missing explicit close"));
    }

    #[test]
    fn test_misnested_close() {
        let d = Diagnostic::misnested_close("add", "nd", 20..24);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::MisnestedMarker);
        assert_eq!(d.span, 20..24);
        assert!(d.message.contains("\\add*"));
        assert!(d.message.contains("\\nd*"));
    }

    #[test]
    fn test_stray_close() {
        let d = Diagnostic::stray_close("nd", 30..34);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::StrayCloseMarker);
        assert_eq!(d.span, 30..34);
        assert!(d.message.contains("\\nd*"));
        assert!(d.message.contains("no matching opener"));
    }

    #[test]
    fn test_missing_nesting_prefix() {
        let d = Diagnostic::missing_nesting_prefix("nd", 5..8);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.code, DiagnosticCode::MissingNestingPrefix);
        assert_eq!(d.span, 5..8);
        assert!(d.message.contains("\\nd"));
        assert!(d.message.contains("\\+"));
    }

    #[test]
    fn test_unclosed_note() {
        let d = Diagnostic::unclosed_note("f", 100..105);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::UnclosedNote);
        assert_eq!(d.span, 100..105);
        assert!(d.message.contains("\\f"));
    }

    #[test]
    fn test_unclosed_at_eof() {
        let d = Diagnostic::unclosed_at_eof("add", 200..204);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::UnclosedAtEof);
        assert_eq!(d.span, 200..204);
        assert!(d.message.contains("\\add"));
        assert!(d.message.contains("end of file"));
    }

    #[test]
    fn test_missing_id_marker() {
        let d = Diagnostic::missing_id_marker();
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::MissingIdMarker);
        assert_eq!(d.span, 0..0);
        assert!(d.message.contains("\\id"));
    }

    #[test]
    fn test_invalid_book_code() {
        let d = Diagnostic::invalid_book_code("XYZ", 4..7);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::InvalidBookCode);
        assert_eq!(d.span, 4..7);
        assert!(d.message.contains("XYZ"));
    }

    #[test]
    fn test_invalid_chapter_sequence() {
        let d = Diagnostic::invalid_chapter_sequence(2, 5, 10..12);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.code, DiagnosticCode::InvalidChapterSequence);
        assert_eq!(d.span, 10..12);
        assert!(d.message.contains("2"));
        assert!(d.message.contains("5"));
    }

    #[test]
    fn test_invalid_verse_sequence() {
        let d = Diagnostic::invalid_verse_sequence("3", "7", 20..22);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.code, DiagnosticCode::InvalidVerseSequence);
        assert_eq!(d.span, 20..22);
        assert!(d.message.contains("3"));
        assert!(d.message.contains("7"));
    }

    #[test]
    fn test_duplicate_chapter() {
        let d = Diagnostic::duplicate_chapter(3, 40..42);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::DuplicateChapter);
        assert_eq!(d.span, 40..42);
        assert!(d.message.contains("3"));
    }

    #[test]
    fn test_note_submarker_outside_note() {
        let d = Diagnostic::note_submarker_outside_note("ft", 60..63);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::NoteSubmarkerOutsideNote);
        assert_eq!(d.span, 60..63);
        assert!(d.message.contains("\\ft"));
    }

    #[test]
    fn test_text_before_id() {
        let d = Diagnostic::text_before_id(0..5);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::TextBeforeId);
        assert_eq!(d.span, 0..5);
        assert!(d.message.contains("\\id"));
    }

    #[test]
    fn test_header_after_body() {
        let d = Diagnostic::header_after_body("h", 80..82);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.code, DiagnosticCode::HeaderAfterBody);
        assert_eq!(d.span, 80..82);
        assert!(d.message.contains("\\h"));
    }

    #[test]
    fn test_milestone_mismatch() {
        let d = Diagnostic::milestone_mismatch("qt-s", 90..95);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::MilestoneMismatch);
        assert_eq!(d.span, 90..95);
        assert!(d.message.contains("\\qt-s"));
    }

    #[test]
    fn test_missing_chapter_number() {
        let d = Diagnostic::missing_chapter_number(10..12);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::MissingChapterNumber);
        assert_eq!(d.span, 10..12);
        assert!(d.message.contains("\\c"));
    }

    #[test]
    fn test_missing_verse_number() {
        let d = Diagnostic::missing_verse_number(14..16);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::MissingVerseNumber);
        assert_eq!(d.span, 14..16);
        assert!(d.message.contains("\\v"));
    }

    #[test]
    fn test_verse_outside_paragraph() {
        let d = Diagnostic::verse_outside_paragraph(16..18);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.code, DiagnosticCode::VerseOutsideParagraph);
        assert_eq!(d.span, 16..18);
        assert!(d.message.contains("\\v"));
        assert!(d.message.contains("\\p"));
    }

    #[test]
    fn test_invalid_table_column_sequence() {
        let d = Diagnostic::invalid_table_column_sequence(2, 3, 20..24);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.code, DiagnosticCode::InvalidTableColumnSequence);
        assert_eq!(d.span, 20..24);
        assert!(d.message.contains("column 2"));
        assert!(d.message.contains("column 3"));
    }

    // ── Display tests ───────────────────────────────────────────────────

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", Severity::Info), "info");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Error), "error");
    }

    #[test]
    fn test_diagnostic_display_error() {
        let d = Diagnostic::unknown_marker("zzz", 0..3);
        let s = format!("{d}");
        assert!(s.starts_with("error: "));
        assert!(s.contains("\\zzz"));
    }

    #[test]
    fn test_diagnostic_display_warning() {
        let d = Diagnostic::invalid_chapter_sequence(1, 3, 20..22);
        let s = format!("{d}");
        assert!(s.starts_with("warning: "));
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    // ── DiagnosticList tests ────────────────────────────────────────────

    #[test]
    fn test_diagnostic_list_new_is_empty() {
        let list = DiagnosticList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        assert!(!list.has_errors());
    }

    #[test]
    fn test_diagnostic_list_push_and_len() {
        let mut list = DiagnosticList::new();
        list.push(Diagnostic::unknown_marker("zzz", 0..3));
        list.push(Diagnostic::implicitly_closed("p", 0..2, "q"));
        assert!(!list.is_empty());
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_diagnostic_list_extend() {
        let mut list_a = DiagnosticList::new();
        list_a.push(Diagnostic::unknown_marker("zzz", 0..3));

        let mut list_b = DiagnosticList::new();
        list_b.push(Diagnostic::stray_close("nd", 10..14));
        list_b.push(Diagnostic::implicitly_closed("p", 0..2, "q"));

        list_a.extend(list_b);
        assert_eq!(list_a.len(), 3);
    }

    #[test]
    fn test_diagnostic_list_iter() {
        let mut list = DiagnosticList::new();
        list.push(Diagnostic::unknown_marker("a", 0..1));
        list.push(Diagnostic::unknown_marker("b", 1..2));
        let codes: Vec<_> = list.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![DiagnosticCode::UnknownMarker, DiagnosticCode::UnknownMarker]
        );
    }

    #[test]
    fn test_diagnostic_list_errors_filter() {
        let mut list = DiagnosticList::new();
        list.push(Diagnostic::unknown_marker("zzz", 0..3)); // Error
        list.push(Diagnostic::implicitly_closed("p", 0..2, "q")); // Error
        list.push(Diagnostic::stray_close("nd", 10..14)); // Error
        list.push(Diagnostic::invalid_chapter_sequence(1, 3, 20..22)); // Warning

        let errors: Vec<_> = list.errors().collect();
        assert_eq!(errors.len(), 3);
        assert!(errors.iter().all(|d| d.severity == Severity::Error));
    }

    #[test]
    fn test_diagnostic_list_warnings_filter() {
        let mut list = DiagnosticList::new();
        list.push(Diagnostic::unknown_marker("zzz", 0..3)); // Error
        list.push(Diagnostic::implicitly_closed("p", 0..2, "q")); // Error
        list.push(Diagnostic::stray_close("nd", 10..14)); // Error
        list.push(Diagnostic::invalid_chapter_sequence(1, 3, 20..22)); // Warning
        list.push(Diagnostic::deprecated_marker(
            "addpn",
            "nested \\pn ...\\pn* within \\add ...\\add*",
            30..36,
        )); // Warning

        let warnings: Vec<_> = list.warnings().collect();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|d| d.severity == Severity::Warning));
    }

    #[test]
    fn test_diagnostic_list_has_errors() {
        let mut list = DiagnosticList::new();
        list.push(Diagnostic::implicitly_closed("p", 0..2, "q"));
        assert!(list.has_errors());
    }

    #[test]
    fn test_diagnostic_list_into_inner() {
        let mut list = DiagnosticList::new();
        list.push(Diagnostic::unknown_marker("a", 0..1));
        list.push(Diagnostic::unknown_marker("b", 1..2));
        let vec = list.into_inner();
        assert_eq!(vec.len(), 2);
    }

    #[test]
    fn test_diagnostic_list_default() {
        let list = DiagnosticList::default();
        assert!(list.is_empty());
    }
}
