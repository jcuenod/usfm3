//! Editor-facing JSON: the concrete syntax tree (for decoration/folding) and the
//! diagnostics array (for squiggles). Both are consumed by tooling, not by the
//! document-content pipeline, so they live together here.

use std::fmt::{self, Write};

use super::render::json_str;
use crate::Document;
use crate::cst::Cursor;
use crate::diagnostic::Diagnostic;

impl Document<'_> {
    /// CST as JSON, for editor decoration (token spans, marker categories, tree
    /// structure).
    pub fn to_cst_json(&self) -> String {
        let mut s = String::new();
        let _ = write_cst_node(self.cursor(), &mut s);
        s
    }

    /// Diagnostics (errors + warnings) as a JSON array.
    pub fn to_diagnostics_json(&self) -> String {
        let mut s = String::new();
        let _ = write_diagnostics_json(self.diagnostics(), self.source(), &mut s);
        s
    }
}

/// Serialize the concrete syntax tree as JSON, mirroring the tape's tree. Each
/// node carries its byte `span` (UTF-8 offsets into the source), and marker nodes
/// carry `marker`/`category`/`role`/`closing` plus the `close` token span — i.e.
/// everything an editor needs to colorize tags and fold structure. Synthetic
/// nodes (implicit block ends) are flagged so a decorator can ignore them.
fn write_cst_node<W: Write>(c: Cursor<'_>, out: &mut W) -> fmt::Result {
    write!(out, "{{\"type\":\"{}\"", c.kind().as_str())?;

    if let Some(info) = c.marker_info() {
        out.write_str(",\"marker\":")?;
        json_str(c.marker_name(), out)?;
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

/// Serialize diagnostics as a JSON array. Each entry carries `severity`, a stable
/// `code`, the human `message`, the byte `start`/`end`, and 1-based `line`/`column`
/// (resolved from the source) for editors that work in line/column space.
fn write_diagnostics_json<W: Write>(
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
        json_str(d.code.as_str(), out)?;
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
