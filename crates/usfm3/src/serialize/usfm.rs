//! USFM export (normalized).
//!
//! Re-serialize the tree to canonical USFM: one block marker per line, single
//! spaces between tokens (docs.usfm.bible whitespace normalization), character /
//! note / figure markers explicitly closed, attributes written back as
//! `|key="value"`. Round-trips to the same USJ as the source.

use std::fmt::{self, Write};

use super::render::{
    collapse_all, collect_attributes, figure_legacy_raw, note_caller, remainder_after_first_token,
    verse_label,
};
use crate::Document;
use crate::cst::Cursor;
use crate::marker::Category;
use crate::tape::NodeKind;

impl Document<'_> {
    /// Re-serialize to normalized USFM (canonical spacing, explicit closes).
    pub fn to_usfm(&self) -> String {
        let mut s = String::new();
        let _ = write_usfm(self.cursor(), &mut s);
        s
    }
}

fn write_usfm<W: Write>(root: Cursor<'_>, out: &mut W) -> fmt::Result {
    let mut w = Usfm { out, last: None };
    w.children(root)?;
    if !w.at_line_start() {
        w.out.write_char('\n')?;
    }
    Ok(())
}

struct Usfm<'a, W: Write> {
    out: &'a mut W,
    last: Option<char>,
}

impl<W: Write> Usfm<'_, W> {
    fn put(&mut self, s: &str) -> fmt::Result {
        if let Some(c) = s.chars().last() {
            self.last = Some(c);
        }
        self.out.write_str(s)
    }
    fn at_line_start(&self) -> bool {
        matches!(self.last, None | Some('\n'))
    }
    fn newline(&mut self) -> fmt::Result {
        if !self.at_line_start() {
            self.put("\n")?;
        }
        Ok(())
    }
    /// Ensure a single separating space, unless we're at line start or already
    /// sitting on whitespace.
    fn space(&mut self) -> fmt::Result {
        if matches!(self.last, Some(c) if !c.is_whitespace()) {
            self.put(" ")?;
        }
        Ok(())
    }
    fn marker(&mut self, name: &str) -> fmt::Result {
        self.put("\\")?;
        self.put(name)
    }
    fn attrs(&mut self, c: Cursor<'_>, marker: &str, lead: &str) -> fmt::Result {
        // Deprecated positional figure (`\fig DESC|FILE|SIZE|…\fig*`): re-emit the
        // raw `|`-field list verbatim. collect_attributes would mislabel it as a
        // single default attribute (`lemma="FILE|SIZE|…"`), corrupting the round-trip.
        if marker == "fig"
            && let Some(raw) = figure_legacy_raw(c)
        {
            self.put(raw)?; // raw already includes the leading `|`
            return Ok(());
        }
        let attrs = collect_attributes(c, marker);
        if attrs.is_empty() {
            return Ok(());
        }
        self.put(lead)?; // " |" for milestones, "|" for char/figure
        for (i, (k, v)) in attrs.iter().enumerate() {
            if i > 0 {
                self.put(" ")?;
            }
            self.put(k)?;
            self.put("=\"")?;
            // Re-escape embedded quotes so the value round-trips through re-parsing.
            if v.contains('"') {
                self.put(&v.replace('"', "\\\""))?;
            } else {
                self.put(v)?;
            }
            self.put("\"")?;
        }
        Ok(())
    }
    fn children(&mut self, parent: Cursor<'_>) -> fmt::Result {
        for child in parent.children() {
            self.node(child)?;
        }
        Ok(())
    }
    fn node(&mut self, c: Cursor<'_>) -> fmt::Result {
        match c.kind() {
            NodeKind::Text => {
                let t = collapse_all(c.text());
                if !t.is_empty() {
                    self.space()?;
                    self.put(&t)?;
                }
            }
            NodeKind::Chapter => {
                self.newline()?;
                self.marker("c")?;
                self.put(" ")?;
                self.put(&c.number().unwrap_or(0).to_string())?;
            }
            NodeKind::Verse => {
                self.space()?;
                self.marker("v")?;
                self.put(" ")?;
                self.put(verse_label(c))?;
            }
            NodeKind::MarkerOpen => {
                let Some(info) = c.marker_info() else {
                    return Ok(());
                };
                let name = c.marker_name(); // actual token, e.g. "q1", "tcr1-2"
                match info.category {
                    Category::Note => {
                        self.space()?;
                        self.marker(name)?;
                        self.put(" ")?;
                        self.put(note_caller(c))?;
                        for (i, ch) in c.children().enumerate() {
                            if i == 0 && ch.kind() == NodeKind::Text {
                                let rem = collapse_all(remainder_after_first_token(ch.text()));
                                if !rem.is_empty() {
                                    self.put(" ")?;
                                    self.put(&rem)?;
                                }
                            } else {
                                self.node(ch)?;
                            }
                        }
                        self.marker(name)?;
                        self.put("*")?;
                    }
                    Category::Milestone => {
                        self.space()?;
                        self.marker(name)?;
                        self.attrs(c, name, " |")?;
                        self.put("\\*")?;
                    }
                    Category::Character => {
                        // Restore the `+` nesting prefix so `\+xt …\+xt*` round-trips
                        // (without it, re-parsing would close the enclosing marker).
                        let prefix = if c.is_nested() { "+" } else { "" };
                        self.space()?;
                        self.put("\\")?;
                        self.put(prefix)?;
                        self.put(name)?;
                        self.put(" ")?;
                        self.children(c)?;
                        self.attrs(c, name, "|")?;
                        self.put("\\")?;
                        self.put(prefix)?;
                        self.put(name)?;
                        self.put("*")?;
                    }
                    // Paragraph / identification (incl. \id, \usfm) — block markers.
                    _ => {
                        self.newline()?;
                        self.marker(name)?;
                        match info.usx_element {
                            // `\periph alt|id="…"` then its block children.
                            "periph" => {
                                if let Some(alt) = c
                                    .children()
                                    .find(|ch| ch.kind() == NodeKind::Text)
                                    .map(|ch| collapse_all(ch.text()))
                                    .filter(|s| !s.is_empty())
                                {
                                    self.put(" ")?;
                                    self.put(&alt)?;
                                }
                                self.attrs(c, "periph", "|")?;
                                for ch in c.children() {
                                    if matches!(ch.kind(), NodeKind::Text | NodeKind::Attribute) {
                                        continue;
                                    }
                                    self.node(ch)?;
                                }
                            }
                            // `\esb` … `\esbe` — emit the explicit sidebar close.
                            "sidebar" => {
                                self.children(c)?;
                                self.newline()?;
                                self.marker("esbe")?;
                            }
                            _ => self.children(c)?,
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
