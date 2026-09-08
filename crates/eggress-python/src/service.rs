//! Native service lifecycle: config, service, and handle classes.

use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::errors::{map_error, EggressError};

#[pyclass]
pub(crate) struct PyEggressConfig {
    pub(crate) inner: eggress_embed::EggressConfig,
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
pub(crate) struct PyEggressService {
    pub(crate) inner: Option<eggress_embed::EggressService>,
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

    /// Start with the compatibility-only runtime options parsed from pproxy
    /// arguments. Native Eggress service startup never uses this path.
    fn start_with_compatibility_options(
        &mut self,
        py: Python<'_>,
        auth_timeout_seconds: u64,
        system_proxy: bool,
        debug: bool,
        verbose_level: u8,
    ) -> PyResult<PyEggressHandle> {
        let svc = self
            .inner
            .take()
            .ok_or_else(|| EggressError::new_err("service already started"))?;
        let options = eggress_runtime::CompatibilityOptions {
            compatibility_mode: true,
            auth_timeout: Some(Duration::from_secs(auth_timeout_seconds)),
            system_proxy,
            debug,
            verbose_level,
        };
        let handle = py
            .detach(|| svc.start_blocking_with_compatibility_options(options))
            .map_err(|e| map_error(py, e))?;
        Ok(PyEggressHandle {
            inner: Some(handle),
        })
    }
}

#[pyclass]
pub(crate) struct PyEggressHandle {
    pub(crate) inner: Option<eggress_embed::EggressHandle>,
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
            if let Err(e) = py.detach(|| handle.shutdown_blocking()) {
                eprintln!("shutdown error in __exit__: {e}");
            }
        }
        Ok(false)
    }
}
