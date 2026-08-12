use crate::apply::{ApplyEngine, decompose_plan};
use crate::rollback::RollbackEngine;
use crate::types::{ExecutionInput, ExecutionResult, RollbackInfo};
use crate::validate::ValidationEngine;

pub trait Executor {
    fn execute(&self, input: ExecutionInput) -> ExecutionResult;
}

/// IR-first executor: Plan(IR) → Step decomposition → Apply → Validate → Commit/Rollback.
/// All operations are transactional: validation failure triggers automatic rollback.
pub struct IrExecutor;

impl Executor for IrExecutor {
    fn execute(&self, input: ExecutionInput) -> ExecutionResult {
        let dry_run = input.context.dry_run;
        let steps = decompose_plan(&input.plan, input.context.max_steps);
        let apply_engine = ApplyEngine::new(dry_run);

        // Apply: each IR step maps 1:1 to an ExecutionStep (traceability contract)
        let applied: Vec<_> = steps
            .iter()
            .filter_map(|step| apply_engine.apply(step))
            .collect();

        // Validate: semantic + structural checks on the applied change set
        let validation = ValidationEngine::validate(&applied);

        // Rollback on failure; commit on success
        let rollback_info = if validation.success {
            RollbackInfo::committed(applied.len())
        } else {
            RollbackEngine::rollback(applied.clone(), dry_run)
        };

        ExecutionResult {
            applied_changes: applied,
            validation_result: validation,
            rollback_info,
            dry_run,
        }
    }
}
