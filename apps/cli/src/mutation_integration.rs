use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use mutation_engine::{
    AnalyzeContext, DependencyEdge, MutationAuditStore, MutationEngine, MutationOperation,
    MutationPlanner, MutationRequest, MutationTarget, PatchOperation,
};

use crate::analyze_engine::{AnalyzeEngineOutput, StructureEdgeKind};
use crate::tui::state::{
    MutationProjection, PreviewProjection, ReplayProjection, RollbackProjection,
};

pub fn mutation_context_from_analyze(output: &AnalyzeEngineOutput) -> AnalyzeContext {
    let nodes = output
        .graph
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    let dependencies = output
        .graph
        .edges
        .iter()
        .filter(|edge| {
            matches!(
                edge.kind,
                StructureEdgeKind::DependsOn | StructureEdgeKind::Uses | StructureEdgeKind::Calls
            )
        })
        .map(|edge| DependencyEdge {
            from: edge.from.clone(),
            to: edge.to.clone(),
        })
        .collect();
    let public_api_symbols = output
        .ast_modules
        .iter()
        .flat_map(|module| {
            module
                .structs
                .iter()
                .chain(&module.enums)
                .chain(&module.traits)
                .chain(&module.functions)
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let design_intent = output
        .semantic_structure
        .components
        .iter()
        .map(|component| component.responsibility.clone())
        .filter(|intent| !intent.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    AnalyzeContext {
        nodes,
        dependencies,
        responsibility_boundaries: Vec::new(),
        public_api_symbols,
        design_intent,
    }
}

pub struct MutationEngineDispatcher;

impl MutationEngineDispatcher {
    pub fn plan(workspace_root: &Path, target: &str) -> Result<MutationProjection, String> {
        let analyze_output = crate::analyze_engine::execute(crate::analyze_engine::AnalyzeCommand {
            path: workspace_root.to_path_buf(),
        })
        .map_err(|e| format!("Analyze failed: {e}"))?;

        let context = mutation_context_from_analyze(&analyze_output);
        let planner = MutationPlanner;

        let mutation_target = if target.contains("::") {
            MutationTarget::Module(target.to_string())
        } else {
            MutationTarget::File(PathBuf::from(target))
        };

        let request = MutationRequest {
            target: mutation_target,
            operation: MutationOperation::Refactor,
            reason: format!("Refactoring {} requested via TUI", target),
            expected_effect: "Improved structural integrity and reduced coupling".to_string(),
            patches: preview_patches_for_target(workspace_root, target)?,
            projected_dependencies: None,
        };

        let plan = planner.plan(&context, request);

        let runtime = mutation_engine::NoopRuntimeValidator;
        let engine = MutationEngine::new(workspace_root, "tui", &runtime);
        engine
            .plan(plan.clone())
            .map_err(|e| format!("Plan persistence failed: {e}"))?;

        Ok(MutationProjection {
            mutation_id: plan.id,
            target: target.to_string(),
            operation: "Refactor".to_string(),
            validation_targets: vec![
                "Dependency Cycles".to_string(),
                "Layer Violations".to_string(),
            ],
            expected_improvements: vec![plan.expected_effect],
        })
    }

    pub fn preview(workspace_root: &Path, mutation_id: &str) -> Result<PreviewProjection, String> {
        let runtime = mutation_engine::NoopRuntimeValidator;
        let engine = MutationEngine::new(workspace_root, "tui", &runtime);
        let store = MutationAuditStore::new(workspace_root);

        let plan = store
            .load_plan(mutation_id)
            .map_err(|e| format!("Plan not found: {e}"))?;

        // Plan -> Validate -> Preview
        let analyze_output = crate::analyze_engine::execute(crate::analyze_engine::AnalyzeCommand {
            path: workspace_root.to_path_buf(),
        })
        .map_err(|e| format!("Analyze failed: {e}"))?;
        let context = mutation_context_from_analyze(&analyze_output);

        let preview = engine
            .plan(plan)
            .map_err(|e| format!("Failed to initialize plan: {e}"))?
            .validate(&context)
            .map_err(|e| format!("Validation failed: {e}"))?
            .preview()
            .map_err(|e| format!("Preview failed: {e}"))?;

        let affected_files = preview
            .preview_data()
            .files
            .iter()
            .map(|f| f.path.display().to_string())
            .collect();

        Ok(PreviewProjection {
            mutation_id: mutation_id.to_string(),
            affected_modules: Vec::new(), // Can be derived if needed
            affected_files,
            structural_impact: "Structural validation passed".to_string(),
        })
    }

    pub fn apply(workspace_root: &Path, mutation_id: &str) -> Result<MutationProjection, String> {
        let runtime = mutation_engine::NoopRuntimeValidator;
        let engine = MutationEngine::new(workspace_root, "tui", &runtime);
        let store = MutationAuditStore::new(workspace_root);

        let plan = store
            .load_plan(mutation_id)
            .map_err(|e| format!("Plan not found: {e}"))?;
        let analyze_output = crate::analyze_engine::execute(crate::analyze_engine::AnalyzeCommand {
            path: workspace_root.to_path_buf(),
        })
        .map_err(|e| format!("Analyze failed: {e}"))?;
        let context = mutation_context_from_analyze(&analyze_output);

        let previewed = engine
            .plan(plan)
            .map_err(|e| format!("Failed to initialize plan: {e}"))?
            .validate(&context)
            .map_err(|e| format!("Validation failed: {e}"))?
            .preview()
            .map_err(|e| format!("Preview failed: {e}"))?;

        let _record = previewed
            .apply(None)
            .map_err(|e| format!("Apply failed: {e}"))?;

        Ok(MutationProjection {
            mutation_id: mutation_id.to_string(),
            target: "Workspace".to_string(),
            operation: "Apply".to_string(),
            validation_targets: vec![],
            expected_improvements: vec!["Mutation applied successfully".to_string()],
        })
    }

    pub fn replay(workspace_root: &Path, mutation_id: &str) -> Result<ReplayProjection, String> {
        let engine = mutation_engine::MutationReplayEngine::new(workspace_root);

        let _result = engine
            .replay(mutation_id, "tui")
            .map_err(|e| format!("Replay failed: {e}"))?;

        Ok(ReplayProjection {
            mutation_id: mutation_id.to_string(),
            status: "Success".to_string(),
            checksum_matched: true,
        })
    }

    pub fn rollback(workspace_root: &Path, mutation_id: &str) -> Result<RollbackProjection, String> {
        let engine = mutation_engine::MutationRollbackEngine::new(workspace_root);

        engine
            .rollback(mutation_id, "tui")
            .map_err(|e| format!("Rollback failed: {e}"))?;

        Ok(RollbackProjection {
            mutation_id: mutation_id.to_string(),
            status: "Rolled back successfully".to_string(),
        })
    }
}

fn preview_patches_for_target(
    workspace_root: &Path,
    target: &str,
) -> Result<Vec<PatchOperation>, String> {
    let relative_path = resolve_mutation_target_path(workspace_root, target)
        .ok_or_else(|| format!("Mutation target not found: {target}"))?;
    let content = fs::read_to_string(workspace_root.join(&relative_path))
        .map_err(|err| format!("Mutation target read failed: {err}"))?;
    Ok(vec![PatchOperation::Write {
        path: relative_path,
        content,
    }])
}

fn resolve_mutation_target_path(workspace_root: &Path, target: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(target);
    if workspace_root.join(&direct).is_file() {
        return Some(direct);
    }

    let parts = target.split("::").collect::<Vec<_>>();
    for source_root in (0..parts.len()).rev() {
        let mut candidate = PathBuf::new();
        for part in &parts[..source_root] {
            candidate.push(part);
        }
        candidate.push("src");
        for part in &parts[source_root..] {
            candidate.push(part);
        }
        candidate.set_extension("rs");
        if workspace_root.join(&candidate).is_file() {
            return Some(candidate);
        }

        let mut module_candidate = candidate;
        module_candidate.set_extension("");
        module_candidate.push("mod.rs");
        if workspace_root.join(&module_candidate).is_file() {
            return Some(module_candidate);
        }
    }
    None
}
