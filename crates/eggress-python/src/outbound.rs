//! Listener-free outbound API: connector and stream classes.
//!
//! `PyOutboundConnector` builds chains natively (no TOML round trip) and
//! `PyOutboundStream` bridges async I/O onto the shared runtime with
//! loop-affinity and idempotent close semantics.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use tokio::io::{split, AsyncReadExt, AsyncWriteExt, ReadHalf};
use tokio::sync::{mpsc, oneshot};

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

enum WriteCommand {
    Data(Vec<u8>),
    Barrier(oneshot::Sender<Result<(), String>>),
    Eof(oneshot::Sender<Result<(), String>>),
}

struct WritePumpStatus {
    terminal_error: Option<String>,
    closed: bool,
    eof: bool,
}

struct WritePump {
    sender: Mutex<Option<mpsc::UnboundedSender<WriteCommand>>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    status: Arc<Mutex<WritePumpStatus>>,
    submit_lock: Mutex<()>,
}

impl WritePump {
    fn new(
        runtime: &tokio::runtime::Runtime,
        stream: eggress_core::BoxStream,
    ) -> (ReadHalf<eggress_core::BoxStream>, Self) {
        let (read_half, mut write_half) = split(stream);
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let status = Arc::new(Mutex::new(WritePumpStatus {
            terminal_error: None,
            closed: false,
            eof: false,
        }));
        let task_status = Arc::clone(&status);
        let task = runtime.spawn(async move {
            while let Some(command) = receiver.recv().await {
                match command {
                    WriteCommand::Data(data) => {
                        if let Err(error) = write_half.write_all(&data).await {
                            let message = bounded_error(error.to_string());
                            if let Ok(mut state) = task_status.lock() {
                                state.terminal_error = Some(message.clone());
                                state.closed = true;
                            }
                            while let Ok(command) = receiver.try_recv() {
                                match command {
                                    WriteCommand::Barrier(waiter) | WriteCommand::Eof(waiter) => {
                                        let _ = waiter.send(Err(message.clone()));
                                    }
                                    WriteCommand::Data(_) => {}
                                }
                            }
                            break;
                        }
                    }
                    WriteCommand::Barrier(waiter) => {
                        let result = task_status
                            .lock()
                            .ok()
                            .and_then(|state| state.terminal_error.clone())
                            .map_or(Ok(()), Err);
                        let _ = waiter.send(result);
                    }
                    WriteCommand::Eof(waiter) => {
                        let prior_error = task_status
                            .lock()
                            .ok()
                            .and_then(|state| state.terminal_error.clone());
                        let result = if let Some(error) = prior_error {
                            Err(error)
                        } else {
                            match write_half.shutdown().await {
                                Ok(()) => {
                                    if let Ok(mut state) = task_status.lock() {
                                        state.eof = true;
                                    }
                                    Ok(())
                                }
                                Err(error) => {
                                    let message = bounded_error(error.to_string());
                                    if let Ok(mut state) = task_status.lock() {
                                        state.terminal_error = Some(message.clone());
                                        state.closed = true;
                                    }
                                    Err(message)
                                }
                            }
                        };
                        let _ = waiter.send(result);
                    }
                }
            }
        });
        (
            read_half,
            Self {
                sender: Mutex::new(Some(sender)),
                task: Mutex::new(Some(task)),
                status,
                submit_lock: Mutex::new(()),
            },
        )
    }

    fn submit(&self, data: &[u8]) -> PyResult<usize> {
        let _submit_guard = self
            .submit_lock
            .lock()
            .map_err(|_| ConnectionError::new_err("outbound write lock poisoned"))?;
        let sender = {
            let state = self
                .status
                .lock()
                .map_err(|_| ConnectionError::new_err("outbound write state poisoned"))?;
            if let Some(error) = &state.terminal_error {
                return Err(ConnectionError::new_err(format!("write failed: {error}")));
            }
            if state.closed || state.eof {
                return Err(ConnectionClosedError::new_err("outbound stream is closed"));
            }
            self.sender
                .lock()
                .map_err(|_| ConnectionError::new_err("outbound write lock poisoned"))?
                .clone()
                .ok_or_else(|| ConnectionClosedError::new_err("outbound stream is closed"))?
        };
        let len = data.len();
        if sender.send(WriteCommand::Data(data.to_vec())).is_err() {
            if let Ok(mut state) = self.status.lock() {
                state.terminal_error = Some("write pump stopped".to_string());
                state.closed = true;
            }
            return Err(ConnectionError::new_err("write failed: write pump stopped"));
        }
        Ok(len)
    }

