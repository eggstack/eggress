use crate::PacConfig;

fn js_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result
}

pub fn generate_pac(config: &PacConfig) -> String {
    let mut body = String::new();

    body.push_str("function FindProxyForURL(url, host) {\n");
    body.push_str("  if (isPlainHostName(host)) return \"DIRECT\";\n");

    let mut direct_hosts = config.direct_hosts.clone();
    direct_hosts.sort();

    if !direct_hosts.is_empty() {
        body.push_str("  var directHosts = [");
        for (i, h) in direct_hosts.iter().enumerate() {
            if i > 0 {
                body.push_str(", ");
            }
            body.push('"');
            body.push_str(&js_escape(h));
            body.push('"');
        }
        body.push_str("];\n");
        body.push_str("  for (var i = 0; i < directHosts.length; i++) {\n");
        body.push_str(
            "    if (host == directHosts[i] || shExpMatch(host, \"*.\" + directHosts[i]))\n",
        );
        body.push_str("      return \"DIRECT\";\n");
        body.push_str("  }\n");
    }

    let mut direct_suffixes = config.direct_suffixes.clone();
    direct_suffixes.sort();

    if !direct_suffixes.is_empty() {
        body.push_str("  var directSuffixes = [");
        for (i, s) in direct_suffixes.iter().enumerate() {
            if i > 0 {
                body.push_str(", ");
            }
            body.push('"');
            body.push_str(&js_escape(s));
            body.push('"');
        }
        body.push_str("];\n");
        body.push_str("  for (var i = 0; i < directSuffixes.length; i++) {\n");
        body.push_str("    if (shExpMatch(host, \"*.\" + directSuffixes[i]))\n");
        body.push_str("      return \"DIRECT\";\n");
        body.push_str("  }\n");
    }

    let proxy = js_escape(&config.proxy_directive);
    body.push_str(&format!(
        "  return \"PROXY {}{}",
        proxy,
        if config.direct_fallback {
            "; DIRECT\";\n"
        } else {
            "\";\n"
        }
    ));

    body.push_str("}\n");
    body
}
