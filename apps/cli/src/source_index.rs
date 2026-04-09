use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBinding {
    pub file: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyTargetResolution {
    pub module: String,
    pub resolution_strategy: String,
    pub resolved_path: PathBuf,
    pub resolved_relative_path: PathBuf,
    pub sandbox_path: Option<PathBuf>,
}

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
    debug_by_bare: BTreeMap<String, Vec<SourceEntry>>,
    all_paths: Vec<PathBuf>,
    preferred_crate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolUsePathCandidate {
    crate_name: String,
    use_path: String,
    module_path: Option<String>,
    same_crate: bool,
    proof_rank: u8,
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

        let mut debug_files = Vec::new();
        for include_root in include_roots(root) {
            collect_debug_source_files(root, &include_root, &mut debug_files)?;
        }
        debug_files.sort();
        debug_files.dedup();

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
        let mut debug_by_bare = BTreeMap::<String, Vec<SourceEntry>>::new();
        for relative in &debug_files {
            let Some(id) = qualified_module_id(relative, preferred_crate.as_deref()) else {
                continue;
            };
            let priority = path_priority(relative, preferred_crate.as_deref());
            debug_by_bare
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
        for entries in debug_by_bare.values_mut() {
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
            debug_by_bare,
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

    pub fn bind_graph_node(&self, module: &str) -> Option<(QualifiedModuleId, PathBuf)> {
        if let Some(id) = parse_qualified_module_id(module) {
            return self.by_qualified.get(&id).cloned().map(|path| (id, path));
        }

        if let Ok(Some(path)) = self.resolve(module) {
            if let Some(id) = qualified_module_id(&path, self.preferred_crate.as_deref()) {
                return Some((id, path));
            }
        }
        None
    }

    pub fn bind_graph_node_debug_fallback(
        &self,
        module: &str,
    ) -> Option<(QualifiedModuleId, PathBuf)> {
        let key = normalize_key(module);
        if let Some(entries) = self.debug_by_bare.get(&key) {
            let entry = representative_entry(entries, module)?;
            return Some((entry.id.clone(), entry.path.clone()));
        }

        let candidates = self.debug_fuzzy_candidates(module);
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

    pub fn resolve_apply_target(&self, module: &str) -> Option<ApplyTargetResolution> {
        if let Some(id) = parse_qualified_module_id(module) {
            let path = self.by_qualified.get(&id)?.clone();
            return Some(ApplyTargetResolution {
                module: module.to_string(),
                resolution_strategy: resolution_strategy(&path, module).to_string(),
                resolved_relative_path: path.clone(),
                resolved_path: path,
                sandbox_path: None,
            });
        }

        let key = normalize_key(module);
        if let Some(entries) = self.by_bare.get(&key) {
            let entry = representative_entry(entries, module)?;
            return Some(ApplyTargetResolution {
                module: module.to_string(),
                resolution_strategy: resolution_strategy(&entry.path, module).to_string(),
                resolved_relative_path: entry.path.clone(),
                resolved_path: entry.path.clone(),
                sandbox_path: None,
            });
        }

        let entry = self.fuzzy_candidates(module).into_iter().next()?;
        Some(ApplyTargetResolution {
            module: module.to_string(),
            resolution_strategy: "legacy_source_index_fallback".to_string(),
            resolved_relative_path: entry.path.clone(),
            resolved_path: entry.path,
            sandbox_path: None,
        })
    }

    pub fn exact_binding(&self, root: &Path, module: &str) -> Option<SourceBinding> {
        let (_, relative) = self.bind_graph_node(module)?;
        let absolute = root.join(&relative);
        let content = fs::read_to_string(&absolute).ok()?;
        let mut first_code_line = 1;
        let mut last_code_line = 1;
        let mut symbol = None;

        for (index, line) in content.lines().enumerate() {
            let line_no = index + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            if symbol.is_none() {
                first_code_line = line_no;
            }
            last_code_line = line_no;
            if let Some(found) = extract_symbol(trimmed) {
                symbol = Some(found);
                first_code_line = line_no;
                break;
            }
        }

        if symbol.is_some() {
            for (index, line) in content
                .lines()
                .enumerate()
                .skip(first_code_line.saturating_sub(1))
            {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    continue;
                }
                last_code_line = index + 1;
            }
        } else {
            last_code_line = content.lines().count().max(first_code_line);
        }

        Some(SourceBinding {
            file: relative,
            line_start: first_code_line,
            line_end: last_code_line,
            symbol,
        })
    }

    pub fn generated_sibling_path(&self, module: &str, file_name: &str) -> Result<PathBuf, String> {
        let path = self
            .resolve_apply_target(module)
            .map(|resolution| resolution.resolved_path)
            .or_else(|| self.resolve(module).ok().flatten())
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

    pub fn generated_path_from_source(
        &self,
        root: &Path,
        source_path: &Path,
        file_name: &str,
    ) -> PathBuf {
        let absolute = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            root.join(source_path)
        };
        let relative = relative_to_root(root, &absolute);
        relative
            .parent()
            .map(|parent| parent.join(file_name))
            .unwrap_or_else(|| PathBuf::from(file_name))
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

    pub fn qualified_id_for_path(&self, path: &Path) -> Option<QualifiedModuleId> {
        qualified_module_id(path, self.preferred_crate.as_deref())
    }

    pub fn resolve_symbol_module(
        &self,
        root: &Path,
        crate_name: &str,
        current_module: Option<&str>,
        symbol: &str,
    ) -> Result<Option<String>, String> {
        let normalized_crate = normalize_key(crate_name);
        let mut candidates = Vec::<(String, PathBuf)>::new();
        for (id, path) in &self.by_qualified {
            if id.crate_name != normalized_crate
                || path.extension().and_then(|ext| ext.to_str()) != Some("rs")
            {
                continue;
            }
            let absolute = root.join(path);
            let content = fs::read_to_string(&absolute)
                .map_err(|err| format!("failed to read {}: {err}", absolute.display()))?;
            if rust_symbols(&content)
                .iter()
                .any(|candidate| candidate == symbol)
            {
                candidates.push((id.module_path.clone(), path.clone()));
            }
        }
        candidates.sort_by(|lhs, rhs| {
            symbol_candidate_rank(current_module, &lhs.0)
                .cmp(&symbol_candidate_rank(current_module, &rhs.0))
                .then_with(|| lhs.0.cmp(&rhs.0))
                .then_with(|| lhs.1.cmp(&rhs.1))
        });
        Ok(candidates.into_iter().map(|(module, _)| module).next())
    }

    pub fn resolve_symbol_use_path(
        &self,
        root: &Path,
        current_id: &QualifiedModuleId,
        symbol: &str,
    ) -> Result<Option<String>, String> {
        if let Some(module_path) =
            self.resolve_same_crate_symbol_module(root, &current_id.crate_name, current_id, symbol)?
        {
            return Ok(Some(format_symbol_use_path(
                "crate",
                Some(&module_path),
                symbol,
            )));
        }
        let mut candidates = self.collect_workspace_symbol_use_path_candidates(
            root,
            &current_id.crate_name,
            symbol,
        )?;
        candidates.sort_by(|lhs, rhs| {
            workspace_symbol_candidate_rank(symbol, current_id, lhs)
                .cmp(&workspace_symbol_candidate_rank(symbol, current_id, rhs))
                .then_with(|| lhs.use_path.cmp(&rhs.use_path))
        });
        Ok(candidates
            .into_iter()
            .map(|candidate| candidate.use_path)
            .next())
    }

    pub fn crate_root_publicly_exports(
        &self,
        root: &Path,
        crate_name: &str,
        item: &str,
    ) -> Result<bool, String> {
        let normalized_crate = normalize_key(crate_name);
        let root_relative = self.by_qualified.iter().find_map(|(id, path)| {
            (id.crate_name == normalized_crate
                && matches!(
                    path.file_name().and_then(|value| value.to_str()),
                    Some("lib.rs" | "main.rs")
                ))
            .then_some(path.clone())
        });
        let Some(root_relative) = root_relative else {
            return Ok(false);
        };
        let content = fs::read_to_string(root.join(&root_relative)).map_err(|err| {
            format!(
                "failed to read {}: {err}",
                root.join(&root_relative).display()
            )
        })?;
        let item = normalize_key(item);
        Ok(content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == format!("pub mod {item};")
                || trimmed == format!("mod {item};")
                || parse_reexport_name(
                    trimmed
                        .strip_prefix("pub use ")
                        .and_then(|value| value.strip_suffix(';'))
                        .unwrap_or_default(),
                )
                .map(|candidate| normalize_key(&candidate) == item)
                .unwrap_or(false)
        }))
    }

    fn resolve_same_crate_symbol_module(
        &self,
        root: &Path,
        crate_name: &str,
        current_id: &QualifiedModuleId,
        symbol: &str,
    ) -> Result<Option<String>, String> {
        if let Some(preferred_module) = preferred_symbol_module(symbol) {
            if self.crate_module_defines_symbol(root, crate_name, preferred_module, symbol)? {
                return Ok(Some(preferred_module.to_string()));
            }
        }
        self.resolve_symbol_module(root, crate_name, Some(&current_id.module_path), symbol)
    }

    fn crate_module_defines_symbol(
        &self,
        root: &Path,
        crate_name: &str,
        module_path: &str,
        symbol: &str,
    ) -> Result<bool, String> {
        let id = QualifiedModuleId {
            crate_name: normalize_key(crate_name),
            module_path: normalize_key(module_path),
        };
        let Some(path) = self.by_qualified.get(&id) else {
            return Ok(false);
        };
        let content = fs::read_to_string(root.join(path))
            .map_err(|err| format!("failed to read {}: {err}", root.join(path).display()))?;
        Ok(rust_symbols(&content)
            .iter()
            .any(|candidate| candidate == symbol))
    }

    fn collect_workspace_symbol_use_path_candidates(
        &self,
        root: &Path,
        current_crate_name: &str,
        symbol: &str,
    ) -> Result<Vec<SymbolUsePathCandidate>, String> {
        let mut candidates = Vec::new();
        for (id, path) in &self.by_qualified {
            if id.crate_name == normalize_key(current_crate_name)
                || path.extension().and_then(|ext| ext.to_str()) != Some("rs")
            {
                continue;
            }

            let absolute = root.join(path);
            let content = fs::read_to_string(&absolute)
                .map_err(|err| format!("failed to read {}: {err}", absolute.display()))?;
            let import_crate = workspace_import_crate_name(root, path)?;
            if rust_public_symbols(&content)
                .iter()
                .any(|candidate| candidate == symbol)
                && has_public_module_chain(root, path)
            {
                candidates.push(SymbolUsePathCandidate {
                    crate_name: id.crate_name.clone(),
                    use_path: format_symbol_use_path(
                        &import_crate,
                        direct_module_use_prefix(path).as_deref(),
                        symbol,
                    ),
                    module_path: direct_module_use_prefix(path),
                    same_crate: false,
                    proof_rank: 1,
                });
            }
            if public_reexports_symbol(&content, symbol) && has_public_module_chain(root, path) {
                candidates.push(SymbolUsePathCandidate {
                    crate_name: id.crate_name.clone(),
                    use_path: format_symbol_use_path(
                        &import_crate,
                        reexport_module_use_prefix(path).as_deref(),
                        symbol,
                    ),
                    module_path: reexport_module_use_prefix(path),
                    same_crate: false,
                    proof_rank: 0,
                });
            }
        }
        candidates.sort_by(|lhs, rhs| lhs.use_path.cmp(&rhs.use_path));
        candidates.dedup_by(|lhs, rhs| lhs.use_path == rhs.use_path);
        Ok(candidates)
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

    fn debug_fuzzy_candidates(&self, module: &str) -> Vec<SourceEntry> {
        let terms = semantic_terms(module);
        let mut matches = self
            .debug_by_bare
            .values()
            .flat_map(|entries| entries.iter())
            .filter(|entry| {
                let haystack = entry.path.display().to_string().to_ascii_lowercase();
                terms.iter().any(|term| haystack.contains(term))
            })
            .cloned()
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
    let scoped = [
        "apps",
        "crates",
        "tests",
        "tools",
        "xtask",
        "core",
        "contracts",
    ]
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

fn collect_debug_source_files(
    root: &Path,
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read {}: {err}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let relative = relative_to_root(root, &path);
        if should_exclude_debug(&relative) {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if file_type.is_dir() {
            collect_debug_source_files(root, &path, files)?;
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
    if parts
        .windows(2)
        .any(|window| window == ["tests", "fixtures"])
    {
        return true;
    }
    parts.iter().any(|part| {
        matches!(
            *part,
            ".git" | ".dbm" | "node_modules" | "target" | "tmp" | "sandbox" | "golden" | "snapshot"
        )
    }) || parts.contains(&"fixtures")
        || parts.contains(&"examples")
}

fn should_exclude_debug(relative: &Path) -> bool {
    let parts = path_parts(relative);
    if parts.is_empty() {
        return false;
    }
    parts.iter().any(|part| {
        matches!(
            *part,
            ".git" | ".dbm" | "node_modules" | "target" | "tmp" | "sandbox"
        )
    })
}

fn is_supported_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "py")
    )
}

fn qualified_module_id(
    relative: &Path,
    preferred_crate: Option<&str>,
) -> Option<QualifiedModuleId> {
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
    parts
        .windows(2)
        .find_map(|window| (window[1] == "src").then(|| window[0].to_string()))
}

fn module_path(relative: &Path, preferred_crate: Option<&str>) -> Option<String> {
    let parts = path_parts(relative);
    let src_index = parts.iter().position(|part| *part == "src")?;
    let after_src = &parts[src_index + 1..];
    if after_src.is_empty() {
        return None;
    }

    let file_name = *after_src.last()?;
    if matches!(
        file_name,
        "mod.rs" | "index.ts" | "index.tsx" | "index.js" | "index.jsx"
    ) {
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
    source_binding_rank(relative)
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
    let crates = root.join("crates").join(crate_name).join("src");
    if crates.is_dir() {
        return crates;
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

fn representative_entry<'a>(entries: &'a [SourceEntry], module: &str) -> Option<&'a SourceEntry> {
    let normalized = normalize_key(module);
    let module_leaf = normalized.split("::").last().unwrap_or(module);
    entries.iter().min_by(|lhs, rhs| {
        representative_rank(&lhs.path, module_leaf)
            .cmp(&representative_rank(&rhs.path, module_leaf))
            .then_with(|| lhs.path.cmp(&rhs.path))
    })
}

fn representative_rank(path: &Path, module_leaf: &str) -> (u8, u8, usize) {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let parent = path
        .parent()
        .and_then(|value| value.file_name())
        .and_then(|value| value.to_str())
        .map(normalize_key)
        .unwrap_or_default();
    let depth = path_parts(path).len();
    let kind = if file_name == "mod.rs" && parent == module_leaf {
        0
    } else if normalize_key(stem) == module_leaf {
        1
    } else {
        2
    };
    (source_binding_rank(path), kind, depth)
}

pub(crate) fn source_binding_rank(path: &Path) -> u8 {
    if is_production_src(path) {
        0
    } else if is_workspace_crate(path) {
        1
    } else if is_test_support(path) {
        2
    } else if is_tests(path) {
        3
    } else {
        4
    }
}

fn is_production_src(path: &Path) -> bool {
    let normalized = normalized_path(path);
    normalized.contains("/src/")
        && !normalized.contains("/tests/")
        && !normalized.contains("/fixtures/")
        && !normalized.contains("/examples/")
}

fn is_workspace_crate(path: &Path) -> bool {
    normalized_path(path).starts_with("crates/")
}

fn is_test_support(path: &Path) -> bool {
    let normalized = normalized_path(path);
    normalized.contains("/tests/support/")
        || normalized.contains("/tests/integration/support/")
        || normalized.contains("/test_support/")
}

fn is_tests(path: &Path) -> bool {
    normalized_path(path).contains("/tests/")
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn resolution_strategy(path: &Path, module: &str) -> &'static str {
    let normalized = normalize_key(module);
    let module_leaf = normalized.split("::").last().unwrap_or(module);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let parent = path
        .parent()
        .and_then(|value| value.file_name())
        .and_then(|value| value.to_str())
        .map(normalize_key)
        .unwrap_or_default();
    if file_name == "mod.rs" && parent == module_leaf {
        "directory_mod_rs"
    } else if normalize_key(stem) == module_leaf {
        "flat_module_file"
    } else {
        "shortest_path_fallback"
    }
}

fn semantic_terms(value: &str) -> Vec<String> {
    let key = normalize_key(value).to_ascii_lowercase();
    let mut terms = vec![key.clone()];
    match key.as_str() {
        "determinism" => terms.extend(["replay".to_string(), "controller".to_string()]),
        "replay" => terms.extend(["determinism".to_string(), "controller".to_string()]),
        "controller" => terms.extend(["determinism".to_string(), "replay".to_string()]),
        _ => {}
    }
    terms.sort();
    terms.dedup();
    terms
}

fn extract_symbol(line: &str) -> Option<String> {
    let patterns = [
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "pub fn ",
        "fn ",
    ];
    for pattern in patterns {
        if let Some(rest) = line.strip_prefix(pattern) {
            let symbol = rest
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
                .unwrap_or_default();
            if !symbol.is_empty() {
                return Some(symbol.to_string());
            }
        }
    }
    None
}

fn rust_symbols(content: &str) -> Vec<String> {
    let mut symbols = content
        .lines()
        .filter_map(|line| extract_symbol(line.trim()))
        .collect::<Vec<_>>();
    symbols.sort();
    symbols.dedup();
    symbols
}

fn symbol_candidate_rank(current_module: Option<&str>, candidate: &str) -> (usize, usize) {
    let depth = candidate.split("::").count();
    let shared_prefix = current_module
        .map(|module| {
            module
                .split("::")
                .zip(candidate.split("::"))
                .take_while(|(lhs, rhs)| lhs == rhs)
                .count()
        })
        .unwrap_or(0);
    (usize::MAX.saturating_sub(shared_prefix), depth)
}

fn workspace_symbol_candidate_rank(
    symbol: &str,
    current_id: &QualifiedModuleId,
    candidate: &SymbolUsePathCandidate,
) -> (u8, u8, usize, usize, String, String) {
    let semantic_priority = preferred_symbol_module(symbol)
        .map(|preferred| {
            if candidate
                .module_path
                .as_deref()
                .map(|module| module == preferred || module.starts_with(&format!("{preferred}::")))
                .unwrap_or(false)
            {
                0
            } else {
                1
            }
        })
        .unwrap_or(1);
    let shared_prefix = candidate
        .module_path
        .as_deref()
        .map(|module| {
            current_id
                .module_path
                .split("::")
                .zip(module.split("::"))
                .take_while(|(lhs, rhs)| lhs == rhs)
                .count()
        })
        .unwrap_or(0);
    let depth = candidate
        .module_path
        .as_deref()
        .map(|module| module.split("::").count())
        .unwrap_or(0);
    (
        semantic_priority,
        candidate.proof_rank,
        usize::MAX.saturating_sub(shared_prefix),
        depth,
        candidate.crate_name.clone(),
        candidate.use_path.clone(),
    )
}

fn preferred_symbol_module(symbol: &str) -> Option<&'static str> {
    if symbol.starts_with("Agent") || symbol == "DomainError" {
        Some("domain")
    } else {
        None
    }
}

fn rust_public_symbols(content: &str) -> Vec<String> {
    let patterns = [
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "pub fn ",
        "pub type ",
        "pub const ",
    ];
    let mut symbols = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        for pattern in patterns {
            if let Some(rest) = trimmed.strip_prefix(pattern) {
                let symbol = rest
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or_default();
                if !symbol.is_empty() {
                    symbols.push(symbol.to_string());
                }
                break;
            }
        }
    }
    symbols.sort();
    symbols.dedup();
    symbols
}

fn public_reexports_symbol(content: &str, symbol: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        let Some(remainder) = trimmed
            .strip_prefix("pub use ")
            .and_then(|value| value.strip_suffix(';'))
        else {
            return false;
        };
        if let Some((_, tail)) = remainder.split_once('{') {
            return tail
                .trim_end_matches('}')
                .split(',')
                .filter_map(parse_reexport_name)
                .any(|candidate| candidate == symbol);
        }
        parse_reexport_name(remainder)
            .map(|candidate| candidate == symbol)
            .unwrap_or(false)
    })
}

fn parse_reexport_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((_, alias)) = trimmed.rsplit_once(" as ") {
        let alias = alias.trim();
        if !alias.is_empty() {
            return Some(alias.to_string());
        }
    }
    trimmed
        .rsplit("::")
        .next()
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(ToString::to_string)
}

