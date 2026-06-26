# Dependency Policy

## Crypto/TLS Policy

eggress uses `rustls` for TLS with the `ring` crypto provider. The project
enforces a pure-Rust dependency policy with the following constraints:

### Allowed

- Pure Rust implementations (Tokio, rustls, RustCrypto primitives)
- `ring` as the TLS crypto provider (ships prebuilt assembly, no cmake required)
- Platform FFI only for OS-level features (transparent proxying, system-proxy config)

### Prohibited

- `aws-lc-rs` / `aws-lc-sys` — requires cmake + cc native build tools
- `openssl-sys` / `native-tls` — C library dependencies
- Any dependency requiring cmake, autoconf, or external C compilers

### Rustls Configuration

The workspace `Cargo.toml` configures rustls to exclude native build deps:

```toml
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "logging", "tls12"] }
```

`default-features = false` disables the `aws_lc_rs` default feature. The `ring`
feature enables the ring crypto provider. `ring` contains platform-specific
assembly but does not require cmake or external build tools.

### Verification

To verify no native deps are present:

```bash
cargo tree -i aws-lc-sys 2>&1  # should show nothing or error
cargo tree -i cmake 2>&1        # should show nothing or error
cargo tree -i openssl-sys 2>&1  # should show nothing or error
```

### Rationale

The project targets Linux, macOS, and Windows with minimal build prerequisites.
Native C build toolchains (cmake, cc) increase build complexity, cross-compilation
difficulty, and CI requirements. Pure-Rust crypto via `ring` provides equivalent
security with simpler builds.

Note: `ring` does contain platform-specific assembly code for performance-critical
crypto operations. This is acceptable because:
1. It ships prebuilt objects for common targets (no build-time compilation)
2. It has no external build dependencies
3. It is widely deployed and audited
