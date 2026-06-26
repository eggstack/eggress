use crate::error::CompatError;
use crate::uri::PproxyUri;

/// Parsed pproxy-compatible CLI arguments.
#[derive(Debug, Clone)]
pub struct PproxyArgs {
    /// Local listener URIs (from `-l` flags or positional args).
    pub local: Vec<String>,
    /// Remote/upstream URIs (from `-r` flags or positional args).
    pub remotes: Vec<String>,
    /// Raw flags that are not recognized.
    pub raw_flags: Vec<String>,
}

impl PproxyArgs {
    /// Parse from raw argument list (excluding argv[0]).
    pub fn parse(raw: &[String]) -> Result<Self, CompatError> {
        let mut local = Vec::new();
        let mut remotes = Vec::new();
        let mut raw_flags = Vec::new();
        let mut i = 0;

        while i < raw.len() {
            let arg = &raw[i];
            match arg.as_str() {
                "-l" | "--listen" => {
                    i += 1;
                    if i < raw.len() {
                        local.push(raw[i].clone());
                    } else {
                        return Err(CompatError::MissingArgument(
                            "-l requires a value".to_string(),
                        ));
                    }
                }
                "-r" | "--remote" => {
                    i += 1;
                    if i < raw.len() {
                        remotes.push(raw[i].clone());
                    } else {
                        return Err(CompatError::MissingArgument(
                            "-r requires a value".to_string(),
                        ));
                    }
                }
                "--daemon" | "-d" => {
                    raw_flags.push("daemon".to_string());
                }
                "--log" | "-log" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("log={}", raw[i]));
                    }
                }
                "-ul" | "--udp-listen" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("udp-listen={}", raw[i]));
                    }
                }
                "-ur" | "--udp-remote" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("udp-remote={}", raw[i]));
                    }
                }
                "--rulefile" | "-rulefile" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("rulefile={}", raw[i]));
                    }
                }
                other if other.starts_with('-') => {
                    raw_flags.push(other.to_string());
                }
                other => {
                    // Positional: treat as local if no locals yet, else remote
                    if local.is_empty() {
                        local.push(other.to_string());
                    } else {
                        remotes.push(other.to_string());
                    }
                }
            }
            i += 1;
        }

        Ok(PproxyArgs {
            local,
            remotes,
            raw_flags,
        })
    }

    /// Parse all local URIs into typed representations.
    pub fn parse_local_uris(&self) -> Result<Vec<PproxyUri>, CompatError> {
        self.local
            .iter()
            .map(|s| crate::uri::parse_pproxy_uri(s))
            .collect()
    }

    /// Parse all remote URIs into typed representations.
    pub fn parse_remote_uris(&self) -> Result<Vec<PproxyUri>, CompatError> {
        self.remotes
            .iter()
            .map(|s| crate::uri::parse_pproxy_uri(s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy:8080".into(),
        ])
        .unwrap();
        assert_eq!(args.local.len(), 1);
        assert_eq!(args.remotes.len(), 1);
        assert_eq!(args.local[0], "socks5://127.0.0.1:1080");
        assert_eq!(args.remotes[0], "http://proxy:8080");
    }

    #[test]
    fn test_parse_positional() {
        let args =
            PproxyArgs::parse(&["socks5://127.0.0.1:1080".into(), "http://proxy:8080".into()])
                .unwrap();
        assert_eq!(args.local.len(), 1);
        assert_eq!(args.remotes.len(), 1);
    }

    #[test]
    fn test_parse_multiple_remotes() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-r".into(),
            "http://proxy1:8080".into(),
            "-r".into(),
            "socks5://proxy2:1080".into(),
        ])
        .unwrap();
        assert_eq!(args.remotes.len(), 2);
    }

    #[test]
    fn test_parse_missing_value() {
        let result = PproxyArgs::parse(&["-l".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_daemon_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--daemon".into(),
        ])
        .unwrap();
        assert!(args.raw_flags.contains(&"daemon".to_string()));
    }
}
