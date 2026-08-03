//! Python bindings for the `usfm3` parser.
//!
//! The core is "one tape, many views": a single parse produces a flat tape, and
//! every output (USJ, USX, USFM, CST, vref, diagnostics) is a cheap view over it.
//! So this binding exposes a single [`parse`] entry point returning a lazy
//! `ParsedDocument` handle — there is no separate token / AST / source-map stage.
//!
//! The core serializers emit JSON as strings; the JSON-shaped views
//! (`cst`/`to_usj`/`to_vref`/`diagnostics`) are decoded at the boundary with the
//! standard-library `json` module (native C speed, no extra Rust deps), so callers
//! get plain dicts/lists. `to_usx`/`to_usfm` are returned as strings.

use pyo3::prelude::*;
use pyo3::types::PyModule;

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
        json_loads(py, &self.doc().to_vref_json())
    }

    /// Diagnostics as a flat list. Empty unless `diagnostics=True` was passed.
    #[getter]
    fn diagnostics(&self, py: Python<'_>) -> PyResult<PyObject> {
        json_loads(py, &self.doc().to_diagnostics_json())
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

#[pymodule]
fn usfm3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_cst, m)?)?;
    m.add_class::<ParsedDocument>()?;
    Ok(())
}

fn version_from(s: &str) -> usfm3_lib::Version {
    match s {
        "3.2" => usfm3_lib::Version::V3_2,
        _ => usfm3_lib::Version::V3_1,
    }
}

/// Decode a JSON string emitted by the core serializer into a Python object using
/// the standard-library `json` module. The input is always well-formed JSON (we
/// produced it), so an error here is an internal bug.
fn json_loads(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    let json = PyModule::import(py, "json")?;
    Ok(json.call_method1("loads", (s,))?.unbind())
}