    fn submit_and_wait(&self, runtime: &tokio::runtime::Runtime, data: &[u8]) -> PyResult<usize> {
        // Synchronous completion semantics: enqueue, then wait for the
        // ordered transport write to complete. Native `PyOutboundStream.write()`
        // delegates to exactly this helper; the async adapter uses queue-only
        // `submit()` instead. Private Rust detail, never exposed via PyO3.
        let len = self.submit(data)?;
        self.barrier(runtime, false)?;
        Ok(len)
    }

    fn barrier(&self, runtime: &tokio::runtime::Runtime, eof: bool) -> PyResult<()> {
        let _submit_guard = self
            .submit_lock
            .lock()
            .map_err(|_| ConnectionError::new_err("outbound write lock poisoned"))?;
        let (waiter, receiver) = oneshot::channel();
        {
            let state = self
                .status
                .lock()
                .map_err(|_| ConnectionError::new_err("outbound write state poisoned"))?;
            if let Some(error) = &state.terminal_error {
                return Err(ConnectionError::new_err(format!(
                    "{} failed: {error}",
                    if eof { "write_eof" } else { "drain" }
                )));
            }
            if state.closed {
                return Err(ConnectionClosedError::new_err("outbound stream is closed"));
            }
            if eof && state.eof {
                return Ok(());
            }
            let sender = self
                .sender
                .lock()
                .map_err(|_| ConnectionError::new_err("outbound write lock poisoned"))?
                .clone()
                .ok_or_else(|| ConnectionClosedError::new_err("outbound stream is closed"))?;
            let command = if eof {
                WriteCommand::Eof(waiter)
            } else {
                WriteCommand::Barrier(waiter)
            };
            if sender.send(command).is_err() {
                return Err(ConnectionError::new_err("drain failed: write pump stopped"));
            }
        }
        let result = runtime.block_on(receiver);
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(ConnectionError::new_err(format!(
                "{} failed: {error}",
                if eof { "write_eof" } else { "drain" }
            ))),
            Err(_) => Err(ConnectionError::new_err("drain failed: write pump stopped")),
        }
    }

    fn close(&self) -> PyResult<()> {
        if let Ok(mut state) = self.status.lock() {
            state.closed = true;
        } else {
            return Err(ConnectionError::new_err("outbound write state poisoned"));
        }
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        if let Ok(task) = self.task.lock() {
            if let Some(task) = task.as_ref() {
                task.abort();
            }
        }
        Ok(())
    }

    fn wait_closed(&self, runtime: &tokio::runtime::Runtime) -> PyResult<()> {
        let task = self
            .task
            .lock()
            .map_err(|_| ConnectionError::new_err("outbound write lock poisoned"))?
            .take();
        if let Some(task) = task {
            let _ = runtime.block_on(task);
        }
        Ok(())
    }
}

impl Drop for WritePump {
    fn drop(&mut self) {
        if let Ok(mut state) = self.status.lock() {
            state.closed = true;
        }
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        if let Ok(mut task) = self.task.lock() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }
}

fn bounded_error(message: String) -> String {
    const MAX_ERROR_LEN: usize = 512;
    if message.len() <= MAX_ERROR_LEN {
        message
    } else {
        let mut bounded: String = message.chars().take(MAX_ERROR_LEN - 1).collect();
        bounded.push('…');
        bounded
    }
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
    read_state: Mutex<Option<ReadHalf<eggress_core::BoxStream>>>,
    write_pump: WritePump,
    peer_addr: Option<String>,
    local_addr: Option<String>,
    hop_count: usize,
}

impl PyOutboundStream {
    fn with_read_stream<T>(
        &self,
        operation: impl FnOnce(&mut ReadHalf<eggress_core::BoxStream>) -> T,
    ) -> PyResult<T> {
        let mut state = self
            .read_state
            .lock()
            .map_err(|_| ConnectionError::new_err("outbound read lock poisoned"))?;
        let stream = state
            .as_mut()
            .ok_or_else(|| ConnectionClosedError::new_err("outbound stream is closed"))?;
        Ok(operation(stream))
    }

