//! Opt-in semantic validation: a recursive walk over the CST emitting warnings
//! and errors for things the recovering parser tolerates (verse not in a
//! paragraph, out-of-sequence chapters/verses, note-internal markers outside a
//! note, …). Runs only when [`crate::ParseOptions::diagnostics`] is set, so it
//! never touches the hot path.

use std::collections::HashSet;

use crate::cst::Cursor;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::marker::{Category, TextRole, default_attribute};
use crate::serialize::vref::BookId;
use crate::serialize::{alt_pub_kind, attached_numbers};
use crate::span::Span;
use crate::tape::NodeKind;

/// Validate a document (given its root cursor) and return the diagnostics.
pub(crate) fn validate(root: Cursor<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut st = State {
        expected_chapter: 1,
        expected_verse: VersePart {
            num: 1,
            suffix: None,
        },
        ..State::default()
    };

    // Text before the \id marker.
    if let Some(first) = root.children().next()
        && first.kind() == NodeKind::Text
        && !first.text().trim().is_empty()
    {
        diags.push(Diagnostic::warning(
            DiagnosticCode::TextBeforeId,
            first.span(),
            "text appears before the \\id marker",
        ));
    }

    walk(root, false, false, false, &mut st, &mut diags);

    // Milestone starts with no matching end.
    for (base, span) in &st.open_milestones {
        diags.push(Diagnostic::error(
            DiagnosticCode::MilestoneMismatch,
            *span,
            format!("milestone \\{base}-s has no matching \\{base}-e"),
        ));
    }

    if !st.has_id && (st.has_chapter || st.has_verse) {
        diags.push(Diagnostic::error(
            DiagnosticCode::MissingIdMarker,
            Span::empty(0),
            "document has chapters/verses but no \\id (book) marker",
        ));
    }
    // A scripture book with no chapter at all. Front matter / peripheral books
    // (\id FRT, INT, GLO, …) legitimately have no \c, so they're exempt.
    if (st.has_id || st.has_verse) && !st.has_chapter && !st.front_matter {
        diags.push(Diagnostic::error(
            DiagnosticCode::MissingChapterMarker,
            Span::empty(0),
            "the document has no \\c (chapter) marker",
        ));
    }
    diags
}

#[derive(Default)]
struct State {
    has_id: bool,
    has_chapter: bool,
    has_verse: bool,
    saw_book: bool,
    /// The `\id` book code is front matter / peripheral (no chapters expected).
    front_matter: bool,
    /// Whether a verse-outside-paragraph warning has already fired (warn once).
    verse_outside_warned: bool,
    expected_chapter: u32,
    seen_chapters: HashSet<u32>,
    expected_verse: VersePart,
    /// Open milestone starts (`\qt-s`, `\zaln-s`) awaiting a matching `-e`,
    /// keyed by base (`qt`, `zaln`), with the start span for error reporting.
    open_milestones: Vec<(String, Span)>,
}

