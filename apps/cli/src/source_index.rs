use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QualifiedModuleId {
    pub crate_name: String,
    pub module_path: String,
}

#[derive(Debug, Clone)]
struct SourceEntry {
    id: QualifiedModuleId,
    path: PathBuf,
    priority: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleSourceIndex {
    by_qualified: BTreeMap<QualifiedModuleId, PathBuf>,
    by_bare: BTreeMap<String, Vec<SourceEntry>>,
    all_paths: Vec<PathBuf>,
    preferred_crate: Option<String>,
}

impl ModuleSourceIndex {
    pub fn build(root: &Path) -> Result<Self, String> {
        let preferred_crate = preferred_crate_name(root);
        let mut files = Vec::new();
        for include_root in include_roots(root) {
            collect_source_files(root, &include_root, &mut files)?;
        }
        files.sort();
        files.dedup();

        let mut by_qualified = BTreeMap::new();
        let mut by_bare = BTreeMap::<String, Vec<SourceEntry>>::new();
        for relative in &files {
            let Some(id) = qualified_module_id(relative, preferred_crate.as_deref()) else {
                continue;
            };
            let priority = path_priority(relative, preferred_crate.as_deref());
            by_qualified.insert(id.clone(), relative.clone());
            by_bare
                .entry(normalize_key(&id.module_path))
                .or_default()
                .push(SourceEntry {
                    id,
                    path: relative.clone(),
                    priority,
                });
        }

        for entries in by_bare.values_mut() {
            entries.sort_by(|lhs, rhs| {
                lhs.priority
                    .cmp(&rhs.priority)
                    .then_with(|| lhs.id.crate_name.cmp(&rhs.id.crate_name))
                    .then_with(|| lhs.path.cmp(&rhs.path))
            });
        }

        Ok(Self {
            by_qualified,
            by_bare,
            all_paths: files,
            preferred_crate,
        })
    }

    pub fn resolve(&self, module: &str) -> Result<Option<PathBuf>, String> {
        if let Some(id) = parse_qualified_module_id(module) {
            return Ok(self.by_qualified.get(&id).cloned());
        }

        let key = normalize_key(module);
        let Some(entries) = self.by_bare.get(&key) else {
            return Ok(None);
        };
        if entries.len() == 1 {
            return Ok(Some(entries[0].path.clone()));
        }

        let best_priority = entries[0].priority;
        let best = entries
            .iter()
            .take_while(|entry| entry.priority == best_priority)
            .collect::<Vec<_>>();
        if best.len() == 1 {
            return Ok(Some(best[0].path.clone()));
        }

        Err(format!(
            "ambiguous module path for {module}: {}. use --target to select the apply file",
            best.iter()
                .map(|entry| format!("{} -> {}", display_id(&entry.id), entry.path.display()))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    pub fn bind_graph_node(
        &self,
        module: &str,
    ) -> Option<(QualifiedModuleId, PathBuf)> {
        if let Some(id) = parse_qualified_module_id(module) {
            return self.by_qualified.get(&id).cloned().map(|path| (id, path));
        }

        if let Ok(Some(path)) = self.resolve(module) {
            if let Some(id) = qualified_module_id(&path, self.preferred_crate.as_deref()) {
                return Some((id, path));
            }
        }

        let candidates = self.fuzzy_candidates(module);
        if candidates.is_empty() {
            return None;
        }
        let best_priority = candidates[0].priority;
        let best = candidates
            .into_iter()
            .filter(|entry| entry.priority == best_priority)
            .collect::<Vec<_>>();
        if best.len() == 1 {
            let entry = &best[0];
            return Some((entry.id.clone(), entry.path.clone()));
        }
        None
    }

    pub fn generated_sibling_path(
        &self,
        module: &str,
        file_name: &str,
    ) -> Result<PathBuf, String> {
        let path = self
            .resolve(module)?
            .ok_or_else(|| format!("unable to resolve source path for module {module}"))?;
        let parent = path
            .parent()
            .ok_or_else(|| format!("module path has no parent: {}", path.display()))?;
        Ok(parent.join(file_name))
    }

    pub fn generated_path(&self, root: &Path, module: &str, file_name: &str) -> PathBuf {
        if let Ok(path) = self.generated_sibling_path(module, file_name) {
            return path;
        }
        if let Some(crate_name) = self.preferred_crate.as_deref() {
            let current = current_crate_src_root(root, crate_name);
            if current.is_dir() {
                return relative_to_root(root, &current.join(file_name));
            }
        }
        if root.join("src").is_dir() {
            return PathBuf::from("src").join(file_name);
        }
        if let Some(parent) = self.all_paths.first().and_then(|path| path.parent()) {
            return parent.join(file_name);
        }
        PathBuf::from(file_name)
    }

    pub fn all_paths(&self) -> &[PathBuf] {
        &self.all_paths
    }

    pub fn all_bindings(&self) -> Vec<(QualifiedModuleId, PathBuf)> {
        self.by_qualified
            .iter()
            .map(|(id, path)| (id.clone(), path.clone()))
            .collect()
    }

    fn fuzzy_candidates(&self, module: &str) -> Vec<SourceEntry> {
        let terms = semantic_terms(module);
        let mut matches = self
            .by_qualified
            .iter()
            .filter(|(_, path)| {
                let haystack = path.display().to_string().to_ascii_lowercase();
                terms.iter().any(|term| haystack.contains(term))
            })
            .map(|(id, path)| SourceEntry {
                id: id.clone(),
                path: path.clone(),
                priority: path_priority(path, self.preferred_crate.as_deref()),
            })
            .collect::<Vec<_>>();
        matches.sort_by(|lhs, rhs| {
            lhs.priority
                .cmp(&rhs.priority)
                .then_with(|| lhs.id.crate_name.cmp(&rhs.id.crate_name))
                .then_with(|| lhs.path.cmp(&rhs.path))
        });
        matches
    }
}

fn include_roots(root: &Path) -> Vec<PathBuf> {
    let scoped = ["apps", "core", "contracts"]
        .into_iter()
        .map(|name| root.join(name))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    if scoped.is_empty() {
        vec![root.to_path_buf()]
    } else {
        scoped
    }
}

fn collect_source_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read {}: {err}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let relative = relative_to_root(root, &path);
        if should_exclude(&relative) {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if file_type.is_dir() {
            collect_source_files(root, &path, files)?;
            continue;
        }

        if !file_type.is_file() || !is_supported_source_file(&path) {
            continue;
        }

        files.push(relative);
    }

    Ok(())
}

fn should_exclude(relative: &Path) -> bool {
    let parts = path_parts(relative);
    if parts.is_empty() {
        return false;
    }
    if parts.windows(2).any(|window| window == ["tests", "fixtures"]) {
        return true;
    }
    parts
        .iter()
        .any(|part| matches!(*part, ".git" | ".dbm" | "node_modules" | "target" | "tmp" | "sandbox"))
}

fn is_supported_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "py")
    )
}

