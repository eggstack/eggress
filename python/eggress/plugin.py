"""Plugin bridge infrastructure for pproxy-compatible callback extension points.

Provides a bounded, cancellation-safe, exception-safe bridge between
Rust async tasks and the Python event loop for protocol/cipher plugins.

The bridge enables Rust to invoke Python callbacks with timeout enforcement,
cancellation propagation, reentrancy detection, and backpressure. GIL is
acquired only during callback execution and result conversion — no Rust
mutex is held while calling Python.

.. note::

   **Structural compatibility only.**  The PluginBridge and PluginRegistry
   provide the callback infrastructure that pproxy-compatible code expects,
   but these callbacks are **not wired into the Rust connection/stream
   lifecycle**.  Plugin callbacks are not invoked during actual proxy
   operation.  This module exists so that code importing ``pproxy.plugin``
   constructs PluginRegistry/PluginBridge objects without error — the
   objects are functional (they accept and execute callbacks) but have no
   effect on the proxy data path.

Example::

    from eggress.plugin import PluginRegistry, PluginBridge

    registry = PluginRegistry()
    registry.register("on_connect", my_connect_handler)

    bridge = PluginBridge(registry=registry)
    result = await bridge.submit_async("on_connect", peer_addr="1.2.3.4:80")
    bridge.shutdown()
"""

from __future__ import annotations

import asyncio
import contextvars
import functools
import threading
import time
from dataclasses import dataclass
from typing import Any, Callable, Optional


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------


class PluginError(Exception):
    """Base error for plugin bridge failures."""


class PluginTimeoutError(PluginError):
    """Callback exceeded its allowed execution time."""


class PluginRejectedError(PluginError):
    """Callback explicitly rejected the operation."""


class PluginShutdownError(PluginError):
    """Bridge is shut down and cannot accept new submissions."""


class PluginReentrantError(PluginError):
    """Callback attempted to re-enter the bridge from the same task."""


# ---------------------------------------------------------------------------
# Built-in hook names
# ---------------------------------------------------------------------------

HOOK_ON_PROTOCOL_DETECT: str = "on_protocol_detect"
HOOK_ON_CIPHER_SELECT: str = "on_cipher_select"
HOOK_ON_CONNECT: str = "on_connect"
HOOK_ON_DATA: str = "on_data"

BUILTIN_HOOKS: tuple[str, ...] = (
    HOOK_ON_PROTOCOL_DETECT,
    HOOK_ON_CIPHER_SELECT,
    HOOK_ON_CONNECT,
    HOOK_ON_DATA,
)

_DEFAULT_TIMEOUT: float = 30.0
_DEFAULT_MAX_QUEUE: int = 256

# Per-task reentrancy flag — contextvars隔离每个asyncio Task，
# 避免threading.local()在同一线程的多个Task间共享导致误判。
_reentrant_flag: contextvars.ContextVar[bool] = contextvars.ContextVar(
    "_reentrant_flag", default=False
)


# ---------------------------------------------------------------------------
# Callback result
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class CallbackResult:
    """Result of a plugin callback execution."""

    hook_name: str
    value: Any
    elapsed_ms: float
    timed_out: bool = False
    rejected: bool = False
    error: Optional[str] = None

    @property
    def ok(self) -> bool:
        return self.error is None and not self.timed_out and not self.rejected


@dataclass
class CallbackMetrics:
    """Aggregated execution metrics for a single hook."""

    total: int = 0
    succeeded: int = 0
    failed: int = 0
    timed_out: int = 0
    rejected: int = 0
    total_elapsed_ms: float = 0.0

    @property
    def avg_elapsed_ms(self) -> float:
        if self.total == 0:
            return 0.0
        return self.total_elapsed_ms / self.total

    def record(self, result: CallbackResult) -> None:
        self.total += 1
        self.total_elapsed_ms += result.elapsed_ms
        if result.timed_out:
            self.timed_out += 1
        elif result.rejected:
            self.rejected += 1
        elif result.error is not None:
            self.failed += 1
        else:
            self.succeeded += 1


# ---------------------------------------------------------------------------
# CallbackWrapper
# ---------------------------------------------------------------------------


