use crate::nl::types::{CodingOptions, CommandPlan, PlannedStep};

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

pub fn to_command_plan(plan: &MutationPlan, request: &str) -> CommandPlan {
    let mut steps = Vec::new();
    for path in &plan.target_files {
        steps.push(PlannedStep::Coding(
            path.clone(),
            CodingOptions {
                request: Some(request.to_string()),
                ..CodingOptions::default()
            },
        ));
        steps.push(PlannedStep::Validate(path.clone()));
    }
    if steps.is_empty() {
        steps.push(PlannedStep::Validate(std::path::PathBuf::from(".")));
    }
    steps.push(PlannedStep::GitCommit(plan.delta.workspace_root.clone()));
    steps.push(PlannedStep::GitPR(plan.delta.workspace_root.clone()));
    CommandPlan { steps }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::design_delta::{DesignDelta, MutationPlan};

    #[test]
    fn bridge_creates_patch_and_command_plan() {
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
        let commands = to_command_plan(&plan, "trait 分離して");
        assert_eq!(patch.target_files.len(), 1);
        assert_eq!(commands.steps.len(), 4);
    }
}
