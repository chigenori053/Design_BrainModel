//! # execution_hardening
//!
//! Phase C.5 hardening primitives for the DBM execution pipeline.
//!
//! ## Design Principles (spec §3)
//!
//! | Principle            | Guarantee                                              |
//! |----------------------|--------------------------------------------------------|
//! | Strong Determinism   | Same input → same side-effects, not just same output   |
//! | Atomic Execution     | All-success or all-failure; no intermediate state      |
//! | Full Traceability    | Every execution is replayable and auditable            |
//! | Isolation            | External environment influence is eliminated           |
//!
//! ## Module Overview
//!
//! - [`checksum`]  — blake3-based `Checksum` + `ExecutionTraceHash`
//! - [`sandbox`]   — `SandboxedCommand`: env_clear / stdin-null / arg pre-split
//! - [`effect`]    — `Effect` + `StagedEffectManager` (stage / commit / rollback)
//! - [`snapshot`]  — `StateSnapshot` (id + checksum + serialized data)
//! - [`trace`]     — `HardenedStepTrace` + `TraceWriter` (JSONL)
//! - [`replay`]    — `ReplayValidator` (trace hash comparison)
//! - [`error`]     — `HardeningError` (all Phase C.5 error variants)

pub mod checksum;
pub mod effect;
pub mod error;
pub mod real_execution_substrate;
pub mod replay;
pub mod sandbox;
pub mod snapshot;
pub mod trace;

// Convenience re-exports at crate root
pub use checksum::{Checksum, ChecksumBuilder, ExecutionTraceHash};
pub use effect::{Effect, StagedEffectManager};
pub use error::HardeningError;
pub use real_execution_substrate::{
    EnvironmentState, ExecutionSubstrateEngine, ExecutionTransaction, FilesystemMutation,
    GovernedProcess, RollbackSnapshot, VerificationExecutionResult,
};
pub use replay::ReplayValidator;
pub use sandbox::{SandboxedCommand, SandboxedOutput};
pub use snapshot::{SerializedState, SnapshotId, StateSnapshot};
pub use trace::{HardenedStepTrace, TraceWriter};
