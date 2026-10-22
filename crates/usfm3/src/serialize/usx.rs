//! USX (XML) serialization — <https://docs.usfm.bible/usfm/3.1/usx.html>
//!
//! USX models chapters and verses as milestone *pairs*: a start element carrying
//! `sid` and an end element carrying `eid`, emitted when the next chapter/verse
//! begins or the document ends. Char markers closed implicitly (note-internal
//! `\fr \ft`, or by a block boundary) carry `closed="false"`.

use std::fmt::{self, Write};

use super::render::{
    AltPub, alt_pub_kind, category_of, cell_align, collect_attributes, figure_legacy,
    is_category_marker, is_plain_text, is_table_row, marker_inner_text, normalize_text,
    note_caller, remainder_after_first_token, trim_leading, verse_label,
};
use crate::Document;
use crate::cst::Cursor;
use crate::marker::Category;
use crate::tape::NodeKind;

impl Document<'_> {
    /// Serialize to USX (XML).
    pub fn to_usx(&self) -> String {
        let mut s = String::new();
        let _ = write_usx(self.cursor(), &mut s);
        s
    }
}

/// Open chapter/verse milestones awaiting their `eid` end element.
#[derive(Default)]
struct UsxState {
    book: String,
    chapter_num: String,
    chapter_sid: Option<String>,
    verse_sid: Option<String>,
}

impl UsxState {
    fn close_verse<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        if let Some(sid) = self.verse_sid.take() {
            write!(out, "<verse eid=\"{sid}\" />")?;
        }
        Ok(())
    }
    fn close_chapter<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        if let Some(sid) = self.chapter_sid.take() {
            write!(out, "<chapter eid=\"{sid}\" />")?;
        }
        Ok(())
    }
}

fn write_usx<W: Write>(root: Cursor<'_>, out: &mut W) -> fmt::Result {
    out.write_str("<usx version=\"3.0\">")?;
    let mut st = UsxState::default();
    let children: Vec<_> = root.children().collect();
    usx_blocks(&children, 1, out, &mut st)?;
    st.close_verse(out)?;
    st.close_chapter(out)?;
    out.write_str("\n</usx>")
}

/// Start a block-level element on its own line, indented two spaces per level.
fn block_line<W: Write>(depth: usize, out: &mut W) -> fmt::Result {
    out.write_char('\n')?;
    for _ in 0..depth {
        out.write_str("  ")?;
    }
    Ok(())
}

/// Emit block-level children: paragraphs, chapters, tables (a run of `\tr` rows)
/// and the `\esb`/`\periph` containers, whose own children are blocks again.
fn usx_blocks<W: Write>(
    children: &[Cursor<'_>],
    depth: usize,
    out: &mut W,
    st: &mut UsxState,
) -> fmt::Result {
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        if is_table_row(child) {
            let start = i;
            while i < children.len() && is_table_row(children[i]) {
                i += 1;
            }
            block_line(depth, out)?;
            usx_table(&children[start..i], depth, out, st)?;
            continue;
        }
        if child.kind() == NodeKind::Chapter {
            let (alt, pubn, next) = fold_alt_pub(children, i);
            block_line(depth, out)?;
            usx_chapter(child, alt.as_deref(), pubn.as_deref(), out, st)?;
            i = next;
            continue;
        }
        match child.marker_info().map(|m| m.usx_element) {
            Some("sidebar") => {
                block_line(depth, out)?;
                usx_sidebar(child, depth, out, st)?;
                i += 1;
                continue;
            }
            Some("periph") => {
                block_line(depth, out)?;
                usx_periph(child, depth, out, st)?;
                i += 1;
                continue;
            }
            _ => {}
        }
        if is_top_level_block(child) {
            block_line(depth, out)?;
            usx_node(child, out, st)?;
            i += 1;
            continue;
        }
        // A root-level verse run (e.g. \c directly followed by \v with no \p) is
        // malformed-but-recovered input; preserve it in USX as an implicit \p.
        if child.kind() == NodeKind::Verse {
            let start = i;
            i += 1;
            while i < children.len() && !is_top_level_block(children[i]) {
                i += 1;
            }
            block_line(depth, out)?;
            out.write_str("<para style=\"p\">")?;
            usx_inline_nodes(&children[start..i], out, st)?;
            out.write_str("</para>")?;
            continue;
        }
        i += 1;
    }
    Ok(())
}

