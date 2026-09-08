//! Listener-free outbound API: connector and stream classes.
//!
//! `PyOutboundConnector` builds chains natively (no TOML round trip) and
//! `PyOutboundStream` bridges async I/O onto the shared runtime with
//! loop-affinity and idempotent close semantics.

use std::sync::Arc;
use std::time::Duration;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::errors::{ConnectionClosedError, ConnectionError};
use super::runtime::outbound_runtime;

/// Python-accessible outbound connector for direct proxy chain connections.
///
/// This wraps the Rust `OutboundConnector` and provides a Python interface
/// for making outbound TCP connections through a configured proxy chain
/// without starting a listener service.
///
/// The returned stream owns the Tokio runtime needed by the Rust transport,
/// so it remains usable after the connector is dropped. Blocking methods
/// release the Python GIL while they wait for network I/O.
#[pyclass]
pub(crate) struct PyOutboundConnector {
    inner: eggress_embed::outbound::OutboundConnector,
}

pub(crate) struct OutboundStreamState {
    stream: Option<eggress_core::BoxStream>,
}

/// A connected native outbound stream.
///
/// This deliberately exposes a small socket/asyncio-stream compatible surface
/// instead of a file descriptor. Advanced transports (TLS, WebSocket, H2,
/// and multi-hop chains) do not necessarily have a meaningful OS socket after
/// the first hop. `recv`/`sendall` aliases are provided for pproxy programs;
/// `read`/`write` are the canonical APIs.
#[pyclass]
pub(crate) struct PyOutboundStream {
    runtime: Arc<tokio::runtime::Runtime>,
    state: std::sync::Mutex<OutboundStreamState>,
    peer_addr: Option<String>,
    local_addr: Option<String>,
    hop_count: usize,
}

impl PyOutboundStream {
    fn with_stream<T>(
        &self,
        operation: impl FnOnce(&mut eggress_core::BoxStream) -> T,
    ) -> PyResult<T> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ConnectionError::new_err("outbound stream lock poisoned"))?;
        let stream = state
            .stream
            .as_mut()
            .ok_or_else(|| ConnectionClosedError::new_err("outbound stream is closed"))?;
        Ok(operation(stream))
    }

    fn closed_inner(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.stream.is_none())
            .unwrap_or(true)
    }
}

impl Drop for PyOutboundStream {
    fn drop(&mut self) {
        // Dropping the BoxStream closes the transport. Do not block from a
        // destructor: Python may be shutting down and the runtime can be
        // unavailable at that point. Use try_lock to avoid blocking the
        // Python GC while another thread holds the state mutex.
        if let Ok(mut state) = self.state.try_lock() {
            state.stream.take();
        }
    }
}

#[pymethods]
impl PyOutboundStream {
    #[getter]
    fn closed(&self) -> bool {
        self.closed_inner()
    }

    fn is_closing(&self) -> bool {
        self.closed_inner()
    }

    #[getter]
    fn peername(&self) -> Option<&str> {
        self.peer_addr.as_deref()
    }

    #[getter]
    fn sockname(&self) -> Option<&str> {
        self.local_addr.as_deref()
    }

    fn get_extra_info(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match name {
            "peername" => match self.peer_addr.as_ref() {
                Some(value) => Ok(PyString::new(py, value).into_any().unbind()),
                None => Ok(default.unwrap_or_else(|| py.None())),
            },
            "sockname" => match self.local_addr.as_ref() {
                Some(value) => Ok(PyString::new(py, value).into_any().unbind()),
                None => Ok(default.unwrap_or_else(|| py.None())),
            },
            "hop_count" => {
                let hop_count = self.hop_count.to_string();
                Ok(PyString::new(py, &hop_count).into_any().unbind())
            }
            _ => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    const MAX_READ_LEN: usize = 16 * 1024 * 1024;

    fn read(&self, py: Python<'_>, n: i64) -> PyResult<Vec<u8>> {
        if n < -1 {
            return Err(PyValueError::new_err(
                "read length must be -1 or non-negative",
            ));
        }
        let len = if n == -1 {
            None
        } else {
            let len = usize::try_from(n).map_err(|_| {
                PyValueError::new_err(format!(
                    "read length does not fit in usize on this platform (n={n})"
                ))
            })?;
            if len > Self::MAX_READ_LEN {
                return Err(PyValueError::new_err(format!(
                    "read length {len} exceeds maximum {}",
                    Self::MAX_READ_LEN
                )));
            }
            Some(len)
        };
        let runtime = self.runtime.clone();
        self.with_stream(|stream| {
            py.detach(|| {
                runtime.block_on(async {
                    if let Some(len) = len {
                        let mut data = vec![0_u8; len];
                        let count = stream.read(&mut data).await?;
                        data.truncate(count);
                        Ok(data)
                    } else {
                        let mut data = Vec::new();
                        let mut limited = stream.take(Self::MAX_READ_LEN as u64);
                        limited.read_to_end(&mut data).await.map(|_| data)
                    }
                })
            })
        })?
        .map_err(|e: std::io::Error| ConnectionError::new_err(format!("read failed: {e}")))
    }

    fn readexactly(&self, py: Python<'_>, n: usize) -> PyResult<Vec<u8>> {
        if n > Self::MAX_READ_LEN {
            return Err(PyValueError::new_err(format!(
                "readexactly length {n} exceeds maximum {}",
                Self::MAX_READ_LEN
            )));
        }
        let runtime = self.runtime.clone();
        self.with_stream(|stream| {
            py.detach(|| {
                runtime.block_on(async {
                    let mut data = vec![0_u8; n];
                    stream.read_exact(&mut data).await.map(|_| data)
                })
            })
        })?
        .map_err(|e: std::io::Error| ConnectionError::new_err(format!("readexactly failed: {e}")))
    }

    fn write(&self, py: Python<'_>, data: &[u8]) -> PyResult<usize> {
        let runtime = self.runtime.clone();
        self.with_stream(|stream| {
            py.detach(|| runtime.block_on(async { stream.write(data).await }))
        })?
        .map_err(|e: std::io::Error| ConnectionError::new_err(format!("write failed: {e}")))
    }

    fn sendall(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        let runtime = self.runtime.clone();
        self.with_stream(|stream| {
            py.detach(|| runtime.block_on(async { stream.write_all(data).await }))
        })?
        .map_err(|e: std::io::Error| ConnectionError::new_err(format!("sendall failed: {e}")))
    }

    fn drain(&self, py: Python<'_>) -> PyResult<()> {
        let runtime = self.runtime.clone();
        self.with_stream(|stream| py.detach(|| runtime.block_on(async { stream.flush().await })))?
            .map_err(|e: std::io::Error| ConnectionError::new_err(format!("drain failed: {e}")))
    }

    fn write_eof(&self, py: Python<'_>) -> PyResult<()> {
        let runtime = self.runtime.clone();
        self.with_stream(|stream| {
            py.detach(|| runtime.block_on(async { stream.shutdown().await }))
        })?
        .map_err(|e: std::io::Error| ConnectionError::new_err(format!("write_eof failed: {e}")))
    }

    fn close(&self) -> PyResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ConnectionError::new_err("outbound stream lock poisoned"))?;
        state.stream.take();
        Ok(())
    }