/// `in_body_para` is true when an ancestor is a paragraph that can *hold verse
/// text* — i.e. a `Boundary`-role paragraph (`\p`, `\q`, `\m`, …), not a heading
/// like `\s`/`\mt` (`Excluded` role). So a `\v` inside `\s` still warns.
fn walk(
    parent: Cursor<'_>,
    in_body_para: bool,
    in_note: bool,
    in_char: bool,
    st: &mut State,
    diags: &mut Vec<Diagnostic>,
) {
    // The latest chapter/verse among these siblings: the only one a following
    // `\ca`/`\cp`/`\va`/`\vp` can be attached to.
    let mut anchor = None;
    for child in parent.children() {
        match child.kind() {
            NodeKind::Chapter => {
                st.has_chapter = true;
                let num = child.number().unwrap_or(0);
                let number = number_label(child);
                if number.label.is_empty() {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::MissingChapterNumber,
                        child.span(),
                        "\\c is missing its chapter number",
                    ));
                }
                if !st.seen_chapters.insert(num) {
                    // Warning, not error: glossaries/wordlists legitimately repeat
                    // \c 1 (the corpus has such valid documents).
                    diags.push(Diagnostic::warning(
                        DiagnosticCode::DuplicateChapter,
                        child.span(),
                        format!("chapter {num} appears more than once"),
                    ));
                }
                if num != st.expected_chapter {
                    diags.push(Diagnostic::warning(
                        DiagnosticCode::InvalidChapterSequence,
                        child.span(),
                        format!("expected chapter {} but found {num}", st.expected_chapter),
                    ));
                }
                anchor = Some(child);
                st.expected_chapter = num + 1;
                st.expected_verse = VersePart {
                    num: 1,
                    suffix: None,
                };
                check_number_form(child, &number, "chapter", diags);
            }
            NodeKind::Verse => {
                st.has_verse = true;
                if !in_body_para && !st.verse_outside_warned {
                    // Warn once: a run of root-level verses is one recovery, not N.
                    st.verse_outside_warned = true;
                    diags.push(Diagnostic::warning(
                        DiagnosticCode::VerseOutsideParagraph,
                        child.span(),
                        "\\v is not inside a body paragraph (e.g. \\c or \\s directly followed by \\v with no \\p)",
                    ));
                }
                if in_char {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::CharCrossesVerseBoundary,
                        child.span(),
                        "a character marker spans a verse boundary (\\v inside a \\char span)",
                    ));
                }
                let number = number_label(child);
                let label = number.label;
                if label.is_empty() {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::MissingVerseNumber,
                        child.span(),
                        "\\v is missing its verse number",
                    ));
                } else if let Some(start) = parse_verse_part(label) {
                    if !is_valid_verse_sequence(st.expected_verse, start) {
                        diags.push(Diagnostic::warning(
                            DiagnosticCode::InvalidVerseSequence,
                            child.span(),
                            format!("expected verse {} but found {label}", st.expected_verse),
                        ));
                    }
                    st.expected_verse = parse_verse_range_end(label)
                        .unwrap_or(start)
                        .next_expected();
                }
                anchor = Some(child);
                check_number_form(child, &number, "verse", diags);
            }
            NodeKind::MarkerOpen => {
                let Some(info) = child.marker_info() else {
                    continue;
                };
                if info.name == "id" {
                    if st.saw_book {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::DuplicateId,
                            child.span(),
                            "more than one \\id marker",
                        ));
                    }
                    st.saw_book = true;
                    st.has_id = true;
                    if let Some(code) = book_code(child) {
                        st.front_matter = is_front_matter_code(code);
                        if BookId::from_code(code).is_none() {
                            // Warning: an unrecognized code is suspicious but not
                            // malformed — the corpus has valid documents using
                            // placeholder codes (e.g. TST). Outputs keep the code as
                            // written either way.
                            let upper = code.to_ascii_uppercase();
                            let message = if BookId::from_code(&upper).is_some() {
                                format!("book code \"{code}\" should be uppercase: \"{upper}\"")
                            } else {
                                format!("\"{code}\" is not a valid USFM book code")
                            };
                            diags.push(Diagnostic::warning(
                                DiagnosticCode::InvalidBookCode,
                                child.span(),
                                message,
                            ));
                        }
                    }
                }
                if info.category == Category::Character
                    && !in_note
                    && is_note_only_marker(info.name)
                {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::NoteSubmarkerOutsideNote,
                        child.span(),
                        format!("\\{} is only valid inside a note", info.name),
                    ));
                }
                if info.name == "b" && blank_line_has_inline_content(child) {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::NonEmptyBlankLine,
                        child.span(),
                        "\\b (blank line) must not contain content",
                    ));
                }
                if info.category == Category::Paragraph
                    && info.text_role == TextRole::Boundary
                    && !st.has_chapter
                    && !st.front_matter
                {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::BodyParagraphBeforeChapter,
                        child.span(),
                        format!(
                            "body paragraph \\{} appears before any \\c (chapter)",
                            info.name
                        ),
                    ));
                }
                // The table-name test is a cheap prefilter for the rare markers
                // that need the full check.
                if matches!(info.name, "ca" | "cp" | "va" | "vp")
                    && let Some((owner, _)) = alt_pub_kind(child)
                    && !anchor.is_some_and(|a| is_attached(a, child))
                {
                    // Warning: published texts do this (TPI Greek Esther re-labels
                    // the chapter mid-chapter with \cp), and the marker is kept as
                    // character text, so nothing is lost.
                    let (c, unit) = match owner {
                        NodeKind::Chapter => ("c", "chapter"),
                        _ => ("v", "verse"),
                    };
                    diags.push(Diagnostic::warning(
                        DiagnosticCode::MisplacedNumberMarker,
                        child.span(),
                        format!(
                            "\\{} must directly follow its \\{c} (at most once per {unit}); \
                             it is kept as character text",
                            child.marker_name()
                        ),
                    ));
                }
                if info.name == "fig" && figure_is_empty(child) {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::EmptyFigure,
                        child.span(),
                        "\\fig has no caption content",
                    ));
                }
                if info.name == "w" && word_marker_is_empty(child) {
                    diags.push(Diagnostic::error(
                        DiagnosticCode::EmptyWordMarker,
                        child.span(),
                        "\\w has no text content or attributes",
                    ));
                }
                check_required_attributes(child, info.name, diags);
                if info.name == "tr" {
                    validate_table_row(child, diags);
                }
                validate_attributes(child, child.marker_name(), diags);
                if info.category == Category::Milestone
                    && let Some(base) = milestone_base(child.marker_name())
                {
                    if child.marker_name().ends_with("-s") {
                        st.open_milestones.push((base.to_string(), child.span()));
                    } else if child.marker_name().ends_with("-e") {
                        match st.open_milestones.iter().rposition(|(b, _)| b == base) {
                            Some(i) => {
                                st.open_milestones.remove(i);
                            }
                            None => diags.push(Diagnostic::error(
                                DiagnosticCode::MilestoneMismatch,
                                child.span(),
                                format!("milestone \\{base}-e has no matching \\{base}-s"),
                            )),
                        }
                    }
                }
                let body = in_body_para
                    || (info.category == Category::Paragraph
                        && info.text_role == TextRole::Boundary);
                walk(
                    child,
                    body,
                    in_note || info.category == Category::Note,
                    in_char || info.category == Category::Character,
                    st,
                    diags,
                );
            }
            _ => {}
        }
    }
}

