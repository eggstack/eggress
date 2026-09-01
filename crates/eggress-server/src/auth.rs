use base64::Engine;
use zeroize::Zeroizing;

pub(crate) fn parse_basic_auth(value: &str) -> Option<(String, Zeroizing<String>)> {
    let value = value.trim();
    if !value.starts_with("Basic ") {
        return None;
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value[6..].trim())
        .ok()?;
    let decoded = Zeroizing::new(String::from_utf8(decoded).ok()?);
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), Zeroizing::new(password.to_string())))
}

#[cfg(test)]
mod tests {
    use super::parse_basic_auth;

    #[test]
    fn rejects_invalid_padding() {
        assert!(parse_basic_auth("Basic dXNlcjpwYXNz=").is_none());
    }
}
