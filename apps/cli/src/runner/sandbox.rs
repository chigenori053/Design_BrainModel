use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use fs_extra::dir::{CopyOptions, copy as copy_dir};
use uuid::Uuid;

use super::types::{
    AllowedCommand, ExecutionTarget, RunnerError, SandboxCacheEntry, SandboxGuard, SandboxInstance,
    SandboxKey, SandboxMode,
};
use super::validation::canonical_dir;

pub fn create_sandbox(src: &Path) -> Result<SandboxInstance, RunnerError> {
    let canonical_src = canonical_dir(src).map_err(RunnerError::ValidationError)?;
    let key = compute_sandbox_key(&canonical_src).map_err(RunnerError::ExecutionError)?;
    let source_id = canonical_src.display().to_string();

    let cache_root = std::env::temp_dir().join("dbm_run_cache");
    fs::create_dir_all(&cache_root).map_err(|err| {
        RunnerError::ExecutionError(format!(
            "failed to create sandbox cache root {}: {err}",
            cache_root.display()
        ))
    })?;
    let execution_dir = std::env::temp_dir().join(format!("dbm_run_{}", Uuid::new_v4()));
    fs::create_dir_all(&execution_dir).map_err(|err| {
        RunnerError::ExecutionError(format!(
            "failed to create sandbox {}: {err}",
            execution_dir.display()
        ))
    })?;

    let mut cache = global_sandbox_cache()
        .lock()
        .map_err(|_| RunnerError::ExecutionError("sandbox cache lock poisoned".to_string()))?;

    let (mode, cache_dir) = match cache.get(&source_id) {
        Some(entry) if entry.key == key => (SandboxMode::Reuse, entry.cache_dir.clone()),
        Some(entry) => {
            let cache_dir = cache_root.join(Uuid::new_v4().to_string());
            materialize_cache(&canonical_src, &cache_dir)?;
            let _ = fs::remove_dir_all(&entry.cache_dir);
            cache.insert(
                source_id.clone(),
                SandboxCacheEntry {
                    key: key.clone(),
                    cache_dir: cache_dir.clone(),
                },
            );
            (SandboxMode::Incremental, cache_dir)
        }
        None => {
            let cache_dir = cache_root.join(Uuid::new_v4().to_string());
            materialize_cache(&canonical_src, &cache_dir)?;
            cache.insert(
                source_id.clone(),
                SandboxCacheEntry {
                    key: key.clone(),
                    cache_dir: cache_dir.clone(),
                },
            );
            (SandboxMode::FullCopy, cache_dir)
        }
    };

    drop(cache);
    copy_from_cache(&cache_dir, &execution_dir)?;

    Ok(SandboxInstance {
        guard: SandboxGuard::new(execution_dir),
        mode,
        key,
    })
}

pub fn detect_target(path: &Path) -> Result<ExecutionTarget, RunnerError> {
    if path.join("Cargo.toml").exists() {
        return Ok(ExecutionTarget::RustCargo);
    }
    if path.join("package.json").exists() && path.join("index.js").exists() {
        return Ok(ExecutionTarget::NodeScript("index.js".to_string()));
    }
    if path.join("pyproject.toml").exists() {
        return Ok(ExecutionTarget::PythonModule("app".to_string()));
    }
    Err(RunnerError::ValidationError(format!(
        "no supported execution target detected under {}",
        path.display()
    )))
}

pub fn build_command(target: &ExecutionTarget) -> (String, Vec<String>) {
    match target {
        ExecutionTarget::RustCargo => (
            AllowedCommand::Cargo.as_str().to_string(),
            vec!["run".to_string(), "--quiet".to_string()],
        ),
        ExecutionTarget::NodeScript(path) => (
            AllowedCommand::Node.as_str().to_string(),
            vec![path.clone()],
        ),
        ExecutionTarget::PythonModule(module) => (
            AllowedCommand::Python.as_str().to_string(),
            vec!["-m".to_string(), module.clone()],
        ),
    }
}

pub fn fixed_env() -> Vec<(String, String)> {
    let mut env = vec![
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("LANG".to_string(), "C".to_string()),
        ("TZ".to_string(), "UTC".to_string()),
    ];
    for key in ["HOME", "CARGO_HOME", "RUSTUP_HOME", "USER", "TMPDIR"] {
        if let Ok(value) = std::env::var(key) {
            env.push((key.to_string(), value));
        }
    }
    env
}

fn global_sandbox_cache() -> &'static Mutex<HashMap<String, SandboxCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, SandboxCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn materialize_cache(src: &Path, cache_dir: &Path) -> Result<(), RunnerError> {
    fs::create_dir_all(cache_dir).map_err(|err| {
        RunnerError::ExecutionError(format!(
            "failed to create cache dir {}: {err}",
            cache_dir.display()
        ))
    })?;
    copy_dir(src, cache_dir, &CopyOptions::new().content_only(true)).map_err(|err| {
        RunnerError::ExecutionError(format!("failed to create cache copy: {err}"))
    })?;
    Ok(())
}

fn copy_from_cache(cache_dir: &Path, execution_dir: &Path) -> Result<(), RunnerError> {
    copy_dir(
        cache_dir,
        execution_dir,
        &CopyOptions::new().content_only(true),
    )
    .map_err(|err| {
        RunnerError::ExecutionError(format!(
            "failed to materialize execution sandbox from cache: {err}"
        ))
    })?;
    Ok(())
}

fn compute_sandbox_key(root: &Path) -> Result<SandboxKey, String> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    files.sort();
    let mut hasher = DefaultHasher::new();
    root.display().to_string().hash(&mut hasher);
    for path in &files {
        path.hash(&mut hasher);
    }
    Ok(SandboxKey {
        path_hash: hasher.finish(),
        file_count: files.len(),
    })
}

fn collect_files(root: &Path, files: &mut Vec<String>) -> Result<(), String> {
    for entry in
        fs::read_dir(root).map_err(|err| format!("failed to read {}: {err}", root.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read dir entry: {err}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), ".git" | "target" | "node_modules") {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, files)?;
        } else {
            files.push(path.display().to_string());
        }
    }
    Ok(())
}
