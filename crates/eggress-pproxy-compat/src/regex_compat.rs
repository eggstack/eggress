use std::fmt;
use std::path::{Path, PathBuf};

/// Maximum pattern length for compiled regexes (compile-time guard).
const MAX_PATTERN_LEN: usize = 4096;

/// Maximum number of rule entries per file.
const MAX_RULE_ENTRIES: usize = 10_000;

/// Regex backend used for pattern compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegexBackend {
    /// Native Rust `regex` crate (fast, no look-around/backreferences).
    Fast,
    /// `fancy_regex` crate (Perl/Python-like features: look-around, backtracking).
    Fancy,
}

impl fmt::Display for RegexBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fast => f.write_str("fast"),
            Self::Fancy => f.write_str("fancy"),
        }
    }
}

/// A compiled regex that uses either the fast `regex` backend or the
/// `fancy_regex` backend for pproxy compatibility mode.
///
/// The `fancy_regex` backend supports Perl/Python-like constructs such as
/// look-around and backreferences, which are common in pproxy rule files.
/// The fast backend is used when fancy features are not needed.
#[derive(Clone)]
pub enum CompatRegex {
    Fast(regex::Regex),
    Fancy(fancy_regex::Regex),
}

impl fmt::Debug for CompatRegex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fast(r) => write!(f, "CompatRegex::Fast({})", r.as_str()),
            Self::Fancy(r) => write!(f, "CompatRegex::Fancy({})", r.as_str()),
        }
    }
}

impl CompatRegex {
    /// Try to compile a pattern using the fast `regex` backend first.
    /// Falls back to `fancy_regex` if the pattern contains unsupported
    /// constructs (look-around, backreferences, etc.).
    pub fn compile(pattern: &str) -> Result<Self, RegexCompileError> {
        if pattern.len() > MAX_PATTERN_LEN {
            return Err(RegexCompileError::PatternTooLong {
                len: pattern.len(),
                max: MAX_PATTERN_LEN,
            });
        }

        // Try fast regex first
        match regex::Regex::new(pattern) {
            Ok(r) => Ok(Self::Fast(r)),
            Err(_) => {
                // Fall back to fancy_regex for Perl/Python-like constructs
                match fancy_regex::Regex::new(pattern) {
                    Ok(r) => Ok(Self::Fancy(r)),
                    Err(e) => Err(RegexCompileError::CompileError {
                        pattern: pattern.to_string(),
                        message: e.to_string(),
                    }),
                }
            }
        }
    }

    /// Compile using only the fancy_regex backend (force compatibility mode).
    pub fn compile_fancy(pattern: &str) -> Result<Self, RegexCompileError> {
        if pattern.len() > MAX_PATTERN_LEN {
            return Err(RegexCompileError::PatternTooLong {
                len: pattern.len(),
                max: MAX_PATTERN_LEN,
            });
        }

        match fancy_regex::Regex::new(pattern) {
            Ok(r) => Ok(Self::Fancy(r)),
            Err(e) => Err(RegexCompileError::CompileError {
                pattern: pattern.to_string(),
                message: e.to_string(),
            }),
        }
    }

    /// Returns true if the given text matches this regex.
    pub fn is_match(&self, text: &str) -> Result<bool, RegexMatchError> {
        match self {
            Self::Fast(r) => Ok(r.is_match(text)),
            Self::Fancy(r) => r.is_match(text).map_err(|e| RegexMatchError {
                pattern: r.as_str().to_string(),
                message: e.to_string(),
            }),
        }
    }

    /// Returns the backend used for this compiled regex.
    pub fn backend(&self) -> RegexBackend {
        match self {
            Self::Fast(_) => RegexBackend::Fast,
            Self::Fancy(_) => RegexBackend::Fancy,
        }
    }

    /// Returns the original pattern string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Fast(r) => r.as_str(),
            Self::Fancy(r) => r.as_str(),
        }
    }

    /// Returns true if this regex was compiled with the fancy backend.
    pub fn is_fancy(&self) -> bool {
        matches!(self, Self::Fancy(_))
    }
}

/// Error during regex compilation.
#[derive(Debug, Clone)]
pub enum RegexCompileError {
    /// Pattern exceeds the maximum allowed length.
    PatternTooLong { len: usize, max: usize },
    /// The pattern could not be compiled by either backend.
    CompileError { pattern: String, message: String },
}

