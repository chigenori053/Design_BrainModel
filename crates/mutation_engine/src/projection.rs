use serde::{Deserialize, Serialize};

use crate::model::{MutationApplyRecord, MutationPreview, MutationValidation, ValidationStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationProjectionState {
    pub mutation_id: String,
    pub stage: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationValidationProjection {
    pub status: String,
    pub violations: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationReplayProjection {
    pub step: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationRollbackProjection {
    pub ready: bool,
    pub affected_files: usize,
}

impl From<&MutationValidation> for MutationValidationProjection {
    fn from(value: &MutationValidation) -> Self {
        Self {
            status: match value.status {
                ValidationStatus::Passed => "Passed",
                ValidationStatus::Rejected => "Rejected",
            }
            .to_string(),
            violations: value.violations.len(),
        }
    }
}

impl From<&MutationPreview> for MutationProjectionState {
    fn from(value: &MutationPreview) -> Self {
        Self {
            mutation_id: value.mutation_id.clone(),
            stage: "Preview".to_string(),
            summary: format!("{} affected files", value.files.len()),
        }
    }
}

impl From<&MutationApplyRecord> for MutationRollbackProjection {
    fn from(value: &MutationApplyRecord) -> Self {
        Self {
            ready: true,
            affected_files: value.after.len(),
        }
    }
}
