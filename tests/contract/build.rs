use std::fs;
use std::path::{Path, PathBuf};

const CANONICAL_NAMES: &[&str] = &[
    "ReasoningInput",
    "SemanticRepresentation",
    "MemoryCandidate",
    "Hypothesis",
    "Relation",
    "EvaluationScore",
    "Decision",
    "ValidationResult",
    "ReasoningTrace",
    "TraceStep",
    "TraceStats",
];

const STRICT_BOUNDARY_FORBIDDEN: &[(&str, &[&str])] = &[
    (
        "crates/runtime/runtime_core/src/stable_v03.rs",
        &["SearchInput", "legacy::"],
    ),
    (
        "apps/cli/src/renderer.rs",
        &["legacy::", "SearchInput"],
    ),
];

fn main() {
    let strict = std::env::var_os("CARGO_FEATURE_CONTRACT_STRICT").is_some();
    let workspace_root = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"),
    )
    .parent()
    .expect("tests dir")
    .parent()
    .expect("workspace root")
    .to_path_buf();
    let targets = [
        workspace_root.join("crates/engine/design_search_engine/src/stable_v03.rs"),
        workspace_root.join("crates/memory_space/src/stable_v03.rs"),
        workspace_root.join("crates/runtime/runtime_core/src/stable_v03.rs"),
        workspace_root.join("apps/cli/src/renderer.rs"),
    ];

    let mut violations = Vec::new();
    for target in targets {
        scan_dir(&target, &mut violations);
    }

    if !violations.is_empty() {
        panic!(
            "contract type redefinition detected outside contracts crate:\n{}",
            violations.join("\n")
        );
    }

    if strict {
        let mut strict_violations = Vec::new();
        for (relative, needles) in STRICT_BOUNDARY_FORBIDDEN {
            let path = workspace_root.join(relative);
            let content = fs::read_to_string(&path).expect("read strict source");
            for needle in *needles {
                if content.contains(needle) {
                    strict_violations.push(format!("{} contains forbidden boundary token {}", path.display(), needle));
                }
            }
        }
        if !strict_violations.is_empty() {
            panic!(
                "contract_strict boundary violations detected:\n{}",
                strict_violations.join("\n")
            );
        }
    }
}

fn scan_dir(path: &Path, violations: &mut Vec<String>) {
    let content = fs::read_to_string(path).expect("read source");
    for name in CANONICAL_NAMES {
        let struct_def = format!("pub struct {name}");
        let enum_def = format!("pub enum {name}");
        if content.contains(&struct_def) || content.contains(&enum_def) {
            violations.push(format!("{} defines {}", path.display(), name));
        }
    }
}
