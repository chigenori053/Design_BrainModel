use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

pub const DBM_WORKSPACE_ROOT: &str = "DBM_WORKSPACE_ROOT";

#[derive(Debug, Clone, Copy, Default)]
pub struct WorkspaceRoot;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceMigrationReport {
    pub canonical_root: PathBuf,
    pub migrated_roots: Vec<PathBuf>,
    pub merged_files: usize,
}

impl WorkspaceRoot {
    pub fn invocation_dir() -> PathBuf {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    pub fn discover() -> PathBuf {
        let current = Self::invocation_dir();
        Self::discover_from(&current)
    }

    pub fn discover_from(start: &Path) -> PathBuf {
        if let Some(root) = env::var_os(DBM_WORKSPACE_ROOT)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
        {
            return absolute_path(&root, start);
        }
        if let Some(root) = git_root(start) {
            return root;
        }
        if let Some(root) = find_dbm_ancestor(start) {
            return root;
        }
        absolute_path(start, start)
    }

    pub fn dbm_dir() -> PathBuf {
        Self::discover().join(".dbm")
    }

    pub fn ensure_layout(root: &Path) -> io::Result<()> {
        for directory in ["analyze", "memory", "logs", "mutations", "runtime"] {
            fs::create_dir_all(root.join(".dbm").join(directory))?;
        }
        Ok(())
    }

    pub fn migrate_legacy_layout() -> io::Result<WorkspaceMigrationReport> {
        let current = Self::invocation_dir();
        let canonical_root = Self::discover_from(&current);
        Self::migrate_legacy_layout_from(&canonical_root, &current)
    }

    pub fn migrate_legacy_layout_from(
        canonical_root: &Path,
        invocation_dir: &Path,
    ) -> io::Result<WorkspaceMigrationReport> {
        let invocation_dir = absolute_path(invocation_dir, invocation_dir);
        let canonical_root = absolute_path(canonical_root, &invocation_dir);
        let canonical_dbm = canonical_root.join(".dbm");
        fs::create_dir_all(&canonical_dbm)?;

        let mut sources = BTreeSet::new();
        sources.insert(canonical_root.join("apps/cli/.dbm"));
        sources.insert(canonical_root.join("apps/cli/src/.dbm"));
        sources.insert(invocation_dir.join(".dbm"));
        collect_nested_dbm_roots(&canonical_root, &canonical_dbm, &mut sources)?;
        sources.remove(&canonical_dbm);

        let mut report = WorkspaceMigrationReport {
            canonical_root: canonical_root.clone(),
            ..WorkspaceMigrationReport::default()
        };
        for source in sources {
            if !source.is_dir() {
                continue;
            }
            report.merged_files += merge_directory(&source, &canonical_dbm)?;
            fs::remove_dir_all(&source)?;
            report.migrated_roots.push(source);
        }
        Self::ensure_layout(&canonical_root)?;
        Ok(report)
    }
}

fn git_root(start: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8(output.stdout).ok()?;
    let root = root.trim();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

fn find_dbm_ancestor(start: &Path) -> Option<PathBuf> {
    let start = absolute_path(start, start);
    start
        .ancestors()
        .find(|candidate| candidate.join(".dbm").is_dir())
        .map(Path::to_path_buf)
}

fn absolute_path(path: &Path, base: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    path.canonicalize().unwrap_or(path)
}

fn merge_directory(source: &Path, destination: &Path) -> io::Result<usize> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    let mut merged = 0;
    for entry in entries {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            merged += merge_directory(&source_path, &destination_path)?;
            continue;
        }
        merge_file(&source_path, &destination_path)?;
        merged += 1;
    }
    Ok(merged)
}

fn collect_nested_dbm_roots(
    directory: &Path,
    canonical_dbm: &Path,
    roots: &mut BTreeSet<PathBuf>,
) -> io::Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        if path == canonical_dbm
            || matches!(name.to_str(), Some(".git" | "target" | "node_modules"))
        {
            continue;
        }
        if name == ".dbm" {
            roots.insert(path);
            continue;
        }
        collect_nested_dbm_roots(&path, canonical_dbm, roots)?;
    }
    Ok(())
}

fn merge_file(source: &Path, destination: &Path) -> io::Result<()> {
    if !destination.exists() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        return Ok(());
    }
    if source.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
        return merge_jsonl(source, destination);
    }
    if source_wins(source, destination)? {
        fs::copy(source, destination)?;
    }
    Ok(())
}

