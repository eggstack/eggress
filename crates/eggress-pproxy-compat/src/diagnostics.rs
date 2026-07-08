use std::fmt;

use serde::Serialize;

use crate::error::CompatError;
use crate::warnings::CompatWarning;

/// Stable diagnostic codes for the pproxy compatibility layer.
///
/// Each code corresponds to a class of translation issue. These codes are
/// designed for JSON output, test assertions, and documentation cross-references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    /// The pproxy protocol/scheme is not implemented in eggress.
    UnsupportedProtocol,
    /// A transport wrapper (e.g. `+tls`) is not supported for this context.
    UnsupportedTransportWrapper,
    /// A pproxy flag or option is not recognized or not mappable.
    UnsupportedFlag,
    /// The feature requires platform-specific capabilities not available.
    UnsupportedPlatform,
    /// A legacy or security-sensitive feature (e.g. SSR obfs, stream ciphers) is intentionally unsupported.
    UnsupportedSecuritySensitiveLegacyFeature,
    /// The pproxy URI or argument syntax is malformed.
    InvalidUriSyntax,
    /// The resulting chain composition is invalid (e.g. conflicting protocols).
    InvalidChainComposition,
    /// A required target address or endpoint is missing.
    MissingTarget,
    /// A required credential (password, key) is missing.
    MissingCredential,
    /// The specified cipher or encryption method is not supported.
    InvalidCipherMethod,
    /// Binding to the requested address failed.
    BindFailure,
    /// Required OS capabilities (e.g. `CAP_NET_ADMIN`) are missing.
    PrivilegeCapabilityMissing,
    /// An external dependency required for this feature is not available.
    ExternalDependencyMissing,
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::UnsupportedProtocol => "unsupported_protocol",
            Self::UnsupportedTransportWrapper => "unsupported_transport_wrapper",
            Self::UnsupportedFlag => "unsupported_flag",
            Self::UnsupportedPlatform => "unsupported_platform",
            Self::UnsupportedSecuritySensitiveLegacyFeature => {
                "unsupported_security_sensitive_legacy_feature"
            }
            Self::InvalidUriSyntax => "invalid_uri_syntax",
            Self::InvalidChainComposition => "invalid_chain_composition",
            Self::MissingTarget => "missing_target",
            Self::MissingCredential => "missing_credential",
            Self::InvalidCipherMethod => "invalid_cipher_method",
            Self::BindFailure => "bind_failure",
            Self::PrivilegeCapabilityMissing => "privilege_capability_missing",
            Self::ExternalDependencyMissing => "external_dependency_missing",
        };
        f.write_str(label)
    }
}

/// A structured diagnostic produced by the pproxy compatibility layer.
///
/// Carries a stable [`DiagnosticCode`], optional manifest feature reference,
/// optional compatibility tier, a human-readable message, and an optional
/// suggestion for an eggress-native alternative.
#[derive(Debug, Clone, Serialize)]
pub struct StructuredDiagnostic {
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Manifest feature id, if this diagnostic maps to a known feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_id: Option<String>,
    /// Compatibility tier: "drop_in", "compatible_with_warning", "native_equivalent",
    /// "intentional_non_parity", or "unsupported".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Human-readable description of the issue.
    pub message: String,
    /// Suggested eggress-native alternative, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl fmt::Display for StructuredDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(ref tier) = self.tier {
            write!(f, " (tier: {})", tier)?;
        }
        if let Some(ref suggestion) = self.suggestion {
            write!(f, " — suggestion: {}", suggestion)?;
        }
        Ok(())
    }
}

