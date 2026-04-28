use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::nl::types::{ExecutionPlan, ExecutionStage};
use crate::nl::validation::ValidationResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiffSnapshot {
    pub summary: String,
    pub unified_diff: String,
    pub files_changed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub stage: ExecutionStage,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionState {
    pub plan_id: String,
    pub current_stage: ExecutionStage,
    pub diff: DiffSnapshot,
    pub validation: ValidationResult,
    pub history: Vec<ExecutionEvent>,
}

impl ExecutionState {
    pub fn new(plan_id: Uuid, plan: &ExecutionPlan) -> Self {
        Self {
            plan_id: plan_id.to_string(),
            current_stage: ExecutionStage::Plan,
            diff: DiffSnapshot::from_plan(plan),
            validation: ValidationResult::valid(),
            history: vec![ExecutionEvent {
                stage: ExecutionStage::Plan,
                message: "plan accepted".to_string(),
            }],
        }
    }

    pub fn advance(&mut self, stage: ExecutionStage, message: impl Into<String>) {
        self.current_stage = stage;
        self.history.push(ExecutionEvent {
            stage,
            message: message.into(),
        });
    }

    pub fn set_validation(&mut self, validation: ValidationResult) {
        self.validation = validation;
    }

    pub fn set_output_diff(&mut self, output: &str) {
        if let Some(diff) = output.split_once("[DIFF]\n").map(|(_, diff)| diff.trim()) {
            self.diff.unified_diff = diff.to_string();
            self.diff.summary = "execution diff".to_string();
        }
    }
}

impl DiffSnapshot {
    pub fn from_plan(plan: &ExecutionPlan) -> Self {
        let mut files_changed = Vec::new();
        if let Some(target) = &plan.target {
            files_changed.push(target.display().to_string());
        }
        Self {
            summary: format!("{:?}", plan.operation),
            unified_diff: String::new(),
            files_changed,
        }
    }
}
