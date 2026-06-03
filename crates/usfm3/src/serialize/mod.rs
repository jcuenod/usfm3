//! Output formats, one module per format. Each format module owns its serializer
//! *and* the [`crate::Document`] convenience methods (`to_usx`, `vref_dict`, …)
//! that expose it; shared helpers live in [`render`].
//!
//! Whitespace follows docs.usfm.bible (collapse runs to one space; drop the
//! separator after an opening marker/number; keep word-separating spaces after a
//! closing marker). Round-tripped whitespace is not authoritative.

mod render;

mod cst;
mod usfm;
mod usj;
mod usx;
pub mod vref;
