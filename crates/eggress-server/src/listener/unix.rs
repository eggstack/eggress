use std::fmt;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use tokio::io;
use tokio::net::UnixStream;

/// Errors that can occur with Unix domain socket listeners.
#[derive(Debug)]
pub enum UnixListenerError {
    /// An I/O error occurred.
    Io(std::io::Error),
    /// The socket path is not valid (e.g., parent directory does not exist).
    InvalidPath(String),
}

impl fmt::Display for UnixListenerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnixListenerError::Io(e) => write!(f, "I/O error: {e}"),
            UnixListenerError::InvalidPath(msg) => write!(f, "invalid socket path: {msg}"),
        }
    }
}

impl std::error::Error for UnixListenerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UnixListenerError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for UnixListenerError {
    fn from(e: std::io::Error) -> Self {
        UnixListenerError::Io(e)
    }
}

/// Information about an accepted Unix domain socket connection.
#[derive(Debug)]
pub struct AcceptedConnection {
    /// The accepted stream.
    pub stream: UnixStream,
    /// The abstract namespace ID, if the socket is in the Linux abstract namespace.
    pub abstract_id: Option<u8>,
}

/// Unix domain socket listener.
///
/// Wraps `tokio::net::UnixListener` with lifecycle management: tracks whether
/// the socket was created and provides cleanup to remove it on shutdown.
///
/// Socket cleanup is performed explicitly via [`cleanup()`](Self::cleanup).
/// No automatic cleanup is performed on drop to avoid surprising side effects;
/// callers that need RAII-style cleanup should call `cleanup()` in a `Drop`
/// implementation or a shutdown handler.
pub struct UnixListener {
    inner: tokio::net::UnixListener,
    path: PathBuf,
    created_socket: bool,
}

impl UnixListener {
    /// Binds a new Unix domain socket listener.
    ///
    /// If `unlink_existing` is `true` and a Unix socket file already exists
    /// at `path`, it is removed before binding. To avoid surprising or
    /// destructive behavior, `unlink_existing=true` will refuse to remove
    /// anything other than a Unix socket (regular files, directories,
    /// symlinks, and special device files are all preserved). When
    /// `unlink_existing=false` and any file exists at `path`, binding fails.
    pub fn bind(path: &Path, unlink_existing: bool) -> Result<Self, UnixListenerError> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(UnixListenerError::InvalidPath(format!(
                    "parent directory '{}' does not exist",
                    parent.display()
                )));
            }
        }

        if path.exists() || path.symlink_metadata().is_ok() {
            // Distinguish "a socket file lives here" from "something else
            // lives here and we should not destroy it".
            let meta = std::fs::symlink_metadata(path).map_err(|e| {
                UnixListenerError::Io(std::io::Error::other(format!(
                    "failed to stat '{}': {}",
                    path.display(),
                    e
                )))
            })?;
            let file_type = meta.file_type();
            let is_socket = file_type.is_socket();
            let is_symlink = file_type.is_symlink();
            if is_socket {
                if unlink_existing {
                    std::fs::remove_file(path).map_err(|e| {
                        UnixListenerError::Io(std::io::Error::other(format!(
                            "failed to unlink existing socket '{}': {}",
                            path.display(),
                            e
                        )))
                    })?;
                } else {
                    return Err(UnixListenerError::Io(std::io::Error::other(format!(
                        "socket file '{}' already exists and unlink_existing=false",
                        path.display()
                    ))));
                }
            } else if is_symlink {
                return Err(UnixListenerError::Io(std::io::Error::other(format!(
                    "refusing to unlink symlink at '{}' (target type not verified)",
                    path.display()
                ))));
            } else {
                return Err(UnixListenerError::Io(std::io::Error::other(format!(
                    "refusing to unlink non-socket path '{}' (file_type is not a socket)",
                    path.display()
                ))));
            }
        }

        let listener = tokio::net::UnixListener::bind(path)?;
        Ok(Self {
            inner: listener,
            path: path.to_path_buf(),
            created_socket: true,
        })
    }

    /// Wraps an existing `tokio::net::UnixListener` without taking ownership of cleanup.
    pub fn from_tokio(listener: tokio::net::UnixListener, path: PathBuf) -> Self {
        Self {
            inner: listener,
            path,
            created_socket: false,
        }
    }

    /// Accepts an incoming connection.
    pub async fn accept(&self) -> Result<(UnixStream, tokio::net::unix::SocketAddr), io::Error> {
        self.inner.accept().await
    }

    /// Returns the path this listener is bound to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the local address (path) of this listener.
    pub fn local_addr(&self) -> Result<tokio::net::unix::SocketAddr, io::Error> {
        self.inner.local_addr()
    }

    /// Removes the socket file if this listener created it.
    ///
    /// This is idempotent: calling `cleanup()` multiple times is safe.
    /// After cleanup, the listener can still be used for accepting connections
    /// (the OS keeps the socket alive while the fd is open), but no new
    /// clients will be able to connect.
    pub fn cleanup(&self) -> Result<(), std::io::Error> {
        if self.created_socket && self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    /// Returns a reference to the underlying `tokio::net::UnixListener`.
    pub fn inner(&self) -> &tokio::net::UnixListener {
        &self.inner
    }
}

/// Configuration for creating a Unix domain socket listener.
#[derive(Debug, Clone)]
pub struct UnixListenerConfig {
    /// The filesystem path for the socket.
    pub path: PathBuf,
    /// Whether to remove an existing socket file before binding.
    pub unlink_existing: bool,
    /// File permissions to set after binding (e.g., `0o666`). `None` keeps defaults.
    pub mode: Option<u32>,
}

