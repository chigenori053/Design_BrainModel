pub mod analyze;
pub mod converge;
pub mod diff;
pub mod init;
pub mod step;
pub mod suggest;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::command::{CommandHandler, CommandPlugin, CommandRegistry};
use unified_design_ir::{
    ContextSpec, DesignDocument, DesignHistory, DesignVersion, ExecutionSpec, FunctionSpec,
    IssueInput, IssueResult, Metadata, Stage, VersionId, VersionStatus, compute_hash,
    create_version, detect_issues, diff_versions, get_version, normalize,
};

pub struct DesignPlugin;

impl CommandPlugin for DesignPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("design");
        cmd.register_subcommand(init::handler());
        cmd.register_subcommand(converge::handler());
        cmd.register_subcommand(step::handler());
        cmd.register_subcommand(analyze::handler());
        cmd.register_subcommand(diff::handler());
        cmd.register_subcommand(suggest::handler());
        registry.register(cmd);
    }
}

// ── File paths ──────────────────────────────────────────────────────────────

pub fn design_file(root: &Path) -> PathBuf {
    root.join("design.md")
}

pub fn baseline_file(root: &Path) -> PathBuf {
    root.join(".design").join("baseline.json")
}

pub fn history_dir(root: &Path) -> PathBuf {
    root.join(".design").join("history")
}

// ── Root resolution ─────────────────────────────────────────────────────────

pub fn resolve_root(arg: Option<&str>) -> PathBuf {
    arg.map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

// ── Design document I/O ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    FileNotFound,
    ParseError,
    InvalidStage,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound => write!(f, "design.md not found"),
            Self::ParseError => write!(f, "invalid design format"),
            Self::InvalidStage => write!(f, "invalid stage"),
        }
    }
}

impl std::error::Error for LoadError {}

pub fn load_design(path: &str) -> Result<DesignDocument, LoadError> {
    let content = fs::read_to_string(path).map_err(|_| LoadError::FileNotFound)?;
    let doc: DesignDocument = serde_yaml::from_str(&content).map_err(|_| LoadError::ParseError)?;
    match doc.stage {
        Stage::Context
        | Stage::Function
        | Stage::Architecture
        | Stage::Interface
        | Stage::Data
        | Stage::Execution => Ok(doc),
    }
}

pub fn load_design_doc(root: &Path) -> Result<DesignDocument, String> {
    let path = design_file(root);
    load_design(&path.to_string_lossy()).map_err(|err| match err {
        LoadError::FileNotFound => "Cannot read design.md: design.md not found".to_string(),
        LoadError::ParseError => "Cannot parse design.md: invalid design format".to_string(),
        LoadError::InvalidStage => "Cannot parse design.md: invalid stage".to_string(),
    })
}

pub fn save_design_doc(root: &Path, doc: &DesignDocument) -> Result<(), String> {
    let path = design_file(root);
    let yaml = design_to_yaml(doc);
    fs::write(&path, yaml).map_err(|e| format!("Cannot write design.md: {e}"))
}

// ── Baseline I/O ────────────────────────────────────────────────────────────

