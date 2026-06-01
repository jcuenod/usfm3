//! Serializers: USX (XML) and USJ (JSON). Both are pure walks over the [`Cursor`]
//! tree — no intermediate model — writing into any `fmt::Write`, so callers can
//! stream straight into a buffer, file, or FFI string without a second copy.
//!
//! TODO(serialize): the walks below emit text content and structural scaffolding
//! so the API and tests are exercisable end-to-end. Full element/attribute mapping
//! (driven by `MarkerInfo::usx_element` and the USJ `type` table) lands with the
//! real parser, validated against the official USX/USJ sample corpus via golden
//! files.

use std::fmt::{self, Write};

use crate::cst::Cursor;
use crate::diagnostic::Diagnostic;
use crate::marker::Category;
use crate::tape::NodeKind;

pub fn write_usx<W: Write>(root: Cursor<'_>, out: &mut W) -> fmt::Result {
    write!(out, "<usx version=\"3.1\">")?;
    write_usx_children(root, out)?;
    write!(out, "</usx>")
}

fn write_usx_children<W: Write>(node: Cursor<'_>, out: &mut W) -> fmt::Result {
    for child in node.children() {
        match child.kind() {
            NodeKind::Text => escape_xml(child.text(), out)?,
            // TODO: map MarkerOpen -> <para>/<char>/<note> using marker_info().
            _ => write_usx_children(child, out)?,
        }
    }
    Ok(())
}

// --- USJ (https://docs.usfm.bible/usfm/3.1/usj.html) ------------------------
//
// A walk over the [`Cursor`] tree producing the USJ content model: text nodes are
// bare strings; markers become `{type,marker,…,content}` objects; char/milestone
// attributes inline as object keys; chapters/verses carry a computed `sid`. We do
// NOT canonicalize whitespace here beyond what the tape already did — per project
// policy, round-tripped whitespace is not authoritative.

/// Running context for `sid` construction.
#[derive(Default)]
struct Ctx {
    book: String,    // current book code, e.g. "GEN"
    chapter: String, // current chapter number, e.g. "1"
}

pub fn write_usj<W: Write>(root: Cursor<'_>, out: &mut W) -> fmt::Result {
    let mut ctx = Ctx::default();
    out.write_str("{\"type\":\"USJ\",\"version\":\"3.1\",\"content\":")?;
    write_content(root, out, &mut ctx)?;
    out.write_char('}')
}

/// Write the `[ … ]` USJ array of a parent's children, tracking the previous
/// emitted sibling so text whitespace can be normalized in context.
fn write_content<W: Write>(parent: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_char('[')?;
    let mut first = true;
    let mut prev: Option<NodeKind> = None;
    for child in parent.children() {
        if emit_node(child, out, ctx, &mut first, trim_leading(prev))? {
            prev = Some(child.kind());
        }
    }
    out.write_char(']')
}

/// Per docs.usfm.bible: the single space separating a marker (or verse/chapter
/// number) from the text that follows is not content, so it is dropped. After a
/// *closing* marker the space is a real word separator and is kept. `prev` is the
/// previous emitted sibling (`None` = right after the parent's opening marker).
fn trim_leading(prev: Option<NodeKind>) -> bool {
    matches!(prev, None | Some(NodeKind::Chapter) | Some(NodeKind::Verse))
}

fn sep<W: Write>(out: &mut W, first: &mut bool) -> fmt::Result {
    if !*first {
        out.write_char(',')?;
    }
    *first = false;
    Ok(())
}

/// Emit one child as a USJ value. Returns `true` if a value was written (so the
/// caller can advance its "previous sibling" tracking).
fn emit_node<W: Write>(
    c: Cursor<'_>,
    out: &mut W,
    ctx: &mut Ctx,
    first: &mut bool,
    trim_leading: bool,
) -> Result<bool, fmt::Error> {
    match c.kind() {
        NodeKind::Text => {
            let t = normalize_text(c.text(), trim_leading);
            if t.is_empty() {
                return Ok(false);
            }
            sep(out, first)?;
            json_str(&t, out)?;
        }
        NodeKind::Chapter => {
            sep(out, first)?;
            let n = c.number().unwrap_or(0).to_string();
            ctx.chapter = n.clone();
            out.write_str("{\"type\":\"chapter\",\"marker\":\"c\",\"number\":")?;
            json_str(&n, out)?;
            out.write_str(",\"sid\":")?;
            json_str(&format!("{} {}", ctx.book, n), out)?;
            out.write_char('}')?;
        }
        NodeKind::Verse => {
            sep(out, first)?;
            let label = c.text().split_whitespace().nth(1).unwrap_or("");
            out.write_str("{\"type\":\"verse\",\"marker\":\"v\",\"number\":")?;
            json_str(label, out)?;
            out.write_str(",\"sid\":")?;
            json_str(&format!("{} {}:{}", ctx.book, ctx.chapter, label), out)?;
            out.write_char('}')?;
        }
        NodeKind::MarkerOpen => {
            let Some(info) = c.marker_info() else { return Ok(false) };
            // `\usfm` (version declaration) is metadata, not USJ content.
            if info.name == "usfm" {
                return Ok(false);
            }
            sep(out, first)?;
            match info.category {
                _ if info.name == "fig" => write_figure(c, out, ctx)?,
                Category::Identification if info.name == "id" => write_book(c, out, ctx)?,
                Category::Note => write_note(c, out, ctx)?,
                Category::Milestone => write_ms(c, info.name, out)?,
                Category::Character => write_char(c, info.name, out, ctx)?,
                _ => write_para(c, info.name, out, ctx)?,
            }
        }
        // Attribute / MarkerClose / BlockEnd / Error / Root — not USJ nodes.
        _ => return Ok(false),
    }
    Ok(true)
}

