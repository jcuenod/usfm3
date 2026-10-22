//! Editor-facing JSON: the concrete syntax tree (for decoration/folding) and the
//! diagnostics array (for squiggles). Both are consumed by tooling, not by the
//! document-content pipeline, so they live together here.

use std::fmt::{self, Write};

use super::render::json_str;
use crate::Document;
use crate::cst::Cursor;
use crate::diagnostic::Diagnostic;
use crate::span::{OffsetMap, Offsets};

impl Document<'_> {
    /// CST as JSON, for editor decoration (token spans, marker categories, tree
    /// structure).
    pub fn to_cst_json(&self) -> String {
        let mut s = String::new();
        let _ = write_cst_node(self.cursor(), &mut s);
        s
    }

    /// Diagnostics (errors + warnings) as a JSON array, with UTF-8 byte offsets.
    ///
    /// This is the Rust/CLI-facing form: offsets match [`crate::low_level::Span`] and
    /// the rest of the crate. FFI bindings call [`Self::to_diagnostics_json_with`] with
    /// their host's native space instead.
    pub fn to_diagnostics_json(&self) -> String {
        self.to_diagnostics_json_with(Offsets::Utf8)
    }

    /// Diagnostics as a JSON array with `start`/`end`/`column` in the given space.
    ///
    /// `line` is space-independent. See [`Offsets`] for why a host wants this rather
    /// than converting byte offsets itself.
    pub fn to_diagnostics_json_with(&self, offsets: Offsets) -> String {
        let mut s = String::new();
        let _ = write_diagnostics_json(self.diagnostics(), self.source(), offsets, &mut s);
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
/// `code`, the human `message`, `start`/`end`, and 1-based `line`/`column` for editors
/// that work in line/column space. Offsets and `column` are in `offsets` space; `line`
/// is space-independent.
///
/// One [`OffsetMap`] serves the whole array. That is the difference between O(len + N)
/// and the O(N x len) this used to be: [`crate::low_level::Span::line_col`] rescans the
/// prefix from byte 0 on every call, so 500 diagnostics on a book-sized document cost
/// ~21 ms of pure re-scanning. The scanner is forward-only and relies on the
/// `(span.start, span.end)` sort in `parse_with` for that amortization — but it
/// restarts on a backwards query, so the output is correct even if that sort changes.
fn write_diagnostics_json<W: Write>(
    diagnostics: &[Diagnostic],
    source: &str,
    offsets: Offsets,
    out: &mut W,
) -> fmt::Result {
    let mut map = OffsetMap::new(source, offsets);
    out.write_char('[')?;
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            out.write_char(',')?;
        }
        // `point_at` drives the scanner (starts are monotonic); the end is measured
        // locally so a nested span cannot force a backwards restart.
        let at = map.point_at(d.span.start);
        let end = at.offset + map.span_len(d.span.start, d.span.end());
        write!(out, "{{\"severity\":\"{}\",\"code\":", d.severity.as_str())?;
        json_str(d.code.as_str(), out)?;
        out.write_str(",\"message\":")?;
        json_str(&d.message, out)?;
        write!(
            out,
            ",\"start\":{},\"end\":{},\"line\":{},\"column\":{}}}",
            at.offset, end, at.line, at.col
        )?;
    }
    out.write_char(']')
}
