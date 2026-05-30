#[path = "integration/runtime_bootstrap_isolation.rs"]
mod runtime_bootstrap_isolation;

#[path = "integration/constraint_enforcement.rs"]
mod constraint_enforcement;

#[path = "integration/policy_layer.rs"]
mod policy_layer;

#[cfg(feature = "integration-late")]
#[path = "integration/external_integration_late.rs"]
mod external_integration_late;