/// Collapse internal whitespace runs to a single space (docs.usfm.bible). A
/// leading space is dropped when it is a marker separator (`trim_leading`); a
/// trailing space is preserved as a word separator before the next sibling.
fn normalize_text(s: &str, trim_leading: bool) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return String::new();
    }
    let lead = !trim_leading && s.starts_with(char::is_whitespace);
    let trail = s.ends_with(char::is_whitespace);
    let mut out = String::with_capacity(collapsed.len() + 2);
    if lead {
        out.push(' ');
    }
    out.push_str(&collapsed);
    if trail {
        out.push(' ');
    }
    out
}

fn write_para<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"para\",\"marker\":")?;
    json_str(marker, out)?;
    out.write_str(",\"content\":")?;
    write_content(c, out, ctx)?;
    out.write_char('}')
}

fn write_char<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"char\",\"marker\":")?;
    json_str(marker, out)?;
    write_attributes(c, marker, out)?;
    out.write_str(",\"content\":")?;
    write_content(c, out, ctx)?; // Attribute children are skipped by emit_node
    out.write_char('}')
}

fn write_ms<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W) -> fmt::Result {
    out.write_str("{\"type\":\"ms\",\"marker\":")?;
    json_str(marker, out)?;
    write_attributes(c, marker, out)?;
    out.write_char('}')
}

fn write_figure<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"figure\",\"marker\":\"fig\"")?;
    write_attributes(c, "fig", out)?;
    out.write_str(",\"content\":")?;
    write_content(c, out, ctx)?;
    out.write_char('}')
}

fn write_book<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    let raw = c.children().find(|ch| ch.kind() == NodeKind::Text).map(|ch| ch.text()).unwrap_or("");
    let mut tokens = raw.split_whitespace();
    let code = tokens.next().unwrap_or("");
    ctx.book = code.to_string();
    out.write_str("{\"type\":\"book\",\"marker\":\"id\",\"code\":")?;
    json_str(code, out)?;
    out.write_str(",\"content\":")?;
    let rest = tokens.collect::<Vec<_>>().join(" ");
    if rest.is_empty() {
        out.write_str("[]")?;
    } else {
        out.write_char('[')?;
        json_str(&rest, out)?;
        out.write_char(']')?;
    }
    out.write_char('}')
}

fn write_note<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    let info = c.marker_info().expect("note has marker info");
    // The caller is the first whitespace-delimited token after the note marker.
    let caller = c
        .children()
        .next()
        .filter(|ch| ch.kind() == NodeKind::Text)
        .and_then(|ch| ch.text().split_whitespace().next())
        .unwrap_or("+");

    out.write_str("{\"type\":\"note\",\"marker\":")?;
    json_str(info.name, out)?;
    out.write_str(",\"caller\":")?;
    json_str(caller, out)?;
    out.write_str(",\"content\":[")?;
    let mut first = true;
    let mut prev: Option<NodeKind> = None;
    for (i, ch) in c.children().enumerate() {
        if i == 0 && ch.kind() == NodeKind::Text {
            // Drop the caller token; keep any text that follows it on the line.
            let rem = remainder_after_first_token(ch.text());
            if !rem.trim().is_empty() {
                sep(out, &mut first)?;
                json_str(rem.trim(), out)?;
                prev = Some(NodeKind::Text);
            }
        } else if emit_node(ch, out, ctx, &mut first, trim_leading(prev))? {
            prev = Some(ch.kind());
        }
    }
    out.write_str("]}")
}

fn remainder_after_first_token(t: &str) -> &str {
    let t = t.trim_start();
    match t.find(char::is_whitespace) {
        Some(i) => &t[i..],
        None => "",
    }
}

/// Emit `,"key":"value"` pairs parsed from this node's `|…` attribute child(ren).
fn write_attributes<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W) -> fmt::Result {
    for ch in c.children() {
        if ch.kind() != NodeKind::Attribute {
            continue;
        }
        let raw = ch.text();
        let body = raw.strip_prefix('|').unwrap_or(raw).trim();
        if body.is_empty() {
            continue;
        }
        if !body.contains('=') {
            // A bare value is the marker's default attribute.
            out.write_char(',')?;
            json_str(default_attribute(marker), out)?;
            out.write_char(':')?;
            json_str(body, out)?;
            continue;
        }
        let mut rest = body;
        while !rest.trim_start().is_empty() {
            rest = rest.trim_start();
            let Some(eq) = rest.find('=') else { break };
            let key = rest[..eq].trim();
            rest = rest[eq + 1..].trim_start();
            let value = if let Some(stripped) = rest.strip_prefix('"') {
                let end = stripped.find('"').unwrap_or(stripped.len());
                let v = &stripped[..end];
                rest = &stripped[(end + 1).min(stripped.len())..];
                v
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let v = &rest[..end];
                rest = &rest[end..];
                v
            };
            out.write_char(',')?;
            json_str(attr_key(marker, key), out)?;
            out.write_char(':')?;
            json_str(value, out)?;
        }
    }
    Ok(())
}

