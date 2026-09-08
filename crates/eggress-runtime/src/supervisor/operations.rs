//! Admin/operations integration for the runtime.
//!
//! [`RuntimeAdminListenerInfos`] implements `AdminSnapshotProvider` so admin
//! handlers read the live `ArcSwap<CompiledRuntimeSnapshot>` on every
//! request rather than a startup-captured copy.

use std::sync::Arc;

#[cfg(feature = "operations")]
use eggress_admin::{AdminSnapshot, AdminSnapshotProvider, ListenerInfo};

use crate::snapshot::CompiledRuntimeSnapshot;

use super::state::RuntimeState;

/// Adapter that exposes the runtime's compiled snapshot to the admin server.
///
/// Implements `AdminSnapshotProvider` so that admin handlers see live data:
/// each request reads the current `ArcSwap<CompiledRuntimeSnapshot>` rather
/// than a startup-captured copy. Reloads take effect on the next request.
#[cfg(feature = "operations")]
pub(crate) struct RuntimeAdminListenerInfos {
    pub(crate) state: Arc<RuntimeState>,
}

#[cfg(feature = "operations")]
pub(crate) struct RuntimeAdminState {
    pub(crate) snapshot: Arc<CompiledRuntimeSnapshot>,
    pub(crate) listener_addrs: Vec<Option<std::net::SocketAddr>>,
}

#[cfg(feature = "operations")]
impl AdminSnapshotProvider for RuntimeAdminListenerInfos {
    fn generation(&self) -> u64 {
        self.state.admin_snapshot.load().snapshot.generation
    }

    fn snapshot(&self) -> AdminSnapshot {
        let admin_state = self.state.admin_snapshot.load();
        let snap = &admin_state.snapshot;
        let addrs = &admin_state.listener_addrs;
        let listeners: Vec<ListenerInfo> = snap
            .listeners
            .iter()
            .enumerate()
            .map(|(idx, lcfg)| {
                let mode = if lcfg.transparent.as_ref().is_some_and(|t| t.enabled) {
                    Some("transparent".to_string())
                } else if lcfg.unix.is_some() {
                    Some("unix".to_string())
                } else {
                    Some("standard".to_string())
                };

                let (capability_status, original_dst_support) =
                    if lcfg.transparent.as_ref().is_some_and(|t| t.enabled) {
                        let cap = crate::platform::check_capability(
                            crate::platform::PlatformCapability::LinuxOriginalDstIpv4,
                        );
                        (
                            Some(cap.to_string()),
                            Some(cap == crate::platform::CapabilityStatus::Available),
                        )
                    } else {
                        (None, None)
                    };

                let (unix_socket_path, unix_socket_unlink_existing) =
                    if let Some(ref unix_cfg) = lcfg.unix {
                        (
                            Some(unix_cfg.path.display().to_string()),
                            Some(unix_cfg.unlink_existing),
                        )
                    } else {
                        (None, None)
                    };

                ListenerInfo {
                    name: lcfg.name.clone(),
                    bind: lcfg.bind.clone(),
                    local_addr: addrs
                        .get(idx)
                        .and_then(|a| *a)
                        .map(|a| a.to_string())
                        .or_else(|| unix_socket_path.clone())
                        .unwrap_or_default(),
                    protocols: lcfg.protocols.iter().map(|p| p.to_string()).collect(),
                    udp_enabled: lcfg.udp.as_ref().is_some_and(|u| u.enabled),
                    mode,
                    capability_status,
                    original_dst_support,
                    unix_socket_path,
                    unix_socket_unlink_existing,
                }
            })
            .collect();
        AdminSnapshot {
            generation: snap.generation,
            router: snap.router.clone(),
            pac: snap.admin.as_ref().and_then(|a| a.pac.clone()),
            static_routes: snap
                .admin
                .as_ref()
                .map(|a| a.static_content.clone())
                .unwrap_or_default(),
            listeners,
        }
    }
}
