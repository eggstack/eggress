use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule, PyModuleMethods, PySequence};
pyo3::create_exception!(_eggress, EggressError, PyException);
pyo3::create_exception!(_eggress, ConfigError, EggressError);
pyo3::create_exception!(_eggress, StartupError, EggressError);
pyo3::create_exception!(_eggress, ReloadError, EggressError);
pyo3::create_exception!(_eggress, ShutdownError, EggressError);
pyo3::create_exception!(_eggress, UnsupportedFeatureError, EggressError);
pyo3::create_exception!(_eggress, InternalError, EggressError);
pyo3::create_exception!(_eggress, ConnectionError, EggressError);
pyo3::create_exception!(_eggress, ConnectionClosedError, EggressError);
pyo3::create_exception!(_eggress, TimeoutError, EggressError);
pyo3::create_exception!(_eggress, DnsError, EggressError);
pyo3::create_exception!(_eggress, AuthError, EggressError);
pyo3::create_exception!(_eggress, TlsError, EggressError);
pyo3::create_exception!(_eggress, LoopMismatchError, EggressError);

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
            if let Err(e) = py.detach(|| handle.shutdown_blocking()) {
                eprintln!("shutdown error in __exit__: {e}");
            }
        }
        Ok(false)
    }
}

const STATE_CREATED: u8 = 0;
const STATE_CONNECTING: u8 = 1;
const STATE_CONNECTED: u8 = 2;
const STATE_CLOSING: u8 = 3;
const STATE_CLOSED: u8 = 4;
const STATE_FAILED: u8 = 5;

#[pyclass]
struct PyConnection {
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

        let output = py
            .detach(|| eggress_pproxy_compat::translate_pproxy_args(&parsed))
            .map_err(|e| ConnectionError::new_err(format!("translation failed: {e}")))?;

        if output.has_unsupported() {
            let features: Vec<_> = output.unsupported.iter().map(|u| u.feature).collect();
            return Err(UnsupportedFeatureError::new_err(format!(
                "unsupported features: {}",
                features.join(", ")
            )));
        }

        let config = eggress_embed::EggressConfig::from_toml_str(&output.toml)
            .map_err(|e| ConnectionError::new_err(format!("config error: {e}")))?;
        let service = eggress_embed::EggressService::new(config);
        let handle = py
            .detach(|| service.start_blocking())
            .map_err(|e| ConnectionError::new_err(format!("startup failed: {e}")))?;

        let addrs = handle.bound_addresses();
        let bound = addrs.listeners.first().map(|l| l.addr.to_string());