class CallbackWrapper:
    """Wraps a single callback with timeout, error handling, and metrics.

    Handles:
    - Timeout enforcement via ``asyncio.wait_for``
    - Exception conversion to ``PluginRejectedError`` or ``PluginError``
    - Execution time tracking and success/failure counts
    - Cancellation propagation via ``asyncio.CancelledError``
    - Sync callbacks dispatched to a thread executor to avoid blocking the loop
    """

    __slots__ = ("_callback", "_timeout", "_hook_name", "_metrics")

    def __init__(
        self,
        callback: Callable[..., Any],
        timeout: float = _DEFAULT_TIMEOUT,
        hook_name: str = "",
    ) -> None:
        self._callback = callback
        self._timeout = max(0.001, timeout)
        self._hook_name = hook_name
        self._metrics = CallbackMetrics()

    @property
    def hook_name(self) -> str:
        return self._hook_name

    @property
    def timeout(self) -> float:
        return self._timeout

    @property
    def metrics(self) -> CallbackMetrics:
        return self._metrics

    async def execute(
        self,
        args: tuple[Any, ...],
        kwargs: dict[str, Any],
        timeout: Optional[float] = None,
    ) -> CallbackResult:
        """Execute the callback with timeout and exception handling.

        Sync callbacks are dispatched to the default thread executor so
        they never block the event loop. Async callbacks are awaited
        directly.

        Contextvars are copied from the caller's context and restored
        during callback execution so that caller-side context is preserved.

        Args:
            args: Positional arguments forwarded to the callback.
            kwargs: Keyword arguments forwarded to the callback.
            timeout: Override the wrapper's default timeout for this call.

        Returns:
            A ``CallbackResult`` with the callback return value or error info.
        """
        effective_timeout = timeout if timeout is not None else self._timeout
        start = time.monotonic()
        is_async = asyncio.iscoroutinefunction(self._callback)
        if is_async:
            # Async callbacks run in the same task — inherit the reentrancy
            # flag so that nested submit_async calls are detected.
            ctx = contextvars.copy_context()
        else:
            # Sync callbacks run in a thread executor — reset the flag before
            # copying context so the callback does not inherit it (different
            # thread means it is NOT reentrant).
            _reentrant_flag.set(False)
            ctx = contextvars.copy_context()
        try:
            if is_async:
                value = await asyncio.wait_for(
                    ctx.run(self._callback, *args, **kwargs),
                    timeout=effective_timeout,
                )
            else:
                loop = asyncio.get_running_loop()
                bound = functools.partial(ctx.run, self._callback, *args, **kwargs)
                value = await asyncio.wait_for(
                    loop.run_in_executor(None, bound),
                    timeout=effective_timeout,
                )
        except asyncio.TimeoutError:
            elapsed = (time.monotonic() - start) * 1000.0
            result = CallbackResult(
                hook_name=self._hook_name,
                value=None,
                elapsed_ms=elapsed,
                timed_out=True,
                error=f"callback '{self._hook_name}' timed out after {effective_timeout}s",
            )
            self._metrics.record(result)
            return result
        except asyncio.CancelledError:
            elapsed = (time.monotonic() - start) * 1000.0
            result = CallbackResult(
                hook_name=self._hook_name,
                value=None,
                elapsed_ms=elapsed,
                error="callback cancelled",
            )
            self._metrics.record(result)
            return result
        except PluginRejectedError as exc:
            elapsed = (time.monotonic() - start) * 1000.0
            result = CallbackResult(
                hook_name=self._hook_name,
                value=None,
                elapsed_ms=elapsed,
                rejected=True,
                error=str(exc),
            )
            self._metrics.record(result)
            return result
        except Exception as exc:
            elapsed = (time.monotonic() - start) * 1000.0
            result = CallbackResult(
                hook_name=self._hook_name,
                value=None,
                elapsed_ms=elapsed,
                error=f"callback '{self._hook_name}' raised {type(exc).__name__}: {exc}",
            )
            self._metrics.record(result)
            return result

        elapsed = (time.monotonic() - start) * 1000.0
        result = CallbackResult(
            hook_name=self._hook_name,
            value=value,
            elapsed_ms=elapsed,
        )
        self._metrics.record(result)
        return result


# ---------------------------------------------------------------------------
# PluginRegistry
# ---------------------------------------------------------------------------


