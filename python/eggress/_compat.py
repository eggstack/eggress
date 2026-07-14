"""Python version compatibility helpers for Phase C5.

Centralizes version-specific logic so the rest of the package avoids
scattered ``sys.version_info`` checks.  All helpers are small, pure-Python,
and safe to import at module load time.
"""

from __future__ import annotations

import sys

__all__ = [
    "PY_VERSION",
    "PY_MAJOR",
    "PY_MINOR",
    "HAS_TASKGROUP",
    "HAS_EXCEPTIONGROUP",
    "CANCELLED_ERROR_BASE",
    "get_running_loop",
    "cancelled_error_is_base",
]

PY_VERSION: tuple[int, int, int] = (
    sys.version_info.major,
    sys.version_info.minor,
    sys.version_info.micro,
)
PY_MAJOR: int = PY_VERSION[0]
PY_MINOR: int = PY_VERSION[1]

# Python 3.11+ has TaskGroup and ExceptionGroup built in.
HAS_TASKGROUP: bool = PY_MINOR >= 11 or PY_MAJOR > 3
HAS_EXCEPTIONGROUP: bool = HAS_TASKGROUP

# In Python < 3.9 asyncio.CancelledError inherits from
# concurrent.futures.CancelledError.  In 3.9+ it inherits directly from
# BaseException.  We expose the base class for ``except`` compatibility.
if PY_MINOR >= 9 or PY_MAJOR > 3:
    CANCELLED_ERROR_BASE: type = BaseException  # type: ignore[assignment]
else:
    CANCELLED_ERROR_BASE: type = Exception  # type: ignore[assignment]


def get_running_loop():
    """Wrapper around :func:`asyncio.get_running_loop`.

    Returns the running event loop or ``None`` if no loop is running.
    """
    import asyncio

    try:
        return asyncio.get_running_loop()
    except RuntimeError:
        return None


def cancelled_error_is_base(exc: BaseException) -> bool:
    """Return ``True`` if *exc* is a ``CancelledError``.

    On Python < 3.9 the exception may come from
    ``concurrent.futures``; this helper normalises the check.
    """
    import asyncio

    return isinstance(exc, asyncio.CancelledError)
