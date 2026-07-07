FROM rust:1.75-slim AS builder

ARG TARGETARCH
ARG VERSION=0.1.0

WORKDIR /build

RUN apt-get update && apt-get install -y \
    gcc-aarch64-linux-gnu \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN if [ "$TARGETARCH" = "arm64" ]; then \
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc && \
      cargo build --release --locked --target aarch64-unknown-linux-gnu -p eggress-cli && \
      cp target/aarch64-unknown-linux-gnu/release/eggress /eggress; \
    else \
      cargo build --release --locked -p eggress-cli && \
      cp target/release/eggress /eggress; \
    fi

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /eggress /eggress
COPY LICENSE-MIT LICENSE-APACHE /licenses/

EXPOSE 8080 1080 9090

ENTRYPOINT ["/eggress"]
