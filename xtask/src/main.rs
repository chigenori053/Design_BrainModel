use std::env;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("test") => run_suite(args.collect()),
        _ => {
            eprintln!("usage: cargo xtest <category>");
            eprintln!(
                "categories: fast, architecture, invariants, engine, knowledge-engine, contract, determinism, integration, long-run, runtime-heavy, stress, experiments, all"
            );
            ExitCode::from(2)
        }
    }
}

fn run_suite(args: Vec<String>) -> ExitCode {
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
        "long-run" => long_run_specs(),
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
            long_run_specs(),
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

fn spec(name: &'static str, args: Vec<&'static str>) -> TestSpec {
    TestSpec { name, args }
}

/// cargo nextest run 用引数を構築
/// 各テストを独立プロセスで実行し、完了後にリソースをOSに返却
/// 並列数は .config/nextest.toml の profile.default で管理
fn nextest(args: &[&'static str]) -> Vec<&'static str> {
    let mut full = vec!["nextest", "run"];
    full.extend_from_slice(args);
    full
}

fn nextest_release(args: &[&'static str]) -> Vec<&'static str> {
    let mut full = vec!["nextest", "run", "--release"];
    full.extend_from_slice(args);
    full
}

fn nextest_ignored(args: &[&'static str]) -> Vec<&'static str> {
    let mut full = vec!["nextest", "run", "--run-ignored", "ignored-only"];
    full.extend_from_slice(args);
    full
}

fn architecture_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "design_cli architecture_enforcement",
            nextest(&["-p", "design_cli", "--test", "architecture_enforcement"]),
        ),
        spec(
            "design_cli negative_cases",
            nextest(&["-p", "design_cli", "--test", "negative_cases"]),
        ),
        spec(
            "design_cli reasoning_engine",
            nextest(&["-p", "design_cli", "--test", "reasoning_engine"]),
        ),
    ]
}

fn invariants_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "architecture_domain invariants",
            nextest(&["-p", "architecture_domain", "--test", "invariants"]),
        ),
        spec(
            "design_search_engine invariants",
            nextest(&["-p", "design_search_engine", "--test", "invariants"]),
        ),
    ]
}

fn engine_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "design_search_engine engine",
            nextest(&["-p", "design_search_engine", "--test", "engine"]),
        ),
        spec(
            "evaluation_engine engine",
            nextest(&["-p", "evaluation_engine", "--test", "engine"]),
        ),
        spec(
            "memory_graph memory",
            nextest(&["-p", "memory_graph", "--test", "memory"]),
        ),
    ]
}

fn knowledge_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "knowledge_engine retrieval",
            nextest(&["-p", "knowledge_engine", "--test", "knowledge_retrieval"]),
        ),
        spec(
            "knowledge_engine parsing",
            nextest(&["-p", "knowledge_engine", "--test", "knowledge_parsing"]),
        ),
        spec(
            "knowledge_engine validation",
            nextest(&["-p", "knowledge_engine", "--test", "knowledge_validation"]),
        ),
        spec(
            "knowledge_engine reasoning integration",
            nextest(&[
                "-p",
                "knowledge_engine",
                "--test",
                "knowledge_reasoning_integration",
            ]),
        ),
        spec(
            "knowledge_engine search impact",
            nextest(&[
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
            nextest(&["-p", "contract_audit_tests"]),
        ),
        spec(
            "pipeline contract tests",
            nextest(&["-p", "pipeline_tests"]),
        ),
    ]
}

fn determinism_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "design_search_engine determinism",
            nextest(&["-p", "design_search_engine", "--test", "determinism"]),
        ),
        spec(
            "ai_context determinism",
            nextest(&["-p", "ai_context", "--test", "determinism"]),
        ),
        spec(
            "runtime_vm determinism",
            nextest(&["-p", "runtime_vm", "--test", "determinism"]),
        ),
    ]
}

fn integration_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "runtime_vm integration",
            nextest(&["-p", "runtime_vm", "--test", "integration"]),
        ),
        spec(
            "phase1_integration_tests concept pipeline",
            nextest(&[
                "-p",
                "phase1_integration_tests",
                "--test",
                "concept_pipeline",
            ]),
        ),
        spec(
            "phase1_integration_tests canonicalization",
            nextest(&[
                "-p",
                "phase1_integration_tests",
                "--test",
                "canonicalization",
            ]),
        ),
        spec(
            "phase1_integration_tests reasoning pipeline",
            nextest(&[
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
        nextest_ignored(&["-p", "runtime_vm"]),
    )]
}

fn long_run_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "agent_core engine long-run",
            nextest_ignored(&["-p", "agent_core", "--test", "engine"]),
        ),
        spec(
            "knowledge_lifecycle long-run",
            nextest_ignored(&[
                "-p",
                "knowledge_lifecycle",
                "--test",
                "knowledge_lifecycle_long_run",
            ]),
        ),
        spec(
            "knowledge_lifecycle long-run simulation",
            nextest_ignored(&[
                "-p",
                "knowledge_lifecycle",
                "--test",
                "lifecycle_long_run_simulation",
            ]),
        ),
    ]
}

fn stress_specs() -> Vec<TestSpec> {
    vec![
        spec(
            "reasoning_agent scaling",
            nextest_release(&["-p", "reasoning_agent", "--test", "scaling"]),
        ),
        spec(
            "agent_core heavy",
            nextest_release(&[
                "-p",
                "agent_core",
                "--test",
                "heavy",
                "--features",
                "ci-heavy",
            ]),
        ),
    ]
}

fn experiments_specs() -> Vec<TestSpec> {
    vec![spec(
        "design_search_engine experiments",
        nextest_ignored(&["-p", "design_search_engine", "--test", "experiments"]),
    )]
}
