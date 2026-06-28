"""Start an eggress service from pproxy-style CLI arguments."""

from eggress import start_pproxy

with start_pproxy([
    "-l", "socks5://127.0.0.1:1080",
    "-r", "http://proxy:8080",
]) as handle:
    print("Listening on", handle.bound_addresses)
    print("Metrics preview:", handle.metrics_text()[:200])
    try:
        while True:
            pass
    except KeyboardInterrupt:
        pass