        Ok(Self {
            state: Arc::new(AtomicU8::new(STATE_CREATED)),
            handle: Some(handle),
            config_toml: output.toml,
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
        let current = self.state.load(Ordering::Acquire);
        if current == STATE_CLOSED || current == STATE_FAILED {
            return Ok(());
        }
        self.state.store(STATE_CLOSING, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            py.detach(|| handle.shutdown_blocking())
                .map_err(|e| ConnectionError::new_err(format!("shutdown error: {e}")))?;
        }
        self.state.store(STATE_CLOSED, Ordering::Release);
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

    fn __del__(&mut self, py: Python<'_>) {
        let current = self.state.load(Ordering::Acquire);
        if current == STATE_CLOSED || current == STATE_FAILED {
            return;
        }
        eprintln!(
            "Warning: Connection object was not properly closed. Calling close() in __del__."
        );
        if let Some(handle) = self.handle.take() {
            let _ = py.detach(|| handle.shutdown_blocking());
        }
        self.state.store(STATE_CLOSED, Ordering::Release);
    }

    fn __repr__(&self) -> String {
        format!(
            "Connection(state='{}', bound='{}')",
            self.state(),
            self.bound_addr.as_deref().unwrap_or("None")
        )
    }
}

// --- pproxy URI inspection helpers ---

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
struct PyUriInfo {
    scheme: String,
    host: String,
    port: u16,
    tls: bool,
    ssl: bool,
    inbound: bool,
    backward_num: u32,
    has_auth: bool,
    has_rule: bool,
    is_reverse_listener: bool,
    redacted_display: String,
    error: Option<String>,
}

#[pymethods]
impl PyUriInfo {
    #[getter]
    fn scheme(&self) -> &str {
        &self.scheme
    }
    #[getter]
    fn host(&self) -> &str {
        &self.host
    }
    #[getter]
    fn port(&self) -> u16 {
        self.port
    }
    #[getter]
    fn tls(&self) -> bool {
        self.tls
    }
    #[getter]
    fn ssl(&self) -> bool {
        self.ssl
    }
    #[getter]
    fn inbound(&self) -> bool {
        self.inbound
    }
    #[getter]
    fn backward_num(&self) -> u32 {
        self.backward_num
    }
    #[getter]
    fn has_auth(&self) -> bool {
        self.has_auth
    }
    #[getter]
    fn has_rule(&self) -> bool {
        self.has_rule
    }
    #[getter]
    fn is_reverse_listener(&self) -> bool {
        self.is_reverse_listener
    }
    #[getter]
    fn redacted_display(&self) -> &str {
        &self.redacted_display
    }
    #[getter]
    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn __repr__(&self) -> String {
        match &self.error {
            Some(e) => format!("UriInfo(error='{}')", e),
            None => format!(
                "UriInfo(scheme='{}', host='{}', port={}, tls={})",
                self.scheme, self.host, self.port, self.tls
            ),
        }
    }
}

#[pyfunction]
fn check_pproxy_uri(uri: &str) -> PyUriInfo {
    match eggress_pproxy_compat::uri::parse_pproxy_uri(uri) {
        Ok(parsed) => PyUriInfo {
            scheme: parsed.scheme.clone(),
            host: parsed.host.clone(),
            port: parsed.port,
            tls: parsed.tls,
            ssl: parsed.ssl,
            inbound: parsed.inbound,
            backward_num: parsed.backward_num,
            has_auth: parsed.username.is_some(),
            has_rule: parsed.rule.is_some(),
            is_reverse_listener: parsed.is_reverse_listener(),
            redacted_display: parsed.redacted_display(),
            error: None,
        },
        Err(e) => PyUriInfo {
            scheme: String::new(),
            host: String::new(),
            port: 0,
            tls: false,
            ssl: false,
            inbound: false,
            backward_num: 0,
            has_auth: false,
            has_rule: false,
            is_reverse_listener: false,
            redacted_display: String::new(),
            error: Some(e.to_string()),
        },
    }
}

#[pyfunction]
fn redact_pproxy_uri(uri: &str) -> PyResult<String> {
    let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid pproxy URI: {e}")))?;
    Ok(parsed.redacted_display())
}

// --- diagnostics ---

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
struct PyDiagnostic {
    code: String,
    feature_id: Option<String>,
    tier: Option<String>,
    message: String,
    suggestion: Option<String>,
}

#[pymethods]
impl PyDiagnostic {
    #[getter]
    fn code(&self) -> &str {
        &self.code
    }
    #[getter]
    fn feature_id(&self) -> Option<&str> {
        self.feature_id.as_deref()
    }
    #[getter]
    fn tier(&self) -> Option<&str> {
        self.tier.as_deref()
    }
    #[getter]
    fn message(&self) -> &str {
        &self.message
    }
    #[getter]
    fn suggestion(&self) -> Option<&str> {
        self.suggestion.as_deref()
    }

    fn __repr__(&self) -> String {
        format!("[{}] {}", self.code, self.message)
    }
}

#[pyfunction]
fn diagnostics_for_uri(py: Python<'_>, uri: &str) -> PyResult<Vec<PyDiagnostic>> {
    let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid pproxy URI: {e}")))?;

    let mut diagnostics: Vec<PyDiagnostic> = Vec::new();

    let output = py
        .detach(|| eggress_pproxy_compat::translate_from_uris(&[parsed], &[], &[]))
        .map_err(|e| UnsupportedFeatureError::new_err(format!("translation failed: {e}")))?;

    for warn in &output.warnings {
        let sd = eggress_pproxy_compat::StructuredDiagnostic::from(warn);
        diagnostics.push(PyDiagnostic {
            code: sd.code.to_string(),
            feature_id: sd.feature_id,
            tier: sd.tier,
            message: sd.message,
            suggestion: sd.suggestion,
        });
    }
    for u in &output.unsupported {
        diagnostics.push(PyDiagnostic {
            code: "unsupported_protocol".to_string(),
            feature_id: Some(u.feature.to_string()),
            tier: Some("unsupported".to_string()),
            message: u.detail.clone(),
            suggestion: None,
        });
    }

    Ok(diagnostics)
}

#[pyfunction]
fn supported_features() -> Vec<&'static str> {
    vec![
        "http",
        "socks4",
        "socks4a",
        "socks5",
        "shadowsocks",
        "trojan",
        "redir",
        "unix",
        "bind",
        "listen",
        "backward",
        "rebind",
        "direct",
    ]
}

// --- config explanation helpers ---

fn parse_toml_config(py: Python<'_>, toml_str: &str) -> PyResult<Py<PyDict>> {
    let parsed: toml::Value = py
        .detach(|| toml::from_str(toml_str))
        .map_err(|e| ConfigError::new_err(format!("failed to parse TOML: {e}")))?;

    let dict = PyDict::new(py);

    // Listeners
    let listeners_list = PyList::empty(py);
    if let Some(listeners) = parsed.get("listeners").and_then(|v| v.as_array()) {
        for l in listeners {
            let ldict = PyDict::new(py);
            if let Some(name) = l.get("name").and_then(|v| v.as_str()) {
                ldict.set_item("name", name)?;
            }
            if let Some(bind) = l.get("bind").and_then(|v| v.as_str()) {
                ldict.set_item("bind", bind)?;
            }
            if let Some(protocols) = l.get("protocols").and_then(|v| v.as_array()) {
                let py_protos = PyList::empty(py);
                for p in protocols {
                    if let Some(s) = p.as_str() {
                        py_protos.append(s)?;
                    }
                }
                ldict.set_item("protocols", py_protos)?;
            }
            if l.get("udp").is_some() {
                ldict.set_item("udp_enabled", true)?;
            }
            if l.get("tls").is_some() {
                ldict.set_item("tls", true)?;
            }
            if l.get("transparent").is_some() {
                ldict.set_item("transparent", true)?;
            }
            if let Some(unix) = l.get("unix") {
                ldict.set_item("unix_socket", true)?;
                if let Some(path) = unix.get("path").and_then(|v| v.as_str()) {
                    ldict.set_item("unix_path", path)?;
                }
            }
            listeners_list.append(ldict)?;
        }
    }
    dict.set_item("listeners", listeners_list)?;

    // Upstreams
    let upstreams_list = PyList::empty(py);
    if let Some(upstreams) = parsed.get("upstreams").and_then(|v| v.as_array()) {
        for u in upstreams {
            let udict = PyDict::new(py);
            if let Some(id) = u.get("id").and_then(|v| v.as_str()) {
                udict.set_item("id", id)?;
            }
            if let Some(uri) = u.get("uri").and_then(|v| v.as_str()) {
                // Redact credentials in the URI
                let redacted = redact_config_uri(uri);
                udict.set_item("uri", redacted)?;
            }
            upstreams_list.append(udict)?;
        }
    }
    dict.set_item("upstreams", upstreams_list)?;

    // Upstream groups
    let groups_list = PyList::empty(py);
    if let Some(groups) = parsed.get("upstream_groups").and_then(|v| v.as_array()) {
        for g in groups {
            let gdict = PyDict::new(py);
            if let Some(id) = g.get("id").and_then(|v| v.as_str()) {
                gdict.set_item("id", id)?;
            }
            if let Some(scheduler) = g.get("scheduler").and_then(|v| v.as_str()) {
                gdict.set_item("scheduler", scheduler)?;
            }
            if let Some(members) = g.get("members").and_then(|v| v.as_array()) {
                let py_members = PyList::empty(py);
                for m in members {
                    if let Some(s) = m.as_str() {
                        py_members.append(s)?;
                    }
                }
                gdict.set_item("members", py_members)?;
            }
            groups_list.append(gdict)?;
        }
    }
    dict.set_item("upstream_groups", groups_list)?;

    // Rules
    let rules_list = PyList::empty(py);
    if let Some(rules) = parsed.get("rules").and_then(|v| v.as_array()) {
        for r in rules {
            let rdict = PyDict::new(py);
            if let Some(id) = r.get("id").and_then(|v| v.as_str()) {
                rdict.set_item("id", id)?;
            }
            if let Some(ug) = r.get("upstream_group").and_then(|v| v.as_str()) {
                rdict.set_item("upstream_group", ug)?;
            }
            if r.get("direct").and_then(|v| v.as_bool()) == Some(true) {
                rdict.set_item("action", "direct")?;
            } else if let Some(reject) = r.get("reject").and_then(|v| v.as_str()) {
                rdict.set_item("action", format!("reject({})", reject))?;
            } else if r.get("upstream_group").is_some() {
                rdict.set_item("action", "upstream")?;
            }
            if r.get("match").is_some() {
                rdict.set_item("has_match", true)?;
            } else if r.get("any").and_then(|v| v.as_bool()) == Some(true) {
                rdict.set_item("match_all", true)?;
            }
            rules_list.append(rdict)?;
        }
    }
    dict.set_item("rules", rules_list)?;

    // Reverse servers
    let reverse_servers_list = PyList::empty(py);
    if let Some(servers) = parsed.get("reverse_servers").and_then(|v| v.as_array()) {
        for s in servers {
            let sdict = PyDict::new(py);
            if let Some(id) = s.get("id").and_then(|v| v.as_str()) {
                sdict.set_item("id", id)?;
            }
            if let Some(bind) = s.get("control_bind").and_then(|v| v.as_str()) {
                sdict.set_item("control_bind", bind)?;
            }
            reverse_servers_list.append(sdict)?;
        }
    }
    dict.set_item("reverse_servers", reverse_servers_list)?;

    // Reverse clients
    let reverse_clients_list = PyList::empty(py);
    if let Some(clients) = parsed.get("reverse_clients").and_then(|v| v.as_array()) {
        for c in clients {
            let cdict = PyDict::new(py);
            if let Some(id) = c.get("id").and_then(|v| v.as_str()) {
                cdict.set_item("id", id)?;
            }
            if let Some(addr) = c.get("server_addr").and_then(|v| v.as_str()) {
                cdict.set_item("server_addr", addr)?;
            }
            reverse_clients_list.append(cdict)?;
        }
    }
    dict.set_item("reverse_clients", reverse_clients_list)?;

    // Security notes
    let security_list = PyList::empty(py);
    // Check for plaintext credentials
    if let Some(listeners) = parsed.get("listeners").and_then(|v| v.as_array()) {
        for l in listeners {
            if l.get("auth").is_some() {
                security_list.append("listener has plaintext auth credentials in TOML")?;
            }
            if l.get("shadowsocks").is_some() {
                security_list.append("listener has Shadowsocks credentials in TOML")?;
            }
        }
    }
    if let Some(servers) = parsed.get("reverse_servers").and_then(|v| v.as_array()) {
        for s in servers {
            if s.get("auth_password").is_some() {
                security_list.append("reverse server has plaintext credentials in TOML")?;
            }
        }
    }
    if let Some(clients) = parsed.get("reverse_clients").and_then(|v| v.as_array()) {
        for c in clients {
            if c.get("auth_password").is_some() {
                security_list.append("reverse client has plaintext credentials in TOML")?;
            }
        }
    }
    if let Some(listeners) = parsed.get("listeners").and_then(|v| v.as_array()) {
        for l in listeners {
            if l.get("transparent").is_some() {
                security_list.append("transparent proxy listener requires elevated privileges")?;
            }
        }
    }
    dict.set_item("security_notes", security_list)?;

    Ok(dict.into())
}

/// Redact credentials from a config URI for safe display.
///
/// The userinfo separator is the LAST unbracketed `@` after the scheme,
/// not the first; a raw password containing `@` must not be split.
fn redact_config_uri(uri: &str) -> String {
    let Some(scheme_end) = uri.find("://") else {
        return uri.to_string();
    };
    let after_scheme = &uri[scheme_end + 3..];
    let mut last_at: Option<usize> = None;
    let mut bracket_depth = 0u32;
    for (i, c) in after_scheme.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '@' if bracket_depth == 0 => last_at = Some(i),
            _ => {}
        }
    }
    if let Some(at_pos) = last_at {
        return format!(
            "{}****@{}",
            &uri[..scheme_end + 3],
            &after_scheme[at_pos + 1..]
        );
    }
    uri.to_string()
}

