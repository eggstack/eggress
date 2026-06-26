# Phase 5 Final Follow-up Handoff Plan

## Purpose

Phase 5 corrective closure mostly succeeded: Shadowsocks support claims were downgraded, Trojan credential/server-name handling was cleaned up, runtime tests were added for HTTP/SOCKS4/SOCKS5 upstreams, TLS dependency policy was documented, and phase numbering was corrected.

This final follow-up plan is intentionally narrow. It closes the remaining test/documentation gaps before Phase 5 is treated as fully closed and Phase 6 hardening begins.

Do not add new protocol support in this pass. Do not re-open Shadowsocks implementation work. Do not modify routing architecture unless required to fix these exact issues.

---

# Remaining issues

1. `test_domain_length_256_rejected` and `test_empty_domain_rejected` in `eggress-protocol-trojan` do not call `trojan_connect()`; they only inspect the constructed `TargetAddr`.
2. The main Trojan happy-path synthetic TLS test still manually performs TLS and manually writes the Trojan request instead of exercising `trojan_connect()`.
3. Completion docs claim CI/status checks are visible, but the GitHub combined-status endpoint has not exposed status contexts for current `main`.
4. Completion docs should distinguish local verification from hosted CI visibility.
5. Phase 5 should not be marked perfectly closed until the exported Trojan connect path is directly covered for success and domain-length rejection.

---

# Non-goals

Do not implement:

- Shadowsocks stream encryption;
- Shadowsocks UDP interoperability;
- additional Trojan features;
- multi-hop UDP;
- QUIC/MASQUE/CONNECT-UDP;
- fuzzing/benchmarks/security review from Phase 6;
- new native dependencies;
- unsafe Rust.

---

# Workstream 1: Make Trojan happy-path test call `trojan_connect()` directly

## Problem

The current visible `test_trojan_connect_through_synthetic_tls_server` builds its own TLS connection and manually writes the Trojan request. That validates the expected wire shape, but it does not prove the exported `trojan_connect()` function performs the TLS handshake and writes the request correctly.

## Required change

Rewrite or add a new test:

```rust
#[tokio::test]
async fn trojan_connect_through_synthetic_tls_server_uses_exported_function() { ... }
```

The test should:

1. install the rustls provider;
2. generate a local self-signed certificate for `localhost`;
3. start a local TLS server using that cert;
4. have the server read the Trojan request after TLS handshake;
5. assert the password hash, CRLF, command, address type, target address, target port, and trailing CRLF;
6. write an echo payload back over the TLS stream;
7. call `trojan_connect()` from the client side with a custom `rustls::ClientConfig` trusting the generated cert;
8. read the echo payload from the returned `BoxStream`;
9. assert the payload matches.

Client-side sketch:

```rust
let tcp_stream = tokio::net::TcpStream::connect(addr).await.unwrap();
let boxed: BoxStream = Box::new(tcp_stream);

let mut root_store = rustls::RootCertStore::empty();
root_store.add(cert_der).unwrap();
let tls_config = Arc::new(
    rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth(),
);

let target = TargetAddr {
    host: TargetHost::Ip("127.0.0.1".parse().unwrap()),
    port: 8080,
};

let mut stream = trojan_connect(
    boxed,
    &target,
    expected_password,
    "localhost",
    Some(tls_config),
)
.await
.unwrap();

let mut response = vec![0u8; 256];
let n = tokio::time::timeout(Duration::from_secs(2), async {
    stream.read(&mut response).await
})
.await
.unwrap()
.unwrap();
response.truncate(n);
assert_eq!(&response, b"hello from trojan server");
```

## Preserve manual wire-format tests

It is acceptable to keep the manual wire-format unit tests, but the exported function must be tested directly. If test runtime becomes too long, delete the manual happy-path integration test and replace it with the exported-function version.

## Acceptance criteria

- At least one synthetic TLS happy-path test calls `trojan_connect()` directly and asserts server-observed request bytes.

---

# Workstream 2: Make Trojan domain-length rejection tests call `trojan_connect()`

## Problem

The current long-domain and empty-domain tests do not prove `trojan_connect()` rejects invalid domain lengths. They only assert that a constructed `TargetAddr` contains a 256-byte or empty domain.

## Required tests

Add tests that invoke `trojan_connect()` and assert `Err(TrojanError::Protocol(_))`.

The implementation performs domain validation after TLS handshake. To keep the test deterministic, use a local synthetic TLS server as in the happy-path test, or refactor address encoding into a pure helper and test that helper directly.

Preferred small refactor:

```rust
pub(crate) fn encode_trojan_request(
    target: &TargetAddr,
    password: &str,
) -> Result<Vec<u8>, TrojanError> {
    // hash + CRLF + command + addr + port + CRLF
}
```

Then `trojan_connect()` calls:

