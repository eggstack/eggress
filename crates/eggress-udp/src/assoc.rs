use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::error::UdpError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UdpAssociationId(pub u64);

#[derive(Debug)]
pub struct UdpAssociationMeta {
    pub id: UdpAssociationId,
    pub listener: String,
    pub client_tcp_peer: SocketAddr,
    pub client_udp_addr: std::sync::Mutex<Option<SocketAddr>>,
    pub identity: eggress_core::ClientIdentity,
    pub created_at: Instant,
    pub last_activity: std::sync::Mutex<Instant>,
    pub generation: u64,
}

pub struct UdpAssociation {
    pub id: UdpAssociationId,
    pub meta: Arc<UdpAssociationMeta>,
    pub state: AtomicBool,
    pub cancel: CancellationToken,
    pub closed_notify: Notify,
}

impl UdpAssociation {
    pub fn new(
        id: UdpAssociationId,
        listener: String,
        client_tcp_peer: SocketAddr,
        identity: eggress_core::ClientIdentity,
        generation: u64,
    ) -> Self {
        let meta = Arc::new(UdpAssociationMeta {
            id,
            listener,
            client_tcp_peer,
            client_udp_addr: std::sync::Mutex::new(None),
            identity,
            created_at: Instant::now(),
            last_activity: std::sync::Mutex::new(Instant::now()),
            generation,
        });
        Self {
            id,
            meta,
            state: AtomicBool::new(true),
            cancel: CancellationToken::new(),
            closed_notify: Notify::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.load(Ordering::Acquire)
    }

    pub fn close(&self) {
        if self.state.swap(false, Ordering::AcqRel) {
            self.cancel.cancel();
            self.closed_notify.notify_waiters();
        }
    }

    pub fn touch(&self) {
        if let Ok(mut last) = self.meta.last_activity.lock() {
            *last = Instant::now();
        }
    }

    pub fn last_activity(&self) -> Instant {
        *self
            .meta
            .last_activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    pub fn pin_client_addr(&self, addr: SocketAddr) -> Result<(), UdpError> {
        let mut stored = self
            .meta
            .client_udp_addr
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = *stored {
            if existing != addr {
                return Err(UdpError::ClientAddressMismatch);
            }
        } else {
            *stored = Some(addr);
        }
        Ok(())
    }

    pub fn client_udp_addr(&self) -> Option<SocketAddr> {
        *self
            .meta
            .client_udp_addr
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
}

impl std::fmt::Debug for UdpAssociation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdpAssociation")
            .field("id", &self.id)
            .field("state", &self.is_open())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_addr() -> SocketAddr {
        "127.0.0.1:1080".parse().unwrap()
    }

    #[test]
    fn association_id_increments() {
        let id1 = UdpAssociationId(1);
        let id2 = UdpAssociationId(2);
        assert_ne!(id1, id2);
        assert_eq!(id1.0 + 1, id2.0);
    }

    #[test]
    fn new_association_is_open() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        assert!(assoc.is_open());
        assert_eq!(assoc.id, UdpAssociationId(1));
    }

    #[test]
    fn close_releases_state() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        assert!(assoc.is_open());
        assoc.close();
        assert!(!assoc.is_open());
    }

    #[test]
    fn close_is_idempotent() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        assoc.close();
        assoc.close();
        assert!(!assoc.is_open());
    }

    #[test]
    fn touch_updates_activity() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        let before = assoc.last_activity();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assoc.touch();
        let after = assoc.last_activity();
        assert!(after >= before);
    }

    #[test]
    fn pin_client_addr_first_time() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        assert!(assoc.pin_client_addr(addr).is_ok());
        assert_eq!(assoc.client_udp_addr(), Some(addr));
    }

    #[test]
    fn pin_client_addr_same_addr_succeeds() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        assert!(assoc.pin_client_addr(addr).is_ok());
        assert!(assoc.pin_client_addr(addr).is_ok());
    }

    #[test]
    fn pin_client_addr_different_addr_fails() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        let addr1: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:5001".parse().unwrap();
        assert!(assoc.pin_client_addr(addr1).is_ok());
        assert!(matches!(
            assoc.pin_client_addr(addr2),
            Err(UdpError::ClientAddressMismatch)
        ));
    }

    #[test]
    fn close_cancel_is_triggered() {
        let assoc = UdpAssociation::new(
            UdpAssociationId(1),
            "test".to_string(),
            test_addr(),
            eggress_core::ClientIdentity::Anonymous,
            1,
        );
        assert!(!assoc.cancel.is_cancelled());
        assoc.close();
        assert!(assoc.cancel.is_cancelled());
    }
}