#[pyfunction]
fn explain_config_toml(py: Python<'_>, toml_str: &str) -> PyResult<Py<PyDict>> {
    parse_toml_config(py, toml_str)
}

#[pyfunction]
fn explain_pproxy_args(py: Python<'_>, args: &Bound<'_, PySequence>) -> PyResult<Py<PyDict>> {
    let result = translate_pproxy_args(py, args)?;
    let toml_str = result.output.toml.clone();
    let warnings: Vec<(String, String)> = result
        .output
        .warnings
        .iter()
        .map(|w| (w.category.to_string(), w.message.clone()))
        .collect();
    let unsupported: Vec<(String, String)> = result
        .output
        .unsupported
        .iter()
        .map(|u| (u.feature.to_string(), u.detail.clone()))
        .collect();
    let is_ok = !result.output.has_unsupported();

    let dict = parse_toml_config(py, &toml_str)?;

    let warnings_list = PyList::empty(py);
    for (cat, msg) in &warnings {
        let wdict = PyDict::new(py);
        wdict.set_item("category", cat.as_str())?;
        wdict.set_item("message", msg.as_str())?;
        warnings_list.append(wdict)?;
    }
    dict.bind(py).set_item("warnings", warnings_list)?;

    let unsupported_list = PyList::empty(py);
    for (feat, detail) in &unsupported {
        let udict = PyDict::new(py);
        udict.set_item("feature", feat.as_str())?;
        udict.set_item("detail", detail.as_str())?;
        unsupported_list.append(udict)?;
    }
    dict.bind(py).set_item("unsupported", unsupported_list)?;

    dict.bind(py).set_item("toml", &toml_str)?;
    dict.bind(py).set_item("ok", is_ok)?;

    Ok(dict)
}

