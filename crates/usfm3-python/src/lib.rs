use pyo3::prelude::*;
use pyo3::types::PyDict;

use ::usfm3 as usfm3_lib;

#[pyclass]
#[derive(Clone)]
struct Diagnostic {
    #[pyo3(get)]
    severity: String,
    #[pyo3(get)]
    message: String,
    #[pyo3(get)]
    code: String,
    #[pyo3(get)]
    start: usize,
    #[pyo3(get)]
    end: usize,
}

#[pymethods]
impl Diagnostic {
    fn __repr__(&self) -> String {
        format!(
            "Diagnostic(severity='{}', code='{}', message='{}', span={}..{})",
            self.severity, self.code, self.message, self.start, self.end
        )
    }
}

#[pyclass]
struct ParseResult {
    ast: usfm3_lib::ast::Document,
    diagnostics: Vec<Diagnostic>,
}

#[pymethods]
impl ParseResult {
    /// Returns USJ as a native Python dict.
    #[pyo3(signature = (spans=false))]
    fn to_usj(&self, py: Python<'_>, spans: bool) -> PyResult<PyObject> {
        let value = usfm3_lib::usj::to_usj_value_with_options(
            &self.ast,
            usfm3_lib::usj::UsjOptions {
                include_spans: spans,
            },
        )
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        pythonize::pythonize(py, &value)
            .map(|obj| obj.unbind())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Returns USX XML as a string.
    fn to_usx(&self) -> PyResult<String> {
        usfm3_lib::usx::to_usx_string(&self.ast)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Returns normalized USFM as a string.
    fn to_usfm(&self) -> String {
        usfm3_lib::usfm::to_usfm_string(&self.ast)
    }

    /// Returns verse reference map as a Python dict.
    fn to_vref(&self, py: Python<'_>) -> PyResult<PyObject> {
        let map = usfm3_lib::vref::to_vref_map(&self.ast);
        let dict = PyDict::new(py);
        for (k, v) in &map {
            let val: String = v.as_str().unwrap_or_default().to_string();
            dict.set_item(k, val)?;
        }
        Ok(dict.into_any().unbind())
    }

    /// Returns parsing + validation diagnostics.
    #[getter]
    fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.clone()
    }

    /// True if any diagnostics have Error severity.
    fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == "error")
    }
}

fn convert_diagnostics(
    parse_diags: &usfm3_lib::diagnostics::DiagnosticList,
    validation_diags: &usfm3_lib::diagnostics::DiagnosticList,
) -> Vec<Diagnostic> {
    parse_diags
        .iter()
        .chain(validation_diags.iter())
        .map(|d| Diagnostic {
            severity: format!("{}", d.severity),
            message: d.message.clone(),
            code: format!("{:?}", d.code),
            start: d.span.start,
            end: d.span.end,
        })
        .collect()
}

/// Parse a USFM string and return a ParseResult.
///
/// Args:
///     usfm: The USFM source text.
///     validate: Whether to run semantic validation (default: True).
#[pyfunction]
#[pyo3(signature = (usfm, validate=true))]
fn parse(usfm: &str, validate: bool) -> ParseResult {
    let result = usfm3_lib::parse_full(usfm, usfm3_lib::ParseOptions { validate });
    let diagnostics =
        convert_diagnostics(&result.parser_diagnostics, &result.validation_diagnostics);
    ParseResult {
        ast: result.ast,
        diagnostics,
    }
}

#[pymodule]
fn usfm3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_class::<ParseResult>()?;
    m.add_class::<Diagnostic>()?;
    Ok(())
}