fn qualified_module_id(relative: &Path, preferred_crate: Option<&str>) -> Option<QualifiedModuleId> {
    let crate_name = crate_name(relative).or_else(|| preferred_crate.map(ToString::to_string))?;
    let module_path = module_path(relative, preferred_crate)?;
    Some(QualifiedModuleId {
        crate_name: normalize_key(&crate_name),
        module_path: normalize_key(&module_path),
    })
}

fn parse_qualified_module_id(value: &str) -> Option<QualifiedModuleId> {
    let (crate_name, module_path) = value.split_once("::")?;
    Some(QualifiedModuleId {
        crate_name: normalize_key(crate_name),
        module_path: normalize_key(module_path),
    })
}

fn crate_name(relative: &Path) -> Option<String> {
    let parts = path_parts(relative);
    parts.windows(2).find_map(|window| {
        (window[1] == "src").then(|| window[0].to_string())
    })
}

fn module_path(relative: &Path, preferred_crate: Option<&str>) -> Option<String> {
    let parts = path_parts(relative);
    let src_index = parts.iter().position(|part| *part == "src")?;
    let after_src = &parts[src_index + 1..];
    if after_src.is_empty() {
        return None;
    }

    let file_name = *after_src.last()?;
    if matches!(file_name, "mod.rs" | "index.ts" | "index.tsx" | "index.js" | "index.jsx") {
        let modules = &after_src[..after_src.len().saturating_sub(1)];
        return Some(modules.join("::"));
    }

    let stem = Path::new(file_name).file_stem()?.to_str()?;
    if matches!(stem, "lib" | "main") {
        return if after_src.len() >= 2 {
            after_src
                .get(after_src.len() - 2)
                .map(|part| (*part).to_string())
        } else {
            crate_name(relative).or_else(|| preferred_crate.map(ToString::to_string))
        };
    }

    let mut modules = after_src[..after_src.len().saturating_sub(1)]
        .iter()
        .map(|part| (*part).to_string())
        .collect::<Vec<_>>();
    modules.push(stem.to_string());
    Some(modules.join("::"))
}

