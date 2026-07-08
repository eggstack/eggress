use crate::uri::PproxyUri;

// Diagnostic messages for pproxy compatibility translation.
// These diagnostics provide actionable guidance when features are partially
// supported or have platform-specific limitations.

/// Diagnostic for transparent proxy capability check failures.
pub fn diagnostic_transparent_proxy_unsupported(uri: &PproxyUri) -> String {
    let bind = if uri.host.is_empty() {
        format!(":{}", uri.port)
    } else {
        format!("{}:{}", uri.host, uri.port)
    };
    format!(
        "Transparent proxy (redir) listener on {} requires platform-specific capabilities. \
         On Linux, ensure sysctl net.ipv4.ip_forward=1 and iptables REDIRECT rules are configured. \
         On macOS, transparent proxy requires pf(4) and is not directly supported. \
         On Windows, requires WinDivert or similar driver. \
         Generated TOML assumes the platform supports transparent proxy.",
        bind
    )
}

/// Diagnostic for Unix socket path validation errors.
pub fn diagnostic_unix_socket_path_invalid(path: &str) -> String {
    if path.is_empty() {
        return "Unix socket path cannot be empty. Use unix:///path/to/socket format.".to_string();
    }
    if !path.starts_with('/') {
        return format!(
            "Unix socket path '{}' is not absolute. Use an absolute path (e.g. /tmp/proxy.sock).",
            path
        );
    }
    if path.len() > 104 {
        // typical sun_path limit on most platforms
        return format!(
            "Unix socket path '{}' exceeds typical maximum length of 104 bytes. \
             Shorten the path or use a symlink.",
            path
        );
    }
    format!("Unix socket path '{}' is valid.", path)
}

/// Diagnostic for platform-specific unsupported features.
pub fn diagnostic_platform_unsupported(feature: &str) -> String {
    match feature {
        "redir" => {
            "Transparent proxy (redir) is platform-dependent. \
             Linux: fully supported with iptables REDIRECT/TPROXY. \
             macOS: not directly supported (pf-based redirect requires manual setup). \
             Windows: not supported without third-party drivers."
                .to_string()
        }
        "unix" => {
            "Unix domain socket listeners are supported on all Unix-like platforms. \
             On Windows, Unix domain sockets require Windows 10 build 17063+ and may have limitations."
                .to_string()
        }
        "transparent" => {
            "Transparent proxy requires elevated privileges (CAP_NET_ADMIN on Linux) \
             and proper kernel/iptables configuration."
                .to_string()
        }
        other => {
            format!("Feature '{}' has platform-specific considerations.", other)
        }
    }
}

/// Validate a redir listener URI and return diagnostics.
pub fn validate_redir_listener(uri: &PproxyUri) -> Vec<String> {
    let mut diagnostics = Vec::new();

    if uri.port == 0 {
        diagnostics.push(
            "Redir listener has port 0; this will bind to an ephemeral port. \
             Specify a fixed port for transparent proxy rules to target."
                .to_string(),
        );
    }

    diagnostics.push(diagnostic_transparent_proxy_unsupported(uri));
    diagnostics
}

/// Validate a unix listener URI and return diagnostics.
pub fn validate_unix_listener(uri: &PproxyUri) -> Vec<String> {
    let mut diagnostics = Vec::new();

    match &uri.path {
        Some(path) => {
            let path_diag = diagnostic_unix_socket_path_invalid(path);
            if !path_diag.ends_with("is valid.") {
                diagnostics.push(path_diag);
            }
            diagnostics.push(diagnostic_platform_unsupported("unix"));
        }
        None => {
            diagnostics.push(
                "Unix socket listener has no path specified. \
                 A default path (/tmp/eggress.sock) will be used."
                    .to_string(),
            );
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_transparent_proxy_unsupported() {
        let uri = PproxyUri {
            scheme: "redir".to_string(),
            username: None,
            password: None,
            host: "127.0.0.1".to_string(),
            port: 12345,
            tls: false,
            ssl: false,
            inbound: false,
            backward_num: 0,
            rule: None,
            rules_file: None,
            path: None,
        };
        let diag = diagnostic_transparent_proxy_unsupported(&uri);
        assert!(diag.contains("127.0.0.1:12345"));
        assert!(diag.contains("iptables") || diag.contains("platform"));
    }

    #[test]
    fn test_diagnostic_unix_socket_path_invalid_empty() {
        let diag = diagnostic_unix_socket_path_invalid("");
        assert!(diag.contains("cannot be empty"));
    }

    #[test]
    fn test_diagnostic_unix_socket_path_invalid_relative() {
        let diag = diagnostic_unix_socket_path_invalid("tmp/proxy.sock");
        assert!(diag.contains("not absolute"));
    }

    #[test]
    fn test_diagnostic_unix_socket_path_valid() {
        let diag = diagnostic_unix_socket_path_invalid("/tmp/proxy.sock");
        assert!(diag.contains("is valid"));
    }

    #[test]
    fn test_diagnostic_unix_socket_path_too_long() {
        let long_path = format!("/tmp/{}.sock", "a".repeat(200));
        let diag = diagnostic_unix_socket_path_invalid(&long_path);
        assert!(diag.contains("exceeds"));
    }

    #[test]
    fn test_diagnostic_platform_unsupported_redir() {
        let diag = diagnostic_platform_unsupported("redir");
        assert!(diag.contains("Linux") || diag.contains("macOS"));
    }

    #[test]
    fn test_diagnostic_platform_unsupported_unix() {
        let diag = diagnostic_platform_unsupported("unix");
        assert!(diag.contains("Unix"));
    }

    #[test]
    fn test_validate_redir_listener_port_zero() {
        let uri = PproxyUri {
            scheme: "redir".to_string(),
            username: None,
            password: None,
            host: String::new(),
            port: 0,
            tls: false,
            ssl: false,
            inbound: false,
            backward_num: 0,
            rule: None,
            rules_file: None,
            path: None,
        };
        let diagnostics = validate_redir_listener(&uri);
        assert!(diagnostics.iter().any(|d| d.contains("port 0")));
    }

    #[test]
    fn test_validate_unix_listener_no_path() {
        let uri = PproxyUri {
            scheme: "unix".to_string(),
            username: None,
            password: None,
            host: String::new(),
            port: 0,
            tls: false,
            ssl: false,
            inbound: false,
            backward_num: 0,
            rule: None,
            rules_file: None,
            path: None,
        };
        let diagnostics = validate_unix_listener(&uri);
        assert!(diagnostics.iter().any(|d| d.contains("no path")));
    }
}