impl fmt::Display for RegexCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PatternTooLong { len, max } => {
                write!(f, "pattern too long: {} bytes (max {})", len, max)
            }
            Self::CompileError { pattern, message } => {
                write!(f, "failed to compile regex '{}': {}", pattern, message)
            }
        }
    }
}

impl std::error::Error for RegexCompileError {}

/// Error during regex matching.
#[derive(Debug, Clone)]
pub struct RegexMatchError {
    pub pattern: String,
    pub message: String,
}

impl fmt::Display for RegexMatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "regex match failed for '{}': {}",
            self.pattern, self.message
        )
    }
}

impl std::error::Error for RegexMatchError {}

/// A diagnostic produced during rulefile loading or regex compilation.
#[derive(Debug, Clone)]
pub struct RuleDiagnostic {
    /// Line number in the rule file (1-indexed).
    pub line_number: Option<usize>,
    /// Severity level.
    pub severity: RuleSeverity,
    /// Human-readable message.
    pub message: String,
}

/// Severity of a rule diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleSeverity {
    /// Informational note (e.g., fancy_regex backend used).
    Info,
    /// Warning about partial compatibility or degraded behavior.
    Warning,
    /// Error that prevented a rule from being loaded.
    Error,
}

impl fmt::Display for RuleSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => f.write_str("info"),
            Self::Warning => f.write_str("warning"),
            Self::Error => f.write_str("error"),
        }
    }
}

/// A single parsed entry from a pproxy rule file.
#[derive(Debug, Clone)]
pub struct PproxyRuleEntry {
    /// Line number in the file (1-indexed).
    pub line_number: usize,
    /// Raw pattern string from the file.
    pub raw: String,
    /// Compiled regex (fast or fancy depending on pattern).
    pub regex: CompatRegex,
    /// Whether this rule uses the fancy_regex backend.
    pub uses_fancy: bool,
}

/// A loaded pproxy rule file with parsed entries and diagnostics.
#[derive(Debug)]
pub struct PproxyRuleFile {
    /// Path to the rule file.
    pub path: PathBuf,
    /// Parsed and compiled rule entries.
    pub entries: Vec<PproxyRuleEntry>,
    /// Diagnostics produced during loading.
    pub diagnostics: Vec<RuleDiagnostic>,
}

impl PproxyRuleFile {
    /// Load and parse a pproxy-style rule file.
    ///
    /// Rule file format:
    /// - Lines starting with `#` are comments (ignored).
    /// - Empty lines are ignored.
    /// - Lines matching `pattern -> reject` or `pattern -> block` are block rules.
    /// - Other `pattern -> action` lines produce a partial-compatibility warning.
    /// - Lines without `->` produce a parse warning.
    pub fn load(path: &Path) -> Result<Self, RegexCompileError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            RegexCompileError::CompileError {
                pattern: String::new(),
                message: format!("failed to read '{}': {}", path.display(), e),
            }
        })?;

        let mut entries = Vec::new();
        let mut diagnostics = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            let line_number = line_num + 1;

            // Skip comments and blank lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if entries.len() >= MAX_RULE_ENTRIES {
                diagnostics.push(RuleDiagnostic {
                    line_number: Some(line_number),
                    severity: RuleSeverity::Error,
                    message: format!(
                        "rule file exceeds maximum of {} entries; remaining lines ignored",
                        MAX_RULE_ENTRIES
                    ),
                });
                break;
            }

            if let Some((pattern, action)) = line.split_once("->") {
                let pattern = pattern.trim().to_string();
                let action = action.trim();

                if action == "reject" || action == "block" {
                    match CompatRegex::compile(&pattern) {
                        Ok(regex) => {
                            let uses_fancy = regex.is_fancy();
                            if uses_fancy {
                                diagnostics.push(RuleDiagnostic {
                                    line_number: Some(line_number),
                                    severity: RuleSeverity::Info,
                                    message: format!(
                                        "pattern '{}' compiled with fancy_regex backend (Python-like features enabled)",
                                        pattern
                                    ),
                                });
                            }
                            entries.push(PproxyRuleEntry {
                                line_number,
                                raw: pattern,
                                regex,
                                uses_fancy,
                            });
                        }
                        Err(e) => {
                            diagnostics.push(RuleDiagnostic {
                                line_number: Some(line_number),
                                severity: RuleSeverity::Error,
                                message: format!(
                                    "line {}: failed to compile regex '{}': {}",
                                    line_number, pattern, e
                                ),
                            });
                        }
                    }
                } else {
                    diagnostics.push(RuleDiagnostic {
                        line_number: Some(line_number),
                        severity: RuleSeverity::Warning,
                        message: format!(
                            "line {}: complex rule '{}' -> '{}' cannot be auto-translated; use eggress TOML [[rules]] with structured matchers",
                            line_number, pattern, action
                        ),
                    });
                }
            } else {
                diagnostics.push(RuleDiagnostic {
                    line_number: Some(line_number),
                    severity: RuleSeverity::Warning,
                    message: format!(
                        "line {}: unrecognized format '{}'; expected 'pattern -> action'",
                        line_number, line
                    ),
                });
            }
        }

        Ok(PproxyRuleFile {
            path: path.to_path_buf(),
            entries,
            diagnostics,
        })
    }

    /// Match a hostname against all rules in the file.
    ///
    /// Returns `true` if any rule matches the hostname (first-match-wins semantics).
    pub fn matches_host(&self, hostname: &str) -> Result<bool, RegexMatchError> {
        for entry in &self.entries {
            if entry.regex.is_match(hostname)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Return only the error-level diagnostics.
    pub fn errors(&self) -> Vec<&RuleDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == RuleSeverity::Error)
            .collect()
    }

    /// Return true if there are any error-level diagnostics.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == RuleSeverity::Error)
    }
}