#[pyfunction]
fn explain_pproxy_uri(py: Python<'_>, uri: &str) -> PyResult<Py<PyDict>> {
    let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid pproxy URI: {e}")))?;

    let output = py
        .detach(|| eggress_pproxy_compat::translate_from_uris(&[parsed], &[], &[]))
        .map_err(|e| UnsupportedFeatureError::new_err(format!("translation failed: {e}")))?;

    let dict = parse_toml_config(py, &output.toml)?;

    let warnings_list = PyList::empty(py);
    for w in &output.warnings {
        let wdict = PyDict::new(py);
        wdict.set_item("category", w.category)?;
        wdict.set_item("message", &w.message)?;
        warnings_list.append(wdict)?;
    }
    dict.bind(py).set_item("warnings", warnings_list)?;

    let unsupported_list = PyList::empty(py);
    for u in &output.unsupported {
        let udict = PyDict::new(py);
        udict.set_item("feature", u.feature)?;
        udict.set_item("detail", &u.detail)?;
        unsupported_list.append(udict)?;
    }
    dict.bind(py).set_item("unsupported", unsupported_list)?;

    dict.bind(py).set_item("toml", &output.toml)?;
    dict.bind(py).set_item("ok", !output.has_unsupported())?;

    Ok(dict)
}