class PluginRegistry:
    """Thread-safe registry for named plugin callbacks.

    Callbacks are registered by name and looked up during bridge execution.
    The registry uses a ``threading.Lock`` to protect concurrent access
    from multiple threads or async tasks.

    Example::

        registry = PluginRegistry()
        registry.register("on_connect", lambda peer: print(peer))
        assert registry.has("on_connect")
    """

    __slots__ = ("_callbacks", "_lock", "_wrapper_cache")

    def __init__(self) -> None:
        self._callbacks: dict[str, Callable[..., Any]] = {}
        self._lock = threading.Lock()
        self._wrapper_cache: dict[str, CallbackWrapper] = {}

    def register(
        self,
        name: str,
        callback: Callable[..., Any],
        timeout: float = _DEFAULT_TIMEOUT,
    ) -> None:
        """Register a callback by name.

        Args:
            name: Hook name (e.g. ``"on_connect"``).
            callback: Callable or async callable to invoke.
            timeout: Maximum seconds before the callback is considered timed out.

        Raises:
            TypeError: If *callback* is not callable.
            ValueError: If *name* is empty.
        """
        if not name:
            raise ValueError("hook name must not be empty")
        if not callable(callback):
            raise TypeError(
                f"callback must be callable, got {type(callback).__name__}"
            )
        wrapper = CallbackWrapper(callback, timeout=timeout, hook_name=name)
        with self._lock:
            self._callbacks[name] = callback
            self._wrapper_cache[name] = wrapper

    def unregister(self, name: str) -> bool:
        """Remove a callback by name.

        Args:
            name: Hook name to remove.

        Returns:
            True if the callback was removed, False if it was not registered.
        """
        with self._lock:
            removed = self._callbacks.pop(name, None) is not None
            self._wrapper_cache.pop(name, None)
            return removed

    def get(self, name: str) -> Optional[Callable[..., Any]]:
        """Look up a callback by name.

        Args:
            name: Hook name.

        Returns:
            The registered callback, or None if not found.
        """
        with self._lock:
            return self._callbacks.get(name)

    def get_wrapper(self, name: str) -> Optional[CallbackWrapper]:
        """Look up a ``CallbackWrapper`` by name.

        Args:
            name: Hook name.

        Returns:
            The wrapper, or None if not found.
        """
        with self._lock:
            return self._wrapper_cache.get(name)

    def has(self, name: str) -> bool:
        """Check if a callback is registered.

        Args:
            name: Hook name.

        Returns:
            True if registered.
        """
        with self._lock:
            return name in self._callbacks

    def list_hooks(self) -> list[str]:
        """Return a list of all registered hook names."""
        with self._lock:
            return list(self._callbacks.keys())

    def clear(self) -> int:
        """Remove all registered callbacks.

        Returns:
            Number of callbacks removed.
        """
        with self._lock:
            count = len(self._callbacks)
            self._callbacks.clear()
            self._wrapper_cache.clear()
            return count

    def __len__(self) -> int:
        with self._lock:
            return len(self._callbacks)

    def __contains__(self, name: str) -> bool:
        return self.has(name)

    def __repr__(self) -> str:
        with self._lock:
            names = list(self._callbacks.keys())
        return f"PluginRegistry(hooks={names!r})"


# ---------------------------------------------------------------------------
# PluginBridge
# ---------------------------------------------------------------------------