fn is_top_level_block(c: Cursor<'_>) -> bool {
    match c.kind() {
        NodeKind::Chapter => true,
        NodeKind::MarkerOpen => c.marker_info().is_some_and(|info| {
            info.name != "usfm"
                && !matches!(
                    info.category,
                    Category::Character | Category::Note | Category::Milestone
                )
        }),
        _ => false,
    }
}

/// The `\ca`/`\va` (alternate) and `\cp`/`\vp` (published) numbers directly after
/// the chapter or verse at `children[at]`, and the index just past them. Only a
/// plain-text number folds; `\vp \+it 21\+it*\vp*` stays a real `<char>`.
fn fold_alt_pub(children: &[Cursor<'_>], at: usize) -> (Option<String>, Option<String>, usize) {
    let (mut alt, mut pubn) = (None, None);
    let mut j = at + 1;
    while j < children.len() {
        let n = children[j];
        if n.kind() == NodeKind::Text && n.text().trim().is_empty() {
            j += 1;
            continue;
        }
        match alt_pub_kind(n).filter(|_| is_plain_text(n)) {
            Some(AltPub::Alt) => alt = Some(marker_inner_text(n)),
            Some(AltPub::Pub) => pubn = Some(marker_inner_text(n)),
            None => break,
        }
        j += 1;
    }
    (alt, pubn, j)
}

/// `<table>` of `<row style="tr">` of `<cell style align>`, one per line.
fn usx_table<W: Write>(
    rows: &[Cursor<'_>],
    depth: usize,
    out: &mut W,
    st: &mut UsxState,
) -> fmt::Result {
    out.write_str("<table>")?;
    for row in rows {
        block_line(depth + 1, out)?;
        out.write_str("<row style=\"tr\">")?;
        for cell in row.children() {
            if cell.kind() != NodeKind::MarkerOpen || cell.marker_info().is_none() {
                continue;
            }
            let name = cell.marker_name();
            block_line(depth + 2, out)?;
            write!(out, "<cell style=\"{name}\" align=\"{}\"", cell_align(name))?;
            if has_content(cell) {
                out.write_char('>')?;
                usx_inline(cell, out, st)?;
                out.write_str("</cell>")?;
            } else {
                out.write_str(" />")?;
            }
        }
        block_line(depth + 1, out)?;
        out.write_str("</row>")?;
    }
    block_line(depth, out)?;
    out.write_str("</table>")
}

/// `\esb …\esbe` → `<sidebar style category?>` holding blocks. `\cat` folds into
/// `category`; a sidebar never closed by `\esbe` carries `closed="false"`.
fn usx_sidebar<W: Write>(
    c: Cursor<'_>,
    depth: usize,
    out: &mut W,
    st: &mut UsxState,
) -> fmt::Result {
    write!(out, "<sidebar style=\"{}\"", c.marker_name())?;
    if let Some(cat) = category_of(c) {
        out.write_str(" category=\"")?;
        escape_xml_attr(cat.trim(), out)?;
        out.write_char('"')?;
    }
    if c.matching_close().is_some_and(|close| close.is_synthetic()) {
        out.write_str(" closed=\"false\"")?;
    }
    let blocks: Vec<_> = c.children().filter(|ch| !is_category_marker(*ch)).collect();
    if !blocks
        .iter()
        .any(|ch| is_top_level_block(*ch) || is_table_row(*ch))
    {
        return out.write_str(" />");
    }
    out.write_char('>')?;
    usx_blocks(&blocks, depth + 1, out, st)?;
    block_line(depth, out)?;
    out.write_str("</sidebar>")
}

/// `\periph alt|id="…"` → `<periph alt id>` holding blocks. The leading text is
/// `alt`; the attributes are inlined.
fn usx_periph<W: Write>(
    c: Cursor<'_>,
    depth: usize,
    out: &mut W,
    st: &mut UsxState,
) -> fmt::Result {
    out.write_str("<periph")?;
    if let Some(alt) = c
        .children()
        .find(|ch| ch.kind() == NodeKind::Text)
        .map(|ch| ch.text().split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
    {
        out.write_str(" alt=\"")?;
        escape_xml_attr(&alt, out)?;
        out.write_char('"')?;
    }
    write_xml_attributes(c, "periph", out)?;
    let blocks: Vec<_> = c.children().collect();
    if !blocks
        .iter()
        .any(|ch| is_top_level_block(*ch) || is_table_row(*ch))
    {
        return out.write_str(" />");
    }
    out.write_char('>')?;
    usx_blocks(&blocks, depth + 1, out, st)?;
    block_line(depth, out)?;
    out.write_str("</periph>")
}

/// Whether a marker has any content besides its attributes and close.
fn has_content(c: Cursor<'_>) -> bool {
    c.children().any(|ch| match ch.kind() {
        NodeKind::Attribute | NodeKind::MarkerClose | NodeKind::BlockEnd => false,
        NodeKind::Text => !ch.text().trim().is_empty(),
        _ => true,
    })
}

fn usx_chapter<W: Write>(
    c: Cursor<'_>,
    alt: Option<&str>,
    pubn: Option<&str>,
    out: &mut W,
    st: &mut UsxState,
) -> fmt::Result {
    st.close_verse(out)?;
    st.close_chapter(out)?;
    let n = c.number().unwrap_or(0).to_string();
    let sid = format!("{} {}", st.book, n);
    write!(out, "<chapter number=\"{n}\" style=\"c\"")?;
    optional_attr(out, "altnumber", alt)?;
    optional_attr(out, "pubnumber", pubn)?;
    out.write_str(" sid=\"")?;
    escape_xml_attr(&sid, out)?;
    out.write_str("\" />")?;
    st.chapter_num = n;
    st.chapter_sid = Some(sid);
    Ok(())
}

fn usx_verse<W: Write>(
    c: Cursor<'_>,
    alt: Option<&str>,
    pubn: Option<&str>,
    out: &mut W,
    st: &mut UsxState,
) -> fmt::Result {
    st.close_verse(out)?;
    let label = verse_label(c);
    let sid = format!("{} {}:{}", st.book, st.chapter_num, label);
    out.write_str("<verse number=\"")?;
    escape_xml_attr(label, out)?;
    out.write_str("\" style=\"v\"")?;
    optional_attr(out, "altnumber", alt)?;
    out.write_str(" sid=\"")?;
    escape_xml_attr(&sid, out)?;
    out.write_char('"')?;
    optional_attr(out, "pubnumber", pubn)?;
    out.write_str(" />")?;
    st.verse_sid = Some(sid);
    Ok(())
}

fn optional_attr<W: Write>(out: &mut W, key: &str, value: Option<&str>) -> fmt::Result {
    if let Some(v) = value {
        write!(out, " {key}=\"")?;
        escape_xml_attr(v, out)?;
        out.write_char('"')?;
    }
    Ok(())
}

/// Serialize one node. Block-level callers add their own leading indentation;
/// inline content (inside para/char/note) is emitted without extra whitespace.
fn usx_node<W: Write>(c: Cursor<'_>, out: &mut W, st: &mut UsxState) -> fmt::Result {
    match c.kind() {
        NodeKind::Text => return Ok(()), // handled by usx_inline (needs trim context)
        NodeKind::Chapter => usx_chapter(c, None, None, out, st)?,
        NodeKind::Verse => usx_verse(c, None, None, out, st)?,
        NodeKind::MarkerOpen => {
            let Some(info) = c.marker_info() else {
                return Ok(());
            };
            if info.name == "usfm" {
                return Ok(());
            }
            match info.category {
                _ if info.name == "fig" => usx_figure(c, out, st)?,
                _ if c.marker_name() == "ref" => usx_ref(c, out, st)?,
                Category::Identification if info.name == "id" => usx_book(c, out, st)?,
                Category::Note => usx_note(c, c.marker_name(), out, st)?,
                Category::Milestone => usx_ms(c, c.marker_name(), out)?,
                Category::Character => usx_char(c, c.marker_name(), out, st)?,
                _ => usx_para(c, c.marker_name(), out, st)?,
            }
        }
        _ => {}
    }
    Ok(())
}

/// Emit a parent's children as inline USX, normalizing text whitespace in context.
fn usx_inline<W: Write>(parent: Cursor<'_>, out: &mut W, st: &mut UsxState) -> fmt::Result {
    let children: Vec<_> = parent.children().collect();
    usx_inline_nodes(&children, out, st)
}

fn usx_inline_nodes<W: Write>(
    children: &[Cursor<'_>],
    out: &mut W,
    st: &mut UsxState,
) -> fmt::Result {
    let mut prev: Option<NodeKind> = None;
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        i += 1;
        if child.kind() == NodeKind::Verse {
            let (alt, pubn, next) = fold_alt_pub(children, i - 1);
            usx_verse(child, alt.as_deref(), pubn.as_deref(), out, st)?;
            prev = Some(NodeKind::Verse);
            i = next;
            continue;
        }
        if child.kind() == NodeKind::Text {
            let t = normalize_text(child.text(), trim_leading(prev));
            if !t.is_empty() {
                // `//` → <optbreak /> (discretionary line break, spec).
                for (i, seg) in t.split("//").enumerate() {
                    if i > 0 {
                        out.write_str("<optbreak />")?;
                    }
                    let seg = if t.contains("//") { seg.trim() } else { seg };
                    escape_xml(seg, out)?;
                }
                prev = Some(NodeKind::Text);
            }
        } else if child.kind() == NodeKind::Attribute {
            // consumed by the enclosing element
        } else {
            usx_node(child, out, st)?;
            if !matches!(
                child.kind(),
                NodeKind::MarkerClose | NodeKind::BlockEnd | NodeKind::Error
            ) {
                prev = Some(child.kind());
            }
        }
    }
    Ok(())
}

fn usx_para<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W, st: &mut UsxState) -> fmt::Result {
    if !c
        .children()
        .any(|ch| !matches!(ch.kind(), NodeKind::MarkerClose | NodeKind::BlockEnd))
    {
        write!(out, "<para style=\"{marker}\" />")?;
        return Ok(());
    }
    write!(out, "<para style=\"{marker}\">")?;
    usx_inline(c, out, st)?;
    out.write_str("</para>")
}

fn usx_char<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W, st: &mut UsxState) -> fmt::Result {
    write!(out, "<char style=\"{marker}\"")?;
    if c.matching_close().is_some_and(|close| close.is_synthetic()) {
        out.write_str(" closed=\"false\"")?;
    }
    write_xml_attributes(c, marker, out)?;
    out.write_char('>')?;
    // A nested `\+xt …|<href>\+xt*` is an inline scripture link: its content is
    // wrapped in `<ref loc>` (as in USJ; the `+` prefix is the discriminator).
    if marker == "xt" && c.is_nested() && has_content(c) {
        let href = collect_attributes(c, marker)
            .into_iter()
            .find(|(k, _)| *k == "href");
        let has_ref = c
            .children()
            .any(|ch| ch.kind() == NodeKind::MarkerOpen && ch.marker_name() == "ref");
        if let Some((_, loc)) = href
            && !has_ref
        {
            out.write_str("<ref loc=\"")?;
            escape_xml_attr(&loc, out)?;
            out.write_str("\">")?;
            usx_inline(c, out, st)?;
            return out.write_str("</ref></char>");
        }
    }
    usx_inline(c, out, st)?;
    out.write_str("</char>")
}

/// `\ref text|loc\ref*` → `<ref loc="…">text</ref>`.
fn usx_ref<W: Write>(c: Cursor<'_>, out: &mut W, st: &mut UsxState) -> fmt::Result {
    out.write_str("<ref")?;
    if let Some((_, loc)) = collect_attributes(c, "ref")
        .into_iter()
        .find(|(k, _)| *k == "loc")
    {
        out.write_str(" loc=\"")?;
        escape_xml_attr(&loc, out)?;
        out.write_char('"')?;
    }
    out.write_char('>')?;
    usx_inline(c, out, st)?;
    out.write_str("</ref>")
}

fn usx_note<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W, st: &mut UsxState) -> fmt::Result {
    let caller = note_caller(c);
    write!(out, "<note caller=\"")?;
    escape_xml_attr(caller, out)?;
    write!(out, "\" style=\"{marker}\"")?;
    if let Some(cat) = category_of(c) {
        out.write_str(" category=\"")?;
        escape_xml_attr(cat.trim(), out)?;
        out.write_char('"')?;
    }
    out.write_char('>')?;
    // Skip the caller token in the note's leading text.
    let mut prev: Option<NodeKind> = None;
    for (i, ch) in c.children().enumerate() {
        if is_category_marker(ch) {
            continue; // folded into `category`
        }
        if i == 0 && ch.kind() == NodeKind::Text {
            let rem = remainder_after_first_token(ch.text());
            let t = normalize_text(rem, true);
            if !t.is_empty() {
                escape_xml(&t, out)?;
                prev = Some(NodeKind::Text);
            }
        } else if ch.kind() == NodeKind::Text {
            let t = normalize_text(ch.text(), trim_leading(prev));
            if !t.is_empty() {
                escape_xml(&t, out)?;
                prev = Some(NodeKind::Text);
            }
        } else if ch.kind() != NodeKind::Attribute {
            usx_node(ch, out, st)?;
            if !matches!(
                ch.kind(),
                NodeKind::MarkerClose | NodeKind::BlockEnd | NodeKind::Error
            ) {
                prev = Some(ch.kind());
            }
        }
    }
    out.write_str("</note>")
}

