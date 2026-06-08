use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mutation_engine::{
    AnalyzeContext, DependencyEdge, MutationAuditStore, MutationEngine, MutationOperation,
    MutationPlan, MutationPlanner, MutationPreview, MutationRequest, MutationTarget,
    PatchOperation,
};
use world_model::{
    MutationValidationGate, PredictionExplainer, PredictionResult, ValidationDecision,
};
use world_model_core::{Action, WorldState};

use design_domain::{Architecture, Dependency, DependencyKind, DesignUnit, DesignUnitId};

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
        let analyze_output =
            crate::analyze_engine::execute(crate::analyze_engine::AnalyzeCommand {
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

        let patches = preview_patches_for_target(workspace_root, target)?;
        let affected_files = affected_files_from_patches(&patches);
        let request = MutationRequest {
            target: mutation_target,
            operation: MutationOperation::Refactor,
            reason: format!("Refactoring {} requested via TUI", target),
            expected_effect: "Improved structural integrity and reduced coupling".to_string(),
            patches,
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
            affected_files,
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
        let analyze_output =
            crate::analyze_engine::execute(crate::analyze_engine::AnalyzeCommand {
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
        let analyze_output =
            crate::analyze_engine::execute(crate::analyze_engine::AnalyzeCommand {
                path: workspace_root.to_path_buf(),
            })
            .map_err(|e| format!("Analyze failed: {e}"))?;
        let context = mutation_context_from_analyze(&analyze_output);

        let previewed = engine
            .plan(plan.clone())
            .map_err(|e| format!("Failed to initialize plan: {e}"))?
            .validate(&context)
            .map_err(|e| format!("Validation failed: {e}"))?
            .preview()
            .map_err(|e| format!("Preview failed: {e}"))?;

        let prediction = predict_mutation_preview(&context, &plan, previewed.preview_data());
        let decision = MutationValidationGate::decide(&prediction);
        let explanation = PredictionExplainer::explain(&prediction);
        let narrative = MutationValidationGate::narrative(decision, &explanation);
        if decision == ValidationDecision::Reject {
            return Err(format!(
                "Mutation rejected by causal validation gate:\n{}",
                narrative
            ));
        }

        let _record = previewed
            .apply(None)
            .map_err(|e| format!("Apply failed: {e}"))?;

        let mut expected_improvements = vec!["Mutation applied successfully".to_string()];
        expected_improvements.push(narrative);
        expected_improvements.extend(prediction.warnings);

        Ok(MutationProjection {
            mutation_id: mutation_id.to_string(),
            target: "Workspace".to_string(),
            affected_files: affected_files_from_patches(&plan.patches),
            operation: "Apply".to_string(),
            validation_targets: vec![],
            expected_improvements,
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

    pub fn rollback(
        workspace_root: &Path,
        mutation_id: &str,
    ) -> Result<RollbackProjection, String> {
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

pub fn predict_mutation_preview(
    context: &AnalyzeContext,
    plan: &MutationPlan,
    preview: &MutationPreview,
) -> PredictionResult {
    let state = structure_world_state(context, plan, preview);
    MutationValidationGate::predict(&state)
}

fn structure_world_state(
    context: &AnalyzeContext,
    plan: &MutationPlan,
    preview: &MutationPreview,
) -> WorldState {
    let mut node_names = context.nodes.iter().cloned().collect::<BTreeSet<_>>();
    let dependencies = plan
        .projected_dependencies
        .as_ref()
        .unwrap_or(&context.dependencies);
    for dependency in dependencies {
        node_names.insert(dependency.from.clone());
        node_names.insert(dependency.to.clone());
    }

    let mut architecture = Architecture::seeded();
    let mut ids = BTreeMap::new();
    for (index, name) in node_names.into_iter().enumerate() {
        let id = index as u64 + 1;
        ids.insert(name.clone(), id);
        architecture.add_design_unit(DesignUnit::new(id, name));
    }
    for dependency in dependencies {
        let (Some(from), Some(to)) = (ids.get(&dependency.from), ids.get(&dependency.to)) else {
            continue;
        };
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(*from),
            to: DesignUnitId(*to),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((*from, *to));
    }

    let mut state = WorldState::from_architecture(0, architecture, Vec::new());
    state.features.push(preview.files.len() as f64);
    state.history.push(action_for(plan.operation));
    state
}

fn action_for(operation: MutationOperation) -> Action {
    match operation {
        MutationOperation::Create => Action::AddDesignUnit {
            name: "mutation_preview".to_string(),
            layer: design_domain::Layer::Service,
        },
        MutationOperation::Delete => Action::RemoveDesignUnit,
        MutationOperation::Move | MutationOperation::Refactor => Action::SplitStructure,
        MutationOperation::Rename | MutationOperation::Update => Action::MergeStructure,
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

fn affected_files_from_patches(patches: &[PatchOperation]) -> Vec<String> {
    patches
        .iter()
        .flat_map(|patch| patch.affected_paths())
        .map(|path| path.display().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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