#[pyfunction]
fn route_explain(py: Python<'_>, toml_str: &str, target: &str) -> PyResult<Py<PyDict>> {
    use eggress_config::compile::compile_config;
    use eggress_config::model::ConfigFile;
    use eggress_core::{ClientIdentity, ProtocolId, TargetAddr};
    use eggress_routing::{RouteRequest, Router, TransportKind};

    let target_addr: TargetAddr = target
        .parse()
        .map_err(|e: String| PyValueError::new_err(format!("invalid target: {e}")))?;

    let explanation = py
        .detach(|| -> Result<_, String> {
            let config: ConfigFile =
                toml::from_str(toml_str).map_err(|e| format!("failed to parse TOML: {e}"))?;

            let runtime_config =
                compile_config(&config).map_err(|e| format!("failed to compile config: {e}"))?;

            let router =
                Router::with_groups(runtime_config.rules, runtime_config.default_action, vec![]);

            let request = RouteRequest {
                target: &target_addr,
                source: None,
                listener: "",
                inbound_protocol: ProtocolId::Socks5,
                identity: &ClientIdentity::Anonymous,
                transport: TransportKind::Tcp,
            };

            Ok(router.explain(&request, 0))
        })
        .map_err(ConfigError::new_err)?;

    let dict = PyDict::new(py);
    dict.set_item("target", &explanation.target)?;
    dict.set_item("listener", &explanation.listener)?;
    dict.set_item("protocol", &explanation.protocol)?;
    dict.set_item("transport", &explanation.transport)?;
    dict.set_item("matched_rule", explanation.matched_rule)?;
    dict.set_item("action", &explanation.action)?;
    dict.set_item("upstream_group", explanation.upstream_group)?;
    dict.set_item("scheduler", explanation.scheduler)?;
    let eligible_list = PyList::empty(py);
    for u in &explanation.eligible_upstreams {
        let udict = PyDict::new(py);
        udict.set_item("id", &u.id)?;
        udict.set_item("health", &u.health)?;
        udict.set_item("eligible", u.eligible)?;
        udict.set_item("active", u.active)?;
        udict.set_item("in_flight", u.in_flight)?;
        eligible_list.append(udict)?;
    }
    dict.set_item("eligible_upstreams", eligible_list)?;
    dict.set_item("selected_upstream", explanation.selected_upstream)?;
    dict.set_item("chain", explanation.chain)?;
    dict.set_item("generation", explanation.generation)?;

    Ok(dict.into())
}