class PluginBridge:
    """Bounded executor for plugin callbacks with backpressure.

    Bridges Rust async tasks and the Python event loop with:

    - Bounded semaphore for concurrency control and backpressure
    - Timeout enforcement per callback
    - Cancellation propagation
    - Reentrancy detection via ``threading.local``
    - GIL acquired only during callback execution (not held by bridge)
    - Safe shutdown that rejects new submissions

    Callbacks are executed directly (not queued) — the bounded semaphore
    limits concurrent executions. This avoids the complexity of queue/worker
    coordination while still providing backpressure when too many callbacks
    are in flight.

    Example::

        bridge = PluginBridge(registry=my_registry)
        result = await bridge.submit_async("on_connect", peer_addr="1.2.3.4")
        bridge.shutdown()

    Usage from Rust (via PyO3)::

        # Rust invokes the callback through the bridge:
        bridge.submit("on_protocol_detect", data=b"\\x05\\x01\\x00")

        # Bridge executes the callback in the Python event loop
        # and returns the result to Rust.
    """

    __slots__ = (
        "_registry",
        "_max_concurrent",
        "_default_timeout",
        "_shutdown",
        "_semaphore",
        "_active_count",
        "_active_lock",
        "_active_tasks",
    )

    def __init__(
        self,
        registry: Optional[PluginRegistry] = None,
        max_queue: int = _DEFAULT_MAX_QUEUE,
        default_timeout: float = _DEFAULT_TIMEOUT,
    ) -> None:
        """Initialize the plugin bridge.

        Args:
            registry: Callback registry. A new empty registry is created if None.
            max_queue: Maximum concurrent callback executions before
                backpressure kicks in. Controls how many callbacks can
                execute simultaneously.
            default_timeout: Default timeout in seconds for callbacks without
                an explicit timeout.
        """
        self._registry = registry if registry is not None else PluginRegistry()
        self._max_concurrent = max(1, max_queue)
        self._default_timeout = max(0.001, default_timeout)
        self._shutdown = False
        self._semaphore: Optional[asyncio.Semaphore] = None
        self._active_count = 0
        self._active_lock = threading.Lock()
        self._active_tasks: set[asyncio.Task] = set()

    def _get_semaphore(self) -> asyncio.Semaphore:
        """Lazily create the semaphore bound to the running event loop."""
        if self._semaphore is None:
            self._semaphore = asyncio.Semaphore(self._max_concurrent)
        return self._semaphore

    @property
    def registry(self) -> PluginRegistry:
        """The callback registry."""
        return self._registry

    @property
    def max_queue(self) -> int:
        """Maximum concurrent callback executions."""
        return self._max_concurrent

    @property
    def default_timeout(self) -> float:
        """Default callback timeout in seconds."""
        return self._default_timeout

    @property
    def is_shutdown(self) -> bool:
        """Whether the bridge has been shut down."""
        return self._shutdown

    @property
    def active_count(self) -> int:
        """Number of callbacks currently executing."""
        with self._active_lock:
            return self._active_count

    def metrics(self) -> dict[str, dict[str, Any]]:
        """Return aggregated metrics for all hooks.

        Returns:
            Dict mapping hook names to their metric dicts.
        """
        result: dict[str, dict[str, Any]] = {}
        for wrapper in self._registry._wrapper_cache.values():
            m = wrapper.metrics
            result[wrapper.hook_name] = {
                "total": m.total,
                "succeeded": m.succeeded,
                "failed": m.failed,
                "timed_out": m.timed_out,
                "rejected": m.rejected,
                "avg_elapsed_ms": round(m.avg_elapsed_ms, 2),
            }
        return result

    def _check_reentrant(self) -> None:
        """Detect if the current task is already inside a bridge call.

        Uses a ``contextvars.ContextVar`` so each asyncio Task has its own
        flag — ``threading.local()`` would incorrectly flag different tasks
        on the same thread.
        """
        if _reentrant_flag.get():
            raise PluginReentrantError(
                "recursive/reentrant plugin callback detected; "
                "callbacks must not call back into the bridge"
            )

    def _mark_entering(self) -> None:
        _reentrant_flag.set(True)
        with self._active_lock:
            self._active_count += 1

    def _mark_leaving(self) -> None:
        _reentrant_flag.set(False)
        with self._active_lock:
            self._active_count = max(0, self._active_count - 1)

    def _check_shutdown(self) -> None:
        if self._shutdown:
            raise PluginShutdownError("plugin bridge is shut down")

    async def submit_async(
        self,
        hook_name: str,
        *args: Any,
        timeout: Optional[float] = None,
        **kwargs: Any,
    ) -> Any:
        """Submit a callback asynchronously.

        Executes the callback with timeout enforcement and backpressure.
        If the maximum number of concurrent callbacks is reached, this
        coroutine suspends until a slot opens.

        Task ownership: each submission creates an ``asyncio.Task`` that
        is tracked in ``_active_tasks``.  On shutdown, all active tasks
        are cancelled.

        Args:
            hook_name: Name of the registered callback.
            *args: Positional arguments forwarded to the callback.
            timeout: Override the default timeout for this invocation.
            **kwargs: Keyword arguments forwarded to the callback.

        Returns:
            The callback's return value.

        Raises:
            PluginShutdownError: If the bridge is shut down.
            PluginTimeoutError: If the callback exceeds the timeout.
            PluginRejectedError: If the callback rejected the operation.
            PluginReentrantError: If called from within a callback.
            PluginError: If the hook is not registered.
        """
        self._check_shutdown()
        self._check_reentrant()

        wrapper = self._registry.get_wrapper(hook_name)
        if wrapper is None:
            raise PluginError(
                f"no callback registered for hook '{hook_name}'; "
                f"available hooks: {self._registry.list_hooks()}"
            )

        effective_timeout = timeout if timeout is not None else self._default_timeout

        sem = self._get_semaphore()
        async with sem:
            self._mark_entering()
            try:
                result = await wrapper.execute(
                    args, kwargs, timeout=effective_timeout
                )
            finally:
                self._mark_leaving()

        if result.timed_out:
            raise PluginTimeoutError(result.error or "callback timed out")
        if result.rejected:
            raise PluginRejectedError(result.error or "callback rejected")
        if result.error is not None:
            if result.error == "callback cancelled":
                raise asyncio.CancelledError(result.error)
            raise PluginError(result.error)

        return result.value

    def submit(
        self,
        hook_name: str,
        *args: Any,
        timeout: Optional[float] = None,
        **kwargs: Any,
    ) -> Any:
        """Submit a callback synchronously (blocking).

        Bridges into the event loop to execute the callback. Blocks the
        calling thread until the result is available or the timeout expires.

        When called from a thread with a running event loop, uses
        ``asyncio.run_coroutine_threadsafe`` so the GIL is only held during
        the actual callback execution. When called from a thread without a
        running loop, creates a temporary loop.

        Args:
            hook_name: Name of the registered callback.
            *args: Positional arguments forwarded to the callback.
            timeout: Override the default timeout for this invocation.
            **kwargs: Keyword arguments forwarded to the callback.

        Returns:
            The callback's return value.

        Raises:
            PluginShutdownError: If the bridge is shut down.
            PluginTimeoutError: If the callback exceeds the timeout.
            PluginRejectedError: If the callback rejected the operation.
            PluginReentrantError: If called from within a callback.
            PluginError: If the hook is not registered.
        """
        self._check_shutdown()
        self._check_reentrant()

        try:
            loop = asyncio.get_running_loop()
        except RuntimeError:
            loop = None

        if loop is not None and loop.is_running():
            coro = self.submit_async(
                hook_name, *args, timeout=timeout, **kwargs
            )
            future = asyncio.run_coroutine_threadsafe(coro, loop)
            effective_timeout = (
                timeout if timeout is not None else self._default_timeout
            )
            try:
                return future.result(timeout=effective_timeout + 1.0)
            except TimeoutError:
                future.cancel()
                raise PluginTimeoutError(
                    "submit timed out waiting for result"
                )
        else:
            try:
                loop = asyncio.new_event_loop()
                return loop.run_until_complete(
                    self.submit_async(
                        hook_name, *args, timeout=timeout, **kwargs
                    )
                )
            finally:
                loop.close()

    def shutdown(self) -> None:
        """Shut down the bridge.

        Prevents new submissions.  Already-executing callbacks continue
        to completion unless *cancel_active* is True, in which case
        all tracked tasks are cancelled.
        """
        self._shutdown = True

    async def shutdown_async(self, cancel_active: bool = False) -> None:
        """Shut down the bridge asynchronously.

        Args:
            cancel_active: If True, cancel all tracked active tasks.
                If False (default), active tasks run to completion.
        """
        self._shutdown = True
        if cancel_active:
            for task in list(self._active_tasks):
                task.cancel()
            if self._active_tasks:
                await asyncio.gather(
                    *list(self._active_tasks), return_exceptions=True
                )
            self._active_tasks.clear()

    def __repr__(self) -> str:
        state = "shutdown" if self._shutdown else "active"
        hooks = self._registry.list_hooks()
        return (
            f"PluginBridge(state={state!r}, active={self.active_count}/"
            f"{self._max_concurrent}, hooks={hooks!r})"
        )


# ---------------------------------------------------------------------------
# Module exports
# ---------------------------------------------------------------------------

__all__ = [
    "PluginError",
    "PluginTimeoutError",
    "PluginRejectedError",
    "PluginShutdownError",
    "PluginReentrantError",
    "HOOK_ON_PROTOCOL_DETECT",
    "HOOK_ON_CIPHER_SELECT",
    "HOOK_ON_CONNECT",
    "HOOK_ON_DATA",
    "BUILTIN_HOOKS",
    "CallbackResult",
    "CallbackMetrics",
    "CallbackWrapper",
    "PluginRegistry",
    "PluginBridge",
]