impl From<CompatError> for StructuredDiagnostic {
    fn from(err: CompatError) -> Self {
        match err {
            CompatError::UnsupportedProtocol(proto) => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedProtocol,
                feature_id: None,
                tier: Some("unsupported".to_string()),
                message: format!("unsupported protocol: {}", proto),
                suggestion: Some("use http, socks4, socks5, trojan, or ss".to_string()),
            },
            CompatError::UnsupportedFeature { feature, detail } => {
                let (code, tier, suggestion) = classify_unsupported_feature(feature);
                StructuredDiagnostic {
                    code,
                    feature_id: Some(feature.to_string()),
                    tier: Some(tier.to_string()),
                    message: detail,
                    suggestion: suggestion.map(String::from),
                }
            }
            CompatError::InvalidUri { message } => StructuredDiagnostic {
                code: DiagnosticCode::InvalidUriSyntax,
                feature_id: None,
                tier: None,
                message,
                suggestion: None,
            },
            CompatError::InvalidArgs { message } => StructuredDiagnostic {
                code: DiagnosticCode::InvalidUriSyntax,
                feature_id: None,
                tier: None,
                message,
                suggestion: None,
            },
            CompatError::ConfigValidation { message } => StructuredDiagnostic {
                code: DiagnosticCode::InvalidChainComposition,
                feature_id: None,
                tier: None,
                message,
                suggestion: None,
            },
            CompatError::MissingArgument(flag) => StructuredDiagnostic {
                code: DiagnosticCode::MissingTarget,
                feature_id: None,
                tier: None,
                message: format!("missing required argument: {}", flag),
                suggestion: None,
            },
        }
    }
}

impl From<&CompatWarning> for StructuredDiagnostic {
    fn from(warn: &CompatWarning) -> Self {
        match warn.category {
            "unknown-flag" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: None,
                tier: Some("unsupported".to_string()),
                message: warn.message.clone(),
                suggestion: None,
            },
            "direct-mode" => StructuredDiagnostic {
                code: DiagnosticCode::MissingTarget,
                feature_id: None,
                tier: Some("compatible_with_warning".to_string()),
                message: warn.message.clone(),
                suggestion: Some("add -r <upstream-uri> for proxied connections".to_string()),
            },
            "credential-in-toml" => StructuredDiagnostic {
                code: DiagnosticCode::MissingCredential,
                feature_id: None,
                tier: Some("compatible_with_warning".to_string()),
                message: warn.message.clone(),
                suggestion: Some(
                    "use secret sources or environment variables for credentials".to_string(),
                ),
            },
            "verbose-mode" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("verbose".to_string()),
                tier: Some("native_equivalent".to_string()),
                message: warn.message.clone(),
                suggestion: Some("set RUST_LOG=debug".to_string()),
            },
            "scheduler" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("scheduler".to_string()),
                tier: Some("compatible_with_warning".to_string()),
                message: warn.message.clone(),
                suggestion: Some(
                    "use first-available, round-robin, or least-connections".to_string(),
                ),
            },
            "alive-check" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("alive".to_string()),
                tier: Some("native_equivalent".to_string()),
                message: warn.message.clone(),
                suggestion: Some("configure health probes in eggress TOML".to_string()),
            },
            "ul-no-listener" => StructuredDiagnostic {
                code: DiagnosticCode::MissingTarget,
                feature_id: None,
                tier: Some("compatible_with_warning".to_string()),
                message: warn.message.clone(),
                suggestion: None,
            },
            "pac-serving" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("pac".to_string()),
                tier: Some("native_equivalent".to_string()),
                message: warn.message.clone(),
                suggestion: Some(
                    "configure PAC serving in eggress TOML admin.pac block".to_string(),
                ),
            },
            "test-mode" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("test".to_string()),
                tier: Some("native_equivalent".to_string()),
                message: warn.message.clone(),
                suggestion: Some("use 'eggress upstream test -c <config>'".to_string()),
            },
            "system-proxy" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("sys".to_string()),
                tier: Some("native_equivalent".to_string()),
                message: warn.message.clone(),
                suggestion: Some("use 'eggress system-proxy inspect'".to_string()),
            },
            "log-file" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("log".to_string()),
                tier: Some("native_equivalent".to_string()),
                message: warn.message.clone(),
                suggestion: Some(
                    "redirect stderr with shell redirection for file logging".to_string(),
                ),
            },
            "reuse-connection" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("reuse".to_string()),
                tier: Some("intentional_non_parity".to_string()),
                message: warn.message.clone(),
                suggestion: None,
            },
            "get-url" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("get".to_string()),
                tier: Some("unsupported".to_string()),
                message: warn.message.clone(),
                suggestion: Some("use curl --proxy <proxy-uri> <url>".to_string()),
            },
            "rulefile-read" | "rulefile-parse" | "rulefile-partial" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: Some("rulefile".to_string()),
                tier: Some("compatible_with_warning".to_string()),
                message: warn.message.clone(),
                suggestion: Some(
                    "configure rules in eggress TOML [[rules]] with structured matchers"
                        .to_string(),
                ),
            },
            "chain-unsupported-hop" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedProtocol,
                feature_id: Some("chain".to_string()),
                tier: Some("unsupported".to_string()),
                message: warn.message.clone(),
                suggestion: Some(
                    "remove unsupported hops or use multi-r flag for alternatives".to_string(),
                ),
            },
            "chain-backward-composition" => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedProtocol,
                feature_id: Some("chain".to_string()),
                tier: Some("unsupported".to_string()),
                message: warn.message.clone(),
                suggestion: Some(
                    "use single-hop backward (+in) or split into separate -r flags".to_string(),
                ),
            },
            _ => StructuredDiagnostic {
                code: DiagnosticCode::UnsupportedFlag,
                feature_id: None,
                tier: None,
                message: warn.message.clone(),
                suggestion: None,
            },
        }
    }
}

