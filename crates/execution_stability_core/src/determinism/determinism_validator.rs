use crate::controller::execution_controller::ExecutionResult;
use crate::determinism::determinism_report::DeterminismReport;

#[derive(Clone, Debug, Default)]
pub struct DeterminismValidator;

impl DeterminismValidator {
    pub fn compare(&self, lhs: &ExecutionResult, rhs: &ExecutionResult) -> DeterminismReport {
        let left = comparable_view(lhs);
        let right = comparable_view(rhs);
        if left == right {
            DeterminismReport {
                is_deterministic: true,
                diff: None,
            }
        } else {
            DeterminismReport {
                is_deterministic: false,
                diff: Some(format!("lhs={left}\nrhs={right}")),
            }
        }
    }
}

fn comparable_view(result: &ExecutionResult) -> String {
    let steps = result
        .trace
        .steps
        .iter()
        .map(|step| {
            format!(
                "{}|{:?}|{}|{}|{}",
                step.step_name, step.command, step.success, step.stdout, step.stderr
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "success={};failure={:?};dep={:?};build={:?};run={:?};test={:?};steps={};snapshot={:?}",
        result.success,
        result.failure_type,
        result.dependency_result,
        result.build_result,
        result.run_result,
        result.test_result,
        steps,
        result.snapshot
    )
}