pub fn load_baseline(root: &Path) -> Option<DesignVersion> {
    let path = baseline_file(root);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_baseline(root: &Path, version: &DesignVersion) -> Result<(), String> {
    let dir = root.join(".design");
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create .design/: {e}"))?;
    let path = baseline_file(root);
    let json = serde_json::to_string_pretty(version)
        .map_err(|e| format!("Cannot serialize baseline: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Cannot write baseline.json: {e}"))
}

// ── History snapshots ───────────────────────────────────────────────────────

pub fn save_version_snapshot(root: &Path, version: &DesignVersion) -> Result<(), String> {
    let dir = history_dir(root);
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create history dir: {e}"))?;
    let filename = format!("v{}.json", version.id.seq);
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(version)
        .map_err(|e| format!("Cannot serialize version: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Cannot write snapshot: {e}"))
}

pub fn list_history_snapshots(root: &Path) -> Vec<DesignVersion> {
    let dir = history_dir(root);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut versions: Vec<DesignVersion> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let content = fs::read_to_string(e.path()).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect();
    versions.sort_by_key(|v| v.id.seq);
    versions
}

// ── Version construction ────────────────────────────────────────────────────

/// init_history from unified_design_ir hardcodes Stage::Context.
/// We replicate the logic here to support any stage.
pub fn make_initial_version(design: DesignDocument) -> (DesignVersion, DesignHistory) {
    let stage = design.stage.clone();
    let canonical = normalize(&design);
    let hash = compute_hash(&canonical, &stage);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let version = DesignVersion {
        id: VersionId {
            seq: 1,
            hash: hash.clone(),
        },
        parent: None,
        stage,
        status: VersionStatus::Draft,
        design,
        created_at: now,
        is_duplicate: false,
    };
    let history = DesignHistory {
        head: version.id.clone(),
        versions: vec![version.clone()],
        next_seq: 2,
    };
    (version, history)
}

/// Load baseline and current from disk and build the two-version history
/// needed by the convergence engine.
pub fn load_versions(root: &Path) -> Result<(DesignVersion, DesignHistory), String> {
    let doc = load_design_doc(root)?;
    let stage = doc.stage.clone();

    let baseline_doc = load_baseline(root)
        .filter(|v| v.stage == stage)
        .map(|v| v.design)
        .unwrap_or_else(|| default_baseline_for_stage(&stage));

    let (baseline_version, mut history) = make_initial_version(baseline_doc);

    let current = create_version(&mut history, Some(baseline_version.id.clone()), stage, doc)
        .map_err(|e| format!("Cannot create version: {e:?}"))?;

    Ok((current, history))
}

#[cfg(test)]
mod load_tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("dbm_design_load_tests");
        fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(name)
    }

    #[test]
    fn load_design_reads_yaml_document() {
        let path = temp_file("design_ok.yaml");
        fs::write(
            &path,
            r#"stage: Context
context:
  target_user: developer
  use_case: build CLI tool
  environment: local
function:
  functions: []
architecture: {}
interface: {}
data: {}
execution: {}
metadata:
  tags: []
"#,
        )
        .expect("write design");

        let doc = load_design(&path.to_string_lossy()).expect("load design");

        assert_eq!(doc.stage, Stage::Context);
        assert_eq!(
            doc.context.and_then(|context| context.target_user),
            Some("developer".to_string())
        );
    }

    #[test]
    fn load_design_defaults_missing_fields() {
        let path = temp_file("design_missing_fields.yaml");
        fs::write(
            &path,
            r#"stage: Context
context:
  target_user: developer
"#,
        )
        .expect("write design");

        let doc = load_design(&path.to_string_lossy()).expect("load design");

        assert!(doc.function.is_none());
        assert!(doc.architecture.is_none());
        assert_eq!(doc.metadata.tags, Vec::<String>::new());
    }

    #[test]
    fn load_design_reports_missing_file() {
        let path = temp_file("missing.yaml");
        let _ = fs::remove_file(&path);

        let err = load_design(&path.to_string_lossy()).expect_err("missing file");

        assert_eq!(err, LoadError::FileNotFound);
    }

    #[test]
    fn load_design_reports_parse_error_for_invalid_yaml() {
        let path = temp_file("invalid_yaml.yaml");
        fs::write(&path, "stage: Context:\ncontext: [").expect("write invalid yaml");

        let err = load_design(&path.to_string_lossy()).expect_err("parse error");

        assert_eq!(err, LoadError::ParseError);
    }

    #[test]
    fn load_design_reports_invalid_stage() {
        let path = temp_file("invalid_stage.yaml");
        fs::write(&path, "stage: Unknown\n").expect("write invalid stage");

        let err = load_design(&path.to_string_lossy()).expect_err("invalid stage");

        assert!(matches!(
            err,
            LoadError::ParseError | LoadError::InvalidStage
        ));
    }

    #[test]
    fn load_versions_are_deterministic_for_same_input() {
        let dir = std::env::temp_dir().join("dbm_design_load_versions");
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(
            dir.join("design.md"),
            r#"stage: Context
context:
  target_user: developer
  use_case: build CLI tool
  environment: local
function:
  functions: []
architecture: {}
interface: {}
data: {}
execution: {}
metadata:
  tags: []
"#,
        )
        .expect("write design.md");

        let (initial_a, history_a) = load_versions(&dir).expect("first load");
        let (initial_b, history_b) = load_versions(&dir).expect("second load");

        assert_eq!(initial_a.stage, initial_b.stage);
        assert_eq!(initial_a.design, initial_b.design);
        assert_eq!(initial_a.id.hash, initial_b.id.hash);
        assert_eq!(history_a.versions.len(), history_b.versions.len());
        assert_eq!(history_a.head.hash, history_b.head.hash);
    }
}

// ── Issue detection (public helper for subcommands) ─────────────────────────

pub fn detect_issues_for(
    history: &DesignHistory,
    current: &DesignVersion,
) -> Result<IssueResult, String> {
    let before = walk_to_baseline(history, current);
    let diff = diff_versions(&before, current).map_err(|e| format!("Diff error: {e:?}"))?;
    detect_issues(IssueInput {
        before,
        after: current.clone(),
        diff,
    })
    .map_err(|e| format!("Issue detection error: {e:?}"))
}

fn walk_to_baseline(history: &DesignHistory, current: &DesignVersion) -> DesignVersion {
    let mut baseline = current.clone();
    let mut cursor = current.parent.clone();
    while let Some(parent_id) = cursor {
        match get_version(history, &parent_id) {
            Some(parent) => {
                baseline = parent.clone();
                cursor = parent.parent.clone();
            }
            None => break,
        }
    }
    baseline
}

// ── Default baseline per stage ──────────────────────────────────────────────