/// Compile a single block regex pattern (from `-b` flag).
///
/// Validates pattern length and compile-time correctness.
pub fn compile_block_pattern(pattern: &str) -> Result<CompatRegex, RegexCompileError> {
    CompatRegex::compile(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn compile_simple_pattern() {
        let re = CompatRegex::compile(".*\\.example\\.com").unwrap();
        assert!(re.is_match("www.example.com").unwrap());
        assert!(!re.is_match("example.org").unwrap());
        assert_eq!(re.backend(), RegexBackend::Fast);
        assert!(!re.is_fancy());
    }

    #[test]
    fn compile_lookahead_pattern() {
        // Lookahead is not supported by regex crate, should fall back to fancy_regex
        let re = CompatRegex::compile("(?=foo)foo").unwrap();
        assert!(re.is_match("foo").unwrap());
        assert!(!re.is_match("bar").unwrap());
        assert_eq!(re.backend(), RegexBackend::Fancy);
        assert!(re.is_fancy());
    }

    #[test]
    fn compile_lookbehind_pattern() {
        let re = CompatRegex::compile("(?<=foo)bar").unwrap();
        assert!(re.is_match("foobar").unwrap());
        assert!(!re.is_match("bazbar").unwrap());
        assert_eq!(re.backend(), RegexBackend::Fancy);
    }

    #[test]
    fn compile_backreference_pattern() {
        // Backreferences are not supported by regex crate
        let re = CompatRegex::compile(r"(.)\1").unwrap();
        assert!(re.is_match("aa").unwrap());
        assert!(!re.is_match("ab").unwrap());
        assert_eq!(re.backend(), RegexBackend::Fancy);
    }

    #[test]
    fn compile_invalid_pattern() {
        let err = CompatRegex::compile("[invalid").unwrap_err();
        match err {
            RegexCompileError::CompileError { pattern, .. } => {
                assert!(pattern.contains("[invalid"));
            }
            _ => panic!("expected CompileError"),
        }
    }

    #[test]
    fn compile_pattern_too_long() {
        let pattern = "a".repeat(MAX_PATTERN_LEN + 1);
        let err = CompatRegex::compile(&pattern).unwrap_err();
        match err {
            RegexCompileError::PatternTooLong { len, max } => {
                assert_eq!(len, MAX_PATTERN_LEN + 1);
                assert_eq!(max, MAX_PATTERN_LEN);
            }
            _ => panic!("expected PatternTooLong"),
        }
    }

    #[test]
    fn compile_fancy_forces_fancy_backend() {
        // Simple pattern that would normally use fast backend
        let re = CompatRegex::compile_fancy(".*\\.com").unwrap();
        assert_eq!(re.backend(), RegexBackend::Fancy);
        assert!(re.is_fancy());
        assert!(re.is_match("example.com").unwrap());
    }

    #[test]
    fn compile_fancy_invalid_pattern() {
        let err = CompatRegex::compile_fancy("[invalid").unwrap_err();
        match err {
            RegexCompileError::CompileError { .. } => {}
            _ => panic!("expected CompileError"),
        }
    }

    #[test]
    fn rulefile_load_simple() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "# comment line").unwrap();
        writeln!(f).unwrap();
        writeln!(f, ".*\\.example\\.com -> reject").unwrap();
        writeln!(f, "ads\\.com -> block").unwrap();

        let file = PproxyRuleFile::load(f.path()).unwrap();
        assert_eq!(file.entries.len(), 2);
        assert_eq!(file.entries[0].raw, ".*\\.example\\.com");
        assert_eq!(file.entries[1].raw, "ads\\.com");
        assert!(file.errors().is_empty());
    }

    #[test]
    fn rulefile_load_with_lookahead() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "(?=foo)foo -> reject").unwrap();

        let file = PproxyRuleFile::load(f.path()).unwrap();
        assert_eq!(file.entries.len(), 1);
        assert!(file.entries[0].uses_fancy);
        // Should have an info diagnostic about fancy backend
        assert!(file
            .diagnostics
            .iter()
            .any(|d| d.severity == RuleSeverity::Info));
    }

    #[test]
    fn rulefile_load_invalid_regex() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[invalid -> reject").unwrap();

        let file = PproxyRuleFile::load(f.path()).unwrap();
        assert!(file.entries.is_empty());
        assert!(file.has_errors());
    }

    #[test]
    fn rulefile_load_partial_action() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, ".*\\.com -> allow").unwrap();

        let file = PproxyRuleFile::load(f.path()).unwrap();
        assert!(file.entries.is_empty());
        assert!(file
            .diagnostics
            .iter()
            .any(|d| d.severity == RuleSeverity::Warning));
    }

    #[test]
    fn rulefile_load_unrecognized_format() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "just a plain line").unwrap();

        let file = PproxyRuleFile::load(f.path()).unwrap();
        assert!(file.entries.is_empty());
        assert!(file
            .diagnostics
            .iter()
            .any(|d| d.severity == RuleSeverity::Warning));
    }

    #[test]
    fn rulefile_matches_host() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, ".*\\.blocked\\.com -> reject").unwrap();
        writeln!(f, "ads\\..* -> block").unwrap();

        let file = PproxyRuleFile::load(f.path()).unwrap();
        assert!(file.matches_host("www.blocked.com").unwrap());
        assert!(file.matches_host("ads.example.com").unwrap());
        assert!(!file.matches_host("safe.example.com").unwrap());
    }

    #[test]
    fn rulefile_matches_first_wins() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, ".* -> reject").unwrap();
        writeln!(f, "safe\\.com -> block").unwrap();

        let file = PproxyRuleFile::load(f.path()).unwrap();
        // First rule matches everything
        assert!(file.matches_host("safe.com").unwrap());
    }

    #[test]
    fn compile_block_pattern_simple() {
        let re = compile_block_pattern(".*\\.ads\\.com").unwrap();
        assert!(re.is_match("banner.ads.com").unwrap());
        assert!(!re.is_match("clean.com").unwrap());
    }

    #[test]
    fn rulefile_empty_file() {
        let f = NamedTempFile::new().unwrap();
        let file = PproxyRuleFile::load(f.path()).unwrap();
        assert!(file.entries.is_empty());
        assert!(!file.has_errors());
    }

    #[test]
    fn regex_display_debug() {
        let re = CompatRegex::compile("test").unwrap();
        let debug = format!("{:?}", re);
        assert!(debug.contains("CompatRegex::Fast"));
        let display = format!("{}", re.backend());
        assert_eq!(display, "fast");
    }

    #[test]
    fn rule_diagnostic_display() {
        let diag = RuleDiagnostic {
            line_number: Some(5),
            severity: RuleSeverity::Error,
            message: "bad pattern".to_string(),
        };
        assert_eq!(diag.severity.to_string(), "error");
        assert_eq!(diag.line_number, Some(5));
        assert_eq!(diag.message, "bad pattern");
    }

    #[test]
    fn regex_compile_error_display() {
        let err = RegexCompileError::PatternTooLong {
            len: 5000,
            max: 4096,
        };
        let s = err.to_string();
        assert!(s.contains("5000"));
        assert!(s.contains("4096"));

        let err = RegexCompileError::CompileError {
            pattern: "bad".to_string(),
            message: "syntax error".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("bad"));
        assert!(s.contains("syntax error"));
    }
}
