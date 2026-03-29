use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Metadata {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub annotations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextSpec {
    pub target_user: Option<String>,
    pub use_case: Option<String>,
    pub environment: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FunctionSpec {
    #[serde(default)]
    pub functions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ArchitectureSpec {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InterfaceSpec {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DataSpec {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExecutionSpec {
    #[serde(default)]
    pub steps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignDocument {
    pub stage: Stage,
    pub context: Option<ContextSpec>,
    pub function: Option<FunctionSpec>,
    pub architecture: Option<ArchitectureSpec>,
    pub interface: Option<InterfaceSpec>,
    pub data: Option<DataSpec>,
    pub execution: Option<ExecutionSpec>,
    #[serde(default)]
    pub metadata: Metadata,
}

pub type CanonicalDesign = Value;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersionId {
    pub seq: u64,
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionStatus {
    Draft,
    Converged,
    Locked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    Context,
    Function,
    Architecture,
    Interface,
    Data,
    Execution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignVersion {
    pub id: VersionId,
    pub parent: Option<VersionId>,
    pub stage: Stage,
    pub status: VersionStatus,
    pub design: DesignDocument,
    pub created_at: u64,
    pub is_duplicate: bool,
    // TODO: support non-linear parent (branch)
    // TODO: convergence fingerprint
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignHistory {
    pub versions: Vec<DesignVersion>,
    pub head: VersionId,
    pub next_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    InvalidParent,
    HistoryEmpty,
}

pub fn normalize(design: &DesignDocument) -> CanonicalDesign {
    let mut root = Map::new();
    root.insert("stage".to_string(), stage_value(&design.stage));
    insert_option(
        &mut root,
        "context",
        design.context.as_ref().map(context_value),
    );
    insert_option(
        &mut root,
        "function",
        design.function.as_ref().map(function_value),
    );
    insert_option(
        &mut root,
        "architecture",
        design
            .architecture
            .as_ref()
            .map(|_| Value::Object(Map::new())),
    );
    insert_option(
        &mut root,
        "interface",
        design.interface.as_ref().map(|_| Value::Object(Map::new())),
    );
    insert_option(
        &mut root,
        "data",
        design.data.as_ref().map(|_| Value::Object(Map::new())),
    );
    insert_option(
        &mut root,
        "execution",
        design.execution.as_ref().map(execution_value),
    );
    root.insert("metadata".to_string(), metadata_value(&design.metadata));

    let mut canonical = Value::Object(root);
    canonicalize_value(None, &mut canonical);
    canonical
}

pub fn compute_hash(design: &CanonicalDesign, stage: &Stage) -> String {
    let payload = json!({
        "design": design,
        "stage": stage,
    });
    let encoded = serde_json::to_vec(&payload).expect("canonical design must serialize");
    let digest = Sha256::digest(encoded);
    format!("{digest:x}")
}

pub fn create_version(
    history: &mut DesignHistory,
    parent: Option<VersionId>,
    stage: Stage,
    design: DesignDocument,
) -> Result<DesignVersion, VersionError> {
    validate_parent(history, parent.as_ref())?;
    let canonical = normalize(&design);
    let hash = compute_hash(&canonical, &stage);
    let is_duplicate = parent
        .as_ref()
        .is_some_and(|parent_id| parent_id.hash == hash);
    let version = DesignVersion {
        id: VersionId {
            seq: history.next_seq,
            hash,
        },
        parent,
        stage,
        status: VersionStatus::Draft,
        design,
        created_at: current_epoch_millis(),
        is_duplicate,
    };
    history.versions.push(version.clone());
    history.head = version.id.clone();
    history.next_seq += 1;
    Ok(version)
}

pub fn init_history(initial_design: DesignDocument) -> DesignHistory {
    let stage = Stage::Context;
    let canonical = normalize(&initial_design);
    let head = VersionId {
        seq: 1,
        hash: compute_hash(&canonical, &stage),
    };
    let initial_version = DesignVersion {
        id: head.clone(),
        parent: None,
        stage,
        status: VersionStatus::Draft,
        design: initial_design,
        created_at: current_epoch_millis(),
        is_duplicate: false,
    };
    DesignHistory {
        versions: vec![initial_version],
        head,
        next_seq: 2,
    }
}

pub fn get_head(history: &DesignHistory) -> Result<&DesignVersion, VersionError> {
    history
        .versions
        .iter()
        .find(|version| version.id == history.head)
        .ok_or(VersionError::HistoryEmpty)
}

pub fn get_version<'a>(history: &'a DesignHistory, id: &VersionId) -> Option<&'a DesignVersion> {
    history.versions.iter().find(|version| &version.id == id)
}

pub fn list_versions(history: &DesignHistory) -> Vec<&DesignVersion> {
    history.versions.iter().collect()
}

fn validate_parent(
    history: &DesignHistory,
    parent: Option<&VersionId>,
) -> Result<(), VersionError> {
    match parent {
        None => Ok(()),
        Some(parent_id)
            if history
                .versions
                .iter()
                .any(|version| version.id == *parent_id)
                && history.head == *parent_id =>
        {
            Ok(())
        }
        Some(_) => Err(VersionError::InvalidParent),
    }
}

fn current_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after unix epoch")
        .as_millis() as u64
}

fn insert_option(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value);
    }
}

fn stage_value(stage: &Stage) -> Value {
    Value::String(
        match stage {
            Stage::Context => "Context",
            Stage::Function => "Function",
            Stage::Architecture => "Architecture",
            Stage::Interface => "Interface",
            Stage::Data => "Data",
            Stage::Execution => "Execution",
        }
        .to_string(),
    )
}

fn context_value(spec: &ContextSpec) -> Value {
    let mut map = Map::new();
    map.insert(
        "target_user".to_string(),
        Value::String(spec.target_user.clone().unwrap_or_default()),
    );
    map.insert(
        "use_case".to_string(),
        Value::String(spec.use_case.clone().unwrap_or_default()),
    );
    map.insert(
        "environment".to_string(),
        Value::String(spec.environment.clone().unwrap_or_default()),
    );
    Value::Object(map)
}

fn function_value(spec: &FunctionSpec) -> Value {
    let mut map = Map::new();
    map.insert(
        "functions".to_string(),
        Value::Array(
            spec.functions
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
    Value::Object(map)
}

fn execution_value(spec: &ExecutionSpec) -> Value {
    let mut map = Map::new();
    map.insert(
        "steps".to_string(),
        Value::Array(
            spec.steps
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
    Value::Object(map)
}

fn metadata_value(metadata: &Metadata) -> Value {
    let mut map = Map::new();
    map.insert(
        "tags".to_string(),
        Value::Array(
            metadata
                .tags
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
    map.insert(
        "annotations".to_string(),
        Value::Array(
            metadata
                .annotations
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
    Value::Object(map)
}

fn canonicalize_value(field_name: Option<&str>, value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items.iter_mut() {
                canonicalize_value(None, item);
            }
            normalize_array(field_name.unwrap_or_default(), items);
        }
        Value::Object(entries) => {
            let mut sorted = Map::new();
            let mut keys = entries.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                if let Some(mut child) = entries.remove(&key) {
                    canonicalize_value(Some(&key), &mut child);
                    if !matches!(child, Value::Null) {
                        sorted.insert(key, child);
                    }
                }
            }
            *entries = sorted;
        }
        _ => {}
    }
}

fn normalize_array(field_name: &str, values: &mut Vec<Value>) {
    if is_order_insensitive(field_name) {
        values.sort_by_key(|value| serde_json::to_string(value).unwrap_or_default());
    }
}

fn is_order_insensitive(field_name: &str) -> bool {
    matches!(field_name, "requirements" | "tags")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_design() -> DesignDocument {
        DesignDocument {
            stage: Stage::Context,
            context: Some(ContextSpec {
                target_user: Some("operator".to_string()),
                use_case: Some("review order".to_string()),
                environment: None,
            }),
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata {
                tags: vec!["beta".to_string(), "core".to_string()],
                annotations: Vec::new(),
            },
        }
    }

    fn function_design(functions: Vec<&str>) -> DesignDocument {
        DesignDocument {
            stage: Stage::Function,
            context: None,
            function: Some(FunctionSpec {
                functions: functions.into_iter().map(str::to_string).collect(),
            }),
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        }
    }

    fn execution_design(steps: Vec<&str>) -> DesignDocument {
        DesignDocument {
            stage: Stage::Execution,
            context: None,
            function: None,
            architecture: None,
            interface: None,
            data: None,
            execution: Some(ExecutionSpec {
                steps: steps.into_iter().map(str::to_string).collect(),
            }),
            metadata: Metadata::default(),
        }
    }

    #[test]
    fn init_history_creates_initial_version() {
        let history = init_history(context_design());

        assert_eq!(history.versions.len(), 1);
        assert_eq!(history.head.seq, 1);
        assert_eq!(history.next_seq, 2);
        let head = get_head(&history).expect("head");
        assert_eq!(head.parent, None);
        assert_eq!(head.stage, Stage::Context);
        assert_eq!(head.status, VersionStatus::Draft);
        assert!(!head.is_duplicate);
    }

    #[test]
    fn create_version_updates_head_and_seq() {
        let mut history = init_history(context_design());
        let parent = history.head.clone();
        let next = create_version(
            &mut history,
            Some(parent.clone()),
            Stage::Function,
            function_design(vec!["search", "approve"]),
        )
        .expect("create version");

        assert_eq!(next.id.seq, 2);
        assert_eq!(history.head, next.id);
        assert_eq!(history.next_seq, 3);
        assert_eq!(next.parent, Some(parent));
    }

    #[test]
    fn normalize_preserves_empty_arrays_and_excludes_null() {
        let design = DesignDocument {
            stage: Stage::Function,
            context: None,
            function: Some(FunctionSpec {
                functions: Vec::new(),
            }),
            architecture: None,
            interface: None,
            data: None,
            execution: None,
            metadata: Metadata::default(),
        };

        let normalized = normalize(&design);
        let object = normalized.as_object().expect("object");
        assert!(object.get("context").is_none());
        assert_eq!(object["function"]["functions"], json!([]));
        assert_eq!(object["metadata"]["tags"], json!([]));
    }

    #[test]
    fn normalize_uses_stable_key_order() {
        let normalized = normalize(&context_design());
        let keys = normalized
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn normalize_applies_array_order_rules() {
        let mut design = context_design();
        design.metadata.tags = vec!["zeta".to_string(), "alpha".to_string()];
        let normalized_tags = normalize(&design);
        assert_eq!(
            normalized_tags["metadata"]["tags"],
            json!(["alpha", "zeta"])
        );

        let execution = normalize(&execution_design(vec!["step-2", "step-1"]));
        assert_eq!(execution["execution"]["steps"], json!(["step-2", "step-1"]));
    }

    #[test]
    fn hash_is_stable_for_same_structure_and_order_insensitive_fields() {
        let left = context_design();
        let mut right = context_design();
        right.metadata.tags = vec!["core".to_string(), "beta".to_string()];

        assert_eq!(
            compute_hash(&normalize(&left), &Stage::Context),
            compute_hash(&normalize(&right), &Stage::Context)
        );
    }

    #[test]
    fn hash_changes_for_order_sensitive_fields() {
        let left = execution_design(vec!["validate", "persist"]);
        let right = execution_design(vec!["persist", "validate"]);

        assert_ne!(
            compute_hash(&normalize(&left), &Stage::Execution),
            compute_hash(&normalize(&right), &Stage::Execution)
        );
    }

    #[test]
    fn duplicate_version_is_detected_from_parent_hash() {
        let mut history = init_history(context_design());
        let parent = history.head.clone();
        let next = create_version(&mut history, Some(parent), Stage::Context, context_design())
            .expect("create version");

        assert!(next.is_duplicate);
    }

    #[test]
    fn invalid_parent_is_rejected() {
        let mut history = init_history(context_design());
        let error = create_version(
            &mut history,
            Some(VersionId {
                seq: 999,
                hash: "missing".to_string(),
            }),
            Stage::Function,
            function_design(vec!["search"]),
        )
        .expect_err("invalid parent must fail");

        assert_eq!(error, VersionError::InvalidParent);
    }

    #[test]
    fn get_head_errors_on_empty_history() {
        let history = DesignHistory {
            versions: Vec::new(),
            head: VersionId {
                seq: 0,
                hash: String::new(),
            },
            next_seq: 1,
        };

        assert_eq!(get_head(&history), Err(VersionError::HistoryEmpty));
    }

    #[test]
    fn get_version_and_list_versions_work() {
        let mut history = init_history(context_design());
        let parent = history.head.clone();
        let second = create_version(
            &mut history,
            Some(parent),
            Stage::Function,
            function_design(vec!["search"]),
        )
        .expect("create version");

        let listed = list_versions(&history);
        assert_eq!(listed.len(), 2);
        assert_eq!(
            get_version(&history, &second.id)
                .expect("version lookup")
                .stage,
            Stage::Function
        );
    }
}
