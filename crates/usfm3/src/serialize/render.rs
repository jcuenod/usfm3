//! Shared rendering helpers used by more than one output format: JSON string
//! escaping, `|key="value"` attribute parsing, note-caller extraction, and the
//! docs.usfm.bible whitespace rules. Format-specific helpers (XML escaping, table
//! grouping, sid construction, …) live in the per-format modules.

use std::borrow::Cow;
use std::fmt::{self, Write};

use crate::cst::Cursor;
use crate::marker::default_attribute;
use crate::tape::NodeKind;

// --- JSON ------------------------------------------------------------------

pub(crate) fn escape_json<W: Write>(s: &str, out: &mut W) -> fmt::Result {
    // Every byte that needs escaping is ASCII, so copy the runs between them whole.
    let mut start = 0;
    for (i, &b) in s.as_bytes().iter().enumerate() {
        let esc = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x00..=0x1f => {
                out.write_str(&s[start..i])?;
                write!(out, "\\u{b:04x}")?;
                start = i + 1;
                continue;
            }
            _ => continue,
        };
        out.write_str(&s[start..i])?;
        out.write_str(esc)?;
        start = i + 1;
    }
    out.write_str(&s[start..])
}

pub(crate) fn json_str<W: Write>(s: &str, out: &mut W) -> fmt::Result {
    out.write_char('"')?;
    escape_json(s, out)?;
    out.write_char('"')
}

// --- whitespace ------------------------------------------------------------

/// Extract the verse or chapter number label from a cursor node.
///
/// Normally the number follows the marker with a space (`\v 11` → span text is
/// `"\v 11"`, second whitespace token is `"11"`). Malformed USFM sometimes omits
/// the space (`\v11`), embedding the number in the marker name itself. In that
/// case the span text is just `"\v11"` with no second token, so we fall back to
/// the suffix of the marker name after the base letters (e.g. `"v11"[1..]` = `"11"`).
pub(crate) fn verse_label<'a>(c: Cursor<'a>) -> &'a str {
    let text = c.text();
    if let Some(label) = text.split_whitespace().nth(1) {
        return label;
    }
    // Fallback: number embedded in marker name (malformed \v11 or \c2 with no space)
    let name = c.marker_name();
    let base_len = c.marker_info().map_or(0, |m| m.name.len());
    if base_len < name.len() {
        &name[base_len..]
    } else {
        ""
    }
}

/// Per docs.usfm.bible: the single space separating a marker (or verse/chapter
/// number) from the text that follows is not content, so it is dropped. After a
/// *closing* marker the space is a real word separator and is kept. `prev` is the
/// previous emitted sibling (`None` = right after the parent's opening marker).
pub(crate) fn trim_leading(prev: Option<NodeKind>) -> bool {
    matches!(prev, None | Some(NodeKind::Chapter) | Some(NodeKind::Verse))
}

/// Collapse internal whitespace runs to a single space (docs.usfm.bible). A
/// leading space is dropped when it is a marker separator (`trim_leading`); a
/// trailing space is preserved as a word separator before the next sibling.
/// Only ASCII whitespace is collapsible: a no-break or ideographic space in the
/// source is text. The no-break space `~` (U+00A0 per spec) is substituted after
/// the collapse.
pub(crate) fn normalize_text(s: &str, trim_leading: bool) -> String {
    let mut words = s.split_ascii_whitespace();
    let Some(first) = words.next() else {
        return String::new();
    };
    // Only a literal space (not \r or \n) is a meaningful word-separator; CRLF
    // files would otherwise get a spurious trailing space from the \r.
    let lead = !trim_leading && s.trim_start_matches(['\r', '\n']).starts_with(' ');
    let trail = s.trim_end_matches(['\r', '\n']).ends_with(' ');
    // One allocation: collapse straight into the output.
    let mut out = String::with_capacity(s.len() + 2);
    if lead {
        out.push(' ');
    }
    out.push_str(first);
    for word in words {
        out.push(' ');
        out.push_str(word);
    }
    if trail {
        out.push(' ');
    }
    if out.contains('~') {
        out = out.replace('~', "\u{a0}");
    }
    out
}

