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

const FORBIDDEN_BOUNDARY_TOKENS: &[&str] = &["SearchInput", "legacy::"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractAuditDiagnostic {
    CanonicalTypeRedefinition,
    ForbiddenBoundaryToken,
    MissingScanTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Diagnostic {
    kind: ContractAuditDiagnostic,
    path: PathBuf,
    detail: String,
}

impl Diagnostic {
    fn new(kind: ContractAuditDiagnostic, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    fn is_structural_violation(&self) -> bool {
        matches!(
            self.kind,
            ContractAuditDiagnostic::CanonicalTypeRedefinition
                | ContractAuditDiagnostic::ForbiddenBoundaryToken
        )
    }

    fn render(&self) -> String {
        format!("{:?}: {} {}", self.kind, self.path.display(), self.detail)
    }
}

fn main() {
    let strict = std::env::var_os("CARGO_FEATURE_CONTRACT_STRICT").is_some();
    let workspace_root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"))
        .parent()
        .expect("tests dir")
        .parent()
        .expect("workspace root")
        .to_path_buf();

    let mut diagnostics = Vec::new();
    let sources = collect_rust_sources(&workspace_root);
    for source in &sources {
        scan_file(source, &workspace_root, strict, &mut diagnostics);
    }

    let violations = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_structural_violation())
        .map(Diagnostic::render)
        .collect::<Vec<_>>();
    if !violations.is_empty() {
        panic!(
            "contract structural invariant violations detected:\n{}",
            violations.join("\n")
        );
    }
}

fn collect_rust_sources(workspace_root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    collect_rust_sources_inner(workspace_root, &mut sources);
    sources.sort();
    sources
}

fn collect_rust_sources_inner(path: &Path, sources: &mut Vec<PathBuf>) {
    if should_exclude(path) {
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if should_exclude(&path) {
            continue;
        }
        if path.is_dir() {
            collect_rust_sources_inner(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

fn should_exclude(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(
            name.as_ref(),
            "target" | ".git" | "backup" | "generated" | "tests"
        )
    })
}

fn scan_file(path: &Path, workspace_root: &Path, strict: bool, diagnostics: &mut Vec<Diagnostic>) {
    if !path.exists() {
        diagnostics.push(Diagnostic::new(
            ContractAuditDiagnostic::MissingScanTarget,
            path,
            "scan target disappeared before read",
        ));
        return;
    }

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            diagnostics.push(Diagnostic::new(
                ContractAuditDiagnostic::MissingScanTarget,
                path,
                format!("read source failed: {error}"),
            ));
            return;
        }
    };

    if is_contract_semantic_scope(path, workspace_root, &content) {
        for name in CANONICAL_NAMES {
            if is_type_defined(&content, name) {
                diagnostics.push(Diagnostic::new(
                    ContractAuditDiagnostic::CanonicalTypeRedefinition,
                    path,
                    format!("defines {name}"),
                ));
            }
        }
    }

    if strict && is_boundary_semantic_scope(path, workspace_root, &content) {
        for token in FORBIDDEN_BOUNDARY_TOKENS {
            if content.contains(token) {
                diagnostics.push(Diagnostic::new(
                    ContractAuditDiagnostic::ForbiddenBoundaryToken,
                    path,
                    format!("contains forbidden boundary token {token}"),
                ));
            }
        }
    }
}

fn is_contract_semantic_scope(path: &Path, workspace_root: &Path, content: &str) -> bool {
    !is_contracts_crate_source(path, workspace_root)
        && (content.contains("contracts::")
            || content.contains("use contracts")
            || path_is_under(path, workspace_root, "crates/bridge")
            || path_is_under(path, workspace_root, "crates/engine/design_search_engine")
            || path_is_under(path, workspace_root, "crates/runtime/runtime_core"))
}

fn is_boundary_semantic_scope(path: &Path, workspace_root: &Path, content: &str) -> bool {
    is_contract_semantic_scope(path, workspace_root, content)
        || path_is_under(path, workspace_root, "apps/cli/src")
}

fn is_contracts_crate_source(path: &Path, workspace_root: &Path) -> bool {
    path_is_under(path, workspace_root, "crates/contracts")
}

fn path_is_under(path: &Path, workspace_root: &Path, relative: &str) -> bool {
    let Ok(relative_path) = path.strip_prefix(workspace_root) else {
        return false;
    };
    relative_path.starts_with(relative)
}

fn is_type_defined(content: &str, name: &str) -> bool {
    for prefix in &["pub struct ", "pub enum "] {
        let pattern = format!("{prefix}{name}");
        let mut start = 0;
        while let Some(pos) = content[start..].find(&pattern) {
            let abs_pos = start + pos;
            let after = abs_pos + pattern.len();
            let next_char = content[after..].chars().next();
            if !matches!(next_char, Some(c) if c.is_alphanumeric() || c == '_') {
                return true;
            }
            start = abs_pos + 1;
        }
    }
    false
}
