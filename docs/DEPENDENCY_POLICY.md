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

### Build-time-only dependencies

These dependencies live in the workspace graph but **never enter production
binary artifacts**. They are explicitly tolerated because they only affect
benchmark or fuzz compilation.

- `criterion` (HTML benchmark reports): workspace dep of the root
  `eggress-bench` package. That package declares only `[[bench]]` targets
  and no `[[bin]]` / `[lib]`. `criterion` is compiled only when building
  benchmarks (`cargo bench`, `cargo build --benches`,
  `cargo test --benches`). `cargo build --bins --release` for the
  deliverable `eggress-cli` does **not** pull it in. Verify with:

  ```bash
  cargo tree -i criterion --package eggress-cli 2>&1   # should show error
  cargo tree -i criterion --package eggress-runtime 2>&1   # should show error
  ```

- `libfuzzer-sys` (fuzz harness): workspace dep of the **standalone** `fuzz/`
  workspace. `fuzz/Cargo.toml` declares its own `[workspace]` block and is
  not a member of the main workspace. `cargo build --workspace` and
  `cargo test --workspace` never compile it. Verify with:

  ```bash
  cargo tree --manifest-path fuzz/Cargo.toml -i libfuzzer-sys 2>&1
  cargo tree --manifest-path . -i libfuzzer-sys 2>&1     # should show error
  ```

- `rcgen` (test-only certificate generation): pulled in by
  `eggress-transport-tls` test targets. It transitively depends on
  `aws-lc-sys` in some versions. The dependency is dev-only and never
  reaches the runtime binary; it is the reason the cryptographic production
  build is ring-only even though test compilation may link against
  `aws-lc-sys` for the in-process mock CA.

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

## Python cipher dependency policy

Python AEAD compatibility is deterministic by installation profile:

- `eggress` keeps `cryptography` optional and reports cipher operations as
  unavailable when the extra is not installed.
- `eggress[cipher-api]` declares the tested `cryptography>=42,<47` range.
- `eggress-pproxy-compat` depends on `eggress==0.1.0` and that same cipher
  range, so a top-level `pproxy.cipher` import never relies on an undeclared
  optional package.

Legacy stream ciphers remain explicit unsupported stubs. Installing the extra
does not promote RC4, CFB, OFB, CTR, SSR, or other legacy methods.
