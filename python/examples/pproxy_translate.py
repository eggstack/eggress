"""Translate pproxy-style arguments to eggress TOML configuration."""

from eggress import translate_pproxy_args

result = translate_pproxy_args([
    "-l", "socks5://127.0.0.1:1080",
    "-r", "http://proxy:8080",
])

print("=== Generated TOML ===")
print(result.toml)

print("=== Warnings ===")
for w in result.warnings:
    print(f"  [{w.category}] {w.message}")

print("=== Unsupported ===")
for u in result.unsupported:
    print(f"  unsupported {u.feature}: {u.message}")

print(f"Translation OK: {result.ok}")
