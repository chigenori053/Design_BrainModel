use std::path::{Path, PathBuf};

pub mod bridge;
pub mod explain;
pub mod planner;
pub mod rationality;
pub mod search;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApiChange {
    pub crate_name: String,
    pub surface: String,
    pub change: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignGraph {
    pub workspace_root: PathBuf,
    pub crates: Vec<CrateNode>,
    pub dependency_edges: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CrateNode {
    pub name: String,
    pub manifest_path: PathBuf,
    pub internal_dependencies: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignDelta {
    pub workspace_root: PathBuf,
    pub impacted_crates: Vec<String>,
    pub introduced_interfaces: Vec<String>,
    pub dependency_moves: Vec<(String, String)>,
    pub api_changes: Vec<ApiChange>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MutationPlan {
    pub delta: DesignDelta,
    pub target_files: Vec<PathBuf>,
    pub expected_tests: Vec<String>,
    pub rollback_units: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum MutationStrategy {
    #[default]
    Conservative,
    TraitExtraction,
    AdapterInsertion,
    CrateSplit,
    InterfacePromotion,
    DependencyInversion,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MutationCandidate {
    pub id: String,
    pub plan: MutationPlan,
    pub expected_score: Option<RationalityScore>,
    pub strategy: MutationStrategy,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MutationSearchResult {
    pub candidates: Vec<MutationCandidate>,
    pub selected: Option<MutationCandidate>,
    pub rejected: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeoffPoint {
    pub dimension: String,
    pub selected_score: f32,
    pub rejected_score: f32,
    pub explanation: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeoffExplanation {
    pub selected_id: String,
    pub rejected_ids: Vec<String>,
    pub rationale_summary: String,
    pub tradeoff_points: Vec<TradeoffPoint>,
    pub adoption_reason: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RationalityScore {
    pub maintainability: f32,
    pub extensibility: f32,
    pub risk: f32,
    pub boundary_integrity: f32,
    pub rollback_complexity: f32,
    pub total: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodingPatchPlan {
    pub target_files: Vec<PathBuf>,
    pub expected_tests: Vec<String>,
    pub follow_up_steps: Vec<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesignDeltaLoopOutput {
    pub baseline: DesignGraph,
    pub delta: DesignDelta,
    pub mutation_plan: MutationPlan,
    pub patch_plan: CodingPatchPlan,
    pub rationality: RationalityScore,
    pub accepted: bool,
    pub retries: u32,
    pub search_result: Option<MutationSearchResult>,
}

pub const RATIONALITY_THRESHOLD: f32 = 0.82;
pub const SEARCH_RATIONALITY_THRESHOLD: f32 = 0.84;

pub fn discover_workspace_graph(workspace_root: &Path) -> Result<DesignGraph, String> {
    let manifests = planner::discover_cargo_manifests(workspace_root)?;
    let crate_names = manifests
        .iter()
        .filter_map(|manifest| planner::manifest_package_name(manifest).ok().flatten())
        .collect::<Vec<_>>();

    let crates = manifests
        .into_iter()
        .filter_map(|manifest| {
            let name = planner::manifest_package_name(&manifest).ok().flatten()?;
            let internal_dependencies =
                planner::manifest_internal_dependencies(&manifest, &crate_names)
                    .unwrap_or_default();
            Some(CrateNode {
                name,
                manifest_path: manifest,
                internal_dependencies,
            })
        })
        .collect::<Vec<_>>();

    let dependency_edges = crates
        .iter()
        .flat_map(|krate| {
            krate
                .internal_dependencies
                .iter()
                .map(|dep| (krate.name.clone(), dep.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    Ok(DesignGraph {
        workspace_root: workspace_root.to_path_buf(),
        crates,
        dependency_edges,
    })
}

pub fn run_reasoning_loop(
    workspace_root: &Path,
    spec: &str,
) -> Result<DesignDeltaLoopOutput, String> {
    let baseline = discover_workspace_graph(workspace_root)?;
    let delta = planner::extract_design_delta(&baseline, spec);

    let primary = planner::build_mutation_plan(&baseline, &delta, spec, false);
    let primary_score = rationality::score_mutation_plan(&baseline, &primary);
    if primary_score.total >= RATIONALITY_THRESHOLD {
        let patch_plan = bridge::to_coding_patch_plan(&primary);
        return Ok(DesignDeltaLoopOutput {
            baseline,
            delta,
            mutation_plan: primary,
            patch_plan,
            rationality: primary_score,
            accepted: true,
            retries: 0,
            search_result: None,
        });
    }

    let fallback = planner::build_mutation_plan(&baseline, &delta, spec, true);
    let fallback_score = rationality::score_mutation_plan(&baseline, &fallback);
    let patch_plan = bridge::to_coding_patch_plan(&fallback);

    Ok(DesignDeltaLoopOutput {
        baseline,
        delta,
        mutation_plan: fallback,
        patch_plan,
        accepted: fallback_score.total >= RATIONALITY_THRESHOLD,
        rationality: fallback_score,
        retries: 1,
        search_result: None,
    })
}

pub fn run_alternative_search_loop(
    workspace_root: &Path,
    spec: &str,
) -> Result<DesignDeltaLoopOutput, String> {
    let baseline = discover_workspace_graph(workspace_root)?;
    let delta = planner::extract_design_delta(&baseline, spec);
    let search_result = search::search_mutations(&baseline, &delta, spec);
    let Some(selected) = search_result.selected.clone() else {
        let fallback = planner::build_mutation_plan(&baseline, &delta, spec, true);
        let fallback_score = rationality::score_mutation_plan(&baseline, &fallback);
        let patch_plan = bridge::to_coding_patch_plan(&fallback);
        return Ok(DesignDeltaLoopOutput {
            baseline,
            delta,
            mutation_plan: fallback,
            patch_plan,
            rationality: fallback_score.clone(),
            accepted: fallback_score.total >= SEARCH_RATIONALITY_THRESHOLD,
            retries: search_result.rejected.len() as u32,
            search_result: Some(search_result),
        });
    };

    let rationality = selected
        .expected_score
        .clone()
        .unwrap_or_else(|| rationality::score_mutation_plan(&baseline, &selected.plan));
    let patch_plan = bridge::to_coding_patch_plan(&selected.plan);
    Ok(DesignDeltaLoopOutput {
        baseline,
        delta,
        mutation_plan: selected.plan,
        patch_plan,
        accepted: rationality.total >= SEARCH_RATIONALITY_THRESHOLD,
        rationality,
        retries: search_result.rejected.len() as u32,
        search_result: Some(search_result),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_workspace() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "design-delta-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("crates/core/src")).expect("mkdir core");
        fs::create_dir_all(root.join("crates/app/src")).expect("mkdir app");
        fs::write(
            root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"core\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
        )
        .expect("write core manifest");
        fs::write(
            root.join("crates/app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ncore = { path = \"../core\" }\n",
        )
        .expect("write app manifest");
        root
    }

    #[test]
    fn reasoning_loop_accepts_conservative_design_mutation() {
        let root = temp_workspace();
        let output = run_reasoning_loop(
            &root,
            "責務を崩さず trait 分離して crate 境界を維持して機能追加して",
        )
        .expect("reasoning");
        assert!(output.accepted);
        assert!(output.rationality.total >= RATIONALITY_THRESHOLD);
        assert!(!output.patch_plan.target_files.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reasoning_loop_retries_on_high_blast_radius_spec() {
        let root = temp_workspace();
        let output = run_reasoning_loop(
            &root,
            "workspace 全体を横断して public API をまとめて変更しつつ trait 分離して",
        )
        .expect("reasoning");
        assert_eq!(output.retries, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn alternative_search_loop_returns_ranked_candidates() {
        let root = temp_workspace();
        let output = run_alternative_search_loop(
            &root,
            "複数の設計変更案を比較して最適案で実装して trait分離案とadapter案を比較して",
        )
        .expect("search reasoning");
        let search = output.search_result.expect("search result");
        assert!(search.candidates.len() >= 4);
        assert!(search.selected.is_some());
        let _ = fs::remove_dir_all(root);
    }
}