/// The default (unnamed) attribute name for a marker, per USFM 3.x.
fn default_attribute(marker: &str) -> &'static str {
    match marker {
        "w" | "wa" | "wg" | "wh" => "lemma",
        "rb" => "gloss",
        m if m.starts_with("qt") => "who", // \qt-s |Speaker\*
        _ => "lemma",
    }
}

/// Map a USFM attribute name to its USJ key. Figures store `src` as `file`.
fn attr_key<'a>(marker: &str, key: &'a str) -> &'a str {
    match (marker, key) {
        ("fig", "src") => "file",
        _ => key,
    }
}

fn escape_xml<W: Write>(s: &str, out: &mut W) -> fmt::Result {
    for ch in s.chars() {
        match ch {
            '<' => out.write_str("&lt;")?,
            '>' => out.write_str("&gt;")?,
            '&' => out.write_str("&amp;")?,
            _ => out.write_char(ch)?,
        }
    }
    Ok(())
}

fn escape_json<W: Write>(s: &str, out: &mut W) -> fmt::Result {
    for ch in s.chars() {
        match ch {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32)?,
            c => out.write_char(c)?,
        }
    }
    Ok(())
}

fn json_str<W: Write>(s: &str, out: &mut W) -> fmt::Result {
    out.write_char('"')?;
    escape_json(s, out)?;
    out.write_char('"')
}

// --- CST (for editor decoration) -------------------------------------------

/// Serialize the concrete syntax tree as JSON, mirroring the tape's tree. Each
/// node carries its byte `span` (UTF-8 offsets into the source), and marker nodes
/// carry `marker`/`category`/`role`/`closing` plus the `close` token span — i.e.
/// everything an editor needs to colorize tags and fold structure. Synthetic
/// nodes (implicit block ends) are flagged so a decorator can ignore them.
pub fn write_cst_json<W: Write>(root: Cursor<'_>, out: &mut W) -> fmt::Result {
    write_cst_node(root, out)
}

fn write_cst_node<W: Write>(c: Cursor<'_>, out: &mut W) -> fmt::Result {
    write!(out, "{{\"type\":\"{}\"", c.kind().as_str())?;

    if let Some(info) = c.marker_info() {
        out.write_str(",\"marker\":")?;
        json_str(info.name, out)?;
        write!(
            out,
            ",\"category\":\"{}\",\"role\":\"{}\",\"closing\":\"{}\"",
            info.category.as_str(),
            info.text_role.as_str(),
            info.closing.as_str()
        )?;
    }
    if let Some(n) = c.number() {
        write!(out, ",\"number\":{n}")?;
    }

    let span = c.span();
    write!(out, ",\"start\":{},\"end\":{}", span.start, span.end())?;
    if c.is_synthetic() {
        out.write_str(",\"synthetic\":true")?;
    }

    // The matching close token, so the editor can decorate `\xxx*` as well.
    if let Some(close) = c.matching_close() {
        let cs = close.span();
        write!(out, ",\"close\":{{\"start\":{},\"end\":{}", cs.start, cs.end())?;
        if close.is_synthetic() {
            out.write_str(",\"synthetic\":true")?;
        }
        out.write_char('}')?;
    }

    let mut children = c.children().peekable();
    if children.peek().is_some() {
        out.write_str(",\"children\":[")?;
        let mut first = true;
        for child in children {
            if !first {
                out.write_char(',')?;
            }
            first = false;
            write_cst_node(child, out)?;
        }
        out.write_char(']')?;
    }

    out.write_char('}')
}

// --- diagnostics ------------------------------------------------------------

/// Serialize diagnostics as a JSON array. Each entry carries `severity`, a stable
/// `code`, the human `message`, the byte `start`/`end`, and 1-based `line`/`column`
/// (resolved from the source) for editors that work in line/column space.
pub fn write_diagnostics_json<W: Write>(
    diagnostics: &[Diagnostic],
    source: &str,
    out: &mut W,
) -> fmt::Result {
    out.write_char('[')?;
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            out.write_char(',')?;
        }
        let lc = d.span.line_col(source);
        write!(out, "{{\"severity\":\"{}\",\"code\":", d.severity.as_str())?;
        json_str(d.code, out)?;
        out.write_str(",\"message\":")?;
        json_str(&d.message, out)?;
        write!(
            out,
            ",\"start\":{},\"end\":{},\"line\":{},\"column\":{}}}",
            d.span.start,
            d.span.end(),
            lc.line,
            lc.col
        )?;
    }
    out.write_char(']')
}
