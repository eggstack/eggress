//! pproxy-compatibility surface: URI inspection, diagnostics, translation,
//! explanation, testing helpers, and reverse-URI summaries.
//!
//! These wrappers are thin views over `eggress-pproxy-compat`; Python-level
//! policy stays in `python/eggress`, never moves into these bindings.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySequence};

use std::time::Duration;

use pyo3::exceptions::PyValueError;

use super::errors::{map_error, ConfigError, UnsupportedFeatureError};
use super::service::PyEggressConfig;

// --- pproxy URI inspection helpers ---

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyUriInfo {
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
pub(crate) fn check_pproxy_uri(uri: &str) -> PyUriInfo {
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
pub(crate) fn redact_pproxy_uri(uri: &str) -> PyResult<String> {
    let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid pproxy URI: {e}")))?;
    Ok(parsed.redacted_display())
}

// --- diagnostics ---

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyDiagnostic {
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

/// Return diagnostic-only compatibility information for a pproxy URI.
///
/// Entries describe translation warnings and unsupported features; this does
/// not prove that the URI is executable. Use `check_pproxy_uri` for the
/// actionable compatibility check.
#[pyfunction]
pub(crate) fn diagnostics_for_uri(py: Python<'_>, uri: &str) -> PyResult<Vec<PyDiagnostic>> {
    let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid pproxy URI: {e}")))?;

    let mut diagnostics: Vec<PyDiagnostic> = Vec::new();

    let output = py
        .detach(|| {
            eggress_pproxy_compat::translate_from_uris(
                &eggress_pproxy_compat::PproxyArgs::default_args(),
                &[parsed],
                &[],
            )
        })
        .map_err(|e| UnsupportedFeatureError::new_err(format!("translation failed: {e}")))?;

    for warn in &output.warnings() {
        let sd = eggress_pproxy_compat::StructuredDiagnostic::from(warn);
        diagnostics.push(PyDiagnostic {
            code: sd.code.to_string(),
            feature_id: sd.feature_id,
            tier: sd.tier,
            message: sd.message,
            suggestion: sd.suggestion,
        });
    }
    for u in &output.unsupported() {
        diagnostics.push(PyDiagnostic {
            code: "unsupported_protocol".to_string(),
            feature_id: Some(u.feature.to_string()),
            tier: Some("unsupported".to_string()),
            message: u.detail.clone(),
            suggestion: None,
        });
    }

    diagnostics.sort_by_key(|diagnostic| {
        if diagnostic.tier.as_deref() == Some("unsupported") {
            0
        } else {
            1
        }
    });

    Ok(diagnostics)
}

#[pyfunction]
pub(crate) fn supported_features() -> Vec<&'static str> {
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

pub(crate) fn parse_toml_config(py: Python<'_>, toml_str: &str) -> PyResult<Py<PyDict>> {
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
pub(crate) fn redact_config_uri(uri: &str) -> String {
    eggress_uri::redact_proxy_uri(uri)
}

#[pyfunction]
pub(crate) fn explain_config_toml(py: Python<'_>, toml_str: &str) -> PyResult<Py<PyDict>> {
    parse_toml_config(py, toml_str)
}

#[pyfunction]
pub(crate) fn explain_pproxy_args(
    py: Python<'_>,
    args: &Bound<'_, PySequence>,
) -> PyResult<Py<PyDict>> {
    let result = translate_pproxy_args(py, args)?;
    let toml_str = result.output.toml.clone();
    let warnings: Vec<(String, String)> = result
        .output
        .warnings()
        .iter()
        .map(|w| (w.category.to_string(), w.message.clone()))
        .collect();
    let unsupported: Vec<(String, String)> = result
        .output
        .unsupported()
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
pub(crate) fn explain_pproxy_uri(py: Python<'_>, uri: &str) -> PyResult<Py<PyDict>> {
    let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri(uri)
        .map_err(|e| UnsupportedFeatureError::new_err(format!("invalid pproxy URI: {e}")))?;

    let output = py
        .detach(|| {
            eggress_pproxy_compat::translate_from_uris(
                &eggress_pproxy_compat::PproxyArgs::default_args(),
                &[parsed],
                &[],
            )
        })
        .map_err(|e| UnsupportedFeatureError::new_err(format!("translation failed: {e}")))?;

    let dict = parse_toml_config(py, &output.toml)?;

    let warnings_list = PyList::empty(py);
    for w in &output.warnings() {
        let wdict = PyDict::new(py);
        wdict.set_item("category", w.category)?;
        wdict.set_item("message", &w.message)?;
        warnings_list.append(wdict)?;
    }
    dict.bind(py).set_item("warnings", warnings_list)?;

    let unsupported_list = PyList::empty(py);
    for u in &output.unsupported() {
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
pub(crate) fn route_explain(py: Python<'_>, toml_str: &str, target: &str) -> PyResult<Py<PyDict>> {
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
pub(crate) fn test_upstream_connect(
    py: Python<'_>,
    uri: &str,
    timeout_secs: f64,
) -> PyResult<Py<PyDict>> {
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
pub(crate) struct PyTranslationWarning {
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

    #[getter]
    fn tier(&self) -> &str {
        eggress_pproxy_compat::manifest_tier_for_category(self.inner.category).as_str()
    }

    fn __repr__(&self) -> String {
        format!("[{}] {}", self.inner.category, self.inner.message)
    }
}

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct PyUnsupportedFeature {
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

    #[getter]
    fn tier(&self) -> &str {
        eggress_pproxy_compat::classify_unsupported_feature_tier(self.inner.feature)
    }

    fn __repr__(&self) -> String {
        format!("unsupported {}: {}", self.inner.feature, self.inner.detail)
    }
}

#[pyclass]
pub(crate) struct PyTranslationResult {
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
        for w in &self.output.warnings() {
            list.append(PyTranslationWarning { inner: w.clone() })?;
        }
        Ok(list.into())
    }

    #[getter]
    fn unsupported(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let list = PyList::empty(py);
        for u in &self.output.unsupported() {
            list.append(PyUnsupportedFeature { inner: u.clone() })?;
        }
        Ok(list.into())
    }

    #[getter]
    fn ok(&self) -> bool {
        !self.output.has_unsupported()
    }

    /// Manifest-aligned aggregate compatibility tier.
    ///
    /// This is the single source of truth for the five-tier compatibility
    /// classification; Python and the canonical manifest must agree with
    /// this value. The same value is reused by the CLI `pproxy check`
    /// reporter.
    #[getter]
    fn tier(&self) -> &'static str {
        eggress_pproxy_compat::classify_aggregate_tier(
            &self.output.warnings(),
            &self.output.unsupported(),
        )
        .as_str()
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
            self.output.warnings().len(),
            self.output.unsupported().len()
        )
    }
}

#[pyfunction]
pub(crate) fn translate_pproxy_args(
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
pub(crate) fn translate_pproxy_uri(
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
        .detach(|| {
            eggress_pproxy_compat::translate_from_uris(
                &eggress_pproxy_compat::PproxyArgs::default_args(),
                &[local_uri],
                &remote_chains,
            )
        })
        .map_err(|e| UnsupportedFeatureError::new_err(format!("translation failed: {e}")))?;

    Ok(PyTranslationResult { output })
}

#[pyfunction]
pub(crate) fn check_pproxy_args(
    py: Python<'_>,
    args: &Bound<'_, PySequence>,
) -> PyResult<PyTranslationResult> {
    translate_pproxy_args(py, args)
}

/// Validate the frozen executable parser contract without starting a service.
/// Migration helpers intentionally retain their broader translation-only
/// extension surface.
#[pyfunction]
pub(crate) fn validate_pproxy_args(args: &Bound<'_, PySequence>) -> PyResult<()> {
    let len = args.len()?;
    let raw: Vec<String> = (0..len)
        .map(|i| args.get_item(i)?.extract::<String>())
        .collect::<PyResult<_>>()?;
    let parsed = if raw.is_empty() {
        eggress_pproxy_compat::PproxyArgs::default_args()
    } else {
        eggress_pproxy_compat::PproxyArgs::parse(&raw)
            .map_err(|e| PyValueError::new_err(format!("pproxy argument error: {e}")))?
    };
    if let Some(flag) = parsed.strict_parser_violations().first() {
        return Err(PyValueError::new_err(format!(
            "pproxy: unknown option or positional argument '{flag}'"
        )));
    }
    parsed
        .validate_strict_values()
        .map_err(|e| PyValueError::new_err(format!("pproxy argument error: {e}")))
}

/// Return compatibility runtime options using the canonical Rust parser.
/// Migration callers may still use the broader translation surface; strict
/// executable entry points perform their separate parser-gate validation.
#[pyfunction]
pub(crate) fn pproxy_runtime_options(args: &Bound<'_, PySequence>) -> PyResult<Py<PyDict>> {
    let len = args.len()?;
    let raw: Vec<String> = (0..len)
        .map(|i| args.get_item(i)?.extract::<String>())
        .collect::<PyResult<_>>()?;
    let parsed = if raw.is_empty() {
        eggress_pproxy_compat::PproxyArgs::default_args()
    } else {
        eggress_pproxy_compat::PproxyArgs::parse(&raw)
            .map_err(|e| PyValueError::new_err(format!("pproxy argument error: {e}")))?
    };
    let dict = PyDict::new(args.py());
    dict.set_item(
        "auth_timeout_seconds",
        parsed.effective_auth_timeout().as_secs(),
    )?;
    dict.set_item("system_proxy", parsed.system_proxy)?;
    dict.set_item("debug", parsed.debug)?;
    dict.set_item("verbose_level", parsed.verbose_level)?;
    dict.set_item("default_log_level", parsed.default_log_level())?;
    Ok(dict.into())
}

/// Install compatibility-process logging for `python -m pproxy` while
/// respecting an existing embedded application's tracing subscriber.
#[pyfunction]
pub(crate) fn init_pproxy_logging(default_level: &str) -> PyResult<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .compact()
        .try_init();
    Ok(())
}

/// Run the native compatibility upstream test without starting listeners.
#[pyfunction]
pub(crate) fn run_pproxy_test(
    py: Python<'_>,
    args: &Bound<'_, PySequence>,
    target: &str,
) -> PyResult<i32> {
    let len = args.len()?;
    let raw: Vec<String> = (0..len)
        .map(|i| args.get_item(i)?.extract::<String>())
        .collect::<PyResult<_>>()?;
    let parsed = if raw.is_empty() {
        eggress_pproxy_compat::PproxyArgs::default_args()
    } else {
        eggress_pproxy_compat::PproxyArgs::parse(&raw)
            .map_err(|e| PyValueError::new_err(format!("pproxy argument error: {e}")))?
    };
    if let Some(flag) = parsed.strict_parser_violations().first() {
        return Err(PyValueError::new_err(format!(
            "pproxy: unknown option or positional argument '{flag}'"
        )));
    }
    parsed
        .validate_strict_values()
        .map_err(|e| PyValueError::new_err(format!("pproxy argument error: {e}")))?;
    let output = eggress_pproxy_compat::translate_pproxy_args(&parsed)
        .map_err(|e| PyValueError::new_err(format!("pproxy translation error: {e}")))?;
    let gate = eggress_pproxy_compat::evaluate_execution_gate(&parsed, &output);
    if !gate.allows_start() {
        return Err(UnsupportedFeatureError::new_err(gate.blocker_summary()));
    }
    let (config, _) = eggress_config::validate_and_compile_toml_with_warnings(&output.toml)
        .map_err(|e| ConfigError::new_err(format!("pproxy config error: {e}")))?;
    let target = eggress_cli::parse_pproxy_test_target(target)
        .map_err(PyValueError::new_err)?
        .to_string();
    if config.upstreams.is_empty() {
        return Ok(0);
    }
    Ok(py.detach(|| {
        eggress_cli::run_upstream_test(&config, Some(&target), Duration::from_secs(10), false)
    }))
}

#[pyclass]
pub(crate) struct PyReverseUriSummary {
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
pub(crate) fn describe_reverse_pproxy_uri(uri: &str) -> PyResult<PyReverseUriSummary> {
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
