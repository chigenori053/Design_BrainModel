use std::env;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("test") => run_test_suite(args.collect()),
        _ => {
            eprintln!("usage: cargo xtest <category>");
            eprintln!(
                "categories: fast, architecture, invariants, engine, knowledge-engine, contract, determinism, integration, runtime-heavy, stress, experiments, all"
            );
            ExitCode::from(2)
        }
    }
}

fn run_test_suite(args: Vec<String>) -> ExitCode {
    let Some(category) = args.first().map(String::as_str) else {
        eprintln!("missing category");
        return ExitCode::from(2);
    };

    let specs = match category {
        "fast" => concat_specs(&[
            architecture_specs(),
            invariants_specs(),
            engine_specs(),
            knowledge_specs(),
            contract_specs(),
            determinism_specs(),
            integration_specs(),
        ]),
        "architecture" => architecture_specs(),
        "invariants" => invariants_specs(),
        "engine" => engine_specs(),
        "knowledge-engine" => knowledge_specs(),
        "contract" => contract_specs(),
        "determinism" => determinism_specs(),
        "integration" => integration_specs(),
        "runtime-heavy" => runtime_heavy_specs(),
        "stress" => stress_specs(),
        "experiments" => experiments_specs(),
        "all" => concat_specs(&[
            architecture_specs(),
            invariants_specs(),
            engine_specs(),
            knowledge_specs(),
            contract_specs(),
            determinism_specs(),
            integration_specs(),
            runtime_heavy_specs(),
            stress_specs(),
            experiments_specs(),
        ]),
        other => {
            eprintln!("unknown category: {other}");
            return ExitCode::from(2);
        }
    };

    for spec in specs {
        if !run_spec(&spec) {
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

#[derive(Clone)]
struct TestSpec {
    name: &'static str,
    args: Vec<&'static str>,
}

fn concat_specs(groups: &[Vec<TestSpec>]) -> Vec<TestSpec> {
    groups.iter().flat_map(|group| group.clone()).collect()
}

fn run_spec(spec: &TestSpec) -> bool {
    eprintln!("==> {}", spec.name);
    match Command::new("cargo").args(&spec.args).status() {
        Ok(status) => status.success(),
        Err(err) => {
            eprintln!("failed to start `{}`: {err}", spec.name);
            false
        }
    }
}

fn cargo_test_args(args: &[&'static str]) -> Vec<&'static str> {
    let mut full = vec!["test"];
    full.extend_from_slice(args);
    full.extend_from_slice(&["--", "--test-threads=1"]);
    full
}

fn cargo_test_ignored_args(args: &[&'static str]) -> Vec<&'static str> {
    let mut full = vec!["test"];
    full.extend_from_slice(args);
    full.extend_from_slice(&["--", "--ignored", "--test-threads=1"]);
    full
}

fn spec(name: &'static str, args: Vec<&'static str>) -> TestSpec {
    TestSpec { name, args }
}

fn architecture_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "design_cli architecture_enforcement",
            cargo_test_args(&["-p", "design_cli", "--test", "architecture_enforcement"]),
        ),
        spec(
            "design_cli negative_cases",
            cargo_test_args(&["-p", "design_cli", "--test", "negative_cases"]),
        ),
        spec(
            "design_cli reasoning_engine",
            cargo_test_args(&["-p", "design_cli", "--test", "reasoning_engine"]),
        ),
    ]
}

fn invariants_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "architecture_domain invariants",
            cargo_test_args(&["-p", "architecture_domain", "--test", "invariants"]),
        ),
        spec(
            "design_search_engine invariants",
            cargo_test_args(&["-p", "design_search_engine", "--test", "invariants"]),
        ),
    ]
}

fn engine_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "design_search_engine engine",
            cargo_test_args(&["-p", "design_search_engine", "--test", "engine"]),
        ),
        spec(
            "evaluation_engine engine",
            cargo_test_args(&["-p", "evaluation_engine", "--test", "engine"]),
        ),
        spec(
            "memory_graph memory",
            cargo_test_args(&["-p", "memory_graph", "--test", "memory"]),
        ),
    ]
}

fn knowledge_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "knowledge_engine retrieval",
            cargo_test_args(&["-p", "knowledge_engine", "--test", "knowledge_retrieval"]),
        ),
        spec(
            "knowledge_engine parsing",
            cargo_test_args(&["-p", "knowledge_engine", "--test", "knowledge_parsing"]),
        ),
        spec(
            "knowledge_engine validation",
            cargo_test_args(&["-p", "knowledge_engine", "--test", "knowledge_validation"]),
        ),
        spec(
            "knowledge_engine reasoning integration",
            cargo_test_args(&[
                "-p",
                "knowledge_engine",
                "--test",
                "knowledge_reasoning_integration",
            ]),
        ),
        spec(
            "knowledge_engine search impact",
            cargo_test_args(&[
                "-p",
                "knowledge_engine",
                "--test",
                "knowledge_search_impact",
            ]),
        ),
    ]
}

fn contract_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "contract audit tests",
            cargo_test_args(&["-p", "contract_audit_tests"]),
        ),
        spec(
            "pipeline contract tests",
            cargo_test_args(&["-p", "pipeline_tests"]),
        ),
    ]
}

fn determinism_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "design_search_engine determinism",
            cargo_test_args(&["-p", "design_search_engine", "--test", "determinism"]),
        ),
        spec(
            "ai_context determinism",
            cargo_test_args(&["-p", "ai_context", "--test", "determinism"]),
        ),
        spec(
            "runtime_vm determinism",
            cargo_test_args(&["-p", "runtime_vm", "--test", "determinism"]),
        ),
    ]
}

fn integration_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "runtime_vm integration",
            cargo_test_args(&["-p", "runtime_vm", "--test", "integration"]),
        ),
        spec(
            "phase1_integration_tests concept pipeline",
            cargo_test_args(&[
                "-p",
                "phase1_integration_tests",
                "--test",
                "concept_pipeline",
            ]),
        ),
        spec(
            "phase1_integration_tests canonicalization",
            cargo_test_args(&[
                "-p",
                "phase1_integration_tests",
                "--test",
                "canonicalization",
            ]),
        ),
        spec(
            "phase1_integration_tests reasoning pipeline",
            cargo_test_args(&[
                "-p",
                "phase1_integration_tests",
                "--test",
                "reasoning_pipeline",
            ]),
        ),
    ]
}

fn runtime_heavy_specs() -> Vec<TestSpec> {
    vec![spec(
        "runtime_vm ignored tests",
        cargo_test_ignored_args(&["-p", "runtime_vm"]),
    )]
}

fn stress_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "reasoning_agent scaling",
            cargo_test_args(&["-p", "reasoning_agent", "--test", "scaling", "--release"]),
        ),
        spec(
            "agent_core heavy",
            cargo_test_args(&[
                "-p",
                "agent_core",
                "--test",
                "heavy",
                "--release",
                "--features",
                "ci-heavy",
            ]),
        ),
    ]
}

fn experiments_specs() -> Vec<TestSpec> {
    vec![spec(
        "design_search_engine experiments",
        cargo_test_ignored_args(&["-p", "design_search_engine", "--test", "experiments"]),
    )]
}
