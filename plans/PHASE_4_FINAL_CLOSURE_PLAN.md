# Phase 4 Final Closure Plan: UDP Upstream Relay Polish

## Purpose

Phase 4 now has real one-hop SOCKS5 UDP upstream relay support. The current implementation includes capability classification, a SOCKS5 upstream UDP ASSOCIATE client, upstream flow handling in the UDP relay loop, synthetic upstream tests, upstream metrics, and completion docs.

This final closure plan addresses the remaining correctness and documentation gaps before Phase 4 is considered cleanly closed.

The work is intentionally narrow. Do not redesign UDP relay, add multi-hop UDP, or add new UDP-capable protocols. Focus on tightening the existing one-hop SOCKS5 upstream path.

---

# Current residual gaps

1. Upstream SOCKS5 timeout only wraps TCP connect; method negotiation, username/password auth, and UDP ASSOCIATE can still stall indefinitely.
2. SOCKS5 username/password lengths are cast to `u8` without validation.
3. SOCKS5 domain address length is cast to `u8` without validation.
4. Upstream UDP ASSOCIATE reply parser supports IPv4 and IPv6 only; domain ATYP is rejected.
5. Upstream response handling discards the decoded upstream response target and sends the flow target back to the client.
6. Upstream UDP metrics are aggregate counters, while docs imply bounded upstream/group labels.
7. Visible runtime tests prove direct UDP through `ServiceSupervisor`, but full configured runtime path for Eggress -> SOCKS5 upstream -> target needs explicit coverage.
8. Historical plan files were removed. Decide whether this is intentional archival policy or restore/archive them.
9. Completion docs should be updated after these fixes so they do not overstate unsupported behavior.

---

# Non-goals

Do not implement:

- multi-hop UDP proxy chains;
- UDP through HTTP, SOCKS4, Shadowsocks, Trojan, SSH, QUIC, MASQUE, or CONNECT-UDP;
- transparent UDP proxying;
- SOCKS5 UDP fragmentation/reassembly;
- multicast or broadcast forwarding;
- persistent UDP state;
- native TLS/OpenSSL;
- unsafe Rust.

---

# Workstream 1: Wrap the complete upstream SOCKS5 handshake in timeout

## Problem

`open_socks5_udp_upstream()` currently applies `connect_timeout` only to `TcpStream::connect`. After TCP connection succeeds, the upstream can stall during method selection, username/password auth, or UDP ASSOCIATE reply parsing.

## Required behavior

The configured `connect_timeout` must bound the full upstream route-open operation:

- DNS resolution;
- TCP connect;
- SOCKS5 method negotiation;
- username/password auth if present;
- UDP ASSOCIATE request write;
- UDP ASSOCIATE reply read and validation;
- UDP socket bind if this is considered part of upstream open.

## Implementation sketch

Refactor:

```rust
pub async fn open_socks5_udp_upstream(
    config: Socks5UdpUpstreamConfig,
    target_hint: Option<SocksAddr>,
) -> Result<Socks5UdpUpstreamAssociation, UdpUpstreamError> {
    tokio::time::timeout(config.connect_timeout, async {
        open_socks5_udp_upstream_inner(config, target_hint).await
    })
    .await
    .map_err(|_| UdpUpstreamError::Timeout)?
}

async fn open_socks5_udp_upstream_inner(
    config: Socks5UdpUpstreamConfig,
    target_hint: Option<SocksAddr>,
) -> Result<Socks5UdpUpstreamAssociation, UdpUpstreamError> {
    // existing connect + handshake + auth + associate + UDP bind logic
}
```

Avoid double-wrapping TCP connect with another timeout inside the inner function. If retaining inner per-operation timeout, make sure it does not exceed the outer deadline and does not mask the reason unpredictably.

## Required tests

Add synthetic upstream modes:

- stall before method response;
- stall before auth response;
- stall before UDP ASSOCIATE reply.

Tests:

```rust
#[tokio::test]
async fn upstream_method_negotiation_timeout_is_bounded() { ... }

#[tokio::test]
async fn upstream_auth_timeout_is_bounded() { ... }

#[tokio::test]
async fn upstream_associate_timeout_is_bounded() { ... }
```

Use a very short timeout such as 50ms and assert `UdpUpstreamError::Timeout`.

## Acceptance criteria

- no upstream TCP peer can hold a UDP upstream open attempt indefinitely after accepting TCP.

---

# Workstream 2: Validate SOCKS5 field lengths before encoding

## Problem

SOCKS5 username/password auth encodes lengths as one byte. Current code uses `username.len() as u8` and `password.len() as u8`, which truncates values above 255 bytes.

SOCKS5 domain addresses also encode domain length as one byte. Current code uses `domain.len() as u8` without checking.

## Required error variants