pub fn default_baseline_for_stage(stage: &Stage) -> DesignDocument {
    match stage {
        Stage::Context => DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some("developer".to_string()),
                use_case: Some("build application".to_string()),
                environment: Some("local".to_string()),
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        },
        Stage::Function => DesignDocument {
            stage: Stage::Function,
            context: None,
            function: Some(FunctionSpec {
                functions: vec!["main".to_string()],
            }),
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        },
        Stage::Execution => DesignDocument {
            stage: Stage::Execution,
            context: None,
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: Some(ExecutionSpec {
                steps: vec!["setup".to_string(), "run".to_string()],
            }),
            metadata: Metadata::default(),
        },
        _ => DesignDocument {
            stage: stage.clone(),
            context: None,
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        },
    }
}

// ── Formatting helpers ──────────────────────────────────────────────────────

pub fn format_issue_list(result: &IssueResult) -> String {
    if result.issues.is_empty() {
        return "  (no issues)\n".to_string();
    }
    result
        .issues
        .iter()
        .map(|issue| {
            format!(
                "  [{:?}] {:?}: {}  (fix: {:?})\n",
                issue.severity,
                issue.issue_type,
                issue.path.segments.join("."),
                issue.fix_hint.as_ref().map(|h| &h.action),
            )
        })
        .collect()
}

pub fn format_summary(result: &IssueResult) -> String {
    format!(
        "Critical: {}  High: {}  Medium: {}  Low: {}  (Total: {})",
        result.summary.critical,
        result.summary.high,
        result.summary.medium,
        result.summary.low,
        result.summary.total,
    )
}

// ── YAML writer ─────────────────────────────────────────────────────────────

pub fn design_to_yaml(doc: &DesignDocument) -> String {
    let mut out = String::new();
    out.push_str(&format!("stage: {}\n", stage_str(&doc.stage)));

    if let Some(ctx) = &doc.context {
        out.push('\n');
        out.push_str("context:\n");
        out.push_str(&format!(
            "  target_user: {}\n",
            yaml_scalar(ctx.target_user.as_deref().unwrap_or(""))
        ));
        out.push_str(&format!(
            "  use_case: {}\n",
            yaml_scalar(ctx.use_case.as_deref().unwrap_or(""))
        ));
        out.push_str(&format!(
            "  environment: {}\n",
            yaml_scalar(ctx.environment.as_deref().unwrap_or(""))
        ));
    }

    if let Some(func) = &doc.function {
        out.push('\n');
        out.push_str("function:\n");
        if func.functions.is_empty() {
            out.push_str("  functions: []\n");
        } else {
            out.push_str("  functions:\n");
            for f in &func.functions {
                out.push_str(&format!("    - {}\n", yaml_scalar(f)));
            }
        }
    }

    if doc.architecture.is_some() {
        out.push('\n');
        out.push_str("architecture: {}\n");
    }

    if doc.interface.is_some() {
        out.push('\n');
        out.push_str("interface: {}\n");
    }

    if doc.data.is_some() {
        out.push('\n');
        out.push_str("data: {}\n");
    }

    if let Some(exec) = &doc.execution {
        out.push('\n');
        out.push_str("execution:\n");
        if exec.steps.is_empty() {
            out.push_str("  steps: []\n");
        } else {
            out.push_str("  steps:\n");
            for s in &exec.steps {
                out.push_str(&format!("    - {}\n", yaml_scalar(s)));
            }
        }
    }

    out.push('\n');
    out.push_str("metadata:\n");
    if doc.metadata.tags.is_empty() {
        out.push_str("  tags: []\n");
    } else {
        out.push_str("  tags:\n");
        for t in &doc.metadata.tags {
            out.push_str(&format!("    - {}\n", yaml_scalar(t)));
        }
    }

    out
}

pub fn stage_str(stage: &Stage) -> &'static str {
    match stage {
        Stage::Context => "Context",
        Stage::Function => "Function",
        Stage::Architecture => "Architecture",
        Stage::Interface => "Interface",
        Stage::Data => "Data",
        Stage::Execution => "Execution",
    }
}

fn yaml_scalar(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    if s.contains(':')
        || s.contains('#')
        || s.starts_with(' ')
        || s.starts_with('"')
        || s.starts_with('\'')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_to_yaml_context_stage() {
        let doc = DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some("developer".to_string()),
                use_case: Some("build CLI".to_string()),
                environment: Some("local".to_string()),
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let yaml = design_to_yaml(&doc);
        assert!(yaml.contains("stage: Context"));
        assert!(yaml.contains("target_user: developer"));
        assert!(yaml.contains("use_case: build CLI"));
    }

    #[test]
    fn design_to_yaml_roundtrip() {
        let doc = DesignDocument {
            stage: Stage::Function,
            context: None,
            function: Some(FunctionSpec {
                functions: vec!["search".to_string(), "submit".to_string()],
            }),
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let yaml = design_to_yaml(&doc);
        let parsed: DesignDocument = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.stage, Stage::Function);
        assert_eq!(parsed.function.unwrap().functions, vec!["search", "submit"]);
    }

    #[test]
    fn make_initial_version_preserves_stage() {
        let doc = DesignDocument {
            stage: Stage::Function,
            context: None,
            function: Some(FunctionSpec {
                functions: vec!["main".to_string()],
            }),
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };
        let (version, _history) = make_initial_version(doc);
        assert_eq!(version.stage, Stage::Function);
    }
}
