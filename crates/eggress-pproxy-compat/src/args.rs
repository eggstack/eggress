use crate::error::CompatError;
use crate::uri::PproxyUri;
use crate::warnings::{CompatWarning, TranslationOutput};

/// Flags that the compat layer explicitly handles.
const KNOWN_FLAGS: &[&str] = &[
    "-l",
    "--listen",
    "-r",
    "--remote",
    "--daemon",
    "-d",
    "--log",
    "-log",
    "-ul",
    "--udp-listen",
    "-ur",
    "--udp-remote",
    "--rulefile",
    "-rulefile",
    "-v",
    "-s",
    "-a",
    "--ssl",
    "-b",
];

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
                "-v" => {
                    raw_flags.push("verbose".to_string());
                }
                "-s" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("scheduler={}", raw[i]));
                    } else {
                        return Err(CompatError::MissingArgument(
                            "-s requires a value".to_string(),
                        ));
                    }
                }
                "-a" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("alive={}", raw[i]));
                    } else {
                        return Err(CompatError::MissingArgument(
                            "-a requires a value".to_string(),
                        ));
                    }
                }
                "--ssl" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("ssl={}", raw[i]));
                    } else {
                        return Err(CompatError::MissingArgument(
                            "--ssl requires a value".to_string(),
                        ));
                    }
                }
                "-b" => {
                    i += 1;
                    if i < raw.len() {
                        raw_flags.push(format!("block={}", raw[i]));
                    } else {
                        return Err(CompatError::MissingArgument(
                            "-b requires a value".to_string(),
                        ));
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

    /// Identify unrecognized flags and return warnings for them.
    pub fn unknown_flag_warnings(&self) -> Vec<CompatWarning> {
        let mut warnings = Vec::new();
        for flag in &self.raw_flags {
            // Check if this is a known structured flag (key=value form)
            let base_flag = flag.split('=').next().unwrap_or(flag);
            let is_known = KNOWN_FLAGS.contains(&base_flag)
                || matches!(
                    base_flag,
                    "verbose" | "scheduler" | "alive" | "ssl" | "block"
                );
            if !is_known {
                warnings.push(CompatWarning {
                    category: "unknown-flag",
                    message: format!(
                        "unrecognized flag '{}'; it will be ignored in translation",
                        flag
                    ),
                });
            }
        }
        warnings
    }

    /// Return a TranslationOutput containing just the unknown-flag warnings.
    pub fn unknown_flag_translation_output(&self) -> TranslationOutput {
        let warnings = self.unknown_flag_warnings();
        TranslationOutput::new(String::new()).with_warnings(warnings)
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

    #[test]
    fn test_parse_verbose_flag() {
        let args = PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-v".into()])
            .unwrap();
        assert!(args.raw_flags.contains(&"verbose".to_string()));
    }

    #[test]
    fn test_parse_scheduler_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-s".into(),
            "rr".into(),
        ])
        .unwrap();
        assert!(args.raw_flags.contains(&"scheduler=rr".to_string()));
    }

    #[test]
    fn test_parse_alive_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-a".into(),
            "10".into(),
        ])
        .unwrap();
        assert!(args.raw_flags.contains(&"alive=10".to_string()));
    }

    #[test]
    fn test_parse_ssl_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--ssl".into(),
            "cert.pem,key.pem".into(),
        ])
        .unwrap();
        assert!(args.raw_flags.contains(&"ssl=cert.pem,key.pem".to_string()));
    }

    #[test]
    fn test_parse_block_flag() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-b".into(),
            ".*\\.example\\.com".into(),
        ])
        .unwrap();
        assert!(args
            .raw_flags
            .contains(&"block=.*\\.example\\.com".to_string()));
    }

    #[test]
    fn test_unknown_flag_warnings() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--unknown-flag".into(),
            "-x".into(),
        ])
        .unwrap();
        let warnings = args.unknown_flag_warnings();
        assert_eq!(warnings.len(), 2);
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("--unknown-flag")));
        assert!(warnings.iter().any(|w| w.message.contains("-x")));
    }

    #[test]
    fn test_known_flags_no_warnings() {
        let args = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "-v".into(),
            "-s".into(),
            "rr".into(),
            "-a".into(),
            "10".into(),
        ])
        .unwrap();
        let warnings = args.unknown_flag_warnings();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_scheduler_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-s".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_alive_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-a".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_ssl_missing_value() {
        let result = PproxyArgs::parse(&[
            "-l".into(),
            "socks5://127.0.0.1:1080".into(),
            "--ssl".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_block_missing_value() {
        let result =
            PproxyArgs::parse(&["-l".into(), "socks5://127.0.0.1:1080".into(), "-b".into()]);
        assert!(result.is_err());
    }
}
