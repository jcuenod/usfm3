//! `usfm3` — an error-tolerant, blazingly fast USFM parser.
//!
//! # Architecture (one tape, many views)
//!
//! Parsing produces a single flat [`tape::Tape`] of 16-byte records plus a list of
//! [`diagnostic::Diagnostic`]s. Everything else — the CST [`cst::Cursor`] for
//! editors/linters, USX and USJ serialization, and the `cat`-speed verse-text
//! extraction — is a *view* over that tape and the original source buffer. Text is
//! held as zero-copy spans; there is no per-node allocation and no intermediate
//! model between the tape and any output.
//!
//! ```ignore
//! let doc = usfm3::parse(src, Version::V3_1);
//! for d in doc.diagnostics() { /* underline d.span */ }
//! let usx = doc.to_usx();
//! let vrefs = doc.vref_text();          // cleaned, version-correct
//! for (r, text) in vrefs.iter() { println!("{r} = {text}"); }
//! ```

// Internal representation. The crate root exposes a curated high-level API
// (below); the flat tape, marker table, and CST cursor are available — opt-in —
// through [`low_level`] for editor tooling and advanced consumers.
mod cst;
mod diagnostic;
mod marker;
mod parse;
mod serialize;
mod span;
mod tape;
mod validate;

use cst::Cursor;
use marker::MarkerTable;
use tape::Tape;

// --- public API surface ----------------------------------------------------

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};
pub use serialize::vref::{BOOK_CODES, BookId, VerseRef, VrefRow, VrefText};

/// Low-level access to the parse representation — the flat [`Tape`], the
/// version-aware [`MarkerTable`], and the zero-allocation CST [`Cursor`] — for
/// editor tooling and advanced consumers. The high-level API at the crate root
/// (parse, [`Document`], `to_usx`/`to_usj`/`to_usfm`, vref, diagnostics) covers
/// most needs; reach in here only when you need the raw structure.
pub mod low_level {
    pub use crate::cst::Cursor;
    pub use crate::marker::{Category, Closing, MarkerId, MarkerInfo, MarkerTable, TextRole};
    pub use crate::span::Span;
    pub use crate::tape::{Node, NodeKind, Tape, flags};
}

/// USFM specification version. Selects the [`MarkerTable`] and lint rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Version {
    V3_1,
    V3_2,
}

impl Default for Version {
    fn default() -> Self {
        Version::V3_1
    }
}

/// Options controlling a parse.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParseOptions {
    /// Collect diagnostics (lexical/structural errors + a semantic validation
    /// pass). Off by default so the hot path (e.g. vref) pays nothing.
    pub diagnostics: bool,
}

/// Parse `source` against the given USFM `version`, with diagnostics OFF.
///
/// Never fails: the parser recovers from malformed input. Use [`parse_with`] to
/// opt into diagnostics. The returned [`Document`] borrows `source` (zero-copy).
pub fn parse(source: &str, version: Version) -> Document<'_> {
    parse_with(source, version, ParseOptions::default())
}

/// Parse with explicit [`ParseOptions`]. With `diagnostics: true`, the parser
/// records lexical/structural diagnostics and runs a semantic validation pass.
pub fn parse_with(source: &str, version: Version, options: ParseOptions) -> Document<'_> {
    let table = MarkerTable::for_version(version);
    let out = parse::build::build(source, table, options.diagnostics);
    let mut doc = Document {
        source,
        version,
        table,
        tape: out.tape,
        diagnostics: out.diagnostics,
    };
    if options.diagnostics {
        doc.validate();
        // Emit in source order. Stable, so parser vs. validation order is kept
        // for diagnostics at the same offset.
        doc.diagnostics.sort_by_key(|d| (d.span.start, d.span.end()));
    }
    doc
}

/// A parsed USFM document: the tape, diagnostics, and zero-copy views over the
/// borrowed source.
pub struct Document<'src> {
    source: &'src str,
    version: Version,
    table: &'static MarkerTable,
    tape: Tape,
    diagnostics: Vec<Diagnostic>,
}

impl<'src> Document<'src> {
    pub fn version(&self) -> Version {
        self.version
    }

    pub fn source(&self) -> &'src str {
        self.source
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn tape(&self) -> &Tape {
        &self.tape
    }