#[pyfunction]
fn test_upstream_connect(py: Python<'_>, uri: &str, timeout_secs: f64) -> PyResult<Py<PyDict>> {
    use std::net::ToSocketAddrs;

    let dict = PyDict::new(py);

    // Parse the URI to extract host:port
    let url =
        url::Url::parse(uri).map_err(|e| PyValueError::new_err(format!("invalid URI: {e}")))?;

    let host = url
        .host_str()
        .ok_or_else(|| PyValueError::new_err("URI has no host"))?
        .to_string();
    let port = url.port().unwrap_or(match url.scheme() {
        "socks5" => 1080,
        "socks4" | "socks4a" => 1080,
        "http" | "https" => 80,
        "ss" => 8388,
        "trojan" => 443,
        _ => 0,
    });

    dict.set_item("host", &host)?;
    dict.set_item("port", port)?;
    dict.set_item("scheme", url.scheme())?;

    // Has auth?
    let has_auth = !url.username().is_empty() || url.password().is_some();
    dict.set_item("has_auth", has_auth)?;

    // Redact for display
    let redacted = if has_auth {
        format!("{}://****@{}:{}", url.scheme(), host, port)
    } else {
        format!("{}://{}:{}", url.scheme(), host, port)
    };
    dict.set_item("redacted_uri", &redacted)?;

    // Attempt TCP connect
    let addr_str = format!("{}:{}", host, port);
    let (connected, latency_us, last_error): (bool, Option<u64>, Option<String>) =
        py.detach(|| {
            let std_duration = std::time::Duration::from_secs_f64(timeout_secs);
            let socket_addrs = match addr_str.to_socket_addrs() {
                Ok(addrs) => addrs,
                Err(e) => {
                    return (false, None, Some(format!("DNS resolution failed: {e}")));
                }
            };

            let mut last_error: Option<String> = None;
            for addr in socket_addrs {
                let start = std::time::Instant::now();
                match std::net::TcpStream::connect_timeout(&addr, std_duration) {
                    Ok(_stream) => {
                        return (true, Some(start.elapsed().as_micros() as u64), None);
                    }
                    Err(e) => {
                        last_error = Some(format!("connect to {addr} failed: {e}"));
                    }
                }
            }
            (false, None, last_error)
        });

    dict.set_item("connected", connected)?;
    dict.set_item("latency_us", latency_us)?;
    dict.set_item("error", last_error)?;

    Ok(dict.into())
}

// --- pproxy compatibility translation helpers ---

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
struct PyTranslationWarning {
    inner: eggress_pproxy_compat::CompatWarning,
}

#[pymethods]
impl PyTranslationWarning {
    #[getter]
    fn category(&self) -> &str {
        self.inner.category
    }

    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }

    fn __repr__(&self) -> String {
        format!("[{}] {}", self.inner.category, self.inner.message)
    }
}

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
struct PyUnsupportedFeature {
    inner: eggress_pproxy_compat::UnsupportedFeature,
}