Add stable errors:

```rust
pub enum UdpUpstreamError {
    // existing
    CredentialTooLong,
    DomainTooLong,
}
```

Reason labels:

```rust
CredentialTooLong => "credential_too_long"
DomainTooLong => "domain_too_long"
```

## Helper functions

```rust
fn checked_u8_len(value: &str, field: &'static str) -> Result<u8, UdpUpstreamError> {
    if value.len() > u8::MAX as usize {
        match field {
            "credential" => Err(UdpUpstreamError::CredentialTooLong),
            "domain" => Err(UdpUpstreamError::DomainTooLong),
            _ => Err(UdpUpstreamError::MalformedSocksReply),
        }
    } else {
        Ok(value.len() as u8)
    }
}
```

Apply in:

- `socks5_auth()` for username/password;
- `encode_socks_addr()` for domain targets;
- any public test helper that encodes auth or UDP ASSOCIATE request.

## Required tests

- username length 256 returns `CredentialTooLong`;
- password length 256 returns `CredentialTooLong`;
- domain length 256 returns `DomainTooLong`;
- max length 255 succeeds;
- error label mapping is stable.

## Acceptance criteria

- no SOCKS5 length field is produced by lossy truncation.

---

# Workstream 3: Support or explicitly document domain ATYP in upstream UDP ASSOCIATE replies

## Problem

The upstream UDP ASSOCIATE reply parser accepts IPv4 and IPv6 relay addresses only. Domain ATYP is currently treated as invalid.

The Phase 4 plan expected domain replies to be supported if an upstream returns them. Supporting domain replies is preferable.

## Preferred implementation

Extend `socks5_udp_associate()` reply parser:

```rust
ATYP_DOMAIN => {
    let len = stream.read_u8().await.map_err(UdpUpstreamError::Io)? as usize;
    if len == 0 {
        return Err(UdpUpstreamError::UdpRelayAddressInvalid);
    }
    let mut domain = vec![0u8; len];
    stream.read_exact(&mut domain).await.map_err(UdpUpstreamError::Io)?;
    let port = stream.read_u16().await.map_err(UdpUpstreamError::Io)?;
    let domain = String::from_utf8(domain).map_err(|_| UdpUpstreamError::UdpRelayAddressInvalid)?;
    resolve_domain_relay(&domain, port).await?
}
```

Add helper:

```rust
async fn resolve_domain_relay(domain: &str, port: u16) -> Result<SocketAddr, UdpUpstreamError> {
    tokio::net::lookup_host((domain, port))
        .await
        .map_err(UdpUpstreamError::Io)?
        .next()
        .ok_or(UdpUpstreamError::UdpRelayAddressInvalid)
}
```

If using `lookup_host((domain, port))` is awkward, use `format!("{domain}:{port}")` but be careful with IPv6-like domain strings.

## Acceptable fallback

If domain reply support is deferred, explicitly document:

```text
SOCKS5 upstream UDP ASSOCIATE replies must return IPv4 or IPv6 relay addresses. Domain relay replies are rejected.
```

and update completion docs accordingly.

Preferred: implement support.

## Required tests

Synthetic upstream modes:

- reply with domain relay address `localhost` and valid port;
- reply with zero-length domain;
- reply with unresolvable domain.

Tests:

- domain relay reply resolves and echo path works;
- zero-length domain returns `UdpRelayAddressInvalid`;
- unresolvable domain returns `UdpRelayAddressInvalid` or `Io`, documented consistently.

## Acceptance criteria

- implementation and docs agree on domain ATYP behavior.

---

# Workstream 4: Preserve upstream response target correctly

## Problem

The upstream response receiver decodes SOCKS5 UDP datagrams from the upstream, but currently sends `flow_target.clone()` back to the client rather than the decoded response target.

Current behavior is acceptable only if per-target upstream flows always intentionally normalize replies to the original target. That should either be documented or changed.

## Preferred behavior

Use the decoded upstream response target when forwarding to the client:

```rust
if let Ok(upstream_resp) = decode_socks5_udp_datagram(&recv_buf[..n]) {
    let _ = flow_response_tx.send(ResponseMsg {
        target: upstream_resp.target.clone(),
        payload: upstream_resp.payload.to_vec(),
    });
}
```

If `upstream_resp.target` borrows from the buffer, clone/own it before sending through the channel.

## Security consideration

Because this flow is per target, validate that the upstream response target is compatible with the flow target before using it. Recommended:

```rust
if !socks_addr_equivalent(&upstream_resp.target, &flow_target) {
    // either drop or use flow_target, but record a metric/log
}
```

Simple initial rule:

- If decoded target equals flow target, use decoded target.
- If target differs, drop and record upstream failure/drop metric.

Avoid forwarding upstream responses that claim unrelated targets.

