# macOS Networksetup

## Overview

On macOS, system proxy settings are managed via the `networksetup` command-line tool. This document describes the read-only inspection backend and the platform command reference retained by the Rust library.

## Inspection

Eggress reads proxy settings using:

```bash
networksetup -getwebproxy <service>        # HTTP proxy
networksetup -getsecurewebproxy <service>  # HTTPS proxy
networksetup -getsocksfirewallproxy <service>  # SOCKS proxy
```

### Network Services

List available services:

```bash
networksetup -listallnetworkservices
```

Eggress uses the first listed service by default (typically `*Wi-Fi` or `*Ethernet`).

## Platform command reference

The following platform-native commands document the operations that the Rust
library can plan. They are not commands exposed by the Eggress CLI and are not
executed by `eggress system-proxy inspect`.

Example platform-native commands:

```bash
# Enable and set HTTP proxy
networksetup -setwebproxy <service> on
networksetup -setwebproxyservers <service> <host>:<port>

# Enable and set HTTPS proxy
networksetup -setsecurewebproxy <service> on
networksetup -setsecurewebproxyservers <service> <host>:<port>

# Enable and set SOCKS proxy
networksetup -setsocksfirewallproxy <service> on
networksetup -setsocksfirewallproxyserver <service> <host>:<port>

# Set bypass domains
networksetup -setwebproxybypassdomains <service> <domains>
```

## Disable Commands

```bash
networksetup -setwebproxy <service> off
networksetup -setsecurewebproxy <service> off
networksetup -setsocksfirewallproxy <service> off
```

## Caveats

- Requires `networksetup` to be available (standard on macOS)
- Some services may require admin privileges to modify
- Eggress never executes these commands automatically
- Review any platform-native mutation carefully before applying it; Eggress
  does not apply these commands through its native CLI.
