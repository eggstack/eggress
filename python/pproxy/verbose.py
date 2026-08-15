"""Small formatting/statistics adapter for pproxy callers.

The compatibility runtime remains Rust-owned.  These helpers preserve the
observable callback and formatting shape used by applications that configure
pproxy's optional verbose module.
"""

from __future__ import annotations

import asyncio
import functools
import sys
import time
from typing import Any


def b2s(value: float) -> str:
    return (
        f"{value / 2**30:.1f}G"
        if value >= 2**30
        else f"{value / 2**20:.1f}M"
        if value >= 2**20
        else f"{value / 1024:.1f}K"
    )


def all_stat_other(stats: dict) -> None:
    sys.stdin.readline()
    all_stat(stats)


def all_stat(stats: dict) -> None:
    if len(stats) <= 1:
        print("no traffic")
        return
    print("=" * 70)
    host_stats: dict[str, list[float]] = {}
    for remote_ip, values in stats.items():
        if remote_ip == 0:
            continue
        remote_stat = [0] * 6
        for host_name, host_value in values.items():
            for aggregate in (remote_stat, host_stats.setdefault(host_name, [0] * 6)):
                for index in range(6):
                    aggregate[index] += host_value[index]
        display = [b2s(value) for value in remote_stat[:4]] + remote_stat[4:]
        print(remote_ip, "\tDIRECT: {5} ({1},{3})  PROXY: {4} ({0},{2})".format(*display))
    print(" " * 3 + "-" * 64)
    ordered = sorted(host_stats.items(), key=lambda item: sum(item[1]), reverse=True)[:15]
    width = max((len(item[0]) for item in ordered), default=0)
    for host_name, values in ordered:
        traffic = (b2s(values[0] + values[1]), b2s(values[2] + values[3]))
        connections = values[4] + values[5]
        suffix = f" / {connections}" if connections else ""
        print(host_name.ljust(width + 5), f"{traffic[0]} / {traffic[1]}{suffix}")
    print("=" * 70)


async def realtime_stat(stats: list[float]) -> None:
    history = [(stats[:4], time.perf_counter())]
    while True:
        await asyncio.sleep(1)
        history.append((stats[:4], time.perf_counter()))
        before, t0 = history[0]
        after, t1 = history[-1]
        elapsed = max(t1 - t0, 1e-9)
        display = [b2s((after[index] - before[index]) / elapsed) + "/s" for index in range(4)] + stats[4:]
        sys.stdout.write(
            "DIRECT: {5} ({1},{3})   PROXY: {4} ({0},{2})\x1b[0K\r".format(*display)
        )
        sys.stdout.flush()
        if len(history) >= 10:
            del history[0]


def setup(loop: Any, args: Any) -> None:
    """Attach pproxy-shaped callbacks to an argparse namespace."""

    def verbose(message: str) -> None:
        if getattr(args, "v", 0) >= 2:
            sys.stdout.write(f"\x1b[32m{time.strftime('%Y-%m-%d %H:%M:%S')}\x1b[m ")
        sys.stdout.write(message + "\x1b[0K\n" if getattr(args, "v", 0) >= 2 else message + "\n")
        sys.stdout.flush()

    args.verbose = verbose
    args.stats = {0: [0] * 6}

    def modstat(user: Any, remote_ip: str, host_name: str, stats: dict = args.stats):
        prefix = user.decode().split(":", 1)[0] + ":" if isinstance(user, (bytes, bytearray)) else ""
        labels = host_name.split(".")
        suffix = -3 if host_name.endswith(".com.cn") else -2
        compact = ".".join(labels[suffix:]) if labels and labels[-1].isalpha() else host_name
        target = (stats[0], stats.setdefault(prefix + remote_ip, {}).setdefault(compact, [0] * 6))

        def increment(index: int):
            return lambda amount: [values.__setitem__(index, values[index] + amount) for values in target]

        return increment

    args.modstat = modstat
    if getattr(args, "v", 0) >= 2:
        loop.create_task(realtime_stat(args.stats[0]))
        if sys.platform != "win32":
            loop.add_reader(sys.stdin, functools.partial(all_stat_other, args.stats))
