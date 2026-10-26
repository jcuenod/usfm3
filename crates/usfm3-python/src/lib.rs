//! Python bindings for the `usfm3` parser.
//!
//! The core is "one tape, many views": a single parse produces a flat tape, and
//! every output (USJ, USX, USFM, CST, vref, diagnostics) is a cheap view over it.
//! So this binding exposes a single [`parse`] entry point returning a lazy
//! `ParsedDocument` handle — there is no separate token / AST / source-map stage.
//!
//! The core serializers emit JSON as strings; the JSON-shaped views
//! (`cst`/`to_usj`/`diagnostics`) are decoded at the boundary with the
//! standard-library `json` module (native C speed, no extra Rust deps), so callers
//! get plain dicts/lists. `to_vref` is a flat string map, so it is built as a dict
//! directly. `to_usx`/`to_usfm` are returned as strings.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyModule};

use ::usfm3 as usfm3_lib;

// `Document<'src>` borrows the source buffer, so the handle owns the source
// String and the borrowing Document together via a self-referential cell.
self_cell::self_cell!(
    struct Parsed {
        owner: String,
        #[covariant]
        dependent: ParsedView,
    }
);

type ParsedView<'src> = usfm3_lib::Document<'src>;

#[pyclass(unsendable)]
struct ParsedDocument {
    inner: Parsed,
}

impl ParsedDocument {
    fn build(usfm: String, version: usfm3_lib::Version, diagnostics: bool) -> Self {
        let inner = Parsed::new(usfm, |source| {
            usfm3_lib::parse_with(source, version, usfm3_lib::ParseOptions { diagnostics })
        });
        ParsedDocument { inner }
    }

    fn doc(&self) -> &usfm3_lib::Document<'_> {
        self.inner.borrow_dependent()
    }
}

#[pymethods]
impl ParsedDocument {
    /// Concrete syntax tree (for editor decoration / folding) as a dict.
    fn cst(&self, py: Python<'_>) -> PyResult<PyObject> {
        json_loads(py, &self.doc().to_cst_json())
    }

    /// USJ document as a dict.
    fn to_usj(&self, py: Python<'_>) -> PyResult<PyObject> {
        json_loads(py, &self.doc().to_usj())
    }

    /// USX serialization (XML string).
    fn to_usx(&self) -> String {
        self.doc().to_usx()
    }

    /// Normalized USFM (string).
    fn to_usfm(&self) -> String {
        self.doc().to_usfm()
    }

    /// Verse-reference → cleaned-text dict, in document order.
    fn to_vref(&self, py: Python<'_>) -> PyResult<PyObject> {
        // Built directly rather than via JSON: escaping it in Rust and decoding it
        // again in Python was half the cost of this call.
        let dict = PyDict::new(py);
        for (key, text) in self.doc().vref_dict() {
            dict.set_item(key, text)?;
        }
        Ok(dict.into_any().unbind())
    }

    /// Diagnostics as a flat list. Empty unless `diagnostics=True` was passed.
    ///
    /// `start`/`end`/`column` are in `offsets` space; `line` is space-independent.
    /// Defaults to `"chars"`, so they index a Python `str` directly. This is a method
    /// rather than a property because it now carries that keyword.
    #[pyo3(signature = (*, offsets="chars"))]
    fn diagnostics(&self, py: Python<'_>, offsets: &str) -> PyResult<PyObject> {
        json_loads(
            py,
            &self.doc().to_diagnostics_json_with(offsets_from(offsets)),
        )
    }

    /// The tape as packed bytes: an 8-word header, then 4 words per node, little-endian
    /// `uint32`.
    ///
    /// The low-level view, for callers who want to process the whole tape numerically
    /// (e.g. `numpy.frombuffer(doc.tape(), dtype="<u4")`) rather than walk a dict. Pair
    /// it with `tape_schema()` to resolve the `kind` and `marker` ids.
    ///
    /// For ordinary traversal prefer `cst()`: unlike the JS binding, a Python cursor
    /// would be an in-process PyO3 object with no serialization to avoid, so packing
    /// buys nothing on that path. (Such a cursor is a planned addition.)
    #[pyo3(signature = (*, offsets="chars"))]
    fn tape<'py>(&self, py: Python<'py>, offsets: &str) -> Bound<'py, PyBytes> {
        let words = self.doc().to_packed_tape(offsets_from(offsets));
        // `Vec<u32>` -> little-endian bytes. Explicit rather than a transmute so the
        // wire format stays byte-identical on a big-endian host.
        let mut bytes = Vec::with_capacity(words.len() * 4);
        for w in words {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        PyBytes::new(py, &bytes)
    }
}

/// Parse USFM into a lazy handle. Never raises on malformed input — the parser
/// recovers and (when `diagnostics=True`) reports.
#[pyfunction]
#[pyo3(signature = (usfm, *, diagnostics=false, version="3.1"))]
fn parse(usfm: String, diagnostics: bool, version: &str) -> ParsedDocument {
    ParsedDocument::build(usfm, version_from(version), diagnostics)
}

/// One-shot convenience: parse and return the concrete syntax tree as a dict.
#[pyfunction]
#[pyo3(signature = (usfm, *, diagnostics=false, version="3.1"))]
fn parse_cst(py: Python<'_>, usfm: String, diagnostics: bool, version: &str) -> PyResult<PyObject> {
    let doc = ParsedDocument::build(usfm, version_from(version), diagnostics);
    doc.cst(py)
}

/// The marker/kind schema needed to interpret a packed tape from `ParsedDocument.tape`.
///
/// A module-level function: it needs no document, and it is small and version-stable,
/// so callers can cache it.
#[pyfunction]
#[pyo3(signature = (*, version="3.1"))]
fn tape_schema(py: Python<'_>, version: &str) -> PyResult<PyObject> {
    json_loads(py, &usfm3_lib::schema_json(version_from(version)))
}

#[pymodule]
fn usfm3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_cst, m)?)?;
    m.add_function(wrap_pyfunction!(tape_schema, m)?)?;
    m.add_class::<ParsedDocument>()?;
    Ok(())
}

fn version_from(s: &str) -> usfm3_lib::Version {
    match s {
        "3.2" => usfm3_lib::Version::V3_2,
        _ => usfm3_lib::Version::V3_1,
    }
}

/// Python `str` is indexed by code point, so `"chars"` is this binding's default.
/// Unrecognized names fall back to it rather than raising.
fn offsets_from(s: &str) -> usfm3_lib::Offsets {
    usfm3_lib::Offsets::from_name(Some(s), usfm3_lib::Offsets::Chars)
}

/// Decode a JSON string emitted by the core serializer into a Python object using
/// the standard-library `json` module. The input is always well-formed JSON (we
/// produced it), so an error here is an internal bug.
fn json_loads(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    let json = PyModule::import(py, "json")?;
    Ok(json.call_method1("loads", (s,))?.unbind())
}
