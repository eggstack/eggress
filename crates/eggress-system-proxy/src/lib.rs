pub mod apply;
pub mod backends;
pub mod capability;
pub mod command_runner;
pub mod inspection;
pub mod redaction;

pub use apply::{
    apply_compatibility_proxy, apply_compatibility_proxy_with_runner, plan_apply, AppliedProxy,
    ApplyPlan, Command, CompatibilityProxyKind, RollbackState,
};
pub use capability::{
    check_system_proxy_capability, system_proxy_platform_info, SystemProxyCapability,
    SystemProxyCapabilityReport, SystemProxyStatus,
};
pub use command_runner::{CommandRunner, MockCommandRunner, RealCommandRunner};
pub use inspection::{inspect_system_proxy, InspectionResult, SystemProxySettings};
