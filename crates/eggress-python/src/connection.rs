//! Compatibility `Connection` object: state machine plus connection counters.
//!
//! Isolated from the native service/outbound classes; connection accounting
//! (`PY_CONNECTION_*`) lives here with its only consumer.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PySequence};

use super::errors::{ConnectionError, UnsupportedFeatureError};

pub(crate) static PY_CONNECTION_LIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
pub(crate) static PY_CONNECTION_TOTAL_CREATED: AtomicUsize = AtomicUsize::new(0);

const STATE_CREATED: u8 = 0;
const STATE_CONNECTING: u8 = 1;
const STATE_CONNECTED: u8 = 2;
const STATE_CLOSING: u8 = 3;
const STATE_CLOSED: u8 = 4;
const STATE_FAILED: u8 = 5;

#[pyclass]
pub(crate) struct PyConnection {
    state: Arc<AtomicU8>,
    handle: Option<eggress_embed::EggressHandle>,
    config_toml: String,
    bound_addr: Option<String>,
    remote_addr: Option<String>,
    peername: Option<String>,
    sockname: Option<String>,
    error: Option<String>,
}

#[pymethods]
impl PyConnection {
    #[new]
    #[pyo3(signature = (uris, /, *args))]
    fn new(py: Python<'_>, uris: &Bound<'_, PySequence>, args: Vec<String>) -> PyResult<Self> {
        let mut all_args: Vec<String> = Vec::new();
        for i in 0..uris.len()? {
            all_args.push(uris.get_item(i)?.extract::<String>()?);
        }
        all_args.extend(args);

        if all_args.is_empty() {
            return Err(ConnectionError::new_err(
                "at least one URI argument is required",
            ));
        }

        let parsed = eggress_pproxy_compat::PproxyArgs::parse(&all_args)
            .map_err(|e| ConnectionError::new_err(format!("argument parse error: {e}")))?;

        // Typed native translation: intermediates -> TOML (display) + RuntimeConfig
        // (startup) in one build, no TOML serialize/parse round trip for startup.
        let combined = py
            .detach(|| eggress_pproxy_compat::translate_pproxy_args_to_native(&parsed))
            .map_err(|e| ConnectionError::new_err(format!("translation failed: {e}")))?;

        if combined.has_unsupported() {
            let features: Vec<_> = combined.unsupported().iter().map(|u| u.feature).collect();
            return Err(UnsupportedFeatureError::new_err(format!(
                "unsupported features: {}",
                features.join(", ")
            )));
        }

        let output_toml = combined.toml.clone();
        let config =
            eggress_embed::EggressConfig::from_compiled(combined.runtime, output_toml.clone());
        let service = eggress_embed::EggressService::new(config);
        let handle = py
            .detach(|| service.start_blocking())
            .map_err(|e| ConnectionError::new_err(format!("startup failed: {e}")))?;

        let addrs = handle.bound_addresses();
        let bound = addrs.listeners.first().map(|l| l.addr.to_string());

        PY_CONNECTION_TOTAL_CREATED.fetch_add(1, Ordering::Relaxed);
        PY_CONNECTION_LIVE_COUNT.fetch_add(1, Ordering::Relaxed);

        Ok(Self {
            state: Arc::new(AtomicU8::new(STATE_CREATED)),
            handle: Some(handle),
            config_toml: output_toml,
            bound_addr: bound,
            remote_addr: None,
            peername: None,
            sockname: None,
            error: None,
        })
    }

    #[getter]
    fn state(&self) -> &str {
        match self.state.load(Ordering::Acquire) {
            STATE_CREATED => "created",
            STATE_CONNECTING => "connecting",
            STATE_CONNECTED => "connected",
            STATE_CLOSING => "closing",
            STATE_CLOSED => "closed",
            STATE_FAILED => "failed",
            _ => "unknown",
        }
    }

    #[getter]
    fn closed(&self) -> bool {
        matches!(
            self.state.load(Ordering::Acquire),
            STATE_CLOSED | STATE_FAILED
        )
    }

    #[getter]
    fn config(&self) -> &str {
        &self.config_toml
    }

    #[getter]
    fn peername(&self) -> Option<&str> {
        self.peername.as_deref()
    }

    #[getter]
    fn sockname(&self) -> Option<&str> {
        self.sockname.as_deref().or(self.bound_addr.as_deref())
    }

    #[getter]
    fn extra_info(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("state", self.state())?;
        if let Some(ref addr) = self.bound_addr {
            dict.set_item("bound_addr", addr)?;
        }
        if let Some(ref addr) = self.remote_addr {
            dict.set_item("remote_addr", addr)?;
        }
        if let Some(ref err) = self.error {
            dict.set_item("error", err)?;
        }
        Ok(dict.into())
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if !begin_close(&self.state) {
            return Ok(());
        }
        if let Some(handle) = self.handle.take() {
            py.detach(|| handle.shutdown_blocking())
                .map_err(|e| ConnectionError::new_err(format!("shutdown error: {e}")))?;
        }
        self.state.store(STATE_CLOSED, Ordering::Release);
        let _ = PY_CONNECTION_LIVE_COUNT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_sub(1));
        Ok(())
    }

    fn wait_closed(&mut self, py: Python<'_>) -> PyResult<()> {
        let current = self.state.load(Ordering::Acquire);
        if current == STATE_CLOSED || current == STATE_FAILED {
            return Ok(());
        }
        self.close(py)?;
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
        self.close(py)?;
        Ok(false)
    }

    fn __del__(&mut self) {
        if !begin_close(&self.state) {
            return;
        }
        eprintln!(
            "Warning: Connection object was not properly closed. Calling close() in __del__."
        );
        if let Some(handle) = self.handle.take() {
            match tokio::runtime::Handle::try_current() {
                Ok(runtime) => {
                    runtime.spawn(async move {
                        if let Err(error) = handle.shutdown().await {
                            eprintln!("shutdown error in __del__: {error}");
                        }
                    });
                }
                Err(error) => {
                    // No runtime available: do not spawn a thread during
                    // interpreter finalization (UB-prone). Drop the handle so
                    // the runtime can reap it; log the deferred shutdown.
                    eprintln!(
                        "could not schedule shutdown in __del__: {error}; \
                         dropping handle to be reaped by runtime"
                    );
                    drop(handle);
                }
            }
        }
        self.state.store(STATE_CLOSED, Ordering::Release);
        // Guard against underflow on double-close paths.
        let _ = PY_CONNECTION_LIVE_COUNT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_sub(1));
    }

    fn __repr__(&self) -> String {
        format!(
            "Connection(state='{}', bound='{}')",
            self.state(),
            self.bound_addr.as_deref().unwrap_or("None")
        )
    }

    #[staticmethod]
    fn connection_stats(py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("live", PY_CONNECTION_LIVE_COUNT.load(Ordering::Relaxed))?;
        dict.set_item(
            "total_created",
            PY_CONNECTION_TOTAL_CREATED.load(Ordering::Relaxed),
        )?;
        Ok(dict.into())
    }

    #[staticmethod]
    fn reset_connection_stats() {
        PY_CONNECTION_LIVE_COUNT.store(0, Ordering::Relaxed);
        PY_CONNECTION_TOTAL_CREATED.store(0, Ordering::Relaxed);
    }
}

pub(crate) fn begin_close(state: &AtomicU8) -> bool {
    loop {
        let current = state.load(Ordering::Acquire);
        if current == STATE_CLOSED || current == STATE_FAILED || current == STATE_CLOSING {
            return false;
        }
        match state.compare_exchange(current, STATE_CLOSING, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(_) => continue,
        }
    }
}
