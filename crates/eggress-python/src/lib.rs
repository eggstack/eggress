use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule, PyModuleMethods};
pyo3::create_exception!(_eggress, EggressError, PyException);
pyo3::create_exception!(_eggress, ConfigError, EggressError);
pyo3::create_exception!(_eggress, StartupError, EggressError);
pyo3::create_exception!(_eggress, ReloadError, EggressError);
pyo3::create_exception!(_eggress, ShutdownError, EggressError);
pyo3::create_exception!(_eggress, UnsupportedFeatureError, EggressError);
pyo3::create_exception!(_eggress, InternalError, EggressError);

fn map_error(_py: Python<'_>, err: eggress_embed::EggressError) -> PyErr {
    use eggress_embed::EggressError as E;
    let msg = err.to_string();
    match err {
        E::Config(_) => ConfigError::new_err(msg),
        E::Runtime(_) => InternalError::new_err(msg),
        E::Startup(_) => StartupError::new_err(msg),
        E::Reload(_) => ReloadError::new_err(msg),
        E::Shutdown(_) => ShutdownError::new_err(msg),
        E::UnsupportedFeature { .. } => UnsupportedFeatureError::new_err(msg),
        E::Internal(_) => InternalError::new_err(msg),
    }
}

#[pyclass]
struct PyEggressConfig {
    inner: eggress_embed::EggressConfig,
}

#[pymethods]
impl PyEggressConfig {
    #[staticmethod]
    fn from_toml(py: Python<'_>, toml_str: &str) -> PyResult<Self> {
        let config = py
            .detach(|| eggress_embed::EggressConfig::from_toml_str(toml_str))
            .map_err(|e| map_error(py, e))?;
        Ok(Self { inner: config })
    }

    #[staticmethod]
    fn from_file(py: Python<'_>, path: &str) -> PyResult<Self> {
        let config = py
            .detach(|| eggress_embed::EggressConfig::from_toml_file(path))
            .map_err(|e| map_error(py, e))?;
        Ok(Self { inner: config })
    }

    fn redacted_toml(&self, py: Python<'_>) -> PyResult<String> {
        py.detach(|| self.inner.to_redacted_toml())
            .map_err(|e| map_error(py, e))
    }
}

#[pyclass]
struct PyEggressService {
    inner: Option<eggress_embed::EggressService>,
}

#[pymethods]
impl PyEggressService {
    #[new]
    fn new(_py: Python<'_>, config: &PyEggressConfig) -> Self {
        Self {
            inner: Some(eggress_embed::EggressService::new(config.inner.clone())),
        }
    }

    #[staticmethod]
    fn from_toml(py: Python<'_>, toml_str: &str) -> PyResult<Self> {
        let svc = py
            .detach(|| eggress_embed::EggressService::from_toml_str(toml_str))
            .map_err(|e| map_error(py, e))?;
        Ok(Self { inner: Some(svc) })
    }

    #[staticmethod]
    fn from_file(py: Python<'_>, path: &str) -> PyResult<Self> {
        let svc = py
            .detach(|| eggress_embed::EggressService::from_toml_file(path))
            .map_err(|e| map_error(py, e))?;
        Ok(Self { inner: Some(svc) })
    }

    fn start(&mut self, py: Python<'_>) -> PyResult<PyEggressHandle> {
        let svc = self
            .inner
            .take()
            .ok_or_else(|| EggressError::new_err("service already started"))?;
        let handle = py
            .detach(|| svc.start_blocking())
            .map_err(|e| map_error(py, e))?;
        Ok(PyEggressHandle {
            inner: Some(handle),
        })
    }
}

#[pyclass]
struct PyEggressHandle {
    inner: Option<eggress_embed::EggressHandle>,
}

#[pymethods]
impl PyEggressHandle {
    fn bound_addresses(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let handle = self
            .inner
            .as_ref()
            .ok_or_else(|| EggressError::new_err("handle consumed"))?;
        let addrs = py.detach(|| handle.bound_addresses());
        let dict = PyDict::new(py);
        for la in &addrs.listeners {
            dict.set_item(&la.name, la.addr.to_string())?;
        }
        if let Some(admin) = addrs.admin {
            dict.set_item("_admin", admin.to_string())?;
        }
        Ok(dict.into())
    }

    fn status(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let handle = self
            .inner
            .as_ref()
            .ok_or_else(|| EggressError::new_err("handle consumed"))?;
        let st = py.detach(|| handle.status());
        let dict = PyDict::new(py);
        dict.set_item("generation", st.generation)?;
        dict.set_item("readiness", st.readiness)?;
        dict.set_item("active_connections", st.active_connections)?;
        dict.set_item("uptime_secs", st.uptime_secs)?;
        dict.set_item("listener_count", st.listener_count)?;
        dict.set_item("udp_associations_active", st.udp_associations_active)?;
        dict.set_item("upstream_count", st.upstream_count)?;
        let py_listeners = PyList::empty(py);
        for ls in &st.listeners {
            let ldict = PyDict::new(py);
            ldict.set_item("name", &ls.name)?;
            ldict.set_item("bind", &ls.bind)?;
            ldict.set_item("local_addr", ls.local_addr.to_string())?;
            ldict.set_item("protocols", &ls.protocols)?;
            ldict.set_item("udp_enabled", ls.udp_enabled)?;
            py_listeners.append(ldict)?;
        }
        dict.set_item("listeners", py_listeners)?;
        Ok(dict.into())
    }

    fn metrics_text(&self, py: Python<'_>) -> PyResult<String> {
        let handle = self
            .inner
            .as_ref()
            .ok_or_else(|| EggressError::new_err("handle consumed"))?;
        py.detach(|| handle.metrics_text())
            .map_err(|e| map_error(py, e))
    }

    fn reload_toml(&self, py: Python<'_>, toml_str: &str) -> PyResult<Py<PyDict>> {
        let handle = self
            .inner
            .as_ref()
            .ok_or_else(|| EggressError::new_err("handle consumed"))?;
        let outcome = py
            .detach(|| handle.reload_toml_str(toml_str))
            .map_err(|e| map_error(py, e))?;
        let dict = PyDict::new(py);
        match outcome {
            eggress_embed::ReloadOutcome::Applied {
                generation,
                upstreams,
            } => {
                dict.set_item("generation", generation)?;
                dict.set_item("upstreams", upstreams)?;
            }
        }
        Ok(dict.into())
    }

    fn shutdown(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(handle) = self.inner.take() {
            py.detach(|| handle.shutdown_blocking())
                .map_err(|e| map_error(py, e))?;
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
        if let Some(handle) = self.inner.take() {
            let _ = py.detach(|| handle.shutdown_blocking());
        }
        Ok(false)
    }
}

#[pymodule]
fn _eggress(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEggressConfig>()?;
    m.add_class::<PyEggressService>()?;
    m.add_class::<PyEggressHandle>()?;
    m.add("EggressError", m.py().get_type::<EggressError>())?;
    m.add("ConfigError", m.py().get_type::<ConfigError>())?;
    m.add("StartupError", m.py().get_type::<StartupError>())?;
    m.add("ReloadError", m.py().get_type::<ReloadError>())?;
    m.add("ShutdownError", m.py().get_type::<ShutdownError>())?;
    m.add(
        "UnsupportedFeatureError",
        m.py().get_type::<UnsupportedFeatureError>(),
    )?;
    m.add("InternalError", m.py().get_type::<InternalError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