fn merge_jsonl(source: &Path, destination: &Path) -> io::Result<()> {
    let destination_body = fs::read_to_string(destination).unwrap_or_default();
    let source_body = fs::read_to_string(source).unwrap_or_default();
    let mut lines = BTreeSet::new();
    let mut merged = Vec::new();
    for line in destination_body.lines().chain(source_body.lines()) {
        let trimmed = line.trim();
        if !trimmed.is_empty() && lines.insert(trimmed.to_string()) {
            merged.push(trimmed.to_string());
        }
    }
    let body = if merged.is_empty() {
        String::new()
    } else {
        format!("{}\n", merged.join("\n"))
    };
    atomic_write(destination, body.as_bytes())
}

fn source_wins(source: &Path, destination: &Path) -> io::Result<bool> {
    let source_modified = modified(source)?;
    let destination_modified = modified(destination)?;
    if source_modified != destination_modified {
        return Ok(source_modified > destination_modified);
    }
    Ok(record_count(source)? > record_count(destination)?)
}

fn modified(path: &Path) -> io::Result<SystemTime> {
    Ok(fs::metadata(path)?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH))
}

fn record_count(path: &Path) -> io::Result<usize> {
    let body = fs::read_to_string(path)?;
    if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
        return Ok(json_record_count(&body));
    }
    Ok(body.lines().filter(|line| !line.trim().is_empty()).count())
}

fn json_record_count(body: &str) -> usize {
    let trimmed = body.trim();
    if trimmed.starts_with('[') {
        trimmed.matches("},{").count() + usize::from(trimmed.len() > 2)
    } else if trimmed.starts_with('{') {
        trimmed.matches("\":").count()
    } else {
        0
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("dbm-workspace-{}.tmp", std::process::id()));
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbm_ancestor_is_discovered() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".dbm")).expect("dbm");
        let nested = dir.path().join("apps/cli/src");
        fs::create_dir_all(&nested).expect("nested");

        assert_eq!(
            find_dbm_ancestor(&nested),
            Some(dir.path().canonicalize().expect("canonical"))
        );
    }

    #[test]
    fn legacy_roots_merge_into_canonical_without_jsonl_loss() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join(".dbm/logs")).expect("canonical");
        fs::write(
            root.join(".dbm/logs/holographic_memory_observation.jsonl"),
            "{\"id\":1}\n",
        )
        .expect("canonical log");
        for legacy in ["apps/cli/.dbm", "apps/cli/src/.dbm"] {
            fs::create_dir_all(root.join(legacy).join("logs")).expect("legacy");
        }
        fs::create_dir_all(root.join("apps/cli/apps/cli/src/.dbm/logs")).expect("nested legacy");
        fs::write(
            root.join("apps/cli/.dbm/logs/holographic_memory_observation.jsonl"),
            "{\"id\":2}\n",
        )
        .expect("legacy log");
        fs::write(
            root.join("apps/cli/src/.dbm/logs/holographic_memory_observation.jsonl"),
            "{\"id\":3}\n",
        )
        .expect("legacy log");
        fs::write(
            root.join("apps/cli/apps/cli/src/.dbm/logs/holographic_memory_observation.jsonl"),
            "{\"id\":4}\n",
        )
        .expect("nested legacy log");

        let report =
            WorkspaceRoot::migrate_legacy_layout_from(root, root).expect("migration succeeds");
        let merged =
            fs::read_to_string(root.join(".dbm/logs/holographic_memory_observation.jsonl"))
                .expect("merged");

        assert_eq!(report.migrated_roots.len(), 3);
        assert!(merged.contains("\"id\":1"));
        assert!(merged.contains("\"id\":2"));
        assert!(merged.contains("\"id\":3"));
        assert!(merged.contains("\"id\":4"));
        assert!(!root.join("apps/cli/.dbm").exists());
        assert!(!root.join("apps/cli/src/.dbm").exists());
        assert!(!root.join("apps/cli/apps/cli/src/.dbm").exists());
    }

    #[test]
    fn canonical_layout_is_created() {
        let dir = tempfile::tempdir().expect("tempdir");
        WorkspaceRoot::ensure_layout(dir.path()).expect("layout");
        for directory in ["analyze", "memory", "logs", "mutations", "runtime"] {
            assert!(dir.path().join(".dbm").join(directory).is_dir());
        }
    }
}