/// Collapse every ASCII whitespace run to a single space, with no leading/trailing
/// space. Non-ASCII spaces (U+00A0, U+3000, …) are text and are kept.
pub(crate) fn collapse_all(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split_ascii_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

// --- notes -----------------------------------------------------------------

/// The note caller — the first whitespace-delimited token after the note marker
/// (`+`, `-`, or a custom character). Defaults to `+`.
pub(crate) fn note_caller(c: Cursor<'_>) -> &str {
    c.children()
        .next()
        .filter(|ch| ch.kind() == NodeKind::Text)
        .and_then(|ch| ch.text().split_whitespace().next())
        .unwrap_or("+")
}

/// The text after the first whitespace-delimited token (used to drop a note's
/// caller token while keeping the rest of its leading text).
pub(crate) fn remainder_after_first_token(t: &str) -> &str {
    let t = t.trim_start();
    match t.find(char::is_whitespace) {
        Some(i) => &t[i..],
        None => "",
    }
}

// --- attributes ------------------------------------------------------------

/// Parse the `|key="value" …` (or bare default) attribute child(ren) into
/// `(key, value)` pairs, applying USJ key remapping (e.g. figure `src`→`file`).
/// Values are usually borrowed; an escaped quote (`\"`) yields an owned, unescaped
/// string.
pub(crate) fn collect_attributes<'a>(c: Cursor<'a>, marker: &str) -> Vec<(&'a str, Cow<'a, str>)> {
    let mut pairs: Vec<(&'a str, Cow<'a, str>)> = Vec::new();
    for ch in c.children() {
        if ch.kind() != NodeKind::Attribute {
            continue;
        }
        let body = ch.text().strip_prefix('|').unwrap_or(ch.text()).trim();
        if body.is_empty() {
            continue;
        }
        if !body.contains('=') {
            // The parser keeps a meaningless bare value as text; one that still
            // arrives here (inside an escaped-quote region) keeps 0.2.1's name.
            let key = default_attribute(marker).unwrap_or("default");
            pairs.push((key, Cow::Borrowed(body)));
            continue;
        }
        let mut rest = body;
        while !rest.trim_start().is_empty() {
            rest = rest.trim_start();
            let Some(eq) = rest.find('=') else { break };
            // Lenient on malformed input: `lemma strong="l"` has a dangling bare
            // word before the real key. Keep the last word, so the name stays a
            // valid XML attribute / sensible JSON key; validation reports the rest.
            let key = rest[..eq].split_whitespace().last().unwrap_or("");
            rest = rest[eq + 1..].trim_start();
            let value: Cow<'a, str> = if let Some(stripped) = rest.strip_prefix('"') {
                // Closing quote, honoring `\"` as an escaped literal quote.
                let raw = stripped.as_bytes();
                let mut k = 0;
                while k < raw.len() {
                    match raw[k] {
                        b'\\' if k + 1 < raw.len() && raw[k + 1] == b'"' => k += 2,
                        b'"' => break,
                        _ => k += 1,
                    }
                }
                let v = &stripped[..k];
                rest = &stripped[(k + 1).min(stripped.len())..];
                if v.contains("\\\"") {
                    Cow::Owned(v.replace("\\\"", "\""))
                } else {
                    Cow::Borrowed(v)
                }
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let v = &rest[..end];
                rest = &rest[end..];
                Cow::Borrowed(v)
            };
            if !key.is_empty() {
                pairs.push((attr_key(marker, key), value));
            }
        }
    }
    pairs
}

/// Map a USFM attribute name to its USJ key. Figures store `src` as `file`.
pub(crate) fn attr_key<'a>(marker: &str, key: &'a str) -> &'a str {
    match (marker, key) {
        ("fig", "src") => "file",
        _ => key,
    }
}

// --- structure shared by USJ and USX ---------------------------------------

/// A `\tr` row; consecutive rows group into one table.
pub(crate) fn is_table_row(c: Cursor<'_>) -> bool {
    c.kind() == NodeKind::MarkerOpen && c.marker_info().map(|m| m.name) == Some("tr")
}

/// Cell alignment from the marker prefix: `thr/tcr`→end, `thc/tcc`→center, else start.
pub(crate) fn cell_align(marker: &str) -> &'static str {
    if marker.starts_with("thr") || marker.starts_with("tcr") {
        "end"
    } else if marker.starts_with("thc") || marker.starts_with("tcc") {
        "center"
    } else {
        "start"
    }
}

/// Whether a `\ca`/`\va` (alternate) or `\cp`/`\vp` (published) number.
pub(crate) enum AltPub {
    Alt,
    Pub,
}

/// Classify a node as an alternate (`\ca`/`\va`) or published (`\cp`/`\vp`) number.
pub(crate) fn alt_pub_kind(c: Cursor<'_>) -> Option<AltPub> {
    if c.kind() != NodeKind::MarkerOpen {
        return None;
    }
    match c.marker_name() {
        "ca" | "va" => Some(AltPub::Alt),
        "cp" | "vp" => Some(AltPub::Pub),
        _ => None,
    }
}

/// Whether a marker's content is plain text (no nested marker children).
pub(crate) fn is_plain_text(c: Cursor<'_>) -> bool {
    !c.children().any(|ch| ch.kind() == NodeKind::MarkerOpen)
}