fn format_symbol_use_path(crate_import: &str, module_path: Option<&str>, symbol: &str) -> String {
    match module_path.filter(|value| !value.is_empty()) {
        Some(module_path) => format!("{crate_import}::{module_path}::{symbol}"),
        None => format!("{crate_import}::{symbol}"),
    }
}

fn direct_module_use_prefix(relative: &Path) -> Option<String> {
    module_segments(relative)
}

fn reexport_module_use_prefix(relative: &Path) -> Option<String> {
    let module = module_path(relative, None)?;
    if is_root_module_relative(relative) {
        None
    } else {
        Some(module)
    }
}

fn module_segments(relative: &Path) -> Option<String> {
    let parts = path_parts(relative);
    let src_index = parts.iter().position(|part| *part == "src")?;
    let after_src = &parts[src_index + 1..];
    if after_src.is_empty() {
        return None;
    }
    let file_name = *after_src.last()?;
    if matches!(file_name, "lib.rs" | "main.rs") {
        return None;
    }
    if file_name == "mod.rs" {
        return Some(after_src[..after_src.len().saturating_sub(1)].join("::"));
    }
    let stem = Path::new(file_name).file_stem()?.to_str()?;
    let mut modules = after_src[..after_src.len().saturating_sub(1)]
        .iter()
        .map(|part| (*part).to_string())
        .collect::<Vec<_>>();
    modules.push(stem.to_string());
    Some(modules.join("::"))
}

