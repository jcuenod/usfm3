//! The tape schema: everything a host needs to *interpret* a packed tape.
//!
//! A packed tape is integers, and two of them are only meaningful against a table:
//! `kind` indexes [`NodeKind::ALL`], and `marker` indexes one version's
//! [`MarkerTable`]. So a host fetches this once per version, caches it, and resolves
//! ids locally.
//!
//! This is the one place JSON is the right wire format: ~159 entries of short ASCII
//! strings, fetched once, never on a hot path. Packing it would save nothing and cost
//! a decoder.
//!
//! Every value here is *derived* from the same constants the packer uses — the kind
//! names from [`NodeKind::ALL`], the flag bits from [`flags`], the marker fields from
//! the generated table. Nothing is transcribed, so a host cannot drift from the core.

use std::fmt::{self, Write};

use super::packed::{PACKED_FORMAT, PACKED_HEADER_WORDS, PACKED_MAGIC, PACKED_NODE_WORDS};
use super::render::json_str;
use crate::Version;
use crate::marker::{MarkerId, MarkerTable};
use crate::tape::{NodeKind, flags};

/// The tape/marker schema for one USFM version, as JSON.
///
/// Shape:
///
/// ```json
/// {
///   "format": 1, "magic": 1431520852, "headerWords": 8, "nodeWords": 4,
///   "version": "3.1",
///   "kinds": ["root", "marker", "close", ...],
///   "flags": { "synthetic": 1, "selfClosing": 2, ... },
///   "unknownMarker": 65535,
///   "markers": [{ "name": "id", "category": "identification", "role": "excluded",
///                 "closing": "none", "usx": "book", "deprecated": null }, ...]
/// }
/// ```
///
/// `kinds[i]` is the packed `kind` code and `markers[i]` is the packed `marker` id,
/// both for a tape built with this same `version`.
pub fn schema_json(version: Version) -> String {
    let mut s = String::new();
    let _ = write_schema(version, &mut s);
    s
}

fn write_schema<W: Write>(version: Version, out: &mut W) -> fmt::Result {
    write!(
        out,
        "{{\"format\":{PACKED_FORMAT},\"magic\":{PACKED_MAGIC},\
         \"headerWords\":{PACKED_HEADER_WORDS},\"nodeWords\":{PACKED_NODE_WORDS},\"version\":"
    )?;
    json_str(version.as_str(), out)?;

    out.write_str(",\"kinds\":[")?;
    for (i, kind) in NodeKind::ALL.iter().enumerate() {
        if i > 0 {
            out.write_char(',')?;
        }
        json_str(kind.as_str(), out)?;
    }

    write!(
        out,
        "],\"flags\":{{\"synthetic\":{},\"selfClosing\":{},\"hasAttrs\":{},\
         \"hasError\":{},\"nested\":{}}},\"unknownMarker\":{},\"markers\":[",
        flags::SYNTHETIC,
        flags::SELF_CLOSING,
        flags::HAS_ATTRS,
        flags::HAS_ERROR,
        flags::NESTED,
        MarkerId::UNKNOWN.0,
    )?;

    for (i, info) in MarkerTable::for_version(version)
        .entries()
        .iter()
        .enumerate()
    {
        if i > 0 {
            out.write_char(',')?;
        }
        // `name` is the table *base* (`q`, not `q1`). The literal written name lives
        // in the source and is recovered by slicing the node's span, exactly as
        // `Cursor::marker_name` does.
        out.write_str("{\"name\":")?;
        json_str(info.name, out)?;
        write!(
            out,
            ",\"category\":\"{}\",\"role\":\"{}\",\"closing\":\"{}\",\"usx\":",
            info.category.as_str(),
            info.text_role.as_str(),
            info.closing.as_str(),
        )?;
        json_str(info.usx_element, out)?;
        out.write_str(",\"deprecated\":")?;
        match info.deprecated {
            Some(replacement) => json_str(replacement, out)?,
            None => out.write_str("null")?,
        }
        out.write_char('}')?;
    }

    out.write_str("]}")
}
