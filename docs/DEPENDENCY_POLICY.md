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

### Rustls + tokio-rustls Configuration

The workspace `Cargo.toml` configures both rustls and tokio-rustls to exclude
native build deps:

```toml
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "logging", "tls12"] }
tokio-rustls = { version = "0.26", default-features = false, features = ["logging", "tls12"] }
```

`default-features = false` on both crates disables the `aws_lc_rs` default
feature. `tokio-rustls` v0.26 includes `aws_lc_rs` in its defaults, which
propagates to `rustls` and pulls in `aws-lc-sys` + `cmake`. Disabling defaults
on `tokio-rustls` is required to maintain a native-dep-free build.

The `ring` feature on `rustls` enables the ring crypto provider. `ring` contains
platform-specific assembly but does not require cmake or external build tools.

### Verification

To verify no native build deps are present in production builds:

```bash
cargo tree -i aws-lc-sys -e normal 2>&1  # should show nothing or error
cargo tree -i cmake -e normal 2>&1        # should show nothing or error
cargo tree -i openssl-sys -e normal 2>&1  # should show nothing or error
```

Note: `aws-lc-sys` may appear in dev-dependencies (via `rcgen` for test certs)
but is not compiled into production builds.

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