impl UnixListenerConfig {
    /// Creates a `UnixListenerConfig` from a path with default settings.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            unlink_existing: true,
            mode: None,
        }
    }

    /// Creates a `UnixListenerConfig` from a compiled configuration.
    pub fn from_compiled(path: &Path, unlink_existing: bool, mode: Option<u32>) -> Self {
        Self {
            path: path.to_path_buf(),
            unlink_existing,
            mode,
        }
    }
}

/// Builds a `UnixListener` from configuration.
pub fn create_unix_listener(
    config: &UnixListenerConfig,
) -> Result<UnixListener, UnixListenerError> {
    let listener = UnixListener::bind(&config.path, config.unlink_existing)?;

    if let Some(mode) = config.mode {
        set_permissions(&config.path, mode)?;
    }

    Ok(listener)
}

/// Sets file permissions on the socket path.
fn set_permissions(path: &Path, mode: u32) -> Result<(), UnixListenerError> {
    use std::os::unix::fs::PermissionsExt;

    let perms = std::fs::Permissions::from_mode(mode);
    std::fs::set_permissions(path, perms).map_err(|e| {
        UnixListenerError::Io(std::io::Error::other(format!(
            "failed to set permissions on '{}': {}",
            path.display(),
            e
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_socket_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("eggress_unix_test");
        std::fs::create_dir_all(&dir).ok();
        dir.join(format!("{}.sock", name))
    }

    #[tokio::test]
    async fn test_unix_listener_bind_and_accept() {
        let path = temp_socket_path("bind_accept");
        let _ = std::fs::remove_file(&path);

        let listener = UnixListener::bind(&path, true).unwrap();
        assert!(path.exists());

        let connect_path = path.clone();
        let connect_jh =
            tokio::spawn(async move { tokio::net::UnixStream::connect(&connect_path).await });

        let (stream, _addr) = listener.accept().await.unwrap();
        drop(stream);

        let _ = connect_jh.await.unwrap();

        listener.cleanup().unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_unix_listener_cleanup_idempotent() {
        let path = temp_socket_path("cleanup_idempotent");
        let _ = std::fs::remove_file(&path);

        let listener = UnixListener::bind(&path, true).unwrap();
        assert!(path.exists());

        listener.cleanup().unwrap();
        assert!(!path.exists());

        // Calling cleanup again should not fail
        listener.cleanup().unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_unix_listener_unlink_existing_socket() {
        // Stale *socket* files should be replaced when unlink_existing=true.
        let path = temp_socket_path("unlink_existing_socket");
        let _ = std::fs::remove_file(&path);

        let stale = UnixListener::bind(&path, true).unwrap();
        // Explicit cleanup because UnixListener does not auto-clean on drop.
        stale.cleanup().unwrap();
        assert!(!path.exists(), "stale listener should be cleaned up");

        // Re-bind: there is no file now, so this should just succeed.
        let listener = UnixListener::bind(&path, true).unwrap();
        assert!(path.exists());

        listener.cleanup().unwrap();
    }

    #[tokio::test]
    async fn test_unix_listener_refuses_to_unlink_regular_file() {
        // Refuse to unlink a regular file even if unlink_existing=true.
        let path = temp_socket_path("regular_file");
        let _ = std::fs::remove_file(&path);

        std::fs::write(&path, "important data").unwrap();
        let result = UnixListener::bind(&path, true);
        assert!(result.is_err(), "must not unlink regular files");
        // The file must still exist and be intact.
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "important data");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_unix_listener_refuses_to_unlink_symlink() {
        // Refuse to follow/remove symlinks; operator should resolve manually.
        let real_path = temp_socket_path("symlink_target");
        let link_path = temp_socket_path("symlink_link");
        let _ = std::fs::remove_file(&real_path);
        let _ = std::fs::remove_file(&link_path);

        std::fs::write(&real_path, "real data").unwrap();
        std::os::unix::fs::symlink(&real_path, &link_path).unwrap();

        let result = UnixListener::bind(&link_path, true);
        assert!(result.is_err(), "must not unlink symlinks");

        // Both must still exist.
        assert!(real_path.exists());
        assert!(link_path.exists());

        let _ = std::fs::remove_file(&real_path);
        let _ = std::fs::remove_file(&link_path);
    }

    #[tokio::test]
    async fn test_unix_listener_unlink_false_fails_when_socket_present() {
        // With unlink_existing=false, refuse to bind when a socket already exists.
        let path = temp_socket_path("unlink_false_existing");
        let _ = std::fs::remove_file(&path);

        let first = UnixListener::bind(&path, true).unwrap();
        let result = UnixListener::bind(&path, false);
        assert!(
            result.is_err(),
            "must refuse to bind without unlink when socket present"
        );

        drop(first);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_unix_listener_error_display() {
        let err = UnixListenerError::InvalidPath("missing parent".into());
        assert_eq!(err.to_string(), "invalid socket path: missing parent");
    }

    #[test]
    fn test_unix_listener_error_is_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<UnixListenerError>();
    }

    #[test]
    fn test_unix_listener_config_defaults() {
        let config = UnixListenerConfig::new("/tmp/test.sock");
        assert_eq!(config.path, PathBuf::from("/tmp/test.sock"));
        assert!(config.unlink_existing);
        assert_eq!(config.mode, None);
    }

    #[test]
    fn test_unix_listener_config_from_compiled() {
        let config = UnixListenerConfig::from_compiled(
            Path::new("/run/eggress/proxy.sock"),
            false,
            Some(0o666),
        );
        assert_eq!(config.path, PathBuf::from("/run/eggress/proxy.sock"));
        assert!(!config.unlink_existing);
        assert_eq!(config.mode, Some(0o666));
    }
}
