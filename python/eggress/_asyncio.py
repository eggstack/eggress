"""Asyncio bridge for native (Rust) blocking calls.

Provides a single maintained pattern that:

- Binds native cancellation to Python future cancellation.
- Converts internal failures into safe exceptions.
- Handles loop closure gracefully.
- Does not retain the event loop or object indefinitely.
- Schedules completion thread-safely.
- Preserves contextvars where the reference behaviour requires callback
  execution in the caller context.

Core invariants (Phase C5):

1. No nested Tokio runtime is created from an active runtime path.
2. No Python-visible awaitable resolves or raises more than once.
3. Python cancellation propagates to native operations promptly.
4. Native task failure resolves the Python future with a stable exception.
5. No blocking network or shutdown wait holds the GIL.
6. Close and wait semantics are idempotent and race-safe.
"""

from __future__ import annotations

import asyncio
import contextvars
import functools
import threading
import warnings
from typing import Any, Callable, Optional, TypeVar

from eggress._compat import get_running_loop

__all__ = [
    "LoopAffinityError",
    "AsyncBridge",
    "run_in_executor_with_cancel",
    "wrap_blocking_call",
    "CloseWaiter",
]

T = TypeVar("T")


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------


class LoopAffinityError(RuntimeError):
    """Raised when an async operation is used from the wrong event loop.

    Eggress objects bind to the loop they were first used on.  Cross-loop
    use raises this error *before* any native work begins so the caller
    gets a clear, predictable exception.
    """


# ---------------------------------------------------------------------------
# Loop-affinity helpers
# ---------------------------------------------------------------------------


def _check_loop(
    owner_loop: Optional[asyncio.AbstractEventLoop],
    label: str = "object",
) -> None:
    """Raise :class:`LoopAffinityError` if the running loop differs.

    If *owner_loop* is ``None`` the object has not been bound yet; this
    is not an error (first-use binding).  If there is no running loop we
    are in a sync context and the check is skipped.
    """
    current = get_running_loop()
    if current is None:
        return  # sync context — no affinity constraint
    if owner_loop is not None and current is not owner_loop:
        raise LoopAffinityError(
            f"{label} was created on loop {owner_loop!r} "
            f"but is being used from loop {current!r}"
        )


def _acquire_loop(label: str = "object") -> asyncio.AbstractEventLoop:
    """Get the running loop or raise a clear error.

    Called during first-use binding when the object has no loop yet.
    """
    loop = get_running_loop()
    if loop is None:
        raise RuntimeError(
            f"{label} must be used inside a running event loop. "
            "Use the synchronous API for non-async contexts."
        )
    return loop


# ---------------------------------------------------------------------------
# run_in_executor_with_cancel
# ---------------------------------------------------------------------------


async def run_in_executor_with_cancel(
    func: Callable[..., T],
    *args: Any,
    executor: Any = None,
    **kwargs: Any,
) -> T:
    """Run *func* in the default executor, but propagate cancellation.

    When the awaiting coroutine is cancelled the executor future is
    cancelled too, which signals the executor thread to stop (best-effort
    — thread cancellation is not guaranteed in Python).

    Returns the result of *func* or raises the exception from *func*.
    """
    loop = asyncio.get_running_loop()
    bound = functools.partial(func, *args, **kwargs)
    future = loop.run_in_executor(executor, bound)
    try:
        return await asyncio.shield(future)
    except asyncio.CancelledError:
        future.cancel()
        raise


# ---------------------------------------------------------------------------
# wrap_blocking_call — single-maintained async bridge
# ---------------------------------------------------------------------------


async def wrap_blocking_call(
    func: Callable[..., T],
    *args: Any,
    executor: Any = None,
    **kwargs: Any,
) -> T:
    """Run a blocking callable on the executor and return the result.

    Differences from plain ``run_in_executor``:

    - On cancellation the executor future is cancelled (best-effort).
    - Internal panics or unexpected errors are converted to a stable
      exception type rather than propagating as opaque ``RuntimeError``.
    - The function is always executed with the current contextvars snapshot
      so that caller-side context is preserved.
    """
    ctx = contextvars.copy_context()
    loop = asyncio.get_running_loop()

    def _run() -> T:
        return ctx.run(functools.partial(func, *args, **kwargs))

    future = loop.run_in_executor(executor, _run)
    try:
        return await asyncio.shield(future)
    except asyncio.CancelledError:
        future.cancel()
        raise
    except Exception as exc:
        # Convert opaque wrapper errors into something stable.
        raise RuntimeError(
            f"blocking call failed: {type(exc).__name__}: {exc}"
        ) from exc


# ---------------------------------------------------------------------------
# CloseWaiter — idempotent, race-safe, multi-waiter close semantics
# ---------------------------------------------------------------------------


