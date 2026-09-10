//! Compatibility regex rules for pproxy-style `host:port` matching.
//!
//! Kept separate from [`crate::matcher`] so native matching stays free of
//! compatibility formatting concerns.

#[derive(Debug, thiserror::Error)]
pub enum RegexError {
    #[error("invalid regex at line {line}: {source}")]
    InvalidRegex { line: usize, source: regex::Error },
}

#[derive(Debug)]
pub struct CompatRegexRule {
    pub pattern: regex::Regex,
}

impl CompatRegexRule {
    pub fn parse_line(line: &str) -> Result<Option<Self>, RegexError> {
        Self::parse_line_at(line, 0)
    }

    pub fn parse_line_at(line: &str, line_num: usize) -> Result<Option<Self>, RegexError> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(None);
        }
        let re = regex::Regex::new(trimmed).map_err(|e| RegexError::InvalidRegex {
            line: line_num,
            source: e,
        })?;
        Ok(Some(Self { pattern: re }))
    }

    pub fn parse_file(content: &str) -> Result<Vec<Self>, RegexError> {
        let mut rules = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let re = regex::Regex::new(trimmed).map_err(|e| RegexError::InvalidRegex {
                line: idx + 1,
                source: e,
            })?;
            rules.push(Self { pattern: re });
        }
        Ok(rules)
    }

    pub fn matches(&self, hostname: &str, port: u16) -> bool {
        // Format into a stack buffer for typical hostnames; fall back to the
        // heap only when the target does not fit.
        let mut buf = [0u8; 320];
        match fmt_target(hostname, port, &mut buf) {
            Some(target) => self.pattern.is_match(target),
            None => self.pattern.is_match(&format!("{}:{}", hostname, port)),
        }
    }
}

/// Writes `hostname:port` into `buf`, returning the filled slice or `None`
/// when the buffer is too small.
fn fmt_target<'a>(hostname: &str, port: u16, buf: &'a mut [u8]) -> Option<&'a str> {
    let bytes = hostname.as_bytes();
    if bytes.len() + 5 > buf.len() {
        return None;
    }
    buf[..bytes.len()].copy_from_slice(bytes);
    let mut pos = bytes.len();
    buf[pos] = b':';
    pos += 1;
    if port == 0 {
        buf[pos] = b'0';
        pos += 1;
    } else {
        let mut digits = [0u8; 5];
        let mut count = 0;
        let mut remaining = port;
        while remaining > 0 {
            digits[count] = b'0' + (remaining % 10) as u8;
            remaining /= 10;
            count += 1;
        }
        for digit in digits[..count].iter().rev() {
            buf[pos] = *digit;
            pos += 1;
        }
    }
    std::str::from_utf8(&buf[..pos]).ok()
}