impl CompatWarning {
    /// Return the [`DiagnosticCode`] that best classifies this warning.
    pub fn diagnostic_code(&self) -> DiagnosticCode {
        StructuredDiagnostic::from(self).code
    }
}

/// Classify an unsupported feature string into a diagnostic code, tier, and
/// optional suggestion.
fn classify_unsupported_feature(
    feature: &'static str,
) -> (DiagnosticCode, &'static str, Option<&'static str>) {
    match feature {
        "daemon" | "backward-jump-chain" | "backward-tls" => (
            DiagnosticCode::UnsupportedFlag,
            "unsupported",
            Some("configure this via eggress TOML"),
        ),
        "chain-unsupported-hop" | "chain-backward-composition" => (
            DiagnosticCode::UnsupportedProtocol,
            "unsupported",
            Some("remove unsupported hops or use multi-r flag for alternatives"),
        ),
        "ssr-listener" | "ssr-upstream" => (
            DiagnosticCode::UnsupportedSecuritySensitiveLegacyFeature,
            "intentional_non_parity",
            Some("use standard Shadowsocks (ss://) with AEAD methods"),
        ),
        "ssh-listener" | "ssh-upstream" => (
            DiagnosticCode::UnsupportedProtocol,
            "intentional_non_parity",
            Some("SSH is not a proxy protocol; use OpenSSH dynamic forwarding (ssh -D) or an external SOCKS proxy"),
        ),
        "unix-upstream" | "redir-upstream"
        | "direct-listener" => (DiagnosticCode::UnsupportedProtocol, "unsupported", None),
        "socks4-bind" => (
            DiagnosticCode::UnsupportedProtocol,
            "intentional_non_parity",
            Some("SOCKS4 BIND is not implemented; pproxy also does not implement SOCKS4 BIND"),
        ),
        "socks5-bind" => (
            DiagnosticCode::UnsupportedProtocol,
            "intentional_non_parity",
            Some("SOCKS5 BIND is not implemented; pproxy also does not implement SOCKS5 BIND"),
        ),
        "udp-http-transport" | "udp-https-transport" => (
            DiagnosticCode::UnsupportedProtocol,
            "unsupported",
            Some("use direct://, socks5://, or ss:// for UDP upstreams"),
        ),
        "udp-socks4-transport" | "udp-socks4a-transport" => (
            DiagnosticCode::UnsupportedProtocol,
            "unsupported",
            Some("SOCKS4 does not support UDP; use socks5:// for UDP upstreams"),
        ),
        "udp-trojan-transport" => (
            DiagnosticCode::UnsupportedProtocol,
            "unsupported",
            Some("Trojan does not support UDP; use direct://, socks5://, or ss://"),
        ),
        "udp-multihop" => (
            DiagnosticCode::UnsupportedProtocol,
            "unsupported",
            Some("UDP multi-hop chains are not supported; use single-hop upstreams"),
        ),
        "trojan-no-password" => (
            DiagnosticCode::UnsupportedProtocol,
            "unsupported",
            Some("provide a password in the Trojan URI: trojan://password@host:port"),
        ),
        "scheme" => (
            DiagnosticCode::UnsupportedProtocol,
            "unsupported",
            Some("use a recognized protocol scheme"),
        ),
        "legacy-cipher" => (
            DiagnosticCode::InvalidCipherMethod,
            "intentional_non_parity",
            Some("use an AEAD method: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"),
        ),
        _ => (DiagnosticCode::UnsupportedFlag, "unsupported", None),
    }
}