    fn write_blocking(&self, py: Python<'_>, data: &[u8]) -> PyResult<usize> {
        // Delegate to the exact synchronous-completion helper exercised by the
        // deterministic gated-transport tests. The owned copy keeps the borrow
        // valid across GIL release; `submit()` copies again into the queue.
        let owned = data.to_vec();
        let runtime = self.runtime.clone();
        py.detach(|| self.write_pump.submit_and_wait(&runtime, &owned))
    }

    fn closed_inner(&self) -> bool {
        self.write_pump
            .status
            .lock()
            .map(|state| state.closed || state.terminal_error.is_some())
            .unwrap_or(true)
    }

    fn close_inner(&self) -> PyResult<()> {
        if let Ok(mut state) = self.read_state.lock() {
            state.take();
        } else {
            return Err(ConnectionError::new_err("outbound read lock poisoned"));
        }
        self.write_pump.close()
    }
}

impl Drop for PyOutboundStream {
    fn drop(&mut self) {
        // The write pump aborts its Tokio task without waiting. Dropping the
        // read half likewise never blocks interpreter finalization.
        if let Ok(mut state) = self.read_state.try_lock() {
            state.take();
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
        self.with_read_stream(|stream| {
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
        self.with_read_stream(|stream| {
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
        self.write_blocking(py, data)
    }

    #[pyo3(name = "_submit_write")]
    fn submit_write(&self, data: &[u8]) -> PyResult<usize> {
        self.write_pump.submit(data)
    }

    fn sendall(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        self.write_blocking(py, data).map(|_| ())
    }

    fn drain(&self, py: Python<'_>) -> PyResult<()> {
        let runtime = self.runtime.clone();
        py.detach(|| self.write_pump.barrier(&runtime, false))
    }

    fn write_eof(&self, py: Python<'_>) -> PyResult<()> {
        let runtime = self.runtime.clone();
        py.detach(|| self.write_pump.barrier(&runtime, true))
    }

    fn close(&self) -> PyResult<()> {
        self.close_inner()
    }

    fn wait_closed(&self, py: Python<'_>) -> PyResult<()> {
        self.close_inner()?;
        let runtime = self.runtime.clone();
        py.detach(|| self.write_pump.wait_closed(&runtime))
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
        let (read_state, write_pump) = WritePump::new(&runtime, stream);
        Ok(PyOutboundStream {
            runtime,
            read_state: Mutex::new(Some(read_state)),
            write_pump,
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
        dict.set_item("hop_count", self.inner.hop_count())?;
        Ok(dict.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::task::{Context, Poll, Waker};
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    /// Deterministic gate-controlled transport for write-completion proofs.
    ///
    /// The first `poll_write` attempt signals `write_polled`. While the gate is
    /// closed, `poll_write` stores the task waker through the normal waker path
    /// and returns `Poll::Pending`. After the gate opens, the pending write is
    /// allowed to complete. Satisfies the same `eggress_core::BoxStream` trait
    /// boundary used in production; production code has no special-casing.
    struct GatedState {
        gate_open: AtomicBool,
        signaled: AtomicBool,
        polled_count: AtomicUsize,
        write_polled_tx: Mutex<Option<mpsc::Sender<()>>>,
        waker: Mutex<Option<Waker>>,
        written: Mutex<Vec<u8>>,
    }

    struct GatedTransport {
        state: Arc<GatedState>,
    }

    impl GatedTransport {
        fn new() -> (Self, Arc<GatedState>, mpsc::Receiver<()>) {
            let (tx, rx) = mpsc::channel();
            let state = Arc::new(GatedState {
                gate_open: AtomicBool::new(false),
                signaled: AtomicBool::new(false),
                polled_count: AtomicUsize::new(0),
                write_polled_tx: Mutex::new(Some(tx)),
                waker: Mutex::new(None),
                written: Mutex::new(Vec::new()),
            });
            (
                Self {
                    state: Arc::clone(&state),
                },
                state,
                rx,
            )
        }
    }

    impl GatedState {
        fn open_gate(&self) {
            self.gate_open.store(true, Ordering::SeqCst);
            let waker = self.waker.lock().ok().and_then(|mut slot| slot.take());
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }

    impl AsyncRead for GatedTransport {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            // Write-completion tests never read; harmless EOF-style readiness.
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncWrite for GatedTransport {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.state.polled_count.fetch_add(1, Ordering::SeqCst);
            if !self.state.signaled.swap(true, Ordering::SeqCst) {
                let sender = self
                    .state
                    .write_polled_tx
                    .lock()
                    .ok()
                    .and_then(|mut slot| slot.take());
                if let Some(tx) = sender {
                    let _ = tx.send(());
                }
            }
            if self.state.gate_open.load(Ordering::SeqCst) {
                if let Ok(mut written) = self.state.written.lock() {
                    written.extend_from_slice(buf);
                }
                Poll::Ready(Ok(buf.len()))
            } else {
                if let Ok(mut slot) = self.state.waker.lock() {
                    *slot = Some(cx.waker().clone());
                }
                Poll::Pending
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime builds")
    }

    fn gated_pump(
        runtime: &tokio::runtime::Runtime,
    ) -> (WritePump, Arc<GatedState>, mpsc::Receiver<()>) {
        let (transport, state, polled) = GatedTransport::new();
        let boxed: eggress_core::BoxStream = Box::new(transport);
        let (_read_half, pump) = WritePump::new(runtime, boxed);
        (pump, state, polled)
    }

    #[test]
    fn native_sync_write_waits_for_transport_completion() {
        let runtime = Arc::new(test_runtime());
        let (pump, gate, polled) = gated_pump(&runtime);
        let pump = Arc::new(pump);
        let (done_tx, done_rx) = mpsc::channel();

        let pump_for_thread = Arc::clone(&pump);
        let runtime_for_thread = Arc::clone(&runtime);
        std::thread::spawn(move || {
            let result = pump_for_thread.submit_and_wait(&runtime_for_thread, b"sync-bytes");
            let _ = done_tx.send(result.map(|len| len.to_string()));
        });

        // Deterministic sync point: the pump has reached transport poll_write.
        polled
            .recv_timeout(Duration::from_secs(5))
            .expect("transport poll_write must be reached");
        assert!(gate.polled_count.load(Ordering::SeqCst) >= 1);

        // While the gate stays closed, the synchronous completion result must
        // not be delivered. The short timeout is only a harness safety bound
        // after the poll_write sync point above.
        assert!(
            done_rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "sync write returned before transport completion"
        );

        gate.open_gate();
        let completed = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("sync write must complete after gate opens");
        assert_eq!(
            completed.expect("sync write succeeds"),
            "sync-bytes".len().to_string()
        );
        assert_eq!(
            *gate.written.lock().expect("written lock"),
            b"sync-bytes".to_vec()
        );
    }

    #[test]
    fn async_submit_returns_before_transport_completion() {
        let runtime = Arc::new(test_runtime());
        let (pump, gate, polled) = gated_pump(&runtime);
        let pump = Arc::new(pump);

        // Queue-only submission used by private `_submit_write` must return
        // the byte count without opening the transport gate.
        let submitted = pump.submit(b"queued-bytes").expect("submit returns len");
        assert_eq!(submitted, b"queued-bytes".len());

        let (barrier_tx, barrier_rx) = mpsc::channel();
        let pump_for_thread = Arc::clone(&pump);
        let runtime_for_thread = Arc::clone(&runtime);
        std::thread::spawn(move || {
            let result = pump_for_thread.barrier(&runtime_for_thread, false);
            let _ = barrier_tx.send(result.is_ok());
        });

        // Wait until the pump has reached transport poll_write, then prove the
        // barrier stays pending while the gate is closed.
        polled
            .recv_timeout(Duration::from_secs(5))
            .expect("transport poll_write must be reached");
        assert!(
            barrier_rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "barrier completed before transport completion"
        );

        gate.open_gate();
        assert!(
            barrier_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("barrier must complete after gate opens"),
            "barrier must succeed after gate opens"
        );
        assert_eq!(
            *gate.written.lock().expect("written lock"),
            b"queued-bytes".to_vec()
        );
    }
}
