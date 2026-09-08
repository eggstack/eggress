//! System-proxy bridging: apply/restore the OS proxy from Python.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass]
pub(crate) struct PyAppliedSystemProxy {
    inner: Option<eggress_system_proxy::AppliedProxy>,
}

#[pymethods]
impl PyAppliedSystemProxy {
    fn restore(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(mut applied) = self.inner.take() {
            py.detach(|| applied.restore())
                .map_err(PyValueError::new_err)?;
        }
        Ok(())
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        py: Python<'_>,
        _exc_type: &Bound<'_, PyAny>,
        _exc_value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.restore(py)?;
        Ok(false)
    }
}

#[pyfunction]
pub(crate) fn apply_system_proxy(
    py: Python<'_>,
    kind: &str,
    address: &str,
) -> PyResult<PyAppliedSystemProxy> {
    let kind = match kind {
        "http" => eggress_system_proxy::CompatibilityProxyKind::Http,
        "socks5" => eggress_system_proxy::CompatibilityProxyKind::Socks5,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown proxy kind: {other}"
            )))
        }
    };
    let address = address
        .parse()
        .map_err(|e| PyValueError::new_err(format!("invalid proxy address: {e}")))?;
    let inner = py
        .detach(|| eggress_system_proxy::apply_compatibility_proxy(kind, address))
        .map_err(PyValueError::new_err)?;
    Ok(PyAppliedSystemProxy { inner: Some(inner) })
}
