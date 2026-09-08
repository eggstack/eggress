//! PyO3 extension module `_eggress`: module registration only.
//!
//! Binding domains live in focused submodules by public surface (`errors`,
//! `service`, `connection`, `compat`, `outbound`, `system_proxy`,
//! `runtime`); this file wires them into the Python module with unchanged
//! exported names, exception hierarchy, and abi3 metadata.

mod compat;
mod connection;
mod errors;
mod outbound;
mod runtime;
mod service;
mod system_proxy;

use pyo3::prelude::*;
use pyo3::types::PyModuleMethods;

use compat::{
    check_pproxy_args, check_pproxy_uri, describe_reverse_pproxy_uri, diagnostics_for_uri,
    explain_config_toml, explain_pproxy_args, explain_pproxy_uri, init_pproxy_logging,
    pproxy_runtime_options, redact_pproxy_uri, route_explain, run_pproxy_test, supported_features,
    test_upstream_connect, translate_pproxy_args, translate_pproxy_uri, validate_pproxy_args,
    PyDiagnostic, PyReverseUriSummary, PyTranslationResult, PyTranslationWarning,
    PyUnsupportedFeature, PyUriInfo,
};
use connection::PyConnection;
use errors::{
    AuthError, ConfigError, ConnectionCancelledError, ConnectionClosedError, ConnectionError,
    DnsError, EggressError, InternalError, LoopMismatchError, ReloadError, ShutdownError,
    StartupError, TimeoutError, TlsError, UdpAssociationError, UnsupportedCompositionError,
    UnsupportedFeatureError, UseAfterCloseError,
};
use outbound::{PyOutboundConnector, PyOutboundStream};
use service::{PyEggressConfig, PyEggressHandle, PyEggressService};
use system_proxy::{apply_system_proxy, PyAppliedSystemProxy};

#[pymodule]
fn _eggress(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEggressConfig>()?;
    m.add_class::<PyEggressService>()?;
    m.add_class::<PyEggressHandle>()?;
    m.add_class::<PyAppliedSystemProxy>()?;
    m.add_class::<PyTranslationWarning>()?;
    m.add_class::<PyUnsupportedFeature>()?;
    m.add_class::<PyTranslationResult>()?;
    m.add_class::<PyReverseUriSummary>()?;
    m.add_class::<PyUriInfo>()?;
    m.add_class::<PyDiagnostic>()?;
    m.add_class::<PyConnection>()?;
    m.add_class::<PyOutboundConnector>()?;
    m.add_class::<PyOutboundStream>()?;
    m.add_function(wrap_pyfunction!(translate_pproxy_args, m)?)?;
    m.add_function(wrap_pyfunction!(translate_pproxy_uri, m)?)?;
    m.add_function(wrap_pyfunction!(check_pproxy_args, m)?)?;
    m.add_function(wrap_pyfunction!(validate_pproxy_args, m)?)?;
    m.add_function(wrap_pyfunction!(pproxy_runtime_options, m)?)?;
    m.add_function(wrap_pyfunction!(init_pproxy_logging, m)?)?;
    m.add_function(wrap_pyfunction!(run_pproxy_test, m)?)?;
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
    m.add_function(wrap_pyfunction!(apply_system_proxy, m)?)?;
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
    m.add(
        "ConnectionCancelledError",
        m.py().get_type::<ConnectionCancelledError>(),
    )?;
    m.add(
        "UseAfterCloseError",
        m.py().get_type::<UseAfterCloseError>(),
    )?;
    m.add(
        "UdpAssociationError",
        m.py().get_type::<UdpAssociationError>(),
    )?;
    m.add(
        "UnsupportedCompositionError",
        m.py().get_type::<UnsupportedCompositionError>(),
    )?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn uri_translation_smoke() {
        let parsed = eggress_pproxy_compat::uri::parse_pproxy_uri("http://127.0.0.1:8080")
            .expect("valid pproxy URI");
        assert_eq!(parsed.scheme, "http");
        assert_eq!(parsed.port, 8080);

        let args = eggress_pproxy_compat::PproxyArgs::parse(&["http://127.0.0.1:8080".to_string()])
            .expect("valid pproxy args");
        let output =
            eggress_pproxy_compat::translate_pproxy_args(&args).expect("translatable pproxy args");
        assert!(output.toml.contains("127.0.0.1:8080"));
    }

    #[test]
    fn config_conversion_smoke() {
        let args = eggress_pproxy_compat::PproxyArgs::parse(&["http://127.0.0.1:8080".to_string()])
            .expect("valid pproxy args");
        let output =
            eggress_pproxy_compat::translate_pproxy_args(&args).expect("translatable pproxy args");
        let config = eggress_embed::EggressConfig::from_toml_str(&output.toml)
            .expect("translated TOML converts to embed config");
        assert!(!config.source_toml().is_empty());
    }

    #[test]
    fn error_mapping_categories_are_stable() {
        use eggress_embed::EggressError;

        let cases = [
            (EggressError::Config("bad config".into()), "config"),
            (EggressError::Runtime("runtime".into()), "runtime"),
            (EggressError::Startup("startup".into()), "startup"),
            (EggressError::Reload("reload".into()), "reload"),
            (EggressError::Shutdown("shutdown".into()), "shutdown"),
            (
                EggressError::UnsupportedFeature {
                    feature: "feature".into(),
                    message: "unsupported".into(),
                },
                "unsupported_feature",
            ),
            (EggressError::Internal("internal".into()), "internal"),
        ];
        for (error, category) in cases {
            assert_eq!(error.category(), category);
        }
    }
}
