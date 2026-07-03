use crate::command_runner::CommandRunner;
use crate::inspection::SystemProxySettings;

/// Inspect system proxy settings from environment variables.
///
/// Reads `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`,
/// and their uppercase variants.
pub fn inspect_environment(runner: &dyn CommandRunner) -> SystemProxySettings {
    let _ = runner;
    let env_vars = collect_env_proxy_vars();
    let http_proxy = env_vars
        .get("HTTP_PROXY")
        .or_else(|| env_vars.get("http_proxy"))
        .cloned();
    let https_proxy = env_vars
        .get("HTTPS_PROXY")
        .or_else(|| env_vars.get("https_proxy"))
        .cloned();
    let socks_proxy = env_vars
        .get("ALL_PROXY")
        .or_else(|| env_vars.get("all_proxy"))
        .cloned();
    let no_proxy = env_vars
        .get("NO_PROXY")
        .or_else(|| env_vars.get("no_proxy"))
        .cloned();

    SystemProxySettings {
        source: "environment".to_string(),
        http_proxy,
        https_proxy,
        socks_proxy,
        no_proxy,
        raw: env_vars,
    }
}

/// Generate shell export commands for proxy environment variables.
pub fn generate_env_exports(settings: &SystemProxySettings) -> Vec<String> {
    let mut exports = Vec::new();
    if let Some(ref http) = settings.http_proxy {
        exports.push(format!("export HTTP_PROXY=\"{http}\""));
    }
    if let Some(ref https) = settings.https_proxy {
        exports.push(format!("export HTTPS_PROXY=\"{https}\""));
    }
    if let Some(ref socks) = settings.socks_proxy {
        exports.push(format!("export ALL_PROXY=\"{socks}\""));
    }
    if let Some(ref no_proxy) = settings.no_proxy {
        exports.push(format!("export NO_PROXY=\"{no_proxy}\""));
    }
    exports
}

fn collect_env_proxy_vars() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for key in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
    ] {
        if let Ok(val) = std::env::var(key) {
            map.insert(key.to_string(), val);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_runner::MockCommandRunner;

    #[test]
    fn inspect_environment_reads_env_vars() {
        std::env::set_var("HTTP_PROXY", "http://proxy:8080");
        std::env::set_var("HTTPS_PROXY", "http://proxy:8443");

        let runner = MockCommandRunner::new();
        let settings = inspect_environment(&runner);

        assert_eq!(settings.source, "environment");
        assert_eq!(settings.http_proxy.as_deref(), Some("http://proxy:8080"));
        assert_eq!(settings.https_proxy.as_deref(), Some("http://proxy:8443"));

        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("HTTPS_PROXY");
    }

    #[test]
    fn generate_exports_from_settings() {
        let settings = SystemProxySettings {
            source: "environment".to_string(),
            http_proxy: Some("http://proxy:8080".to_string()),
            https_proxy: Some("http://proxy:8443".to_string()),
            socks_proxy: None,
            no_proxy: Some("localhost".to_string()),
            raw: std::collections::HashMap::new(),
        };

        let exports = generate_env_exports(&settings);
        assert!(exports.contains(&"export HTTP_PROXY=\"http://proxy:8080\"".to_string()));
        assert!(exports.contains(&"export HTTPS_PROXY=\"http://proxy:8443\"".to_string()));
        assert!(exports.contains(&"export NO_PROXY=\"localhost\"".to_string()));
        assert_eq!(exports.len(), 3);
    }
}
