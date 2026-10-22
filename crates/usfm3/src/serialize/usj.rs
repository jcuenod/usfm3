//! USJ (JSON) serialization — <https://docs.usfm.bible/usfm/3.1/usj.html>
//!
//! A walk over the [`Cursor`] tree producing the USJ content model: text nodes are
//! bare strings; markers become `{type,marker,…,content}` objects; char/milestone
//! attributes inline as object keys; chapters/verses carry a computed `sid`. We do
//! NOT canonicalize whitespace here beyond what the tape already did — per project
//! policy, round-tripped whitespace is not authoritative.

use std::fmt::{self, Write};

use super::render::{
    AltPub, alt_pub_kind, category_of, cell_align, collapse_all, collect_attributes, figure_legacy,
    is_category_marker, is_plain_text, is_table_row, json_str, marker_inner_text, normalize_text,
    note_caller, remainder_after_first_token, trim_leading, verse_label,
};
use crate::Document;
use crate::cst::Cursor;
use crate::marker::Category;
use crate::tape::NodeKind;

impl Document<'_> {
    /// Serialize to USJ (JSON).
    pub fn to_usj(&self) -> String {
        let mut s = String::new();
        let _ = write_usj(self.cursor(), self.implicit_chapter(), &mut s);
        s
    }
}

/// Running context for `sid` construction.
#[derive(Default)]
struct Ctx {
    book: String,    // current book code, e.g. "GEN"
    chapter: String, // current chapter number, e.g. "1"
}

fn write_usj<W: Write>(root: Cursor<'_>, implicit_chapter: u16, out: &mut W) -> fmt::Result {
    let mut ctx = Ctx {
        chapter: implicit_chapter.to_string(),
        ..Ctx::default()
    };
    out.write_str("{\"type\":\"USJ\",\"version\":\"3.1\",\"content\":")?;
    write_content(root, out, &mut ctx)?;
    out.write_char('}')
}

/// Write the `[ … ]` USJ array of a parent's children, tracking the previous
/// emitted sibling so text whitespace can be normalized in context. Consecutive
/// `\tr` rows are grouped into a single `table`.
fn write_content<W: Write>(parent: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    let kids: Vec<Cursor<'_>> = parent.children().collect();
    out.write_char('[')?;
    let mut first = true;
    let mut prev: Option<NodeKind> = None;
    let mut i = 0;
    while i < kids.len() {
        if is_table_row(kids[i]) {
            let start = i;
            while i < kids.len() && is_table_row(kids[i]) {
                i += 1;
            }
            sep(out, &mut first)?;
            write_table(&kids[start..i], out, ctx)?;
            prev = Some(NodeKind::MarkerOpen);
            continue;
        }
        let k = kids[i];
        if matches!(k.kind(), NodeKind::Chapter | NodeKind::Verse) {
            // Fold immediately-following alternate/published numbers into the
            // chapter/verse object: \ca…\ca*/\va…\va* → altnumber, \cp/\vp →
            // pubnumber (spec: docs.usfm.bible chapter/verse numbering).
            let (mut alt, mut pubn) = (None, None);
            let mut j = i + 1;
            while j < kids.len() {
                let n = kids[j];
                if n.kind() == NodeKind::Text && n.text().trim().is_empty() {
                    j += 1; // separator whitespace between the folded markers
                    continue;
                }
                // Only fold a plain-text number. A \vp/\cp carrying nested markup
                // (e.g. \vp \+it 21\+it*\vp*) is a real char element, not a number.
                match alt_pub_kind(n).filter(|_| is_plain_text(n)) {
                    Some(AltPub::Alt) => alt = Some(marker_inner_text(n)),
                    Some(AltPub::Pub) => pubn = Some(marker_inner_text(n)),
                    None => break,
                }
                j += 1;
            }
            sep(out, &mut first)?;
            if k.kind() == NodeKind::Chapter {
                write_chapter(k, alt.as_deref(), pubn.as_deref(), out, ctx)?;
            } else {
                write_verse(k, alt.as_deref(), pubn.as_deref(), out, ctx)?;
            }
            prev = Some(k.kind());
            i = j;
            continue;
        }
        if emit_node(k, out, ctx, &mut first, trim_leading(prev))? {
            prev = Some(k.kind());
        }
        i += 1;
    }
    out.write_char(']')
}

