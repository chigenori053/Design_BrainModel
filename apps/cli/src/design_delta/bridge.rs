use crate::nl::types::{ExecutionPlan, Operation, PlanSource};

use super::{CodingPatchPlan, MutationPlan};

pub fn to_coding_patch_plan(plan: &MutationPlan) -> CodingPatchPlan {
    let mut follow_up_steps = plan.expected_tests.clone();
    if !plan.delta.impacted_crates.is_empty() {
        follow_up_steps.push(format!(
            "commit-ready summary for {}",
            plan.delta.impacted_crates.join(", ")
        ));
    }

    CodingPatchPlan {
        target_files: plan.target_files.clone(),
        expected_tests: plan.expected_tests.clone(),
        follow_up_steps,
        summary: format!(
            "Design delta patch plan across {} target file(s)",
            plan.target_files.len()
        ),
    }
}

pub fn to_execution_plan(plan: &MutationPlan, request: &str) -> ExecutionPlan {
    ExecutionPlan::new(
        Operation::Refactor,
        Some(plan.delta.workspace_root.clone()),
        PlanSource::System,
    )
    .with_query(request)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::design_delta::{DesignDelta, MutationPlan};

    #[test]
    fn bridge_creates_patch_and_execution_plan() {
        use crate::nl::types::Operation;

        let plan = MutationPlan {
            delta: DesignDelta {
                workspace_root: PathBuf::from("."),
                impacted_crates: vec!["design_cli".to_string()],
                ..DesignDelta::default()
            },
            target_files: vec![PathBuf::from("apps/cli/Cargo.toml")],
            expected_tests: vec!["cargo test -p design_cli".to_string()],
            rollback_units: vec!["crate::design_cli".to_string()],
        };
        let patch = to_coding_patch_plan(&plan);
        let exec_plan = to_execution_plan(&plan, "trait 分離して");
        assert_eq!(patch.target_files.len(), 1);
        assert_eq!(exec_plan.operation, Operation::Refactor);
    }
}
