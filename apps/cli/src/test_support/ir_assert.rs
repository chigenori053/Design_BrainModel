use uuid::Uuid;

use crate::ir::{
    IRPlanEventPayload, IRPlanEventRecord, IRTransactionArtifactRecord, IRTransitionRecord,
};
use crate::service::dto::ActionKind;

pub fn assert_plan_proposed(events: &[IRPlanEventRecord]) -> Uuid {
    events
        .iter()
        .find_map(|event| match &event.payload {
            IRPlanEventPayload::PlanProposed(payload) => Some(payload.plan_id),
            _ => None,
        })
        .expect("expected PlanProposed event")
}

pub fn assert_plan_accepted(events: &[IRPlanEventRecord], plan_id: Uuid) {
    assert!(
        events.iter().any(|event| matches!(
            event.payload,
            IRPlanEventPayload::PlanAccepted(ref payload) if payload.plan_id == plan_id
        )),
        "expected PlanAccepted event for plan_id={plan_id}"
    );
}

pub fn assert_execution_result(transitions: &[IRTransitionRecord], action_kind: ActionKind) {
    assert!(
        transitions
            .iter()
            .any(|transition| transition.action_kind == action_kind),
        "expected execution result transition for {:?}",
        action_kind
    );
}

pub fn assert_no_ir_bypass(logs: &[String]) {
    assert!(
        logs.iter()
            .all(|line| !line.contains("[WARN] IR bypass detected")),
        "unexpected IR bypass warning: {:?}",
        logs
    );
}

pub fn assert_artifact_recorded(
    artifacts: &[IRTransactionArtifactRecord],
    action_kind: ActionKind,
) {
    match action_kind {
        ActionKind::CodingPreview | ActionKind::Apply | ActionKind::Validate => {
            assert!(
                !artifacts.is_empty(),
                "expected artifact records for {:?}",
                action_kind
            );
        }
        ActionKind::Rollback | ActionKind::Analyze | ActionKind::Refactor => {
            panic!("artifact assertion not defined for {:?}", action_kind);
        }
    }
}
