//! テレメトリモジュール
//!
//! DBM-ANALYZE-CAPABILITY-ISOLATION v1.2 §4 に基づく Capability 実行テレメトリ。

pub mod capability_telemetry;

pub use capability_telemetry::CapabilityExecutionTelemetry;