fn usx_ms<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W) -> fmt::Result {
    write!(out, "<ms style=\"{marker}\"")?;
    write_xml_attributes(c, marker, out)?;
    out.write_str(" />")
}

fn usx_figure<W: Write>(c: Cursor<'_>, out: &mut W, st: &mut UsxState) -> fmt::Result {
    out.write_str("<figure style=\"fig\"")?;
    // Deprecated positional figure (`\fig DESC|FILE|SIZE|LOC|COPY|CAP|REF\fig*`):
    // map the fields to named attributes and use CAP as the caption content.
    if let Some((attrs, caption)) = figure_legacy(c) {
        for (k, v) in attrs {
            write!(out, " {k}=\"")?;
            escape_xml_attr(v, out)?;
            out.write_char('"')?;
        }
        out.write_char('>')?;
        escape_xml(caption, out)?;
        return out.write_str("</figure>");
    }
    write_xml_attributes(c, "fig", out)?;
    out.write_char('>')?;
    usx_inline(c, out, st)?;
    out.write_str("</figure>")
}

fn usx_book<W: Write>(c: Cursor<'_>, out: &mut W, st: &mut UsxState) -> fmt::Result {
    let raw = c
        .children()
        .find(|ch| ch.kind() == NodeKind::Text)
        .map(|ch| ch.text())
        .unwrap_or("");
    let mut tokens = raw.split_whitespace();
    let code = tokens.next().unwrap_or("");
    st.book = code.to_string();
    write!(out, "<book code=\"{code}\" style=\"id\"")?;
    let rest = tokens.collect::<Vec<_>>().join(" ");
    if rest.is_empty() {
        out.write_str(" />")
    } else {
        out.write_char('>')?;
        escape_xml(&rest, out)?;
        out.write_str("</book>")
    }
}

/// Emit ` key="value"` XML attribute pairs from this node's `|…` child.
fn write_xml_attributes<W: Write>(c: Cursor<'_>, marker: &str, out: &mut W) -> fmt::Result {
    for (key, value) in collect_attributes(c, marker) {
        write!(out, " {key}=\"")?;
        escape_xml_attr(&value, out)?;
        out.write_char('"')?;
    }
    Ok(())
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

fn escape_xml_attr<W: Write>(s: &str, out: &mut W) -> fmt::Result {
    for ch in s.chars() {
        match ch {
            '<' => out.write_str("&lt;")?,
            '>' => out.write_str("&gt;")?,
            '&' => out.write_str("&amp;")?,
            '"' => out.write_str("&quot;")?,
            _ => out.write_char(ch)?,
        }
    }
    Ok(())
}
