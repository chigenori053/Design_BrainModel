use std::sync::{Arc, Mutex};
use std::{fs, path::Path, path::PathBuf};

use agent_core::agent::{AgentContext, DesignAgent, LearningAgent, SearchAgent};
use agent_core::capability::{ScoringCapability, SearchCapability, SearchHit};
use agent_core::domain::{AgentInput, AgentRequest, DomainError, Hypothesis, Score, TelemetryEvent};
use agent_core::ports::{MemoryPort, TelemetryPort};
use agent_core::runtime::{AgentRegistry, Orchestrator};

#[derive(Default)]
struct InMemoryPort {
    items: Mutex<Vec<(String, Vec<u8>)>>,
}

impl MemoryPort for InMemoryPort {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, DomainError> {
        let items = self.items.lock().expect("memory mutex poisoned");
        Ok(items
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone()))
    }

    fn put(&self, key: &str, value: &[u8]) -> Result<(), DomainError> {
        self.items
            .lock()
            .expect("memory mutex poisoned")
            .push((key.to_string(), value.to_vec()));
        Ok(())
    }
}

#[derive(Default)]
struct TraceTelemetry {
    events: Arc<Mutex<Vec<TelemetryEvent>>>,
}

impl TelemetryPort for TraceTelemetry {
    fn emit(&self, event: &TelemetryEvent) {
        self.events
            .lock()
            .expect("telemetry mutex poisoned")
            .push(event.clone());
    }
}

struct FixedSearch;

impl SearchCapability for FixedSearch {
    fn search(&self, query: &str) -> Result<Vec<SearchHit>, DomainError> {
        Ok(vec![SearchHit {
            title: format!("hit for {query}"),
            snippet: "snippet".to_string(),
        }])
    }
}

struct LenScorer;

impl ScoringCapability for LenScorer {
    fn score(&self, hypothesis: &Hypothesis) -> Score {
        Score(hypothesis.content.len() as f64)
    }
}

#[test]
fn dynamic_registry_dispatches_to_target_agent() {
    let mut registry = AgentRegistry::new();
    registry.register(Box::new(SearchAgent::new(FixedSearch, LenScorer)));
    registry.register(Box::new(DesignAgent));

    let memory = InMemoryPort::default();
    let telemetry = TraceTelemetry::default();
    let ctx = AgentContext {
        memory: &memory,
        telemetry: &telemetry,
    };
    let mut orchestrator = Orchestrator::new(registry);

    let result = orchestrator.dispatch(
        AgentRequest {
            target: "search".to_string(),
            input: AgentInput {
                text: "phase3".to_string(),
                metadata: Default::default(),
            },
        },
        &ctx,
    );

    assert!(result.is_ok());
    let out = result.expect("dispatch should succeed");
    assert!(out.summary.contains("search completed"));
    assert_eq!(orchestrator.state().dispatch_count, 1);
    let persisted = memory
        .items
        .lock()
        .expect("memory mutex poisoned")
        .iter()
        .any(|(k, _)| k.starts_with("search/"));
    assert!(persisted);
    let telemetry_count = telemetry
        .events
        .lock()
        .expect("telemetry mutex poisoned")
        .len();
    assert!(telemetry_count >= 1);
}

#[test]
fn learning_agent_persists_via_event_processing() {
    let mut registry = AgentRegistry::new();
    registry.register(Box::new(LearningAgent));

    let memory = InMemoryPort::default();
    let telemetry = TraceTelemetry::default();
    let ctx = AgentContext {
        memory: &memory,
        telemetry: &telemetry,
    };
    let mut orchestrator = Orchestrator::new(registry);

    let _ = orchestrator
        .dispatch(
            AgentRequest {
                target: "learning".to_string(),
                input: AgentInput {
                    text: "knowledge-1".to_string(),
                    metadata: Default::default(),
                },
            },
            &ctx,
        )
        .expect("learning dispatch should succeed");

    let stored = memory
        .items
        .lock()
        .expect("memory mutex poisoned")
        .first()
        .cloned();
    assert!(stored.is_some());
}

#[test]
fn dependency_direction_domain_is_pure() {
    for file in rs_files_under("src/domain") {
        let body = fs::read_to_string(&file).expect("failed to read domain file");
        assert!(
            !body.contains("crate::adapters::"),
            "domain must not depend on adapters: {}",
            file.display()
        );
        assert!(
            !body.contains("std::fs"),
            "domain must not use std::fs: {}",
            file.display()
        );
        assert!(
            !body.contains("reqwest"),
            "domain must not use reqwest: {}",
            file.display()
        );
        assert!(
            !body.contains("serde_json"),
            "domain must not use serde_json: {}",
            file.display()
        );
    }
}