fn path_priority(relative: &Path, preferred_crate: Option<&str>) -> u8 {
    let id = qualified_module_id(relative, preferred_crate);
    if let (Some(id), Some(preferred)) = (id.as_ref(), preferred_crate) {
        if id.crate_name == normalize_key(preferred) {
            return 0;
        }
    }

    let parts = path_parts(relative);
    match parts.as_slice() {
        ["apps", ..] => 1,
        ["core", ..] => 2,
        ["contracts", ..] => 3,
        _ => 4,
    }
}

fn preferred_crate_name(root: &Path) -> Option<String> {
    if root.join("apps/cli/src").is_dir() {
        return Some("cli".to_string());
    }
    if root.join("src").is_dir() {
        return root
            .file_name()
            .and_then(|name| name.to_str())
            .map(normalize_key);
    }
    None
}

fn current_crate_src_root(root: &Path, crate_name: &str) -> PathBuf {
    let apps = root.join("apps").join(crate_name).join("src");
    if apps.is_dir() {
        return apps;
    }
    let core = root.join("core").join(crate_name).join("src");
    if core.is_dir() {
        return core;
    }
    let contracts = root.join("contracts").join(crate_name).join("src");
    if contracts.is_dir() {
        return contracts;
    }
    root.join("src")
}

fn relative_to_root(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

fn path_parts(path: &Path) -> Vec<&str> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect()
}

fn display_id(id: &QualifiedModuleId) -> String {
    format!("{}::{}", id.crate_name, id.module_path)
}

fn normalize_key(value: &str) -> String {
    value.replace('-', "_")
}

fn semantic_terms(value: &str) -> Vec<String> {
    let key = normalize_key(value).to_ascii_lowercase();
    let mut terms = vec![key.clone()];
    match key.as_str() {
        "determinism" => terms.extend([
            "replay".to_string(),
            "controller".to_string(),
        ]),
        "replay" => terms.extend([
            "determinism".to_string(),
            "controller".to_string(),
        ]),
        "controller" => terms.extend([
            "determinism".to_string(),
            "replay".to_string(),
        ]),
        _ => {}
    }
    terms.sort();
    terms.dedup();
    terms
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("design_cli_source_index_{name}_{unique}"));
        fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    #[test]
    fn resolves_workspace_flat_files_and_nested_libs() {
        let root = temp_dir("workspace");
        fs::create_dir_all(root.join("apps/cli/src")).expect("cli src");
        fs::create_dir_all(root.join("apps/viewer/src")).expect("viewer src");
        fs::create_dir_all(root.join("core/world/src")).expect("world src");
        fs::create_dir_all(root.join("tests/fixtures/architecture_clean/src/app"))
            .expect("fixture src");
        fs::write(root.join("apps/cli/src/app.rs"), "fn app() {}\n").expect("app");
        fs::write(
            root.join("apps/viewer/src/renderer.rs"),
            "fn renderer() {}\n",
        )
        .expect("renderer");
        fs::write(root.join("core/world/src/lib.rs"), "pub fn world() {}\n").expect("world");
        fs::write(
            root.join("tests/fixtures/architecture_clean/src/app/mod.rs"),
            "fn fixture() {}\n",
        )
        .expect("fixture");

        let index = ModuleSourceIndex::build(&root).expect("index");
        assert_eq!(
            index.resolve("app").expect("resolve app"),
            Some(PathBuf::from("apps/cli/src/app.rs"))
        );
        assert_eq!(
            index.resolve("cli::app").expect("resolve cli::app"),
            Some(PathBuf::from("apps/cli/src/app.rs"))
        );
        assert_eq!(
            index.resolve("viewer::renderer").expect("resolve viewer::renderer"),
            Some(PathBuf::from("apps/viewer/src/renderer.rs"))
        );
        assert_eq!(
            index.resolve("world").expect("resolve world"),
            Some(PathBuf::from("core/world/src/lib.rs"))
        );
    }

    #[test]
    fn ambiguous_same_priority_requires_target_override() {
        let root = temp_dir("ambiguous");
        fs::create_dir_all(root.join("apps/gui/src")).expect("gui src");
        fs::create_dir_all(root.join("apps/viewer_gui/src")).expect("viewer_gui src");
        fs::write(root.join("apps/gui/src/app.rs"), "fn gui() {}\n").expect("gui app");
        fs::write(
            root.join("apps/viewer_gui/src/app.rs"),
            "fn viewer_gui() {}\n",
        )
        .expect("viewer app");

        let index = ModuleSourceIndex::build(&root).expect("index");
        let error = index.resolve("app").expect_err("ambiguous");
        assert!(error.contains("use --target"), "{error}");
    }
}