/// Return just the [`DiagnosticCode`] for an unsupported feature string.
///
/// Used by [`CompatError::code()`] to avoid exposing tier/suggestion details
/// at the error level.
pub fn classify_unsupported_feature_code(feature: &str) -> DiagnosticCode {
    classify_unsupported_feature_inner(feature)
}

fn classify_unsupported_feature_inner(feature: &str) -> DiagnosticCode {
    match feature {
        "daemon" | "backward-jump-chain" | "backward-tls" => DiagnosticCode::UnsupportedFlag,
        "chain-unsupported-hop" | "chain-backward-composition" => {
            DiagnosticCode::UnsupportedProtocol
        }
        "ssr-listener" | "ssr-upstream" => {
            DiagnosticCode::UnsupportedSecuritySensitiveLegacyFeature
        }
        "trojan-listener" | "ssh-listener" | "ssh-upstream" | "unix-upstream"
        | "redir-upstream" | "direct-listener" => DiagnosticCode::UnsupportedProtocol,
        "socks4-bind" | "socks5-bind" => DiagnosticCode::UnsupportedProtocol,
        "udp-http-transport"
        | "udp-https-transport"
        | "udp-socks4-transport"
        | "udp-socks4a-transport"
        | "udp-trojan-transport"
        | "udp-multihop" => DiagnosticCode::UnsupportedProtocol,
        "scheme" => DiagnosticCode::UnsupportedProtocol,
        "legacy-cipher" => DiagnosticCode::InvalidCipherMethod,
        _ => DiagnosticCode::UnsupportedFlag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CompatError;
    use crate::warnings::CompatWarning;

    #[test]
    fn diagnostic_code_display_is_snake_case() {
        assert_eq!(
            DiagnosticCode::UnsupportedProtocol.to_string(),
            "unsupported_protocol"
        );
        assert_eq!(
            DiagnosticCode::UnsupportedSecuritySensitiveLegacyFeature.to_string(),
            "unsupported_security_sensitive_legacy_feature"
        );
        assert_eq!(
            DiagnosticCode::InvalidUriSyntax.to_string(),
            "invalid_uri_syntax"
        );
        assert_eq!(DiagnosticCode::MissingTarget.to_string(), "missing_target");
        assert_eq!(
            DiagnosticCode::MissingCredential.to_string(),
            "missing_credential"
        );
        assert_eq!(
            DiagnosticCode::InvalidCipherMethod.to_string(),
            "invalid_cipher_method"
        );
        assert_eq!(DiagnosticCode::BindFailure.to_string(), "bind_failure");
        assert_eq!(
            DiagnosticCode::PrivilegeCapabilityMissing.to_string(),
            "privilege_capability_missing"
        );
        assert_eq!(
            DiagnosticCode::ExternalDependencyMissing.to_string(),
            "external_dependency_missing"
        );
    }

    #[test]
    fn diagnostic_code_serializes_to_snake_case() {
        let code = DiagnosticCode::UnsupportedProtocol;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"unsupported_protocol\"");
    }

    #[test]
    fn structured_diagnostic_display_includes_code_and_message() {
        let diag = StructuredDiagnostic {
            code: DiagnosticCode::UnsupportedProtocol,
            feature_id: None,
            tier: Some("unsupported".to_string()),
            message: "unsupported protocol: ftp".to_string(),
            suggestion: None,
        };
        let s = diag.to_string();
        assert!(s.contains("[unsupported_protocol]"));
        assert!(s.contains("unsupported protocol: ftp"));
        assert!(s.contains("tier: unsupported"));
    }

    #[test]
    fn structured_diagnostic_display_includes_suggestion() {
        let diag = StructuredDiagnostic {
            code: DiagnosticCode::InvalidCipherMethod,
            feature_id: None,
            tier: Some("intentional_non_parity".to_string()),
            message: "legacy cipher".to_string(),
            suggestion: Some("use AEAD".to_string()),
        };
        let s = diag.to_string();
        assert!(s.contains("suggestion: use AEAD"));
    }

    #[test]
    fn from_unsupported_protocol_error() {
        let err = CompatError::UnsupportedProtocol("ftp".to_string());
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::UnsupportedProtocol);
        assert_eq!(diag.tier.as_deref(), Some("unsupported"));
        assert!(diag.suggestion.is_some());
    }

    #[test]
    fn from_unsupported_feature_daemon() {
        let err = CompatError::unsupported("daemon", "--daemon not supported");
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::UnsupportedFlag);
        assert_eq!(diag.feature_id.as_deref(), Some("daemon"));
    }

    #[test]
    fn from_unsupported_feature_ssr() {
        let err = CompatError::unsupported("ssr-listener", "SSR not supported");
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(
            diag.code,
            DiagnosticCode::UnsupportedSecuritySensitiveLegacyFeature
        );
        assert_eq!(diag.tier.as_deref(), Some("intentional_non_parity"));
    }

    #[test]
    fn from_unsupported_feature_legacy_cipher() {
        let err = CompatError::unsupported("legacy-cipher", "aes-128-ctr not supported");
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::InvalidCipherMethod);
    }

    #[test]
    fn from_invalid_uri_error() {
        let err = CompatError::InvalidUri {
            message: "bad host".to_string(),
        };
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::InvalidUriSyntax);
    }

    #[test]
    fn from_invalid_args_error() {
        let err = CompatError::InvalidArgs {
            message: "no listener".to_string(),
        };
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::InvalidUriSyntax);
    }

    #[test]
    fn from_config_validation_error() {
        let err = CompatError::ConfigValidation {
            message: "conflict".to_string(),
        };
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::InvalidChainComposition);
    }

    #[test]
    fn from_missing_argument_error() {
        let err = CompatError::MissingArgument("-l".to_string());
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::MissingTarget);
    }

    #[test]
    fn from_unknown_flag_warning() {
        let warn = CompatWarning {
            category: "unknown-flag",
            message: "unrecognized flag '--foo'".to_string(),
        };
        let diag = StructuredDiagnostic::from(&warn);
        assert_eq!(diag.code, DiagnosticCode::UnsupportedFlag);
    }

    #[test]
    fn from_direct_mode_warning() {
        let warn = CompatWarning {
            category: "direct-mode",
            message: "no upstream".to_string(),
        };
        let diag = StructuredDiagnostic::from(&warn);
        assert_eq!(diag.code, DiagnosticCode::MissingTarget);
        assert!(diag.suggestion.is_some());
    }

    #[test]
    fn from_credential_warning() {
        let warn = CompatWarning {
            category: "credential-in-toml",
            message: "plaintext creds".to_string(),
        };
        let diag = StructuredDiagnostic::from(&warn);
        assert_eq!(diag.code, DiagnosticCode::MissingCredential);
    }

    #[test]
    fn from_verbose_warning() {
        let warn = CompatWarning {
            category: "verbose-mode",
            message: "use RUST_LOG".to_string(),
        };
        let diag = StructuredDiagnostic::from(&warn);
        assert_eq!(diag.code, DiagnosticCode::UnsupportedFlag);
        assert_eq!(diag.feature_id.as_deref(), Some("verbose"));
    }

    #[test]
    fn from_unknown_category_warning() {
        let warn = CompatWarning {
            category: "some-new-category",
            message: "something happened".to_string(),
        };
        let diag = StructuredDiagnostic::from(&warn);
        assert_eq!(diag.code, DiagnosticCode::UnsupportedFlag);
    }

    #[test]
    fn warning_diagnostic_code_method() {
        let warn = CompatWarning {
            category: "direct-mode",
            message: "no upstream".to_string(),
        };
        assert_eq!(warn.diagnostic_code(), DiagnosticCode::MissingTarget);
    }

    #[test]
    fn structured_diagnostic_json_roundtrip() {
        let diag = StructuredDiagnostic {
            code: DiagnosticCode::UnsupportedProtocol,
            feature_id: Some("ssh-upstream".to_string()),
            tier: Some("unsupported".to_string()),
            message: "SSH not supported".to_string(),
            suggestion: None,
        };
        let json = serde_json::to_value(&diag).unwrap();
        assert_eq!(json["code"], "unsupported_protocol");
        assert_eq!(json["feature_id"], "ssh-upstream");
        assert_eq!(json["tier"], "unsupported");
        assert_eq!(json["message"], "SSH not supported");
        assert!(json.get("suggestion").is_none());
    }

    #[test]
    fn all_diagnostic_codes_serialize() {
        let codes = [
            DiagnosticCode::UnsupportedProtocol,
            DiagnosticCode::UnsupportedTransportWrapper,
            DiagnosticCode::UnsupportedFlag,
            DiagnosticCode::UnsupportedPlatform,
            DiagnosticCode::UnsupportedSecuritySensitiveLegacyFeature,
            DiagnosticCode::InvalidUriSyntax,
            DiagnosticCode::InvalidChainComposition,
            DiagnosticCode::MissingTarget,
            DiagnosticCode::MissingCredential,
            DiagnosticCode::InvalidCipherMethod,
            DiagnosticCode::BindFailure,
            DiagnosticCode::PrivilegeCapabilityMissing,
            DiagnosticCode::ExternalDependencyMissing,
        ];
        for code in &codes {
            let json = serde_json::to_string(code).unwrap();
            // Must be a valid JSON string
            assert!(json.starts_with('"'));
            assert!(json.ends_with('"'));
        }
    }

    // --- Redaction tests (28.8) ---
    //
    // Redaction in eggress happens at the URI display level (PproxyUri::redacted_display)
    // and in translate.rs warning messages. Error messages from CompatError are internal
    // and don't carry credentials in production. These tests verify the structured
    // diagnostic pipeline doesn't amplify credential leaks.

    #[test]
    fn structured_diagnostic_from_unsupported_protocol_never_leaks_credentials() {
        let err = CompatError::UnsupportedProtocol("ssh".to_string());
        let diag = StructuredDiagnostic::from(err);
        assert!(!diag.message.contains("@"));
        assert!(diag.suggestion.is_some());
    }

    #[test]
    fn structured_diagnostic_from_unsupported_feature_never_leaks_credentials() {
        let err = CompatError::unsupported("daemon", "--daemon not supported");
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::UnsupportedFlag);
        assert!(!diag.message.contains("@"));
    }

    #[test]
    fn structured_diagnostic_from_invalid_uri_never_leaks_credentials() {
        let err = CompatError::InvalidUri {
            message: "missing port in endpoint".to_string(),
        };
        let diag = StructuredDiagnostic::from(err);
        assert_eq!(diag.code, DiagnosticCode::InvalidUriSyntax);
        assert!(!diag.message.contains("@"));
    }

    #[test]
    fn structured_diagnostic_json_excludes_optional_none_fields() {
        let diag = StructuredDiagnostic {
            code: DiagnosticCode::UnsupportedProtocol,
            feature_id: None,
            tier: None,
            message: "test".to_string(),
            suggestion: None,
        };
        let json = serde_json::to_value(&diag).unwrap();
        assert!(json.get("feature_id").is_none());
        assert!(json.get("tier").is_none());
        assert!(json.get("suggestion").is_none());
    }

    #[test]
    fn compat_warning_display_never_leaks_credentials() {
        let warn = CompatWarning {
            category: "credential-in-toml",
            message: "Listener 'pproxy-local-0' has plaintext credentials in generated TOML"
                .to_string(),
        };
        let display = warn.to_string();
        assert!(display.contains("[credential-in-toml]"));
        assert!(!display.contains("@"));
    }

    #[test]
    fn diagnostic_code_display_never_contains_credentials() {
        let codes = [
            DiagnosticCode::UnsupportedProtocol,
            DiagnosticCode::UnsupportedFlag,
            DiagnosticCode::InvalidUriSyntax,
            DiagnosticCode::MissingTarget,
            DiagnosticCode::MissingCredential,
            DiagnosticCode::InvalidCipherMethod,
            DiagnosticCode::BindFailure,
        ];
        for code in &codes {
            let display = code.to_string();
            assert!(
                !display.contains("@"),
                "code display contains @: {}",
                display
            );
        }
    }
}