#[test]
fn dependency_direction_capability_does_not_depend_on_ports() {
    for file in rs_files_under("src/capability") {
        let body = fs::read_to_string(&file).expect("failed to read capability file");
        assert!(
            !body.contains("crate::ports::"),
            "capability must not depend on ports: {}",
            file.display()
        );
    }
}

#[test]
fn dependency_direction_agent_isolation_rules() {
    for file in rs_files_under("src/agent") {
        if file.ends_with("mod.rs") {
            continue;
        }
        let body = fs::read_to_string(&file).expect("failed to read agent file");
        assert!(
            !body.contains("crate::adapters::"),
            "agent must not directly depend on adapters: {}",
            file.display()
        );
        assert!(
            !body.contains("crate::agent::DesignAgent")
                && !body.contains("crate::agent::LearningAgent")
                && !body.contains("crate::agent::SearchAgent")
                && !body.contains("crate::agent::WebSearchAgent"),
            "agent must not directly reference another agent type: {}",
            file.display()
        );
    }
}

#[test]
fn lib_rs_has_no_direct_trace_io() {
    let lib_rs = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    let body = fs::read_to_string(lib_rs).expect("failed to read lib.rs");
    let prod_body = body.split("#[cfg(test)]").next().unwrap_or(&body);
    assert!(!prod_body.contains("std::fs"));
    assert!(!prod_body.contains("OpenOptions"));
    assert!(!prod_body.contains("impl BeamSearch"));
    assert!(!prod_body.contains("impl SystemEvaluator"));
    assert!(!prod_body.contains("fn compute_"));
    assert!(!prod_body.contains("fn moving_average"));
    assert!(!prod_body.contains("fn soft_"));
    assert!(!prod_body.contains("fn front_"));
    assert!(!prod_body.contains("fn distance_"));
    assert!(!prod_body.contains("fn objective_distance"));
    assert!(!prod_body.contains("fn dominance"));
    assert!(!prod_body.contains("fn pareto"));
    assert!(!prod_body.contains("fn jitter"));
    assert!(!prod_body.contains("fn quantize"));
    assert!(!prod_body.contains("SCORE_PRECISION"));
    assert!(!prod_body.contains("fn mean_"));
    assert!(!prod_body.contains("fn variance_"));
    let has_forbidden_normalize = prod_body
        .lines()
        .any(|line| line.contains("fn normalize_") && !line.contains("fn normalize_by_depth"));
    assert!(!has_forbidden_normalize);

    let has_allow = body.contains("ALLOW_LIB_LOOP:");
    let has_loop_token =
        body.contains("for ") || body.contains("while ") || body.contains("loop {");
    assert!(
        !has_loop_token || has_allow,
        "loop found in lib.rs without ALLOW_LIB_LOOP marker"
    );

    assert!(body.contains("pub fn generate_trace(config: TraceRunConfig) -> Vec<TraceRow> {"));
    assert!(body.contains("runtime::execute_trace(config)"));
    assert!(body.contains("pub fn run_bench(config: BenchConfig) -> BenchResult {"));
    assert!(body.contains("runtime::bench::run(config)"));
    assert!(body.contains(
        "pub(crate) fn normalize_by_depth(\n    candidates: Vec<(DesignState, ObjectiveVector)>,\n    alpha: f64,\n) -> (Vec<(DesignState, ObjectiveVector)>, GlobalRobustStats) {"
    ));
    assert!(body.contains("engine::normalization::normalize_by_depth_candidates(candidates, alpha)"));
    assert!(body.contains(
        "pub fn run_phase1_matrix(config: Phase1Config) -> (Vec<Phase1RawRow>, Vec<Phase1SummaryRow>) {"
    ));
    assert!(body.contains("runtime::phase1::run_phase1_matrix(config)"));
    assert!(!body.contains("run_phase1_matrix_impl("));
}

#[test]
fn no_legacy_markers_in_lib_runtime_capability() {
    for rel in ["src/lib.rs", "src/runtime", "src/capability"] {
        let files = if rel.ends_with(".rs") {
            vec![Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)]
        } else {
            rs_files_under(rel)
        };
        for file in files {
            let body = fs::read_to_string(&file).expect("failed to read source file");
            assert!(
                !body.contains("legacy_"),
                "legacy marker must not appear in {}",
                file.display()
            );
        }
    }
}

#[test]
fn no_compat_impl_references_in_runtime_capability_agent() {
    for rel in ["src/runtime", "src/capability", "src/agent"] {
        for file in rs_files_under(rel) {
            let body = fs::read_to_string(&file).expect("failed to read source file");
            assert!(
                !body.contains("compat_impl"),
                "compat_impl reference must not appear in {}",
                file.display()
            );
        }
    }
}

fn rs_files_under(rel: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let mut out = Vec::new();
    collect_rs_files(&root, &mut out);
    out
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    let entries = fs::read_dir(dir).expect("failed to read source dir");
    for entry in entries {
        let entry = entry.expect("failed to read dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}