```rust
let request = encode_trojan_request(target, password)?;
boxed.write_all(&request).await?;
```

Tests can then assert:

```rust
#[test]
fn encode_trojan_request_rejects_domain_length_256() {
    let target = TargetAddr {
        host: TargetHost::Domain("a".repeat(256)),
        port: 443,
    };
    let err = encode_trojan_request(&target, "pass").unwrap_err();
    assert!(matches!(err, TrojanError::Protocol(_)));
}

#[test]
fn encode_trojan_request_rejects_empty_domain() {
    let target = TargetAddr {
        host: TargetHost::Domain(String::new()),
        port: 443,
    };
    let err = encode_trojan_request(&target, "pass").unwrap_err();
    assert!(matches!(err, TrojanError::Protocol(_)));
}
```

This is preferred over forcing an entire TLS handshake just to reach deterministic pre-write request encoding validation.

## Additional required tests

- domain length 255 is accepted by `encode_trojan_request()`;
- IPv4 request encoding unchanged;
- IPv6 request encoding unchanged;
- `trojan_connect()` uses `encode_trojan_request()` in the happy-path test.

## Acceptance criteria

- invalid Trojan domain lengths are tested through the actual exported/request-building path, not by merely inspecting test data.

---

# Workstream 3: Correct Phase 5 completion doc CI/status wording

## Problem

`docs/PHASE_5_CORRECTIVE_CLOSURE_COMPLETION.md` claims CI/status checks are visible. The GitHub combined-status endpoint has repeatedly returned no status contexts for current `main`.

## Required wording change

Replace any claim like:

```text
CI/status checks visible: PASS
```

with:

```text
Hosted CI/status visibility: NOT VERIFIED via connector
Local verification: PASS per recorded command output/commit note
```

or, if a workflow run is now available, cite the workflow run ID and status.

## Required doc distinction

The completion doc must distinguish:

- local verification commands claimed by the implementer;
- GitHub-hosted CI status checks visible on the commit;
- unverified status contexts if the connector cannot see them.

Suggested table row:

```markdown
| 10 | CI/status checks visible | PARTIAL — local verification recorded; hosted status contexts not visible via connector |
```

If actual GitHub Actions workflow runs exist, update this row to:

```markdown
| 10 | CI/status checks visible | PASS — workflow `<run_id>` succeeded on `<sha>` |
```

## Acceptance criteria

- completion docs no longer overclaim hosted CI visibility.

---

# Workstream 4: Optional CI visibility check

## Optional action

If there is a GitHub Actions workflow, verify current head with:

```text
fetch_commit_workflow_runs(repo, sha)
get_commit_combined_status(repo, sha)
```

If results are empty, do not claim hosted CI. If results show success, update the completion doc with the run ID.

## Acceptance criteria

- docs reflect observed status, not assumptions.

---

# Recommended commit sequence

## Commit 1: Trojan exported-path test cleanup

- Add `encode_trojan_request()` helper if needed.
- Make happy-path synthetic TLS test call `trojan_connect()` directly.
- Convert domain length tests to call the helper or exported request-building path.
- Keep existing wire-format assertions where useful.

## Commit 2: Phase 5 completion wording cleanup

- Update `docs/PHASE_5_CORRECTIVE_CLOSURE_COMPLETION.md`.
- If current CI status is visible, include run ID.
- If not visible, say local verification only.

---

# Required verification

Run:

```bash
cargo fmt --all -- --check
cargo test -p eggress-protocol-trojan
cargo test -p eggress-runtime upstream_protocols
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace
cargo deny check
```

Optional dependency confirmation:

```bash
cargo tree -i aws-lc-sys -e normal || true
cargo tree -i cmake -e normal || true
cargo tree -i openssl-sys -e normal || true
cargo tree -i native-tls -e normal || true
```

---

# Definition of done

This follow-up is complete only when:

1. A Trojan happy-path synthetic TLS test calls `trojan_connect()` directly.
2. The test asserts the server-observed Trojan request bytes produced by `trojan_connect()`.
3. Invalid 256-byte domain and empty-domain cases are tested through `encode_trojan_request()` or an equivalent actual request-building path.
4. Domain length 255 remains tested as accepted.
5. IPv4 and IPv6 request encoding tests still pass.
6. Phase 5 completion docs no longer claim hosted CI visibility unless a real workflow/status is visible.
7. All focused Trojan and runtime upstream tests pass.
8. No new protocol behavior, native dependency, or unsafe Rust is introduced.

## Completion record

When complete, update:

```text
docs/PHASE_5_CORRECTIVE_CLOSURE_COMPLETION.md
```

Add a short final-follow-up section with:

- commit SHA(s);
- tests changed;
- CI/local-verification status;
- final statement that Phase 5 is ready for Phase 6 hardening.