## Required tests

- upstream response with matching target is forwarded;
- upstream response with mismatched target is dropped;
- metric increments for mismatched upstream response;
- direct UDP response behavior unchanged.

## Acceptance criteria

- upstream response target handling is explicit and safe.

---

# Workstream 5: Add full ServiceSupervisor runtime test for configured SOCKS5 UDP upstream

## Problem

The crate-level relay tests prove the upstream path, but Phase 4 should also prove the full runtime path:

```text
TOML config -> ServiceSupervisor -> listener -> route rule -> upstream group -> synthetic SOCKS5 UDP upstream -> response back to client
```

## Required test

Add in `crates/eggress-runtime/tests/udp_upstream.rs`:

```rust
#[tokio::test]
async fn runtime_udp_via_configured_socks5_upstream_echoes() { ... }
```

Test setup:

1. Start `Socks5UdpTestServer` in echo mode.
2. Start `ServiceSupervisor` with TOML:

```toml
version = 1

[[listeners]]
name = "socks-in"
bind = "127.0.0.1:0"
protocols = ["socks5"]

[listeners.udp]
enabled = true
bind = "127.0.0.1:0"
advertise = "127.0.0.1"
idle_timeout = "5s"
target_idle_timeout = "1s"
max_associations = 16
max_targets_per_association = 8
max_datagram_size = 65535
client_pin = true

[[upstreams]]
id = "socks-up"
uri = "socks5://127.0.0.1:<UPSTREAM_TCP_PORT>"

[[upstream_groups]]
id = "udp-upstream"
scheduler = "first-available"
members = ["socks-up"]
fallback = "reject"

[[rules]]
id = "udp-via-socks"
upstream_group = "udp-upstream"

[rules.match]
all = [
  { transport = "udp" }
]
```

3. Perform client SOCKS5 UDP ASSOCIATE against Eggress.
4. Send UDP datagram to Eggress relay.
5. Assert response payload matches.
6. Query `/metrics` if admin enabled and assert upstream metrics names are present.
7. Shutdown and assert runtime exits.

## Additional runtime tests

- authenticated upstream URI works in full runtime;
- HTTP upstream selected for UDP drops and does not echo;
- multi-hop selected for UDP drops and does not echo;
- target-flow idle timeout releases upstream active gauge.

## Acceptance criteria

- Phase 4 is proven through actual TOML-driven runtime, not only crate-level relay tests.

---

# Workstream 6: Align upstream UDP metrics implementation and docs

## Problem

The implementation exposes aggregate upstream UDP counters/gauges. The plan requested bounded labels by upstream/group/outcome. The completion docs say “bounded labels,” which may overstate current implementation.

Choose one path.

## Option A: Add labels now

Introduce label families in `MetricsRegistry`:

```rust
#[derive(EncodeLabelSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct UdpUpstreamLabels {
    pub upstream_id: String,
    pub group_id: String,
    pub outcome: String,
}
```

Counters:

```text
egress_udp_upstream_associations_total{upstream_id,group_id,outcome}
egress_udp_upstream_failures_total{upstream_id,group_id,reason}
```

This requires `UdpMetrics` to either store labeled data or for relay code to record directly into `MetricsRegistry`. Since current metrics bridge is aggregate-only, adding labels may be invasive.

## Option B: Keep aggregate metrics and fix docs

If adding labels is too large for closure, update docs to say:

```text
Phase 4 exposes aggregate upstream UDP metrics. Per-upstream/group labels are deferred to a later observability pass to avoid increasing the bridge complexity.
```

Recommended for this closure: Option B unless there is already a clean way to label through `UdpMetrics`.

## Required tests

For Option B:

- `/metrics` contains upstream UDP aggregate metric names;
- docs do not claim upstream/group labels;
- metrics output does not contain target/client/user labels.

For Option A:

- metrics contain bounded upstream/group labels;
- label values are sanitized and bounded;
- no high-cardinality labels.

## Acceptance criteria

- metrics behavior and docs agree.

---

# Workstream 7: Decide plan-file archival policy

## Problem

The compare from the Phase 4 handoff plan shows historical plan files removed, including many previous phase plans and the Phase 4 handoff plan itself.

This may be intentional cleanup, but this project has used plan files as handoff/audit artifacts. Deleting all of them makes it harder to trace why changes were made.

## Required decision

Choose one policy.

### Policy A: Keep active and historical plans

- Restore deleted plans from Git history.
- Move completed plans to `plans/archive/`.
- Keep active/current plans under `plans/`.
- Add `plans/README.md` explaining active vs archived plans.

### Policy B: Keep only current plans and completion docs

- Do not restore old plans.
- Add `plans/README.md` or `docs/ROADMAP.md` note:

