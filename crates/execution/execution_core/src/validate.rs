use crate::types::{AppliedChange, ChangeKind, ValidationResult};

pub struct ValidationEngine;

impl ValidationEngine {
    pub fn validate(changes: &[AppliedChange]) -> ValidationResult {
        let mut messages = Vec::new();

        // Semantic check: each step must map to exactly one IR module (1:1 traceability)
        let dup_ids = find_duplicate_step_ids(changes);
        if !dup_ids.is_empty() {
            messages.push(format!(
                "duplicate step_id(s) detected: {:?} — IR:Execution traceability broken",
                dup_ids
            ));
        }

        // Structural check: DependencyUpdate requires a preceding FileChange for the same module
        if !changes.is_empty() {
            let file_change_modules: std::collections::HashSet<u64> = changes
                .iter()
                .filter(|c| c.kind == ChangeKind::FileChange)
                .map(|c| c.ir_module_id)
                .collect();

            for change in changes {
                if change.kind == ChangeKind::DependencyUpdate
                    && !file_change_modules.contains(&change.ir_module_id)
                {
                    messages.push(format!(
                        "step {}: DependencyUpdate for module {} has no preceding FileChange",
                        change.step_id, change.ir_module_id
                    ));
                }
            }
        }

        if messages.is_empty() {
            ValidationResult::ok()
        } else {
            ValidationResult::failed(messages)
        }
    }
}

fn find_duplicate_step_ids(changes: &[AppliedChange]) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    let mut dups = Vec::new();
    for c in changes {
        if !seen.insert(c.step_id) {
            dups.push(c.step_id);
        }
    }
    dups.sort_unstable();
    dups.dedup();
    dups
}