/// Emit `{type:table, content:[{type:table:row, …}]}` for a run of `\tr` rows.
fn write_table<W: Write>(rows: &[Cursor<'_>], out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"table\",\"content\":[")?;
    for (ri, row) in rows.iter().enumerate() {
        if ri > 0 {
            out.write_char(',')?;
        }
        out.write_str("{\"type\":\"table:row\",\"marker\":\"tr\",\"content\":[")?;
        let mut first = true;
        for cell in row.children() {
            if cell.kind() != NodeKind::MarkerOpen {
                continue;
            }
            if cell.marker_info().is_none() {
                continue;
            }
            let name = cell.marker_name();
            sep(out, &mut first)?;
            out.write_str("{\"type\":\"table:cell\",\"marker\":")?;
            json_str(name, out)?;
            out.write_str(",\"content\":")?;
            write_content(cell, out, ctx)?;
            write!(out, ",\"align\":\"{}\"", cell_align(name))?;
            out.write_char('}')?;
        }
        out.write_str("]}")?;
    }
    out.write_str("]}")
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
            // `//` is a discretionary line break → an `optbreak` node between the
            // surrounding text segments (spec: docs.usfm.bible).
            if !t.contains("//") {
                sep(out, first)?;
                json_str(&t, out)?;
            } else {
                for (i, seg) in t.split("//").enumerate() {
                    if i > 0 {
                        sep(out, first)?;
                        out.write_str("{\"type\":\"optbreak\"}")?;
                    }
                    let seg = seg.trim();
                    if !seg.is_empty() {
                        sep(out, first)?;
                        json_str(seg, out)?;
                    }
                }
            }
        }
        NodeKind::Chapter => {
            sep(out, first)?;
            write_chapter(c, None, None, out, ctx)?;
        }
        NodeKind::Verse => {
            sep(out, first)?;
            write_verse(c, None, None, out, ctx)?;
        }
        NodeKind::MarkerOpen => {
            let Some(info) = c.marker_info() else {
                return Ok(false);
            };
            // `\usfm` (version declaration) is metadata, not USJ content.
            if info.name == "usfm" {
                return Ok(false);
            }
            sep(out, first)?;
            match info.category {
                _ if info.name == "fig" => write_figure(c, out, ctx)?,
                _ if info.usx_element == "sidebar" => write_sidebar(c, out, ctx)?,
                _ if info.usx_element == "periph" => write_periph(c, out, ctx)?,
                _ if c.marker_name() == "ref" => write_ref(c, out, ctx)?,
                Category::Identification if info.name == "id" => write_book(c, out, ctx)?,
                Category::Note => write_note(c, out, ctx)?,
                Category::Milestone => write_ms(c, c.marker_name(), out)?,
                Category::Character => write_char(c, c.marker_name(), out, ctx)?,
                _ => write_para(c, c.marker_name(), out, ctx)?,
            }
        }
        // Attribute / MarkerClose / BlockEnd / Error / Root — not USJ nodes.
        _ => return Ok(false),
    }
    Ok(true)
}

fn write_para<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"para\",\"marker\":")?;
    json_str(marker, out)?;
    // The USJ schema marks `content` OPTIONAL (markerObject requires only `type`),
    // so emitting `[]` and omitting the key are both valid. We choose to ALWAYS
    // emit `content` for paragraph markers — they are containers (`\p (_text…)` per
    // spec), so an empty one is "a paragraph with no content yet", i.e. `[]` — which
    // keeps every para object uniformly shaped for consumers.
    //
    // `\b` (blank line) is the exception: the spec is explicit that it takes NO
    // content ("No text should follow this marker" —
    // ubsicap.github.io/usfm/paragraphs). So an EMPTY `\b` omits `content` entirely.
    // If a `\b` somehow captured content (malformed input), we keep it rather than
    // drop data.
    let mut content = String::new();
    write_content(c, &mut content, ctx)?;
    if !(marker == "b" && content == "[]") {
        out.write_str(",\"content\":")?;
        out.write_str(&content)?;
    }
    out.write_char('}')
}