```text
Completed implementation plans are retired after their corresponding completion record is added under docs/. The plans directory contains only active handoff plans.
```

- Ensure completion docs contain enough detail to replace historical plans.

Recommended: Policy A for this repo, because the user repeatedly asks for handoff plans and post-plan audits.

## Required tests/checks

No code tests needed.

## Acceptance criteria

- plan-file lifecycle is explicit and no longer accidental.

---

# Workstream 8: Completion doc correction

## Required updates

Update `docs/PHASE_4_UDP_UPSTREAM_RELAY_COMPLETION.md` after the closure fixes.

If all preferred fixes land, completion doc may retain:

- one-hop SOCKS5 UDP upstream relay supported;
- auth supported;
- upstream domain relay replies supported;
- full handshake timeout enforced;
- runtime-level TOML test exists;
- aggregate or labeled metrics accurately described.

If some items are deferred, add explicit limitations.

## Required checklist changes

Ensure checklist does not claim:

- domain relay replies if not implemented;
- bounded upstream/group labels if aggregate metrics remain;
- runtime-level config coverage if only crate-level tests exist;
- full handshake timeout if only TCP connect is bounded.

## Acceptance criteria

- docs accurately reflect executable behavior.

---

# Recommended commit sequence

## Commit 1: Full upstream handshake timeout and SOCKS length validation

- Wrap entire upstream SOCKS5 open path in timeout.
- Validate username/password/domain lengths.
- Add unit tests for timeout and length errors.

## Commit 2: Domain relay replies and response target validation

- Add domain ATYP parsing/resolution for upstream UDP ASSOCIATE replies or document unsupported.
- Use decoded upstream response target when safe.
- Drop mismatched upstream response targets.
- Add tests.

## Commit 3: Full runtime UDP upstream test

- Add TOML-driven `ServiceSupervisor` test using synthetic SOCKS5 UDP upstream.
- Add authenticated runtime test if practical.
- Add unsupported upstream runtime test if missing.

## Commit 4: Metrics/doc alignment

- Either add labeled upstream metrics or revise docs to aggregate metrics.
- Add `/metrics` assertion for upstream aggregate/labeled counters.

## Commit 5: Plan archive policy and completion doc update

- Restore/archive plans or document retirement policy.
- Update completion doc and roadmap.
- Run final checks.

---

# Required tests

## Unit tests

- method negotiation stall returns `Timeout`;
- auth stall returns `Timeout`;
- associate stall returns `Timeout`;
- username length 256 rejected;
- password length 256 rejected;
- domain length 256 rejected;
- domain ATYP relay reply resolves or is explicitly rejected;
- upstream response target mismatch drops.

## Integration tests

- full runtime TOML-configured SOCKS5 UDP upstream echo;
- full runtime authenticated SOCKS5 UDP upstream echo if practical;
- full runtime HTTP upstream unsupported for UDP;
- upstream metrics visible in `/metrics`;
- target/client addresses absent from metrics/admin;
- target-flow idle cleanup releases upstream lease/gauge.

---

# Verification commands

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo audit
```

Focused checks:

```bash
cargo test -p eggress-udp socks5_upstream
cargo test -p eggress-runtime udp_upstream
cargo test -p eggress-runtime udp
```

If external pproxy UDP upstream interop is added:

```bash
EGRESS_REQUIRE_EXTERNAL_INTEROP=1 cargo test --test interoperability_pproxy_udp_upstream
```

---

# Definition of done

Phase 4 is cleanly closed only when:

1. The full upstream SOCKS5 UDP open path is bounded by timeout.
2. Username, password, and domain SOCKS5 length fields are validated before encoding.
3. Domain ATYP in upstream UDP ASSOCIATE replies is supported or explicitly documented as unsupported.
4. Upstream response target handling is explicit and safe.
5. A full `ServiceSupervisor` runtime test proves TOML-configured SOCKS5 UDP upstream relay.
6. Authenticated upstream behavior is tested at crate or runtime level.
7. Unsupported HTTP/SOCKS4/multi-hop UDP upstream behavior is tested and metriced.
8. Upstream UDP metrics behavior matches docs.
9. Completion docs do not overstate labels, domain support, timeout coverage, or runtime coverage.
10. Plan archival/retirement policy is explicit.
11. All workspace tests, lint, audit, and applicable interop checks pass.
12. No unsafe Rust, OpenSSL dependency, or native dependency is introduced.

## Completion record

When complete, append:

```markdown
## Final closure record

Implemented by commits:

- `<sha>` — upstream handshake timeout and SOCKS field validation
- `<sha>` — relay reply target/domain handling
- `<sha>` — runtime-level upstream relay tests
- `<sha>` — metrics/docs alignment and plan archival policy

All required checks passed on `<date>`.
```