    /// Semantic validation pass (run by [`parse_with`] when diagnostics are on).
    /// Appends warnings/errors for things the recovering parser tolerates.
    fn validate(&mut self) {
        let mut extra = validate::validate(self.cursor());
        self.diagnostics.append(&mut extra);
    }

    /// A cursor at the document root for CST traversal / typed-AST construction.
    pub fn cursor(&self) -> Cursor<'_> {
        Cursor::new(&self.tape, self.source, self.table, 0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticCode;

    #[test]
    fn parses_and_exposes_views() {
        let doc = parse("\\id GEN\n\\c 1\n\\v 1 In the beginning", Version::V3_1);
        assert!(doc.diagnostics().is_empty());
        assert_eq!(doc.cursor().kind(), tape::NodeKind::Root);
        assert!(doc.to_usx().starts_with("<usx"));
        assert!(doc.to_usj().contains("\"type\":\"USJ\""));
        assert_eq!(
            doc.vref_dict(),
            vec![("GEN 1:1".to_string(), "In the beginning".to_string())]
        );
    }

    #[test]
    fn vref_strips_notes_and_char_tags() {
        let src = "\
\\id GEN
\\c 1
\\s1 The Creation
\\p
\\v 1 In the \\nd Lord\\nd*\\f + \\ft a footnote\\f* beginning.
\\v 2 And\\x + \\xt ref\\x* the earth.";
        let doc = parse(src, Version::V3_1);
        let dict = doc.vref_dict();
        assert_eq!(dict.len(), 2);
        // \nd content kept, footnote excluded, heading not attributed to a verse.
        assert_eq!(dict[0], ("GEN 1:1".into(), "In the Lord beginning.".into()));
        // cross-reference excluded.
        assert_eq!(dict[1], ("GEN 1:2".into(), "And the earth.".into()));
    }

    #[test]
    fn no_space_around_markers_lacking_whitespace() {
        // A footnote wedged between `.` and `”` (no surrounding whitespace) must
        // not introduce a space; poetry line breaks (\q + newline) must.
        let src = "\
\\id PSA
\\c 132
\\q1
\\v 5 until I find a place for the LORD,
\\q2 a dwelling for the Mighty One of Jacob.\\f + \\ft note\\f*”";
        let doc = parse(src, Version::V3_1);
        assert_eq!(
            doc.vref_dict(),
            vec![(
                "PSA 132:5".into(),
                "until I find a place for the LORD, a dwelling for the Mighty One of Jacob.”".into()
            )]
        );
    }

    #[test]
    fn dict_drops_empty_and_merges_duplicates() {
        // \v 1 has only a footnote → empty → dropped. \v 2 appears twice → merged.
        let src = "\\id GEN\n\\c 1\n\\v 1 \\f + \\ft note\\f*\n\\v 2 first part\n\\v 2 second part";
        let doc = parse(src, Version::V3_1);
        assert_eq!(
            doc.vref_dict(),
            vec![("GEN 1:2".into(), "first part second part".into())]
        );
    }

    #[test]
    fn milestone_does_not_error() {
        // \qt-s\* ... \qt-e\* milestones parse cleanly; the quoted text is kept.
        let opts = ParseOptions { diagnostics: true };
        let doc = parse_with("\\id MAT\n\\c 1\n\\p\n\\v 1 He said \\qt-s\\*Follow me\\qt-e\\* today.", Version::V3_1, opts);
        assert!(doc.diagnostics().is_empty(), "{:?}", doc.diagnostics());
        assert_eq!(doc.vref_dict()[0].1, "He said Follow me today.");
    }

    #[test]
    fn warns_on_verse_outside_paragraph() {
        let opts = ParseOptions { diagnostics: true };
        // \v directly after \c with no \p.
        let outside = parse_with("\\id GEN\n\\c 1\n\\v 1 text", Version::V3_1, opts);
        assert!(outside.diagnostics().iter().any(|d| d.code == DiagnosticCode::VerseOutsideParagraph));
        // Same content but inside \p → no such warning.
        let inside = parse_with("\\id GEN\n\\c 1\n\\p\n\\v 1 text", Version::V3_1, opts);
        assert!(!inside.diagnostics().iter().any(|d| d.code == DiagnosticCode::VerseOutsideParagraph));
    }

    #[test]
    fn warns_on_invalid_book_code_and_verse_in_heading() {
        let opts = ParseOptions { diagnostics: true };
        // Bad book code.
        let bad = parse_with("\\id XYZ\n\\c 1\n\\p\n\\v 1 t", Version::V3_1, opts);
        assert!(bad.diagnostics().iter().any(|d| d.code == DiagnosticCode::InvalidBookCode));
        // Valid deuterocanonical code → no InvalidBookCode.
        let good = parse_with("\\id TOB\n\\c 1\n\\p\n\\v 1 t", Version::V3_1, opts);
        assert!(!good.diagnostics().iter().any(|d| d.code == DiagnosticCode::InvalidBookCode));
        // \v inside a \s heading (no body \p) → verse-outside-paragraph.
        let heading = parse_with("\\id GEN\n\\c 1\n\\s Heading\n\\v 1 t", Version::V3_1, opts);
        assert!(heading.diagnostics().iter().any(|d| d.code == DiagnosticCode::VerseOutsideParagraph));
    }

    #[test]
    fn diagnostics_are_opt_in() {
        let src = "\\c 1\n\\v 1 \\add words\n\\p more"; // \add closed implicitly by \p
        // Off by default (hot path): no diagnostics.
        assert!(parse(src, Version::V3_1).diagnostics().is_empty());
        // Opt-in: the \add closed by the following \p is reported, plus a
        // missing-id validation error.
        let doc = parse_with(src, Version::V3_1, ParseOptions { diagnostics: true });
        assert!(doc.diagnostics().iter().any(|d| d.code == DiagnosticCode::ImplicitClose));
        assert!(doc.diagnostics().iter().any(|d| d.code == DiagnosticCode::MissingIdMarker));
    }

    /// Helper: does parsing `src` (with diagnostics on) emit `code` at `severity`?
    fn emits(src: &str, code: DiagnosticCode, sev: crate::diagnostic::Severity) -> bool {
        let doc = parse_with(src, Version::V3_1, ParseOptions { diagnostics: true });
        doc.diagnostics().iter().any(|d| d.code == code && d.severity == sev)
    }

    #[test]
    fn structural_validation_errors() {
        use crate::diagnostic::Severity::Error;
        // A second \id is an error.
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 t\n\\id MAT", DiagnosticCode::DuplicateId, Error));
        // A body paragraph before the first \c (in a scripture book).
        assert!(emits("\\id GAL\n\\p before\n\\c 1\n\\p\n\\v 1 t", DiagnosticCode::BodyParagraphBeforeChapter, Error));
        // An unknown marker (outside the \z user namespace).
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 t\n\\ix bad", DiagnosticCode::UnknownMarker, Error));
        // A \z… custom marker is allowed silently.
        assert!(!emits("\\id GEN\n\\c 1\n\\p\n\\v 1 t \\zwhatever x", DiagnosticCode::UnknownMarker, Error));
        // A \esb sidebar with no \esbe, left open at EOF.
        assert!(emits("\\id GEN\n\\c 1\n\\p \\v 1 text\\esb", DiagnosticCode::UnclosedAtEof, Error));
    }

    #[test]
    fn content_validation_errors() {
        use crate::diagnostic::Severity::Error;
        // \w with neither content nor attributes.
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 a \\w \\w* b", DiagnosticCode::EmptyWordMarker, Error));
        // \rb missing its required gloss attribute.
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\rb text\\rb*", DiagnosticCode::MissingRequiredAttribute, Error));
        // …but providing the gloss (default attribute) clears it.
        assert!(!emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\rb text|gloss\\rb*", DiagnosticCode::MissingRequiredAttribute, Error));
        // A milestone missing its self-closing \*.
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\zaln-s |x-lemma=\"a\"\n\\w b\\w*", DiagnosticCode::MissingMilestoneSelfClose, Error));
        // …but a properly self-closed milestone is fine.
        assert!(!emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\qt-s |who=\"X\"\\*hi\\qt-e\\*", DiagnosticCode::MissingMilestoneSelfClose, Error));
    }

    // --- marker-handling minutiae (ported from rsusfm3's builder/markers tests,
    //     adapted to the tape/Cursor + USJ API) -----------------------------

    /// Does the document's USJ contain a char node with the given marker?
    fn usj_has_char(src: &str, marker: &str) -> bool {
        let usj = parse(src, Version::V3_1).to_usj();
        usj.contains(&format!("\"type\":\"char\",\"marker\":\"{marker}\""))
    }

    #[test]
    fn fm_marker_parses_as_character() {
        // \fm (footnote reference mark) is a known character marker, not unknown.
        let src = "\\id GEN\n\\c 1\n\\p\n\\v 17 text\\fm GEN 2:9\\fm* more text";
        assert!(!emits(src, DiagnosticCode::UnknownMarker, crate::diagnostic::Severity::Error));
        assert!(usj_has_char(src, "fm"));
    }

    #[test]
    fn addpn_parses_as_character_with_deprecation_warning() {
        use crate::diagnostic::Severity::{Error, Warning};
        let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 text \\addpn Added Name\\addpn* more";
        assert!(!emits(src, DiagnosticCode::UnknownMarker, Error), "\\addpn is known");
        assert!(usj_has_char(src, "addpn"), "\\addpn parses as a char node");
        let doc = parse_with(src, Version::V3_1, ParseOptions { diagnostics: true });
        let deprecations = doc
            .diagnostics()
            .iter()
            .filter(|d| d.code == DiagnosticCode::DeprecatedMarker && d.severity == Warning)
            .count();
        assert_eq!(deprecations, 1, "exactly one \\addpn deprecation warning");
    }

    #[test]
    fn implicit_close_vs_unclosed_at_eof_vs_unclosed_note() {
        use crate::diagnostic::Severity::Error;
        // \add crossing a paragraph boundary → ImplicitClose, naming both markers.
        let doc = parse_with("\\id GEN\n\\c 1\n\\p\n\\v 1 \\add text\n\\p next", Version::V3_1, ParseOptions { diagnostics: true });
        assert!(doc.diagnostics().iter().any(|d| d.code == DiagnosticCode::ImplicitClose
            && d.message.contains("\\add")
            && d.message.contains("\\p")));
        // \add open at EOF → UnclosedAtEof (not ImplicitClose).
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\add text", DiagnosticCode::UnclosedAtEof, Error));
        // \f note open at a paragraph boundary → UnclosedNote.
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 t\\f + note\n\\p next", DiagnosticCode::UnclosedNote, Error));
    }

    #[test]
    fn z_namespace_markers_are_silent() {
        use crate::diagnostic::Severity::Error;
        // \z… custom markers never produce unknown/implicit-close/eof diagnostics.
        for src in [
            "\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text\\zcustom*",
            "\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text\n\\p next",
            "\\id GEN\n\\c 1\n\\p\n\\v 1 \\zcustom text",
        ] {
            let doc = parse_with(src, Version::V3_1, ParseOptions { diagnostics: true });
            assert!(!doc.diagnostics().iter().any(|d| matches!(
                d.code,
                DiagnosticCode::UnknownMarker | DiagnosticCode::ImplicitClose | DiagnosticCode::UnclosedAtEof
            ) && d.severity == Error), "no diagnostics for {src:?}");
        }
    }

    #[test]
    fn root_level_verse_warns_once() {
        // Two verses at chapter root (no \p) warn exactly once, not per verse.
        let doc = parse_with("\\id GEN\n\\c 1\n\\v 1 text\n\\v 2 more", Version::V3_1, ParseOptions { diagnostics: true });
        let warns = doc.diagnostics().iter().filter(|d| d.code == DiagnosticCode::VerseOutsideParagraph).count();
        assert_eq!(warns, 1, "only the first root-level verse warns");
    }

    #[test]
    fn verse_range_and_subverse_sids() {
        // Bridges (3-4, 12-83) and sub-verse letters (3b) keep their label in the sid.
        for (num, sid) in [("3-4", "GEN 1:3-4"), ("12-83", "GEN 1:12-83"), ("3b", "GEN 1:3b")] {
            let usj = parse(&format!("\\id GEN\n\\c 1\n\\p\n\\v {num} text"), Version::V3_1).to_usj();
            assert!(usj.contains(&format!("\"number\":\"{num}\"")), "{num}");
            assert!(usj.contains(&format!("\"sid\":\"{sid}\"")), "{sid}");
        }
    }

    #[test]
    fn marker_classification_and_numbered_fallback() {
        // Ported from rsusfm3's markers.rs classification tests, mapped to our
        // (coarser) Category model. Verifies representative markers are KNOWN and
        // land in the right family, that numbered/levelled variants fall back to
        // their base, and that genuinely-unknown markers stay unknown.
        use crate::marker::{Category, MarkerTable};
        let t = MarkerTable::for_version(Version::V3_1);
        let cat = |name: &str| t.lookup(name).and_then(|id| t.info(id)).map(|m| m.category);

        let groups: &[(Category, &[&str])] = &[
            (Category::Identification, &["id", "h", "toc", "usfm", "ide", "sts", "rem", "periph"]),
            (Category::Paragraph, &["p", "m", "q", "b", "pb", "tr", "li", "mt", "ms", "s", "imt", "io", "cl", "cd", "esb", "esbe"]),
            (Category::Character, &["add", "nd", "wj", "w", "bk", "fm", "addpn", "pn", "ca", "vp", "xt", "ref", "fr", "ft", "fdc", "xo", "th", "tc", "fig"]),
            (Category::Note, &["f", "fe", "ef", "x", "ex"]),
            (Category::Milestone, &["qt-s", "zaln-s", "ts-s", "k-s"]),
            (Category::Chapter, &["c"]),
            (Category::Verse, &["v"]),
        ];
        for (expected, names) in groups {
            for name in *names {
                assert_eq!(cat(name), Some(*expected), "\\{name} should be {expected:?}");
            }
        }
        // Numbered / level / column-span variants resolve to their base's family.
        assert_eq!(cat("q5"), Some(Category::Paragraph));
        assert_eq!(cat("mt2"), Some(Category::Paragraph));
        assert_eq!(cat("s4"), Some(Category::Paragraph));
        assert_eq!(cat("th3"), Some(Category::Character)); // table cell
        assert_eq!(cat("tcr1-2"), Some(Category::Character)); // column span
        // Genuinely unknown markers are not in the table.
        assert_eq!(cat("notamarker"), None);
        assert_eq!(cat("xyz99"), None);
    }

    #[test]
    fn vref_text_spans_paragraph_continuations() {
        // A verse whose text continues across a \p (no new \v) is one verse, with a
        // boundary space joining the parts.
        let src = "\\id GEN\n\\c 1\n\\p\n\\v 1 first part\n\\p continued part\n\\v 2 next";
        let dict = parse(src, Version::V3_1).vref_dict();
        assert_eq!(dict[0], ("GEN 1:1".into(), "first part continued part".into()));
        assert_eq!(dict[1].0, "GEN 1:2");
    }

    #[test]
    fn diagnostic_code_names_stay_aligned() {
        // The wire name is indexed by `self as usize` into a parallel array; guard
        // the ends and a reordered middle entry so drift is caught immediately.
        assert_eq!(DiagnosticCode::UnknownMarker.as_str(), "UnknownMarker");
        assert_eq!(DiagnosticCode::InvalidTableColumnSequence.as_str(), "InvalidTableColumnSequence");
        assert_eq!(DiagnosticCode::MissingMilestoneSelfClose.as_str(), "MissingMilestoneSelfClose");
        assert_eq!(DiagnosticCode::ImplicitClose.as_str(), "ImplicitClose");
        assert_eq!(DiagnosticCode::UnclosedAtEof.as_str(), "UnclosedAtEof");
        assert_eq!(DiagnosticCode::DefaultAttributeNotDefined.as_str(), "DefaultAttributeNotDefined");
    }

    #[test]
    fn table_column_sequence_is_validated() {
        use crate::diagnostic::Severity::Error;
        // \th1 then \th3 skips column 2.
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 x\n\\tr \\th1 a \\th3 b", DiagnosticCode::InvalidTableColumnSequence, Error));
        // A well-ordered row is clean.
        assert!(!emits("\\id GEN\n\\c 1\n\\p\n\\v 1 x\n\\tr \\th1 a \\th2 b", DiagnosticCode::InvalidTableColumnSequence, Error));
        // A column span (\tcr1-2) advances past column 2.
        assert!(!emits("\\id GEN\n\\c 1\n\\p\n\\v 1 x\n\\tr \\tc1 a \\tcr2-3 b", DiagnosticCode::InvalidTableColumnSequence, Error));
    }

    #[test]
    fn blank_line_content_and_default_attribute_rules() {
        use crate::diagnostic::Severity::Error;
        // \b carrying inline content (a figure) is an error...
        assert!(emits("\\id GEN\n\\c 1\n\\b \\fig cap|src=\"x.jpg\"\\fig*", DiagnosticCode::NonEmptyBlankLine, Error));
        // ...but a \b that merely precedes a verse is a structural boundary, not
        // content (this keeps valid poetry-style blank lines clean).
        assert!(!emits("\\id GEN\n\\c 1\n\\q1 a\n\\b\n\\v 2 text", DiagnosticCode::NonEmptyBlankLine, Error));
        // A bare default attribute on a marker with no default defined (\pn).
        assert!(emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\w \\+pn name|keyword\\+pn*\\w*", DiagnosticCode::DefaultAttributeNotDefined, Error));
        // ...but a nested \+pn with no keyword is fine.
        assert!(!emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\w \\+pn name\\+pn*\\w*", DiagnosticCode::DefaultAttributeNotDefined, Error));
        // ...and a bare default on a marker that DOES define one (\w) is fine.
        assert!(!emits("\\id GEN\n\\c 1\n\\p\n\\v 1 \\w word|grace\\w*", DiagnosticCode::DefaultAttributeNotDefined, Error));
    }

    #[test]
    fn front_matter_books_need_no_chapter() {
        // \id FRT (front matter) with paragraphs but no \c is valid — no chapter
        // errors. A scripture book in the same shape would error.
        let frt = parse_with("\\id FRT\n\\periph Title|id=\"title\"\n\\p text", Version::V3_1, ParseOptions { diagnostics: true });
        assert!(!frt.diagnostics().iter().any(|d| matches!(
            d.code,
            DiagnosticCode::MissingChapterMarker | DiagnosticCode::BodyParagraphBeforeChapter
        )));
    }

    /// CRLF line endings (\r\n) should not produce trailing spaces in USJ text.
    /// A line like `\ide 65001 - Unicode (UTF8)\r\n` must become `"65001 - Unicode (UTF8)"`
    /// not `"65001 - Unicode (UTF8) "`.
    #[test]
    fn crlf_files_no_trailing_space() {
        let src = "\\id GEN\r\n\\ide 65001 - Unicode (UTF8)\r\n\\c 1\r\n\\p\r\n\\v 1 text\r\n";
        let usj = parse(src, Version::V3_1).to_usj();
        // The ide text must be trimmed of \r
        assert!(usj.contains("\"65001 - Unicode (UTF8)\""), "trailing \\r became a space: {usj}");
        assert!(!usj.contains("\"65001 - Unicode (UTF8) \""), "spurious trailing space in CRLF file");
    }

    /// Empty non-`\b` paragraphs (like `\q2` with no content) must emit `content:[]`,
    /// not omit the key. `\b` alone omits `content` per spec.
    #[test]
    fn empty_para_content_vs_b() {
        let src = "\\id GEN\n\\c 1\n\\q1 a\n\\q2\n\\b\n\\q1 b";
        let usj = parse(src, Version::V3_1).to_usj();
        // \q2 with no text → content:[]
        assert!(usj.contains("\"marker\":\"q2\",\"content\":[]"), "empty \\q2 should have content:[]: {usj}");
        // \b → no content key at all
        assert!(!usj.contains("\"marker\":\"b\",\"content\":"), "\\b must not emit content: {usj}");
    }

    /// Malformed USFM: `\v11` (number fused to marker with no space) should parse
    /// as verse 11, not verse 0 or empty string.
    #[test]
    fn verse_number_embedded_in_marker_name() {
        let src = "\\id GEN\n\\c 1\n\\p \\v11 text eleven";
        let doc = parse(src, Version::V3_1);
        let usj = doc.to_usj();
        assert!(usj.contains("\"number\":\"11\""), "\\v11 should parse as verse 11: {usj}");
        assert!(usj.contains("\"sid\":\"GEN 1:11\""), "sid should be GEN 1:11: {usj}");
        // vref should also work
        let dict = doc.vref_dict();
        assert_eq!(dict[0].0, "GEN 1:11", "vref key should be GEN 1:11");
        assert!(dict[0].1.contains("text eleven"), "vref text: {:?}", dict[0]);
    }

    #[test]
    fn verse_ref_packs_and_formats() {
        let r = VerseRef::new(BookId::from_code("GEN").unwrap(), 1, 1);
        assert_eq!(r.to_string(), "GEN 1:1");
        assert_eq!(r.chapter(), 1);
        assert_eq!(r.verse(), 1);
    }
}
