# Linux Desktop Proxy

## Overview

On Linux desktop environments, system proxy settings are typically managed via `gsettings` (GNOME) or `kwriteconfig5` (KDE). This document describes how Eggress interacts with these tools.

## GNOME (gsettings)

### Schema

```bash
org.gnome.system.proxy mode           # 'none', 'manual', or 'auto'
org.gnome.system.proxy.http host      # HTTP proxy host
org.gnome.system.proxy.http port      # HTTP proxy port
org.gnome.system.proxy.https host     # HTTPS proxy host
org.gnome.system.proxy.https port     # HTTPS proxy port
org.gnome.system.proxy.socks host     # SOCKS proxy host
org.gnome.system.proxy.socks port     # SOCKS proxy port
org.gnome.system.proxy ignore-hosts   # Bypass list
```

### Inspection

```bash
gsettings get org.gnome.system.proxy mode
gsettings get org.gnome.system.proxy.http host
gsettings get org.gnome.system.proxy.http port
```

### Dry-Run Apply Commands

```bash
gsettings set org.gnome.system.proxy mode 'manual'
gsettings set org.gnome.system.proxy.http host 'proxy.example.com'
gsettings set org.gnome.system.proxy.http port 8080
gsettings set org.gnome.system.proxy.https host 'proxy.example.com'
gsettings set org.gnome.system.proxy.https port 8443
gsettings set org.gnome.system.proxy ignore-hosts ['localhost', '127.0.0.1']
```

### Disable Commands

```bash
gsettings set org.gnome.system.proxy mode 'none'
```

## KDE

KDE proxy settings are managed via `kwriteconfig5` and `kreadconfig5`. Support is planned but not yet implemented.

## Caveats

- Requires `gsettings` to be available (GNOME desktop)
- KDE support is deferred
- Non-desktop (server) environments typically use environment variables
- Eggress never executes these commands automatically
