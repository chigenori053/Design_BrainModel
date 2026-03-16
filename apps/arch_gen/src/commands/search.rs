use std::fs;
use std::path::{Path, PathBuf};

use architecture_ir::ComponentType;
use architecture_search::{
    ArchitectureGrammar, ArchitectureSearchEngine, IntentConstraints, IntentModel, SearchConfig,
};
use serde::{Deserialize, Serialize};

pub struct SearchArgs {
    pub spec_file: String,
    pub output_dir: String,
    pub beam_width: usize,
    pub max_depth: usize,
    pub max_candidates: usize,
    pub pareto_limit: usize,
    pub timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
struct SearchSpec {
    system_type: String,
    #[serde(default)]
    requirements: Vec<String>,
    #[serde(default)]
    constraints: SearchSpecConstraints,
    #[serde(default)]
    quality_attributes: Vec<String>,
    #[serde(default, alias = "domain_knowledge")]
    domain_context: Vec<String>,
    grammar_dsl: Option<String>,
    search_config: Option<SearchSpecConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct SearchSpecConfig {
    beam_width: Option<usize>,
    max_depth: Option<usize>,
    max_candidates: Option<usize>,
    pareto_limit: Option<usize>,
    timeout: Option<u64>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct SearchSpecConstraints {
    architecture: Option<String>,
    language: Option<String>,
    #[serde(default)]
    forbidden_components: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SearchArtifact {
    template_id: Option<String>,
    architecture: architecture_ir::ArchitectureIR,
    score: architecture_search::ArchitectureScore,
    depth: usize,
    component_count: usize,
    dependency_count: usize,
}

pub fn run(args: SearchArgs) -> Result<(), String> {
    let spec_path = Path::new(&args.spec_file);
    let spec_text = fs::read_to_string(spec_path)
        .map_err(|e| format!("failed to read spec '{}': {e}", spec_path.display()))?;
    let spec: SearchSpec =
        serde_yaml::from_str(&spec_text).map_err(|e| format!("invalid yaml spec: {e}"))?;
    let grammar_dsl = spec.grammar_dsl.clone();
    let config = resolve_search_config(&spec, &args);
    let intent = to_intent(spec)?;

    let engine = ArchitectureSearchEngine { config };
    let result = match grammar_dsl {
        Some(dsl) => engine.run_with_grammar(
            &intent,
            ArchitectureGrammar::from_dsl(&dsl)
                .map_err(|e| format!("invalid grammar_dsl: {e}"))?,
        ),
        None => engine.run(&intent),
    };

    let output_dir = PathBuf::from(&args.output_dir);
    fs::create_dir_all(&output_dir).map_err(|e| {
        format!(
            "failed to create output directory '{}': {e}",
            output_dir.display()
        )
    })?;

    for (index, candidate) in result.pareto_frontier.iter().enumerate() {
        let artifact = SearchArtifact {
            template_id: result
                .template_selection
                .as_ref()
                .map(|selection| selection.selected.template_id.clone()),
            architecture: candidate.architecture_ir.clone(),
            score: candidate.evaluation,
            depth: candidate.generation_step,
            component_count: candidate.architecture_ir.components.len(),
            dependency_count: candidate.architecture_ir.dependencies.len(),
        };
        let path = output_dir.join(format!("arch_{}.json", index + 1));
        let json = serde_json::to_string_pretty(&artifact)
            .map_err(|e| format!("failed to serialize search artifact: {e}"))?;
        fs::write(&path, json)
            .map_err(|e| format!("failed to write '{}': {e}", path.display()))?;
    }

    println!("Architecture Search Engine");
    println!("  system_type     : {}", intent.system_type);
    if let Some(selection) = &result.template_selection {
        println!("  template        : {}", selection.selected.template_id);
    }
    println!("  pareto_frontier : {}", result.pareto_frontier.len());
    println!("  explored_states : {}", result.telemetry.explored_states);
    println!("  search_depth    : {}", result.telemetry.search_depth);
    println!("  candidate_count : {}", result.telemetry.candidate_count);
    println!("  evaluation_ms   : {}", result.telemetry.evaluation_time_ms);
    println!("  output_dir      : {}", output_dir.display());

    Ok(())
}

fn to_intent(spec: SearchSpec) -> Result<IntentModel, String> {
    let forbidden_components = spec
        .constraints
        .forbidden_components
        .iter()
        .map(|item| parse_component_type(item))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(IntentModel {
        system_type: spec.system_type,
        requirements: spec.requirements,
        constraints: IntentConstraints {
            architecture: spec.constraints.architecture,
            language: spec.constraints.language,
            forbidden_components,
        },
        quality_attributes: spec.quality_attributes,
        domain_context: spec.domain_context,
    })
}

fn resolve_search_config(spec: &SearchSpec, args: &SearchArgs) -> SearchConfig {
    let file_config = spec.search_config.as_ref();
    SearchConfig {
        beam_width: file_config
            .and_then(|cfg| cfg.beam_width)
            .unwrap_or(args.beam_width),
        max_depth: file_config
            .and_then(|cfg| cfg.max_depth)
            .unwrap_or(args.max_depth),
        max_candidates: file_config
            .and_then(|cfg| cfg.max_candidates)
            .unwrap_or(args.max_candidates),
        pareto_limit: file_config
            .and_then(|cfg| cfg.pareto_limit)
            .unwrap_or(args.pareto_limit),
        timeout_ms: file_config
            .and_then(|cfg| cfg.timeout_ms.or(cfg.timeout))
            .unwrap_or(args.timeout_ms),
    }
}

fn parse_component_type(value: &str) -> Result<ComponentType, String> {
    match normalize_key(value).as_str() {
        "controller" => Ok(ComponentType::Controller),
        "service" => Ok(ComponentType::Service),
        "repository" => Ok(ComponentType::Repository),
        "adapter" => Ok(ComponentType::Adapter),
        "domainmodel" | "domain_model" => Ok(ComponentType::DomainModel),
        "usecase" | "use_case" => Ok(ComponentType::UseCase),
        "datamodel" | "data_model" => Ok(ComponentType::DataModel),
        other => Err(format!("unknown component type '{other}'")),
    }
}

fn normalize_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}