fn is_root_module_relative(relative: &Path) -> bool {
    matches!(
        relative.file_name().and_then(|value| value.to_str()),
        Some("lib.rs" | "main.rs")
    )
}

fn has_public_module_chain(root: &Path, relative: &Path) -> bool {
    let segments = match module_segments(relative) {
        Some(segments) if !segments.is_empty() => segments
            .split("::")
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        _ => return true,
    };
    let src_root = match src_root_relative(relative) {
        Some(path) => path,
        None => return false,
    };
    let mut current_module_file = match root_module_file(root, &src_root) {
        Some(path) => path,
        None => return false,
    };
    let mut current_dir = src_root.clone();
    for segment in segments {
        let content = match fs::read_to_string(root.join(&current_module_file)) {
            Ok(content) => content,
            Err(_) => return false,
        };
        if !contains_public_mod_declaration(&content, &segment) {
            return false;
        }
        current_dir.push(&segment);
        current_module_file = if root.join(current_dir.join("mod.rs")).exists() {
            current_dir.join("mod.rs")
        } else {
            current_dir.with_extension("rs")
        };
    }
    true
}

fn contains_public_mod_declaration(content: &str, module: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == format!("pub mod {module};") || trimmed == format!("pub(crate) mod {module};")
    })
}

fn src_root_relative(relative: &Path) -> Option<PathBuf> {
    let parts = path_parts(relative);
    let src_index = parts.iter().position(|part| *part == "src")?;
    Some(
        parts[..=src_index]
            .iter()
            .fold(PathBuf::new(), |mut path, part| {
                path.push(part);
                path
            }),
    )
}

