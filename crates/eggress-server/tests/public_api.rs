//! Representative downstream-shaped compile contracts for `eggress-server`.
//!
//! Maintenance Phase 4: covers the published session/config/metrics seams
//! and executor/result paths used by downstream crates (type-level use only).

#[test]
fn server_seams_and_executor_paths_compile() {
    let _metrics = eggress_server::NoopMetrics;
    let _udp_handle = std::any::type_name::<eggress_server::UdpAssociationHandle>();
    assert!(!_udp_handle.is_empty());
    let _report = std::any::type_name::<eggress_server::SessionReport>();
    assert!(!_report.is_empty());
    let _config = std::any::type_name::<eggress_server::ConnectionConfig>();
    assert!(!_config.is_empty());
    let _context = std::any::type_name::<eggress_server::ConnectionContext>();
    assert!(!_context.is_empty());
    let _auth_cache = std::any::type_name::<eggress_server::accept::AuthReuseCache>();
    assert!(!_auth_cache.is_empty());
}
