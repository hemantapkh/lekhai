//! PyO3 shim over the core engine. `python/lekhai/__init__.py` wraps it as `_RustEngine`
//! and carries the user-facing docs.

// Macro-generated: `#[pymethods]` inserts `.into()` on already-`PyErr` results.
#![allow(clippy::useless_conversion)]

use lekhai::Engine as Core;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

fn err(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

#[pyclass(frozen)]
struct Engine {
    inner: Core,
}

#[pymethods]
impl Engine {
    #[new]
    fn new(assets_dir: &str) -> PyResult<Self> {
        let inner = Core::load(assets_dir).map_err(err)?;
        Ok(Self { inner })
    }

    #[pyo3(signature = (word, prev))]
    fn best(&self, word: &str, prev: Option<&str>) -> PyResult<String> {
        self.inner.best(word, prev).map_err(err)
    }

    #[pyo3(signature = (word, prev))]
    fn analyze(&self, word: &str, prev: Option<&str>) -> PyResult<(String, Vec<String>)> {
        let a = self.inner.analyze(word, prev).map_err(err)?;
        Ok((a.best, a.candidates))
    }

    #[pyo3(signature = (word, prev))]
    fn candidates(&self, word: &str, prev: Option<&str>) -> PyResult<Vec<String>> {
        self.inner.candidates(word, prev).map_err(err)
    }

    #[getter]
    fn context_is_live(&self) -> bool {
        self.inner.context_is_live()
    }

    #[pyo3(signature = (word, prev))]
    fn explain<'py>(
        &self,
        py: Python<'py>,
        word: &str,
        prev: Option<&str>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let x = self.inner.explain(word, prev).map_err(err)?;
        let d = PyDict::new_bound(py);
        d.set_item("token", x.token)?;
        d.set_item("prev", x.prev)?;
        d.set_item("is_roman", x.is_roman)?;
        d.set_item("context_is_live", x.context_is_live)?;
        d.set_item("after_translit", x.after_translit)?;
        d.set_item("after_lexicon", x.after_lexicon)?;
        d.set_item("after_context", x.after_context)?;
        d.set_item("after_personal", x.after_personal)?;
        d.set_item("rule_spelling", x.rule_spelling)?;
        d.set_item("candidates", x.candidates)?;
        Ok(d)
    }

    fn transliterate(&self, text: &str) -> PyResult<String> {
        self.inner.transliterate(text).map_err(err)
    }

    #[pyo3(signature = (roman, word, prev))]
    fn commit(&self, roman: &str, word: &str, prev: Option<&str>) {
        self.inner.commit(roman, word, prev);
    }

    #[pyo3(signature = (roman, word, prev))]
    fn retract(&self, roman: &str, word: &str, prev: Option<&str>) {
        self.inner.retract(roman, word, prev);
    }

    fn export_learned<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new_bound(py, &self.inner.export_learned())
    }

    fn import_learned(&self, data: &[u8]) -> bool {
        self.inner.import_learned(data)
    }

    fn clear_learned(&self) {
        self.inner.clear_learned();
    }

    #[getter]
    fn learned_count(&self) -> usize {
        self.inner.learned_count()
    }

    #[getter]
    fn learned_footprint(&self) -> usize {
        self.inner.learned_footprint()
    }
}

#[pymodule]
fn _lekhai(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    Ok(())
}