#[pymethods]
impl PyUnsupportedFeature {
    #[getter]
    fn feature(&self) -> &str {
        self.inner.feature
    }

    #[getter]
    fn message(&self) -> &str {
        &self.inner.detail
    }

    fn __repr__(&self) -> String {
        format!("unsupported {}: {}", self.inner.feature, self.inner.detail)
    }
}

#[pyclass]
struct PyTranslationResult {
    output: eggress_pproxy_compat::TranslationOutput,
}

#[pymethods]
impl PyTranslationResult {
    #[getter]
    fn toml(&self) -> &str {
        &self.output.toml
    }

    #[getter]
    fn warnings(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let list = PyList::empty(py);
        for w in &self.output.warnings {
            list.append(PyTranslationWarning { inner: w.clone() })?;
        }
        Ok(list.into())
    }

    #[getter]
    fn unsupported(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let list = PyList::empty(py);
        for u in &self.output.unsupported {
            list.append(PyUnsupportedFeature { inner: u.clone() })?;
        }
        Ok(list.into())
    }

    #[getter]
    fn ok(&self) -> bool {
        !self.output.has_unsupported()
    }

    fn config(&self, py: Python<'_>) -> PyResult<PyEggressConfig> {
        let config = py
            .detach(|| eggress_embed::EggressConfig::from_toml_str(&self.output.toml))
            .map_err(|e| map_error(py, e))?;
        Ok(PyEggressConfig { inner: config })
    }

    fn __repr__(&self) -> String {
        format!(
            "TranslationResult(warnings={}, unsupported={})",
            self.output.warnings.len(),
            self.output.unsupported.len()
        )
    }
}

#[pyfunction]
fn translate_pproxy_args(
    py: Python<'_>,
    args: &Bound<'_, PySequence>,
) -> PyResult<PyTranslationResult> {
    let len = args.len()?;
    let raw: Vec<String> = (0..len)
        .map(|i| args.get_item(i)?.extract::<String>())
        .collect::<PyResult<_>>()?;

    let parsed = eggress_pproxy_compat::PproxyArgs::parse(&raw).map_err(|e| {
        UnsupportedFeatureError::new_err(format!("failed to parse pproxy args: {e}"))
    })?;

    let output = py
        .detach(|| eggress_pproxy_compat::translate_pproxy_args(&parsed))
        .map_err(|e| UnsupportedFeatureError::new_err(format!("translation failed: {e}")))?;

    Ok(PyTranslationResult { output })
}

#[pyfunction]
fn translate_pproxy_uri(
    py: Python<'_>,
    local: &str,
    remotes: Option<&Bound<'_, PySequence>>,
) -> PyResult<PyTranslationResult> {
    let local_uri = eggress_pproxy_compat::uri::parse_pproxy_uri(local)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid local URI: {e}")))?;

    let remote_chains: Vec<eggress_pproxy_compat::PproxyChain> = match remotes {
        Some(seq) => {
            let len = seq.len()?;
            (0..len)
                .map(|i| {
                    let s: String = seq.get_item(i)?.extract()?;
                    eggress_pproxy_compat::uri::parse_pproxy_chain(&s).map_err(|e| {
                        UnsupportedFeatureError::new_err(format!("invalid remote URI: {e}"))
                    })
                })
                .collect::<PyResult<_>>()?
        }
        None => Vec::new(),
    };

    let output = py
        .detach(|| eggress_pproxy_compat::translate_from_uris(&[local_uri], &remote_chains, &[]))
        .map_err(|e| UnsupportedFeatureError::new_err(format!("translation failed: {e}")))?;

    Ok(PyTranslationResult { output })
}

#[pyfunction]
fn check_pproxy_args(
    py: Python<'_>,
    args: &Bound<'_, PySequence>,
) -> PyResult<PyTranslationResult> {
    translate_pproxy_args(py, args)
}

#[pyclass]
struct PyReverseUriSummary {
    /// "server" or "client" or "unknown"
    role: String,
    scheme: String,
    /// "host:port" string in redacted form for display
    target: String,
    has_auth: bool,
    /// "reverse_servers" or "reverse_clients" or "unknown"
    toml_section: String,
    tls: bool,
    /// Modifiers parsed (e.g. "+tls", "+in")
    modifiers: Vec<String>,
}

