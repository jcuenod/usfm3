use pyo3::prelude::*;
use serde::Serialize;

use ::usfm3 as usfm3_lib;

#[pyclass(unsendable)]
struct ParsedDocument {
    inner: usfm3_lib::ParsedDocument,
}

#[pymethods]
impl ParsedDocument {
    fn cst(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_object(py, &usfm3_lib::cst::export(self.inner.cst()))
    }

    fn ast(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_object(py, self.inner.ast())
    }

    fn source_map(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_object(py, self.inner.source_map())
    }

    #[getter]
    fn diagnostics(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_object(py, &self.inner.diagnostics())
    }

    #[pyo3(signature = (spans=false))]
    fn to_usj(&self, py: Python<'_>, spans: bool) -> PyResult<PyObject> {
        let value = self
            .inner
            .to_usj(usfm3_lib::usj::UsjOptions {
                include_spans: spans,
            })
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?;
        to_py_object(py, &value)
    }

    fn to_usx(&self) -> PyResult<String> {
        self.inner
            .to_usx()
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
    }

    fn to_usfm(&self) -> String {
        self.inner.to_usfm()
    }

    fn to_vref(&self, py: Python<'_>) -> PyResult<PyObject> {
        to_py_object(py, &self.inner.to_vref())
    }
}

#[pyfunction]
#[pyo3(signature = (usfm, diagnostics=false))]
fn parse(usfm: &str, diagnostics: bool) -> ParsedDocument {
    ParsedDocument {
        inner: usfm3_lib::parse(usfm, usfm3_lib::ParseOptions { diagnostics }),
    }
}

#[pyfunction]
fn parse_cst(py: Python<'_>, usfm: &str) -> PyResult<PyObject> {
    let cst = usfm3_lib::parse_cst(usfm);
    to_py_object(py, &usfm3_lib::cst::export(&cst))
}

#[pyfunction]
#[pyo3(signature = (usfm, diagnostics=false))]
fn parse_ast(py: Python<'_>, usfm: &str, diagnostics: bool) -> PyResult<PyObject> {
    let ast_document = usfm3_lib::parse_ast(usfm, usfm3_lib::ParseOptions { diagnostics });
    to_py_object(py, &ast_document)
}

#[pyfunction]
fn tokenize(py: Python<'_>, usfm: &str) -> PyResult<PyObject> {
    to_py_object(py, &usfm3_lib::tokenize(usfm))
}

#[pymodule]
fn usfm3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_cst, m)?)?;
    m.add_function(wrap_pyfunction!(parse_ast, m)?)?;
    m.add_function(wrap_pyfunction!(tokenize, m)?)?;
    m.add_class::<ParsedDocument>()?;
    Ok(())
}

fn to_py_object<T: Serialize>(py: Python<'_>, value: &T) -> PyResult<PyObject> {
    pythonize::pythonize(py, value)
        .map(|object| object.unbind())
        .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
}
