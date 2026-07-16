from .server import Server
from .proto import BaseProtocol
from eggress.pproxy_connection import ProxyConnection as Connection

__eggress_compat__: bool
__eggress_version__: str
__pproxy_compatibility_version__: str
__version__: str
DIRECT: object

class Rule:
    filename: str | None
    def __init__(self, filename: str | None = ...) -> None: ...