/// Whether `number` is one of the alternate/published numbers the serializers
/// fold into the chapter or verse `anchor`.
fn is_attached(anchor: Cursor<'_>, number: Cursor<'_>) -> bool {
    let at = number.span().start;
    attached_numbers(anchor).any(|c| c.span().start == at)
}

/// A chapter/verse number as written.
struct NumberLabel<'a> {
    label: &'a str,
    /// Glued to the marker (`\v10`, `\c2`): the parser accepts it, but the spec
    /// requires the space.
    glued: bool,
}

/// Read the number glued to the marker if there is one, or else the token after
/// it. A `\c` line can carry a title after the number, so the marker name is
/// checked first.
fn number_label<'a>(c: Cursor<'a>) -> NumberLabel<'a> {
    let name = c.marker_name();
    let base = c.marker_info().map_or(name.len(), |m| m.name.len());
    if name.len() > base {
        return NumberLabel {
            label: &name[base..],
            glued: true,
        };
    }
    NumberLabel {
        label: c.text().split_whitespace().nth(1).unwrap_or(""),
        glued: false,
    }
}

/// Warn about a number glued to its marker, and about leading zeros.
fn check_number_form(c: Cursor<'_>, n: &NumberLabel<'_>, kind: &str, diags: &mut Vec<Diagnostic>) {
    let label = n.label;
    if n.glued {
        let name = c.marker_name();
        let base = &name[..name.len() - label.len()];
        diags.push(Diagnostic::warning(
            DiagnosticCode::MissingSpaceBeforeNumber,
            c.span(),
            format!("\\{name} needs a space before its number; read as \\{base} {label}"),
        ));
    }
    if label.len() > 1 && label.starts_with('0') {
        // USFM chapter/verse numbers must not carry leading zeros (e.g. `\c 091`).
        diags.push(Diagnostic::error(
            DiagnosticCode::LeadingZeros,
            c.span(),
            format!("{kind} number `{label}` has leading zeros"),
        ));
    }
}

/// The book code = first whitespace token of the `\id` line's text.
fn book_code<'a>(id: Cursor<'a>) -> Option<&'a str> {
    id.children()
        .find(|g| g.kind() == NodeKind::Text)
        .and_then(|g| g.text().split_whitespace().next())
}

/// A figure with no caption text (attributes don't count as content).
fn figure_is_empty(fig: Cursor<'_>) -> bool {
    !fig.children()
        .any(|c| c.kind() == NodeKind::Text && !c.text().trim().is_empty())
}

/// Whether a `\b` (blank line) carries genuine inline content (text, a character,
/// a figure, a note). A `\v`/`\c` that lands inside `\b` is a structural boundary
/// recovery — the blank line precedes the verse — not blank-line content, so it
/// does NOT count (this is how a valid `\b` before a poetry verse stays clean
/// while `\b \fig…` is correctly flagged).
fn blank_line_has_inline_content(b: Cursor<'_>) -> bool {
    for child in b.children() {
        match child.kind() {
            NodeKind::Text if child.text().trim().is_empty() => continue,
            NodeKind::Verse | NodeKind::Chapter => return false,
            NodeKind::Text | NodeKind::MarkerOpen => return true,
            _ => continue,
        }
    }
    false
}

