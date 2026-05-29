//! Capability モジュール
//!
//! DBM-ANALYZE-CAPABILITY-ISOLATION v1.2 §1-§3 に基づく Capability Registry / Contract / Validator。
//!
//! # 設計方針
//!
//! Intent → Capability → Output を型システムと実行時検証の2層で保護する。
//! キーワードマッチや汎用 Generic Analyze Runtime による推測を禁止し、
//! 明示的なマッピングテーブルのみを許可する。

pub mod contract;
pub mod dispatcher;
pub mod registry;
pub mod validator;

pub use contract::{
    AnalyzeCodeCapability, AnalyzeMemoryCapability, AnalyzeProjectCapability,
    AnalyzeTestsCapability, CapabilityContract, CodeAnalysisResult, MemoryAnalysisResult,
    ProjectStructureAnalysisResult, TestInventoryResult,
};
pub use dispatcher::RuntimeAnalyzeDispatcher;
pub use registry::{CapabilityKind, CapabilityRegistry, CapabilityResolutionError};
pub use validator::{CapabilityMismatchError, OutputTypeId, RuntimeCapabilityValidator};