#[pymethods]
impl PyReverseUriSummary {
    #[getter]
    fn role(&self) -> &str {
        &self.role
    }
    #[getter]
    fn scheme(&self) -> &str {
        &self.scheme
    }
    #[getter]
    fn target(&self) -> &str {
        &self.target
    }
    #[getter]
    fn has_auth(&self) -> bool {
        self.has_auth
    }
    #[getter]
    fn toml_section(&self) -> &str {
        &self.toml_section
    }
    #[getter]
    fn tls(&self) -> bool {
        self.tls
    }
    #[getter]
    fn modifiers(&self) -> Vec<String> {
        self.modifiers.clone()
    }
    fn __repr__(&self) -> String {
        format!(
            "ReverseUriSummary(role={}, target={}, toml_section={}, has_auth={})",
            self.role, self.target, self.toml_section, self.has_auth
        )
    }
}

#[pyfunction]
fn describe_reverse_pproxy_uri(uri: &str) -> PyResult<PyReverseUriSummary> {
    let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid pproxy URI: {e}")))?;

    let (role, toml_section) = if parsed.is_reverse_listener() {
        ("server", "reverse_servers")
    } else if parsed.is_backward() {
        ("client", "reverse_clients")
    } else {
        ("unknown", "unknown")
    };

    let target = parsed.redacted_display();

    // Modifiers encoded in the scheme: +tls, +ssl, +in, ...
    let mut modifiers: Vec<String> = Vec::new();
    if parsed.tls {
        modifiers.push("+tls".to_string());
    }
    if parsed.ssl {
        modifiers.push("+ssl".to_string());
    }
    for _ in 0..parsed.backward_num {
        modifiers.push("+in".to_string());
    }

    Ok(PyReverseUriSummary {
        role: role.to_string(),
        scheme: parsed.scheme.clone(),
        target,
        has_auth: parsed.username.is_some(),
        toml_section: toml_section.to_string(),
        tls: parsed.tls,
        modifiers,
    })
}

#[pymodule]
fn _eggress(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEggressConfig>()?;
    m.add_class::<PyEggressService>()?;
    m.add_class::<PyEggressHandle>()?;
    m.add_class::<PyTranslationWarning>()?;
    m.add_class::<PyUnsupportedFeature>()?;
    m.add_class::<PyTranslationResult>()?;
    m.add_class::<PyReverseUriSummary>()?;
    m.add_class::<PyUriInfo>()?;
    m.add_class::<PyDiagnostic>()?;
    m.add_class::<PyConnection>()?;
    m.add_function(wrap_pyfunction!(translate_pproxy_args, m)?)?;
    m.add_function(wrap_pyfunction!(translate_pproxy_uri, m)?)?;
    m.add_function(wrap_pyfunction!(check_pproxy_args, m)?)?;
    m.add_function(wrap_pyfunction!(describe_reverse_pproxy_uri, m)?)?;
    m.add_function(wrap_pyfunction!(check_pproxy_uri, m)?)?;
    m.add_function(wrap_pyfunction!(redact_pproxy_uri, m)?)?;
    m.add_function(wrap_pyfunction!(diagnostics_for_uri, m)?)?;
    m.add_function(wrap_pyfunction!(supported_features, m)?)?;
    m.add_function(wrap_pyfunction!(explain_config_toml, m)?)?;
    m.add_function(wrap_pyfunction!(explain_pproxy_args, m)?)?;
    m.add_function(wrap_pyfunction!(explain_pproxy_uri, m)?)?;
    m.add_function(wrap_pyfunction!(route_explain, m)?)?;
    m.add_function(wrap_pyfunction!(test_upstream_connect, m)?)?;
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
    m.add("ConnectionError", m.py().get_type::<ConnectionError>())?;
    m.add(
        "ConnectionClosedError",
        m.py().get_type::<ConnectionClosedError>(),
    )?;
    m.add("TimeoutError", m.py().get_type::<TimeoutError>())?;
    m.add("DnsError", m.py().get_type::<DnsError>())?;
    m.add("AuthError", m.py().get_type::<AuthError>())?;
    m.add("TlsError", m.py().get_type::<TlsError>())?;
    m.add("LoopMismatchError", m.py().get_type::<LoopMismatchError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
