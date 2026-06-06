use sha2::{Digest, Sha256};

use crate::model::{
    AnalyzeContext, DependencyEdge, MutationOperation, MutationPlan, MutationTarget, PatchOperation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationRequest {
    pub target: MutationTarget,
    pub operation: MutationOperation,
    pub reason: String,
    pub expected_effect: String,
    pub patches: Vec<PatchOperation>,
    pub projected_dependencies: Option<Vec<DependencyEdge>>,
}

#[derive(Debug, Default)]
pub struct MutationPlanner;

impl MutationPlanner {
    pub fn plan(&self, analyze: &AnalyzeContext, request: MutationRequest) -> MutationPlan {
        let id = deterministic_id(analyze, &request);
        MutationPlan {
            id,
            target: request.target,
            operation: request.operation,
            reason: request.reason,
            expected_effect: request.expected_effect,
            patches: request.patches,
            design_intent: analyze.design_intent.clone(),
            projected_dependencies: request.projected_dependencies,
        }
    }
}

fn deterministic_id(analyze: &AnalyzeContext, request: &MutationRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{:?}", request.target));
    hasher.update(format!("{:?}", request.operation));
    hasher.update(request.reason.as_bytes());
    hasher.update(request.expected_effect.as_bytes());
    for patch in &request.patches {
        hasher.update(format!("{patch:?}"));
    }
    hasher.update(format!("{:?}", request.projected_dependencies));
    for intent in &analyze.design_intent {
        hasher.update(intent.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    format!("mut-{}", &digest[..16])
}
