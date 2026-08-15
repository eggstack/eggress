"""Execute the same compatibility entry point as the ``pproxy`` script."""

from .server import main


if __name__ == "__main__":  # pragma: no cover - exercised by subprocess tests
    raise SystemExit(main())
