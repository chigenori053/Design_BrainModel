use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub type MutationId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationPlan {
    pub id: MutationId,
    pub target: MutationTarget,
    pub operation: MutationOperation,
    pub reason: String,
    pub expected_effect: String,
    pub patches: Vec<PatchOperation>,
    #[serde(default)]
    pub design_intent: Vec<String>,
    #[serde(default)]
    pub projected_dependencies: Option<Vec<DependencyEdge>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperation {
    Create,
    Update,
    Delete,
    Move,
    Rename,
    Refactor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum MutationTarget {
    File(PathBuf),
    Module(String),
    Struct(String),
    Enum(String),
    Trait(String),
    Function(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum PatchOperation {
    Write { path: PathBuf, content: String },
    Delete { path: PathBuf },
    Move { from: PathBuf, to: PathBuf },
}

impl PatchOperation {
    pub fn affected_paths(&self) -> Vec<&PathBuf> {
        match self {
            Self::Write { path, .. } | Self::Delete { path } => vec![path],
            Self::Move { from, to } => vec![from, to],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzeContext {
    pub nodes: Vec<String>,
    pub dependencies: Vec<DependencyEdge>,
    pub responsibility_boundaries: Vec<ResponsibilityBoundary>,
    pub public_api_symbols: Vec<String>,
    pub design_intent: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsibilityBoundary {
    pub owner: String,
    pub allowed_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationLayer {
    Structural,
    Semantic,
    Runtime,
    Security,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Passed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationViolation {
    pub layer: ValidationLayer,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCheckResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationValidation {
    pub mutation_id: MutationId,
    pub status: ValidationStatus,
    pub violations: Vec<ValidationViolation>,
    pub runtime_checks: Vec<RuntimeCheckResult>,
    pub confirmation_required: bool,
}

impl MutationValidation {
    pub fn passed(&self) -> bool {
        self.status == ValidationStatus::Passed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: PathBuf,
    pub unified_diff: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationPreview {
    pub mutation_id: MutationId,
    pub files: Vec<FileDiff>,
    pub confirmation_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationApplyRecord {
    pub mutation_id: MutationId,
    pub applied_at: u64,
    pub operations: Vec<PatchOperation>,
    pub before: Vec<FileSnapshot>,
    pub after: Vec<FileSnapshot>,
    pub verification: Vec<RuntimeCheckResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationReplayRecord {
    pub mutation_id: MutationId,
    pub timestamp: u64,
    pub operations: Vec<PatchOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Applied,
    Failed,
    Rejected,
    RolledBack,
    Replayed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationAudit {
    pub mutation_id: MutationId,
    pub who: String,
    pub when: u64,
    pub why: String,
    pub what: String,
    pub result: AuditResult,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationAuditLog {
    pub events: Vec<MutationAudit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirmation {
    pub mutation_id: MutationId,
    pub approved_by: String,
}