fn write_char<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"char\",\"marker\":")?;
    json_str(marker, out)?;
    write_attributes(c, marker, out)?;
    out.write_str(",\"content\":")?;
    // A *nested* `\+xt …|<href>\+xt*` carrying a link target represents an inline
    // scripture reference: its content is wrapped in a `ref` node (loc = the link
    // target). A plain `\xt` is a cross-reference list and stays flat. (Matches the
    // reference impl; the `+` nesting prefix is the discriminator.)
    if marker == "xt" && c.is_nested() {
        let href = collect_attributes(c, marker)
            .into_iter()
            .find(|(k, _)| *k == "href");
        let has_content = c.children().any(|ch| {
            !matches!(
                ch.kind(),
                NodeKind::Attribute | NodeKind::MarkerClose | NodeKind::BlockEnd
            )
        });
        let has_ref = c
            .children()
            .any(|ch| ch.kind() == NodeKind::MarkerOpen && ch.marker_name() == "ref");
        if let Some((_, loc)) = href
            && has_content
            && !has_ref
        {
            out.write_str("[{\"type\":\"ref\",\"loc\":")?;
            json_str(&loc, out)?;
            out.write_str(",\"content\":")?;
            write_content(c, out, ctx)?;
            out.write_str("}]")?;
            return out.write_char('}');
        }
    }
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
    // Deprecated positional figure (`\fig DESC|FILE|SIZE|LOC|COPY|CAP|REF\fig*`):
    // map the fields to named attributes and use CAP as the caption content.
    if let Some((attrs, caption)) = figure_legacy(c) {
        for (k, v) in attrs {
            out.write_char(',')?;
            json_str(k, out)?;
            out.write_char(':')?;
            json_str(v, out)?;
        }
        out.write_str(",\"content\":")?;
        if caption.is_empty() {
            out.write_str("[]")?;
        } else {
            out.write_char('[')?;
            json_str(caption, out)?;
            out.write_char(']')?;
        }
        return out.write_char('}');
    }
    write_attributes(c, "fig", out)?;
    out.write_str(",\"content\":")?;
    write_content(c, out, ctx)?;
    out.write_char('}')
}

fn write_book<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    let raw = c
        .children()
        .find(|ch| ch.kind() == NodeKind::Text)
        .map(|ch| ch.text())
        .unwrap_or("");
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

/// `{type:"chapter", …}` with optional folded `altnumber`/`pubnumber`.
fn write_chapter<W: Write>(
    c: Cursor<'_>,
    alt: Option<&str>,
    pubn: Option<&str>,
    out: &mut W,
    ctx: &mut Ctx,
) -> fmt::Result {
    let n = c.number().unwrap_or(0).to_string();
    ctx.chapter = n.clone();
    out.write_str("{\"type\":\"chapter\",\"marker\":\"c\",\"number\":")?;
    json_str(&n, out)?;
    if let Some(a) = alt {
        out.write_str(",\"altnumber\":")?;
        json_str(a, out)?;
    }
    if let Some(p) = pubn {
        out.write_str(",\"pubnumber\":")?;
        json_str(p, out)?;
    }
    out.write_str(",\"sid\":")?;
    json_str(&format!("{} {}", ctx.book, n), out)?;
    out.write_char('}')
}

/// `{type:"verse", …}` with optional folded `altnumber`/`pubnumber`.
fn write_verse<W: Write>(
    c: Cursor<'_>,
    alt: Option<&str>,
    pubn: Option<&str>,
    out: &mut W,
    ctx: &mut Ctx,
) -> fmt::Result {
    let label = verse_label(c);
    out.write_str("{\"type\":\"verse\",\"marker\":\"v\",\"number\":")?;
    json_str(label, out)?;
    if let Some(a) = alt {
        out.write_str(",\"altnumber\":")?;
        json_str(a, out)?;
    }
    out.write_str(",\"sid\":")?;
    json_str(&format!("{} {}:{}", ctx.book, ctx.chapter, label), out)?;
    if let Some(p) = pubn {
        out.write_str(",\"pubnumber\":")?;
        json_str(p, out)?;
    }
    out.write_char('}')
}