/// A `\w …\w*` with neither text content nor any attribute is meaningless.
fn word_marker_is_empty(w: Cursor<'_>) -> bool {
    !w.children().any(|c| {
        (c.kind() == NodeKind::Text && !c.text().trim().is_empty())
            || c.kind() == NodeKind::Attribute
            || c.kind() == NodeKind::MarkerOpen
    })
}

/// Attributes a marker must carry (named, or supplied as the `|default`).
fn required_attributes(marker: &str) -> &'static [&'static str] {
    match marker {
        "rb" => &["gloss"], // ruby glosses require the gloss text
        _ => &[],
    }
}

/// Error if a marker is missing a required attribute. The attribute may be given
/// by name (`|gloss="…"`) or as the bare default (`|…`), both of which surface as
/// an `Attribute` child.
fn check_required_attributes(c: Cursor<'_>, marker: &str, diags: &mut Vec<Diagnostic>) {
    let req = required_attributes(marker);
    if req.is_empty() {
        return;
    }
    let has_any_attr = c.children().any(|ch| ch.kind() == NodeKind::Attribute);
    for &attr in req {
        if !has_any_attr {
            diags.push(Diagnostic::error(
                DiagnosticCode::MissingRequiredAttribute,
                c.span(),
                format!("\\{marker} is missing its required \"{attr}\" attribute"),
            ));
        }
    }
}

/// Validate the column sequence of a table row: cells run `1, 2, 3, …`, with a
/// span (`\tcr1-2`) advancing past its end column. A gap or reorder is an error.
fn validate_table_row(tr: Cursor<'_>, diags: &mut Vec<Diagnostic>) {
    let mut expected = 1u32;
    for cell in tr.children() {
        if cell.kind() != NodeKind::MarkerOpen {
            continue;
        }
        let Some((start, end)) = table_cell_columns(cell.marker_name()) else {
            continue;
        };
        if start != expected {
            diags.push(Diagnostic::error(
                DiagnosticCode::InvalidTableColumnSequence,
                cell.span(),
                format!("expected table column {expected} but found column {start}"),
            ));
        }
        expected = end + 1;
    }
}

/// Parse a table-cell column / span from its marker name: `tc1`→(1,1),
/// `tcr1-2`→(1,2), `th3`→(3,3). `None` if it isn't a numbered table cell.
fn table_cell_columns(marker: &str) -> Option<(u32, u32)> {
    let base = marker
        .strip_prefix("thr")
        .or_else(|| marker.strip_prefix("thc"))
        .or_else(|| marker.strip_prefix("th"))
        .or_else(|| marker.strip_prefix("tcr"))
        .or_else(|| marker.strip_prefix("tcc"))
        .or_else(|| marker.strip_prefix("tc"))?;
    if base.is_empty() {
        return None;
    }
    let (start, end) = match base.split_once('-') {
        Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
        None => {
            let n = base.parse().ok()?;
            (n, n)
        }
    };
    Some((start, end))
}

