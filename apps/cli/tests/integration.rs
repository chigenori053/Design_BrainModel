#[path = "integration/runtime_bootstrap_isolation.rs"]
mod runtime_bootstrap_isolation;

#[cfg(feature = "integration-late")]
#[path = "integration/external_integration_late.rs"]
mod external_integration_late;