/// `\ref text|loc\ref*` → `{type:"ref", loc, content:[text]}` (scripture link).
fn write_ref<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"ref\"")?;
    if let Some((_, loc)) = collect_attributes(c, "ref")
        .into_iter()
        .find(|(k, _)| *k == "loc")
    {
        out.write_str(",\"loc\":")?;
        json_str(&loc, out)?;
    }
    out.write_str(",\"content\":")?;
    write_content(c, out, ctx)?;
    out.write_char('}')
}

/// `\esb …\esbe` → `{type:"sidebar", marker, category?, content}`. The `\cat` child
/// folds into `category`; the rest are the sidebar's block children.
fn write_sidebar<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"sidebar\",\"marker\":")?;
    json_str(c.marker_name(), out)?;
    if let Some(cat) = category_of(c) {
        out.write_str(",\"category\":")?;
        json_str(cat.trim(), out)?;
    }
    out.write_str(",\"content\":[")?;
    let mut first = true;
    let mut prev: Option<NodeKind> = None;
    for ch in c.children() {
        if is_category_marker(ch) {
            continue; // folded into `category`
        }
        if emit_node(ch, out, ctx, &mut first, trim_leading(prev))? {
            prev = Some(ch.kind());
        }
    }
    out.write_str("]}")
}

/// `\periph alt|id="…"` → `{type:"periph", alt?, id?, content}` (no `marker` key
/// per the USJ schema). Leading text is `alt`; `|…` attributes (e.g. `id`) inline.
fn write_periph<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    out.write_str("{\"type\":\"periph\"")?;
    let alt = c
        .children()
        .find(|ch| ch.kind() == NodeKind::Text)
        .map(|ch| collapse_all(ch.text()))
        .filter(|s| !s.is_empty());
    if let Some(alt) = &alt {
        out.write_str(",\"alt\":")?;
        json_str(alt, out)?;
    }
    for (k, v) in collect_attributes(c, "periph") {
        out.write_char(',')?;
        json_str(k, out)?;
        out.write_char(':')?;
        json_str(&v, out)?;
    }
    out.write_str(",\"content\":[")?;
    let mut first = true;
    let mut prev: Option<NodeKind> = None;
    for ch in c.children() {
        if matches!(ch.kind(), NodeKind::Text | NodeKind::Attribute) {
            continue; // leading text is `alt`; attributes already inlined
        }
        if emit_node(ch, out, ctx, &mut first, trim_leading(prev))? {
            prev = Some(ch.kind());
        }
    }
    out.write_str("]}")
}

fn write_note<W: Write>(c: Cursor<'_>, out: &mut W, ctx: &mut Ctx) -> fmt::Result {
    let caller = note_caller(c);

    out.write_str("{\"type\":\"note\",\"marker\":")?;
    json_str(c.marker_name(), out)?;
    out.write_str(",\"caller\":")?;
    json_str(caller, out)?;
    // A `\cat …\cat*` inside the note sets its `category` (not a content child).
    if let Some(cat) = category_of(c) {
        out.write_str(",\"category\":")?;
        json_str(cat.trim(), out)?;
    }
    out.write_str(",\"content\":[")?;
    let mut first = true;
    let mut prev: Option<NodeKind> = None;
    for (i, ch) in c.children().enumerate() {
        if is_category_marker(ch) {
            continue; // folded into the `category` field above
        }
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

/// Emit `,"key":"value"` pairs from this node's `|…` attribute child (USJ).
fn write_attributes<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W) -> fmt::Result {
    for (key, value) in collect_attributes(c, marker) {
        out.write_char(',')?;
        json_str(key, out)?;
        out.write_char(':')?;
        json_str(&value, out)?;
    }
    Ok(())
}