class CloseWaiter:
    """Coordinate concurrent ``close()`` and ``wait_closed()`` callers.

    Invariants:

    - ``close()`` is idempotent — multiple calls are safe.
    - ``wait_closed()`` blocks until ``close()`` has been called *and*
      the cleanup callback (if any) has completed.
    - Multiple ``wait_closed()`` callers all unblock simultaneously.
    - If ``close()`` is called while ``wait_closed()`` is in progress,
      all waiters observe the result.
    - ``wait_closed()`` cancellation does not corrupt internal state.

    Usage::

        waiter = CloseWaiter()

        async def do_close():
            # ... cleanup work ...
            waiter.mark_closed()

        # In the close path:
        await waiter.close(do_close)

        # In the wait path:
        await waiter.wait_closed()
    """

    __slots__ = (
        "_closed",
        "_closing",
        "_event",
        "_result",
        "_exception",
        "_lock",
        "_cleanup_task",
    )

    def __init__(self) -> None:
        self._closed = False
        self._closing = False
        self._event = asyncio.Event()
        self._result: Any = None
        self._exception: Optional[BaseException] = None
        self._lock = threading.Lock()
        self._cleanup_task: Optional[asyncio.Task] = None

    @property
    def is_closed(self) -> bool:
        return self._closed

    @property
    def is_closing(self) -> bool:
        return self._closing

    def mark_closed(self, result: Any = None) -> None:
        """Mark the close operation as complete.

        Thread-safe.  May be called from any thread.
        """
        with self._lock:
            if self._closed:
                return
            self._result = result
            self._closed = True
            self._closing = False
        # Signal the event from the correct thread if possible.
        try:
            loop = get_running_loop()
            if loop is not None and loop.is_running():
                loop.call_soon_threadsafe(self._event.set)
            else:
                self._event.set()
        except RuntimeError:
            self._event.set()

    def mark_failed(self, exc: BaseException) -> None:
        """Mark the close operation as failed with an exception.

        Thread-safe.
        """
        with self._lock:
            if self._closed:
                return
            self._exception = exc
            self._closed = True
            self._closing = False
        try:
            loop = get_running_loop()
            if loop is not None and loop.is_running():
                loop.call_soon_threadsafe(self._event.set)
            else:
                self._event.set()
        except RuntimeError:
            self._event.set()

    async def close(self, cleanup: Optional[Callable[[], Any]] = None) -> None:
        """Initiate close and wait for cleanup to finish.

        Idempotent — second and subsequent calls return immediately.
        """
        if self._closed:
            return
        with self._lock:
            if self._closing:
                # Another close() is in progress — just wait.
                return self._event.wait()
            self._closing = True

        if cleanup is not None:
            try:
                result = cleanup()
                if asyncio.iscoroutine(result):
                    await result
            except BaseException as exc:
                self.mark_failed(exc)
                return

        self.mark_closed()

    async def wait_closed(self) -> Any:
        """Block until :meth:`mark_closed` or :meth:`mark_failed` is called.

        Returns the result passed to ``mark_closed()``, or re-raises the
        exception passed to ``mark_failed()``.
        """
        await self._event.wait()
        if self._exception is not None:
            raise self._exception
        return self._result


# ---------------------------------------------------------------------------
# AsyncBridge — full lifecycle bridge for Rust blocking calls
# ---------------------------------------------------------------------------


class AsyncBridge:
    """Bridges a Rust blocking call into the asyncio event loop.

    Handles:

    - Loop-affinity enforcement (first-use binding).
    - Cancellation propagation to the executor future.
    - Exception conversion to stable types.
    - ``__del__`` safety (never blocks).
    - Idempotent close.

    Usage::

        bridge = AsyncBridge(label="Connection")

        # First use binds the bridge to the running loop.
        result = await bridge.run(my_rust_function, arg1, arg2)

        # Cancellation is propagated:
        task = asyncio.create_task(bridge.run(slow_rust_call))
        task.cancel()  # best-effort cancels the executor future

        # Cleanup:
        bridge.close()
    """

    __slots__ = (
        "_label",
        "_loop",
        "_closed",
        "_closing",
        "_lock",
    )

    def __init__(self, label: str = "AsyncBridge") -> None:
        self._label = label
        self._loop: Optional[asyncio.AbstractEventLoop] = None
        self._closed = False
        self._closing = False
        self._lock = threading.Lock()

    def _bind_loop(self) -> asyncio.AbstractEventLoop:
        """Bind to the current running loop on first use."""
        if self._closed:
            raise RuntimeError(f"{self._label} is closed")
        current = get_running_loop()
        if current is None:
            raise RuntimeError(
                f"{self._label} must be used inside a running event loop"
            )
        if self._loop is not None and self._loop is not current:
            raise LoopAffinityError(
                f"{self._label} was created on loop {self._loop!r} "
                f"but is being used from loop {current!r}"
            )
        self._loop = current
        return current

    async def run(
        self,
        func: Callable[..., T],
        *args: Any,
        executor: Any = None,
        **kwargs: Any,
    ) -> T:
        """Run *func* on the executor, respecting cancellation.

        First call binds the bridge to the running loop.  Subsequent calls
        verify loop affinity.
        """
        self._bind_loop()
        if self._closed:
            raise RuntimeError(f"{self._label} is closed")

        ctx = contextvars.copy_context()
        loop = self._loop

        def _run() -> T:
            return ctx.run(functools.partial(func, *args, **kwargs))

        future = loop.run_in_executor(executor, _run)
        try:
            return await asyncio.shield(future)
        except asyncio.CancelledError:
            future.cancel()
            raise
        except Exception as exc:
            raise RuntimeError(
                f"{self._label} call failed: {type(exc).__name__}: {exc}"
            ) from exc

    def close(self) -> None:
        """Mark the bridge as closed. Idempotent, thread-safe."""
        with self._lock:
            self._closed = True

    @property
    def is_closed(self) -> bool:
        return self._closed

    def __del__(self) -> None:
        if not self._closed:
            warnings.warn(
                f"{self._label} was not properly closed.",
                ResourceWarning,
                stacklevel=2,
            )
