use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::assoc::{UdpAssociation, UdpAssociationId};
use crate::error::UdpError;
use crate::limits::UdpLimits;

pub struct UdpAssociationRegistry {
    next_id: AtomicU64,
    associations: RwLock<HashMap<UdpAssociationId, Arc<UdpAssociation>>>,
    limits: UdpLimits,
}

impl UdpAssociationRegistry {
    pub fn new(limits: UdpLimits) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            associations: RwLock::new(HashMap::new()),
            limits,
        }
    }

    pub async fn create_association(
        &self,
        listener: &str,
        client_tcp_peer: SocketAddr,
        identity: eggress_core::ClientIdentity,
        generation: u64,
    ) -> Result<Arc<UdpAssociation>, UdpError> {
        let mut assocs = self.associations.write().await;
        if assocs.len() >= self.limits.max_associations_global {
            return Err(UdpError::AssociationLimitExceeded);
        }
        let listener_count = assocs
            .values()
            .filter(|a| a.meta.listener == listener)
            .count();
        if listener_count >= self.limits.max_associations_per_listener {
            return Err(UdpError::ListenerAssociationLimitExceeded);
        }

        let id = UdpAssociationId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let assoc = Arc::new(UdpAssociation::new(
            id,
            listener.to_string(),
            client_tcp_peer,
            identity,
            generation,
        ));

        assocs.insert(id, assoc.clone());
        Ok(assoc)
    }

    pub async fn remove(&self, id: UdpAssociationId) {
        self.associations.write().await.remove(&id);
    }

    pub async fn get(&self, id: UdpAssociationId) -> Option<Arc<UdpAssociation>> {
        self.associations.read().await.get(&id).cloned()
    }

    pub async fn active_count(&self) -> usize {
        self.associations.read().await.len()
    }

    pub async fn active_count_for_listener(&self, listener: &str) -> usize {
        self.associations
            .read()
            .await
            .values()
            .filter(|a| a.meta.listener == listener)
            .count()
    }

    pub async fn close_all(&self) {
        let assocs: Vec<Arc<UdpAssociation>> =
            self.associations.read().await.values().cloned().collect();
        for assoc in assocs {
            assoc.close();
        }
    }

    pub fn limits(&self) -> &UdpLimits {
        &self.limits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_addr() -> SocketAddr {
        "127.0.0.1:1080".parse().unwrap()
    }

    #[tokio::test]
    async fn create_and_get_association() {
        let registry = UdpAssociationRegistry::new(UdpLimits::default());
        let assoc = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        assert!(assoc.is_open());

        let fetched = registry.get(assoc.id).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, assoc.id);
    }

    #[tokio::test]
    async fn remove_association() {
        let registry = UdpAssociationRegistry::new(UdpLimits::default());
        let assoc = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        registry.remove(assoc.id).await;
        assert!(registry.get(assoc.id).await.is_none());
    }

    #[tokio::test]
    async fn global_limit_enforced() {
        let limits = UdpLimits {
            max_associations_global: 2,
            ..Default::default()
        };
        let registry = UdpAssociationRegistry::new(limits);
        registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        let result = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await;
        assert!(matches!(result, Err(UdpError::AssociationLimitExceeded)));
    }

    #[tokio::test]
    async fn per_listener_limit_enforced() {
        let limits = UdpLimits {
            max_associations_global: 100,
            max_associations_per_listener: 1,
            ..Default::default()
        };
        let registry = UdpAssociationRegistry::new(limits);
        registry
            .create_association(
                "listener-a",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        let result = registry
            .create_association(
                "listener-a",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await;
        assert!(matches!(
            result,
            Err(UdpError::ListenerAssociationLimitExceeded)
        ));
    }

    #[tokio::test]
    async fn per_listener_limit_allows_different_listeners() {
        let limits = UdpLimits {
            max_associations_global: 100,
            max_associations_per_listener: 1,
            ..Default::default()
        };
        let registry = UdpAssociationRegistry::new(limits);
        registry
            .create_association(
                "listener-a",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        let result = registry
            .create_association(
                "listener-b",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn active_count_tracks_correctly() {
        let registry = UdpAssociationRegistry::new(UdpLimits::default());
        assert_eq!(registry.active_count().await, 0);

        let a1 = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        assert_eq!(registry.active_count().await, 1);

        let a2 = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        assert_eq!(registry.active_count().await, 2);

        registry.remove(a1.id).await;
        assert_eq!(registry.active_count().await, 1);

        registry.remove(a2.id).await;
        assert_eq!(registry.active_count().await, 0);
    }

    #[tokio::test]
    async fn active_count_for_listener() {
        let registry = UdpAssociationRegistry::new(UdpLimits::default());
        registry
            .create_association(
                "listener-a",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        registry
            .create_association(
                "listener-a",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        registry
            .create_association(
                "listener-b",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();

        assert_eq!(registry.active_count_for_listener("listener-a").await, 2);
        assert_eq!(registry.active_count_for_listener("listener-b").await, 1);
        assert_eq!(registry.active_count_for_listener("listener-c").await, 0);
    }

    #[tokio::test]
    async fn close_all_closes_all_associations() {
        let registry = UdpAssociationRegistry::new(UdpLimits::default());
        let a1 = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        let a2 = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();

        registry.close_all().await;
        assert!(!a1.is_open());
        assert!(!a2.is_open());
    }

    #[tokio::test]
    async fn slot_released_after_remove() {
        let limits = UdpLimits {
            max_associations_global: 1,
            ..Default::default()
        };
        let registry = UdpAssociationRegistry::new(limits);
        let a1 = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        assert!(registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .is_err());

        registry.remove(a1.id).await;
        assert!(registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn association_id_increments() {
        let registry = UdpAssociationRegistry::new(UdpLimits::default());
        let a1 = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        let a2 = registry
            .create_association(
                "test-listener",
                test_addr(),
                eggress_core::ClientIdentity::Anonymous,
                1,
            )
            .await
            .unwrap();
        assert_eq!(a1.id.0 + 1, a2.id.0);
    }
}
