use std::collections::HashSet;
use code_ir::{CodeIr, IrModule};
use crate::{
    dep_graph::{build_import_graph, detect_cycle},
    error::CodegenError,
    generator::generate_function,
    spec::LanguageSpec,
};

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectOutput {
    pub files: Vec<ProjectFile>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Generate a multi-file project from `CodeIr.ir_modules`.
///
/// Processing order:
/// 1. Validate no duplicate module names.
/// 2. Validate all import targets exist in the module list.
/// 3. Cycle detection on the import graph.
/// 4. Validate no duplicate function symbols within each module.
/// 5. Generate files (sorted alphabetically by module name for determinism).
pub fn generate_project(
    ir: &CodeIr,
    spec: &LanguageSpec,
) -> Result<ProjectOutput, CodegenError> {
    let modules = &ir.ir_modules;

    validate_no_duplicate_modules(modules)?;

    let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    validate_imports_exist(modules, &module_names)?;

    let graph_edges: Vec<(String, Vec<String>)> = modules
        .iter()
        .map(|m| {
            let deps: Vec<String> = m.imports.iter().map(|i| i.module.clone()).collect();
            (m.name.clone(), deps)
        })
        .collect();
    let adj = build_import_graph(&graph_edges);
    detect_cycle(&module_names, &adj)?;

    for module in modules {
        validate_no_duplicate_symbols(module)?;
    }

    // Deterministic output order: sort alphabetically by module name.
    let mut sorted: Vec<&IrModule> = modules.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));

    let mut files = Vec::new();
    for module in sorted {
        let path = module_file_path(&module.name, spec);
        let content = generate_module_content(module, spec)?;
        files.push(ProjectFile { path, content });
    }

    Ok(ProjectOutput { files })
}

// ── Validation ────────────────────────────────────────────────────────────────

fn validate_no_duplicate_modules(modules: &[IrModule]) -> Result<(), CodegenError> {
    let mut seen = HashSet::new();
    for m in modules {
        if !seen.insert(m.name.as_str()) {
            return Err(CodegenError::DuplicateModule { name: m.name.clone() });
        }
    }
    Ok(())
}

fn validate_imports_exist(
    modules: &[IrModule],
    known: &[String],
) -> Result<(), CodegenError> {
    for m in modules {
        for imp in &m.imports {
            if !known.iter().any(|n| n == &imp.module) {
                return Err(CodegenError::ModuleNotFound { name: imp.module.clone() });
            }
        }
    }
    Ok(())
}

fn validate_no_duplicate_symbols(module: &IrModule) -> Result<(), CodegenError> {
    let mut seen = HashSet::new();
    for f in &module.functions {
        if !seen.insert(f.name.as_str()) {
            return Err(CodegenError::DuplicateSymbol {
                module: module.name.clone(),
                name: f.name.clone(),
            });
        }
    }
    Ok(())
}

// ── File path ─────────────────────────────────────────────────────────────────

fn module_file_path(name: &str, spec: &LanguageSpec) -> String {
    let snake = to_snake_case(name);
    match spec.name.as_str() {
        "python" => format!("{snake}.py"),
        _ => format!("src/{snake}.rs"),
    }
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ── Content generation ────────────────────────────────────────────────────────

fn generate_module_content(
    module: &IrModule,
    spec: &LanguageSpec,
) -> Result<String, CodegenError> {
    let mut out = String::new();

    // Imports: sort by (module, item) for determinism.
    let mut flat_imports: Vec<(&str, &str)> = module
        .imports
        .iter()
        .flat_map(|imp| imp.items.iter().map(move |item| (imp.module.as_str(), item.as_str())))
        .collect();
    flat_imports.sort_unstable();

    for (module_name, item) in &flat_imports {
        out.push_str(&emit_import_line(module_name, item, spec));
    }

    if !flat_imports.is_empty() && !module.functions.is_empty() {
        out.push('\n');
    }

    // Functions separated by blank lines.
    for (idx, func) in module.functions.iter().enumerate() {
        out.push_str(&generate_function(func, spec)?);
        if idx + 1 < module.functions.len() {
            out.push('\n');
        }
    }

    Ok(out)
}

// ── Import emission ───────────────────────────────────────────────────────────

fn emit_import_line(module_name: &str, item: &str, spec: &LanguageSpec) -> String {
    match spec.name.as_str() {
        "python" => format!("from {module_name} import {item}\n"),
        _ => format!("use {module_name}::{item};\n"),
    }
}
