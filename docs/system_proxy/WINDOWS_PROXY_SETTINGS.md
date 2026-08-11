# Windows Proxy Settings

## Overview

On Windows, system proxy settings are stored in the Windows Registry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`. This document describes how Eggress interacts with these settings.

## Registry Keys

| Key | Type | Description |
|-----|------|-------------|
| `ProxyEnable` | `REG_DWORD` | `1` to enable, `0` to disable |
| `ProxyServer` | `REG_SZ` | Proxy address (e.g., `http=proxy:8080;https=proxy:8443`) |
| `ProxyOverride` | `REG_SZ` | Bypass list (e.g., `localhost;127.0.0.1`) |

## Inspection

Eggress reads settings using:

```cmd
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyServer
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyEnable
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyOverride
```

## Platform command reference

The following platform-native commands are reference material for the Rust
library's planning helpers. They are not Eggress CLI commands and are not
executed by `eggress system-proxy inspect`.

```cmd
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyServer /t REG_SZ /d "http=proxy:8080;https=proxy:8443" /f
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyEnable /t REG_DWORD /d 1 /f
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyOverride /t REG_SZ /d "localhost;127.0.0.1" /f
```

## Disable Commands

```cmd
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings" /v ProxyEnable /t REG_DWORD /d 0 /f
```

## Caveats

- Requires Windows platform (returns `UnsupportedPlatform` on other OS)
- Registry modification may require elevated privileges
- Eggress never executes these commands from its native CLI
- Changes may require applications to be restarted to take effect