    fn wait_closed(&self) -> PyResult<()> {
        self.close()
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &self,
        _py: Python<'_>,
        _exc_type: &Bound<'_, PyAny>,
        _exc_value: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.close()?;
        Ok(false)
    }

    fn __del__(&self) {
        if !self.closed_inner() {
            eprintln!("Warning: outbound stream was not explicitly closed; cleaning up");
            let _ = self.close();
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "OutboundStream(peername={:?}, hop_count={}, closed={})",
            self.peer_addr,
            self.hop_count,
            self.closed_inner()
        )
    }
}

#[pymethods]
impl PyOutboundConnector {
    /// Create a connector from a pproxy-style URI string.
    #[staticmethod]
    fn from_pproxy_uri(uri: &str) -> PyResult<Self> {
        let inner = eggress_embed::outbound::OutboundConnector::from_pproxy_uri(uri)
            .map_err(|e| ConnectionError::new_err(format!("failed to create connector: {e}")))?;
        Ok(Self { inner })
    }

    /// Create a connector from a TOML config string.
    #[staticmethod]
    fn from_toml(config_toml: &str) -> PyResult<Self> {
        let inner = eggress_embed::outbound::OutboundConnector::from_toml(config_toml)
            .map_err(|e| ConnectionError::new_err(format!("failed to create connector: {e}")))?;
        Ok(Self { inner })
    }

    /// Validate that a TOML config is usable for outbound connections.
    ///
    /// Returns the number of hops in the first upstream's chain.
    #[staticmethod]
    fn validate_config(config_toml: &str) -> PyResult<usize> {
        eggress_embed::outbound::OutboundConnector::validate_outbound_config(config_toml)
            .map_err(|e| ConnectionError::new_err(format!("config validation failed: {e}")))
    }

    /// Get the number of upstreams configured.
    fn upstream_count(&self) -> usize {
        self.inner.upstream_count()
    }

    /// Open a native outbound TCP stream. No local listener is created.
    #[pyo3(signature = (host, port, timeout=None))]
    fn connect_tcp(
        &self,
        py: Python<'_>,
        host: &str,
        port: u16,
        timeout: Option<f64>,
    ) -> PyResult<PyOutboundStream> {
        if host.is_empty() {
            return Err(PyValueError::new_err("host must not be empty"));
        }
        let timeout = timeout
            .map(|seconds| {
                if !seconds.is_finite() || seconds <= 0.0 {
                    Err(PyValueError::new_err("timeout must be finite and positive"))
                } else {
                    Ok(Duration::from_secs_f64(seconds))
                }
            })
            .transpose()?;
        let runtime = outbound_runtime().map_err(ConnectionError::new_err)?;
        let inner = &self.inner;
        let result = py.detach(|| {
            runtime.block_on(async {
                match timeout {
                    Some(timeout) => inner.connect_tcp_timeout(host, port, timeout).await,
                    None => inner.connect_tcp(host, port).await,
                }
            })
        });
        let (stream, info) = result
            .map_err(|e| ConnectionError::new_err(format!("outbound connect failed: {e}")))?;
        Ok(PyOutboundStream {
            runtime,
            state: std::sync::Mutex::new(OutboundStreamState {
                stream: Some(stream),
            }),
            peer_addr: info.peer_addr.map(|addr| addr.to_string()),
            local_addr: info.local_addr.map(|addr| addr.to_string()),
            hop_count: info.hop_count,
        })
    }

    /// Get connection metadata for a target host:port.
    ///
    /// Resolves the first hop endpoint and returns metadata about the
    /// configured chain without actually connecting.
    fn preview_connect(&self, py: Python<'_>, host: &str, port: u16) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("target_host", host)?;
        dict.set_item("target_port", port)?;
        dict.set_item("hop_count", self.inner.upstream_count())?;
        Ok(dict.into())
    }
}