/// Validate a marker's `|…` attribute syntax. Errors: empty list, a value-less
/// key among pairs, an unquoted value, a bare default on a marker that defines no
/// default attribute. An *empty quoted* value (`lemma=""`) is well-formed per the
/// spec — the parser keeps it; we only flag it as info.
fn validate_attributes(c: Cursor<'_>, marker: &str, diags: &mut Vec<Diagnostic>) {
    for ch in c.children() {
        if ch.kind() != NodeKind::Attribute {
            continue;
        }
        let span = ch.span();
        let raw = ch.text();
        let body = raw.strip_prefix('|').unwrap_or(raw).trim();
        if body.is_empty() {
            diags.push(Diagnostic::error(
                DiagnosticCode::InvalidAttributes,
                span,
                "empty attribute list after `|`",
            ));
            continue;
        }
        // A single bare token (no `=`) is the marker's default attribute — valid
        // only if the marker actually defines one (e.g. `\w word|lemma`, not
        // `\+pn name|keyword`).
        if !body.contains('=') {
            if default_attribute(marker).is_none() {
                diags.push(Diagnostic::error(
                    DiagnosticCode::DefaultAttributeNotDefined,
                    span,
                    format!("\\{marker} has no default attribute; the value after `|` is invalid"),
                ));
            }
            continue;
        }
        let mut rest = body;
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            let stop = rest
                .find(|c: char| c == '=' || c.is_whitespace())
                .unwrap_or(rest.len());
            if stop == rest.len() || rest.as_bytes()[stop] != b'=' {
                diags.push(Diagnostic::error(
                    DiagnosticCode::InvalidAttributes,
                    span,
                    "attribute key without a `=\"value\"`",
                ));
                break;
            }
            rest = &rest[stop + 1..]; // immediately after `=`
            if let Some(after) = rest.strip_prefix('"') {
                // Find the closing quote, honoring `\"` as an escaped literal.
                let b = after.as_bytes();
                let mut k = 0;
                let mut close = None;
                while k < b.len() {
                    match b[k] {
                        b'\\' if k + 1 < b.len() && b[k + 1] == b'"' => k += 2,
                        b'"' => {
                            close = Some(k);
                            break;
                        }
                        _ => k += 1,
                    }
                }
                match close {
                    Some(q) => {
                        if after[..q].is_empty() {
                            // The spec (ubsicap word-level attributes) doesn't
                            // forbid an empty quoted value, so this is advisory
                            // only — the parser keeps `lemma=""` as an attribute.
                            diags.push(Diagnostic::info(
                                DiagnosticCode::InvalidAttributes,
                                span,
                                "attribute has an empty value (e.g. `lemma=\"\"`)",
                            ));
                        }
                        rest = &after[q + 1..];
                    }
                    None => {
                        diags.push(Diagnostic::error(
                            DiagnosticCode::InvalidAttributes,
                            span,
                            "unterminated quoted attribute value",
                        ));
                        break;
                    }
                }
            } else {
                diags.push(Diagnostic::error(
                    DiagnosticCode::UnquotedAttributeValue,
                    span,
                    "attribute value must be quoted",
                ));
                // Advance past the unquoted token to keep scanning.
                let adv = rest.find(char::is_whitespace).unwrap_or(rest.len());
                rest = &rest[adv..];
            }
        }
    }
}

/// The paired-milestone base: `qt-s`→`qt`, `zaln-e`→`zaln`. `None` for markers
/// that aren't start/end milestones (e.g. `\ts`).
fn milestone_base(name: &str) -> Option<&str> {
    name.strip_suffix("-s").or_else(|| name.strip_suffix("-e"))
}

/// Front-matter / peripheral books that carry no chapters (so `\c` is not required
/// and body paragraphs may appear without one).
fn is_front_matter_code(code: &str) -> bool {
    matches!(
        code,
        "FRT" | "INT" | "BAK" | "OTH" | "CNC" | "GLO" | "TDX" | "NDX"
    )
}

/// Note-internal markers that should only appear inside a `\f…\f*` / `\x…\x*`.
fn is_note_only_marker(marker: &str) -> bool {
    matches!(
        marker,
        "fr" | "ft"
            | "fk"
            | "fq"
            | "fqa"
            | "fl"
            | "fw"
            | "fp"
            | "fv"
            | "fdc"
            | "xop"
            | "xot"
            | "xnt"
            | "xdc"
    )
}

// --- verse-number sequencing ----------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct VersePart {
    num: u32,
    suffix: Option<char>,
}

impl VersePart {
    fn next_expected(self) -> Self {
        match self.suffix {
            None => Self {
                num: self.num + 1,
                suffix: None,
            },
            Some(c) => Self {
                num: self.num,
                suffix: Some((c as u8 + 1) as char),
            },
        }
    }
}

impl std::fmt::Display for VersePart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.suffix {
            None => write!(f, "{}", self.num),
            Some(c) => write!(f, "{}{}", self.num, c),
        }
    }
}

fn parse_verse_part(s: &str) -> Option<VersePart> {
    let s = s.trim_start();
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let digits = &s[..end];
    if digits.is_empty() {
        return None;
    }
    let num = digits.parse().ok()?;
    let suffix = s[end..].chars().next().filter(|c| c.is_ascii_lowercase());
    Some(VersePart { num, suffix })
}

fn parse_verse_range_end(s: &str) -> Option<VersePart> {
    match s.find('-') {
        Some(pos) => parse_verse_part(&s[pos + 1..]),
        None => parse_verse_part(s),
    }
}

fn is_valid_verse_sequence(expected: VersePart, actual: VersePart) -> bool {
    if actual.num != expected.num {
        // After `9a`, `10` may follow; after `9`, a lettered `9a` may (an added
        // segment, e.g. LXX Job 2:9a).
        (expected.suffix.is_some() && actual.num == expected.num + 1)
            || (expected.suffix.is_none()
                && actual.suffix.is_some()
                && actual.num >= 1
                && actual.num + 1 == expected.num)
    } else {
        actual.suffix >= expected.suffix
    }
}
