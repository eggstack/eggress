//! Python exception declarations and error mapping.
//!
//! Every module imports the canonical mapper ([`map_error`]) so Python
//! exception inheritance and `EggressError` categorization stay consistent.

use pyo3::exceptions::PyException;
use pyo3::prelude::*;

pyo3::create_exception!(_eggress, EggressError, PyException);
pyo3::create_exception!(_eggress, ConfigError, EggressError);
pyo3::create_exception!(_eggress, StartupError, EggressError);
pyo3::create_exception!(_eggress, ReloadError, EggressError);
pyo3::create_exception!(_eggress, ShutdownError, EggressError);
pyo3::create_exception!(_eggress, UnsupportedFeatureError, EggressError);
pyo3::create_exception!(_eggress, InternalError, EggressError);
pyo3::create_exception!(_eggress, ConnectionError, EggressError);
pyo3::create_exception!(_eggress, ConnectionClosedError, EggressError);
pyo3::create_exception!(_eggress, TimeoutError, EggressError);
pyo3::create_exception!(_eggress, DnsError, EggressError);
pyo3::create_exception!(_eggress, AuthError, EggressError);
pyo3::create_exception!(_eggress, TlsError, EggressError);
pyo3::create_exception!(_eggress, LoopMismatchError, EggressError);
pyo3::create_exception!(_eggress, ConnectionCancelledError, EggressError);
pyo3::create_exception!(_eggress, UseAfterCloseError, EggressError);
pyo3::create_exception!(_eggress, UdpAssociationError, EggressError);
pyo3::create_exception!(_eggress, UnsupportedCompositionError, EggressError);

pub(crate) fn map_error(_py: Python<'_>, err: eggress_embed::EggressError) -> PyErr {
    use eggress_embed::EggressError as E;
    let msg = err.to_string();
    match err {
        E::Config(_) => ConfigError::new_err(msg),
        E::Runtime(_) => InternalError::new_err(msg),
        E::Startup(_) => StartupError::new_err(msg),
        E::Reload(_) => ReloadError::new_err(msg),
        E::Shutdown(_) => ShutdownError::new_err(msg),
        E::UnsupportedFeature { .. } => UnsupportedFeatureError::new_err(msg),
        E::Internal(_) => InternalError::new_err(msg),
    }
}