/// The whitespace-collapsed concatenation of a marker's direct text children.
pub(crate) fn marker_inner_text(c: Cursor<'_>) -> String {
    let mut s = String::new();
    for ch in c.children() {
        if ch.kind() == NodeKind::Text {
            s.push_str(ch.text());
        }
    }
    collapse_all(&s)
}

/// A `\cat` marker, folded into its note's or sidebar's `category`.
pub(crate) fn is_category_marker(c: Cursor<'_>) -> bool {
    c.kind() == NodeKind::MarkerOpen && c.marker_name() == "cat"
}

/// The text of a `\cat …\cat*` child, if present (note/sidebar category).
pub(crate) fn category_of<'a>(parent: Cursor<'a>) -> Option<&'a str> {
    parent
        .children()
        .find(|c| is_category_marker(*c))
        .and_then(|cat| {
            cat.children()
                .find(|g| g.kind() == NodeKind::Text)
                .map(|g| g.text())
        })
}

/// How the parser shaped a marker that isn't in the table (see `parse_marker`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unknown {
    /// Not explicitly closed: a paragraph, ended by the next block.
    Block,
    /// Closed by its own `\tag*`: a char.
    Inline,
    /// Inside a note or char: just the marker; its text follows as siblings.
    Leaf,
}

/// The shape of an unknown marker, or `None` for anything else.
pub(crate) fn unknown_shape(c: Cursor<'_>) -> Option<Unknown> {
    if c.kind() != NodeKind::MarkerOpen || c.marker_info().is_some() {
        return None;
    }
    Some(match c.matching_close().map(|close| close.kind()) {
        Some(NodeKind::BlockEnd) => Unknown::Block,
        Some(NodeKind::MarkerClose) => Unknown::Inline,
        _ => Unknown::Leaf,
    })
}

// --- deprecated figure syntax ----------------------------------------------

/// The `|…` attribute child of a figure when it uses the *deprecated* USFM 2.x
/// positional form (`\fig DESC|FILE|SIZE|LOC|COPY|CAP|REF\fig*` — a bar-separated
/// field list with no `key="value"` pairs). Returns the raw attribute text (the
/// leading `|` included) for verbatim round-tripping; `None` for the modern form,
/// which [`collect_attributes`] handles. Spec: the deprecated illustration syntax
/// at <https://ubsicap.github.io/usfm/characters/index.html>.
pub(crate) fn figure_legacy_raw<'a>(c: Cursor<'a>) -> Option<&'a str> {
    let attr = c.children().find(|ch| ch.kind() == NodeKind::Attribute)?;
    let body = attr.text().strip_prefix('|').unwrap_or(attr.text());
    (!body.contains('=')).then_some(attr.text())
}

/// Resolve a deprecated positional figure into its USFM 3.0 named attributes plus
/// caption. Per spec the deprecated form is `\fig DESC|FILE|SIZE|LOC|COPY|CAP|REF`,
/// mapping to `alt=DESC, src/file=FILE, size=SIZE, loc=LOC, copy=COPY, ref=REF`
/// with CAP as the figure's caption (its element content). The leading text (before
/// the first `|`) is DESC; the remaining `|`-separated fields follow it. Empty
/// fields are dropped. Returns `None` for the modern `key="value"` form.
///
/// This is preferred over emitting the raw field list as a single default attribute
/// (which would mislabel it `lemma`) or as a non-standard `attributes` array — the
/// spec defines an exact positional→named mapping, so we apply it.
pub(crate) fn figure_legacy<'a>(c: Cursor<'a>) -> Option<(Vec<(&'static str, &'a str)>, &'a str)> {
    let raw = figure_legacy_raw(c)?;
    let body = raw.strip_prefix('|').unwrap_or(raw);
    let desc = c
        .children()
        .find(|ch| ch.kind() == NodeKind::Text)
        .map(|t| t.text().trim())
        .unwrap_or("");
    let mut f = body.split('|');
    let file = f.next().unwrap_or("").trim();
    let size = f.next().unwrap_or("").trim();
    let loc = f.next().unwrap_or("").trim();
    let copy = f.next().unwrap_or("").trim();
    let cap = f.next().unwrap_or("").trim();
    let refr = f.next().unwrap_or("").trim();
    let mut attrs = Vec::new();
    for (k, v) in [
        ("alt", desc),
        ("file", file),
        ("size", size),
        ("loc", loc),
        ("copy", copy),
        ("ref", refr),
    ] {
        if !v.is_empty() {
            attrs.push((k, v));
        }
    }
    Some((attrs, cap))
}
