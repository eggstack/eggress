//! Execute-path regression tests (moved verbatim with the split).

use super::hops::{rewrite_request_head, HttpOnlyStream};
use super::*;

fn http_only_target() -> TargetAddr {
    TargetAddr {
        host: TargetHost::Domain("target.example".into()),
        port: 8080,
    }
}

#[test]
fn httponly_rewrite_preserves_header_terminators() {
    let request = b"GET /path HTTP/1.1\r\nHost: example.com\r\nX-Foo: bar\r\n\r\nbody";
    let rewritten = rewrite_request_head(request, &http_only_target());
    assert_eq!(
        std::str::from_utf8(&rewritten).unwrap(),
        "GET http://target.example:8080/path HTTP/1.1\r\n\
         Host: example.com\r\n\
         X-Foo: bar\r\n\
         \r\n\
         body"
    );
}

#[test]
fn httponly_rewrite_preserves_mixed_line_endings() {
    // Request line terminated by bare LF while the head ends with CRLF.
    let request = b"GET /path HTTP/1.1\nHost: example.com\r\nX-Foo: bar\r\n\r\n";
    let rewritten = rewrite_request_head(request, &http_only_target());
    assert_eq!(
        std::str::from_utf8(&rewritten).unwrap(),
        "GET http://target.example:8080/path HTTP/1.1\n\
         Host: example.com\r\n\
         X-Foo: bar\r\n\
         \r\n"
    );
}

#[test]
fn httponly_rewrite_waits_for_complete_head() {
    // No `\r\n\r\n` terminator yet — unchanged (flush retries later).
    let partial = b"GET /path HTTP/1.1\r\nHost: example.com\r\n".as_slice();
    assert_eq!(rewrite_request_head(partial, &http_only_target()), partial);
}

#[test]
fn httponly_rewrite_leaves_absolute_form_and_incomplete_heads_alone() {
    // Absolute-form request line is not origin-form; unchanged.
    let absolute = b"GET http://example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n".as_slice();
    assert_eq!(
        rewrite_request_head(absolute, &http_only_target()),
        absolute
    );
    // No complete head yet; unchanged (flush will retry later).
    let partial = b"GET /path HTTP/1.1\r\nHost: example.com\r\n".as_slice();
    assert_eq!(rewrite_request_head(partial, &http_only_target()), partial);
    // Empty input stays empty.
    assert!(rewrite_request_head(b"", &http_only_target()).is_empty());
}

#[tokio::test]
async fn httponly_stream_rewrites_once_and_drains_on_shutdown() {
    // A tiny duplex buffer forces the flush loop across multiple polls;
    // the remaining bytes still contain `\r\n\r\n`, which a second
    // rewrite pass would mangle ("X-Foo: /bar baz" looks like an
    // origin-form request line to the rewriter).
    let (mut peer, inner) = tokio::io::duplex(16);
    let mut stream = HttpOnlyStream {
        inner: Box::new(inner),
        target: http_only_target(),
        pending: Vec::new(),
        rewritten: false,
    };
    let request =
        b"GET /path HTTP/1.1\r\nHost: example.com\r\nX-Foo: /bar baz\r\n\r\ntail".as_slice();
    let expected =
        b"GET http://target.example:8080/path HTTP/1.1\r\nHost: example.com\r\nX-Foo: /bar baz\r\n\r\ntail";
    let reader = tokio::spawn(async move {
        let mut received = Vec::new();
        peer.read_to_end(&mut received).await.unwrap();
        received
    });
    use tokio::io::AsyncWriteExt;
    stream.write_all(request).await.unwrap();
    stream.shutdown().await.unwrap();
    let received = reader.await.unwrap();
    assert_eq!(received, expected);
}
