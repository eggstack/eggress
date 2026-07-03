use std::collections::HashMap;

/// Redact sensitive information from proxy URIs and settings.
///
/// Removes passwords from `user:pass@host` patterns and replaces
/// them with `***`. Preserves the rest of the URI structure.
pub fn redact_proxy_uri(uri: &str) -> String {
    if let Some(at_pos) = uri.rfind('@') {
        let prefix = &uri[..at_pos];
        let suffix = &uri[at_pos..]; // includes @
        if let Some(colon_pos) = prefix.rfind(':') {
            let before = &uri[..colon_pos];
            return format!("{}:***{}", before, suffix);
        }
    }
    uri.to_string()
}

/// Redact proxy settings map values.
///
/// Keys containing "proxy" (case-insensitive) have their values
/// processed through `redact_proxy_uri`.
pub fn redact_proxy_settings(settings: &HashMap<String, String>) -> HashMap<String, String> {
    settings
        .iter()
        .map(|(k, v)| {
            if k.to_lowercase().contains("proxy") {
                (k.clone(), redact_proxy_uri(v))
            } else {
                (k.clone(), v.clone())
            }
        })
        .collect()
}

/// Redact a list of proxy URIs.
pub fn redact_proxy_uris(uris: &[String]) -> Vec<String> {
    uris.iter().map(|u| redact_proxy_uri(u)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_uri_with_credentials() {
        assert_eq!(
            redact_proxy_uri("http://user:secret@proxy.example.com:8080"),
            "http://user:***@proxy.example.com:8080"
        );
    }

    #[test]
    fn redact_uri_without_credentials() {
        assert_eq!(
            redact_proxy_uri("http://proxy.example.com:8080"),
            "http://proxy.example.com:8080"
        );
    }

    #[test]
    fn redact_uri_socks_with_credentials() {
        assert_eq!(
            redact_proxy_uri("socks5://admin:password123@127.0.0.1:1080"),
            "socks5://admin:***@127.0.0.1:1080"
        );
    }

    #[test]
    fn redact_uri_no_at_sign() {
        assert_eq!(
            redact_proxy_uri("http://proxy.example.com:8080"),
            "http://proxy.example.com:8080"
        );
    }

    #[test]
    fn redact_settings_map() {
        let mut settings = HashMap::new();
        settings.insert(
            "http_proxy".to_string(),
            "http://user:pass@proxy:8080".to_string(),
        );
        settings.insert("no_proxy".to_string(), "localhost,127.0.0.1".to_string());
        settings.insert(
            "HTTP_PROXY".to_string(),
            "http://admin:secret@proxy:8080".to_string(),
        );

        let redacted = redact_proxy_settings(&settings);
        assert_eq!(
            redacted.get("http_proxy").unwrap(),
            "http://user:***@proxy:8080"
        );
        assert_eq!(redacted.get("no_proxy").unwrap(), "localhost,127.0.0.1");
        assert_eq!(
            redacted.get("HTTP_PROXY").unwrap(),
            "http://admin:***@proxy:8080"
        );
    }

    #[test]
    fn redact_uris_list() {
        let uris = vec![
            "http://user:secret@proxy:8080".to_string(),
            "http://proxy:8080".to_string(),
        ];
        let redacted = redact_proxy_uris(&uris);
        assert_eq!(redacted[0], "http://user:***@proxy:8080");
        assert_eq!(redacted[1], "http://proxy:8080");
    }
}
