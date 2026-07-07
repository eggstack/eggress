# Container Image (Phase 49)

## Overview

eggress provides a minimal container image based on Google's `distroless`
static image. The image contains only the `eggress` binary and its
minimal runtime dependencies — no shell, no package manager, no
unnecessary files.

## Image location

```
ghcr.io/{owner}/eggress:{version}
ghcr.io/{owner}/eggress:latest
```

## Pulling

```bash
# Specific version
docker pull ghcr.io/{owner}/eggress:v0.1.0

# Latest stable
docker pull ghcr.io/{owner}/eggress:latest
```

## Running

```bash
# Simple HTTP proxy
docker run -d -p 8080:8080 ghcr.io/{owner}/eggress:v0.1.0 \
  -l http://:8080

# SOCKS5 proxy with authentication
docker run -d -p 1080:1080 ghcr.io/{owner}/eggress:v0.1.0 \
  -l socks5://user:pass@:1080

# With TOML config
docker run -d -p 1080:1080 -v ./config.toml:/etc/eggress/config.toml \
  ghcr.io/{owner}/eggress:v0.1.0 --config /etc/eggress/config.toml
```

## Configuration

Mount your TOML configuration file into the container:

```bash
docker run -d \
  -p 1080:1080 \
  -p 8080:8080 \
  -v /path/to/config.toml:/etc/eggress/config.toml:ro \
  ghcr.io/{owner}/eggress:v0.1.0 \
  --config /etc/eggress/config.toml
```

## Ports

| Port | Protocol | Purpose |
|---|---|---|
| 8080 | TCP | HTTP proxy (default) |
| 1080 | TCP | SOCKS5 proxy (default) |
| 9090 | TCP | Admin/metrics (if configured) |

Adjust ports based on your configuration.

## Health check

If the admin server is enabled in your config, the container supports
health checks:

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["/eggress", "--version"]
```

## Non-root user

The container runs as a non-root user by default (distroless `nonroot`).

## Multi-architecture

The image is built for:

- `linux/amd64` (x86_64)
- `linux/arm64` (aarch64)

```bash
# Docker automatically pulls the correct architecture
docker pull ghcr.io/{owner}/eggress:v0.1.0
```

## Building locally

```bash
# Build for current platform
docker build -t eggress:local .

# Build multi-arch (requires buildx)
docker buildx build --platform linux/amd64,linux/arm64 -t eggress:local .
```

## Security

- No shell in the image (distroless)
- Non-root user by default
- No OpenSSL — uses rustls with ring crypto
- SBOM generated for each build
- Images signed with cosign (if enabled)

## Resource limits

Recommended resource limits for production:

```yaml
deploy:
  resources:
    limits:
      cpus: "1.0"
      memory: 256M
    reservations:
      memory: 64M
```

The eggress binary is memory-efficient; 64MB is typically sufficient for
moderate traffic loads.