fn root_module_file(root: &Path, src_root: &Path) -> Option<PathBuf> {
    for candidate in ["lib.rs", "main.rs"] {
        let path = src_root.join(candidate);
        if root.join(&path).exists() {
            return Some(path);
        }
    }
    None
}

fn workspace_import_crate_name(root: &Path, relative: &Path) -> Result<String, String> {
    let manifest = manifest_for_source_path(root, relative)
        .ok_or_else(|| format!("failed to resolve Cargo.toml for {}", relative.display()))?;
    parse_manifest_package_name(&manifest).map(|name| normalize_key(&name))
}

fn manifest_for_source_path(root: &Path, relative: &Path) -> Option<PathBuf> {
    let absolute = root.join(relative);
    let mut current = absolute.parent()?.to_path_buf();
    loop {
        let manifest = current.join("Cargo.toml");
        if manifest.exists() {
            return Some(manifest);
        }
        if current == root {
            break;
        }
        current = current.parent()?.to_path_buf();
    }
    None
}

fn parse_manifest_package_name(manifest: &Path) -> Result<String, String> {
    let content = fs::read_to_string(manifest)
        .map_err(|err| format!("failed to read {}: {err}", manifest.display()))?;
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("name") else {
            continue;
        };
        let Some(value) = rest.split('=').nth(1) else {
            continue;
        };
        let package = value.trim().trim_matches('"');
        if !package.is_empty() {
            return Ok(package.to_string());
        }
    }
    Err(format!(
        "failed to parse package.name from {}",
        manifest.display()
    ))
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
            index
                .resolve("viewer::renderer")
                .expect("resolve viewer::renderer"),
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

    #[ignore]
    #[test]
    fn exact_binding_returns_first_symbol_line() {
        let root = temp_dir("exact_binding");
        fs::create_dir_all(root.join("src/runtime")).expect("runtime src");
        fs::write(
            root.join("src/runtime/determinism.rs"),
            "// comment\n\npub fn check() {}\n",
        )
        .expect("determinism");

        let index = ModuleSourceIndex::build(&root).expect("index");
        let binding = index
            .exact_binding(&root, "determinism")
            .expect("exact binding");
        assert_eq!(binding.file, PathBuf::from("src/runtime/determinism.rs"));
        assert_eq!(binding.line_start, 3);
        assert_eq!(binding.symbol.as_deref(), Some("check"));
    }

    #[test]
    fn reverse_lookup_symbol_prefers_same_crate_and_nearest_module() {
        let root = temp_dir("reverse_lookup_symbol");
        fs::create_dir_all(root.join("crates/execution_stability_core/src/domain"))
            .expect("domain dir");
        fs::create_dir_all(root.join("crates/execution_stability_core/src/controller"))
            .expect("controller dir");
        fs::write(
            root.join("crates/execution_stability_core/src/lib.rs"),
            "pub mod domain;\npub mod controller;\n",
        )
        .expect("lib");
        fs::write(
            root.join("crates/execution_stability_core/src/domain/mod.rs"),
            "pub struct AgentInput;\npub struct AgentOutput;\npub enum DomainError {}\n",
        )
        .expect("domain");
        fs::write(
            root.join("crates/execution_stability_core/src/controller/mod.rs"),
            "pub fn noop() {}\n",
        )
        .expect("controller");

        let index = ModuleSourceIndex::build(&root).expect("index");
        let module = index
            .resolve_symbol_module(
                &root,
                "execution_stability_core",
                Some("controller"),
                "AgentInput",
            )
            .expect("lookup");
        assert_eq!(module.as_deref(), Some("domain"));
    }

    #[test]
    fn resolve_symbol_use_path_prefers_domain_module_for_agent_symbols() {
        let root = temp_dir("resolve_symbol_use_path_domain");
        fs::create_dir_all(root.join("crates/execution_stability_core/src/domain"))
            .expect("domain dir");
        fs::create_dir_all(root.join("crates/execution_stability_core/src/controller"))
            .expect("controller dir");
        fs::write(
            root.join("crates/execution_stability_core/Cargo.toml"),
            "[package]\nname = \"execution_stability_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("crates/execution_stability_core/src/lib.rs"),
            "pub mod domain;\npub mod controller;\n",
        )
        .expect("lib");
        fs::write(
            root.join("crates/execution_stability_core/src/domain/mod.rs"),
            "pub struct AgentInput;\n",
        )
        .expect("domain");
        fs::write(
            root.join("crates/execution_stability_core/src/controller/mod.rs"),
            "pub struct AgentInput;\n",
        )
        .expect("controller");

        let index = ModuleSourceIndex::build(&root).expect("index");
        let use_path = index
            .resolve_symbol_use_path(
                &root,
                &QualifiedModuleId {
                    crate_name: "execution_stability_core".to_string(),
                    module_path: "controller".to_string(),
                },
                "AgentInput",
            )
            .expect("resolve use path");
        assert_eq!(use_path.as_deref(), Some("crate::domain::AgentInput"));
    }

    #[test]
    fn resolve_symbol_use_path_rebinds_to_workspace_public_crate() {
        let root = temp_dir("resolve_symbol_use_path_workspace");
        fs::create_dir_all(root.join("crates/agent_core/src/engine")).expect("engine dir");
        fs::create_dir_all(root.join("crates/execution_core/src/dependency"))
            .expect("dependency dir");
        fs::write(
            root.join("crates/agent_core/Cargo.toml"),
            "[package]\nname = \"agent_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nexecution_core = { path = \"../execution_core\" }\n",
        )
        .expect("agent cargo");
        fs::write(
            root.join("crates/execution_core/Cargo.toml"),
            "[package]\nname = \"execution_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("execution cargo");
        fs::write(
            root.join("crates/agent_core/src/lib.rs"),
            "pub mod engine;\n",
        )
        .expect("agent lib");
        fs::write(
            root.join("crates/agent_core/src/engine/mod.rs"),
            "pub fn run() {}\n",
        )
        .expect("engine");
        fs::write(
            root.join("crates/execution_core/src/lib.rs"),
            "pub mod dependency;\n",
        )
        .expect("execution lib");
        fs::write(
            root.join("crates/execution_core/src/dependency/mod.rs"),
            "pub mod dependency_engine_interface;\n",
        )
        .expect("dependency mod");
        fs::write(
            root.join("crates/execution_core/src/dependency/dependency_engine_interface.rs"),
            "pub trait DependencyEngineInterface {}\n",
        )
        .expect("interface");

        let index = ModuleSourceIndex::build(&root).expect("index");
        let use_path = index
            .resolve_symbol_use_path(
                &root,
                &QualifiedModuleId {
                    crate_name: "agent_core".to_string(),
                    module_path: "engine".to_string(),
                },
                "DependencyEngineInterface",
            )
            .expect("resolve use path");
        assert_eq!(
            use_path.as_deref(),
            Some(
                "execution_core::dependency::dependency_engine_interface::DependencyEngineInterface"
            )
        );
    }
}
