use architecture_reasoner::{ArchitectureGraph, ReverseArchitectureReasoner};
use code_ir::CodeIr;
use design_domain::{
    Architecture, ClassUnit, Dependency, DependencyKind, DesignUnit, DesignUnitId, StructureUnit,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSourceFile {
    pub path: String,
    pub source: String,
}

#[derive(Clone, Debug, Default)]
pub struct CodeLanguageCore {
    reasoner: ReverseArchitectureReasoner,
}

impl CodeLanguageCore {
    pub fn parse_sources(&self, files: &[ParsedSourceFile]) -> CodeIr {
        let mut units = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let module_name = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(file.path.as_str())
                .trim_end_matches(".rs")
                .trim_end_matches(".ts")
                .trim_end_matches(".py")
                .to_string();
            let mut unit = DesignUnit::new(index as u64 + 1, module_name);
            for line in file.source.lines() {
                let trimmed = line.trim();
                if let Some(name) = parse_symbol(trimmed, "fn ") {
                    unit.outputs.push(name.to_string());
                }
                if let Some(name) = parse_symbol(trimmed, "struct ") {
                    unit.semantics.push(format!("owns {}", name));
                }
                if let Some(name) = parse_use(trimmed) {
                    unit.inputs.push(name.to_string());
                }
            }
            units.push(unit);
        }

        for index in 0..units.len().saturating_sub(1) {
            let target = units[index + 1].id;
            units[index].dependencies.push(target);
        }

        CodeIr::from_design_units(&units)
    }

    pub fn reverse_architecture(&self, files: &[ParsedSourceFile]) -> ArchitectureGraph {
        let ir = self.parse_sources(files);
        self.reasoner.infer_from_code_ir(&ir)
    }

    pub fn architecture_to_code_ir(&self, architecture: &Architecture) -> CodeIr {
        CodeIr::from_architecture(architecture)
    }

    pub fn generate_code(&self, graph: &ArchitectureGraph) -> Vec<(String, String)> {
        graph.nodes
            .iter()
            .map(|node| {
                let file_name = format!("{}.rs", node.name.to_ascii_lowercase());
                let source = format!(
                    "pub struct {};\n\nimpl {} {{\n    pub fn responsibility(&self) -> &'static str {{\n        \"{}\"\n    }}\n}}\n",
                    node.name, node.name, node.responsibility
                );
                (file_name, source)
            })
            .collect()
    }

    pub fn roundtrip_from_architecture(
        &self,
        architecture: &Architecture,
    ) -> Vec<(String, String)> {
        let ir = self.architecture_to_code_ir(architecture);
        let graph = self.reasoner.infer_from_code_ir(&ir);
        self.generate_code(&graph)
    }
}

pub fn architecture_from_code_ir(ir: &CodeIr) -> Architecture {
    let design_units = ir
        .modules
        .iter()
        .map(|module| {
            let mut unit = DesignUnit::new(module.id, module.name.clone());
            unit.layer = module.layer;
            unit.semantics = module.responsibilities.clone();
            unit.inputs = ir
                .interfaces
                .iter()
                .filter(|interface| interface.module_id == module.id)
                .map(|interface| interface.name.clone())
                .collect();
            unit
        })
        .collect::<Vec<_>>();
    let dependencies = ir
        .dependencies
        .iter()
        .map(|dependency| Dependency {
            from: DesignUnitId(dependency.from),
            to: DesignUnitId(dependency.to),
            kind: match dependency.kind {
                DependencyKind::Calls => DependencyKind::Calls,
                DependencyKind::Reads => DependencyKind::Reads,
                DependencyKind::Writes => DependencyKind::Writes,
                DependencyKind::Emits => DependencyKind::Emits,
            },
        })
        .collect::<Vec<_>>();

    Architecture {
        classes: vec![ClassUnit {
            id: 1,
            name: "GeneratedArchitecture".into(),
            structures: vec![StructureUnit {
                id: design_domain::StructureUnitId(1),
                name: "GeneratedStructure".into(),
                design_units,
            }],
        }],
        dependencies,
        graph: design_domain::ArchitectureGraph {
            edges: ir
                .dependencies
                .iter()
                .map(|dependency| (dependency.from, dependency.to))
                .collect(),
        },
    }
}

fn parse_symbol<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.strip_prefix(marker)
        .and_then(|rest| {
            rest.split(|ch: char| ch == '(' || ch == '{' || ch.is_whitespace())
                .next()
        })
        .filter(|name| !name.is_empty())
}

fn parse_use(line: &str) -> Option<&str> {
    line.strip_prefix("use ")
        .and_then(|rest| rest.split("::").last())
        .map(|segment| segment.trim_end_matches(';').trim())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_code_and_generates_roundtrip_sources() {
        let files = vec![
            ParsedSourceFile {
                path: "src/api_controller.rs".into(),
                source: "use crate::user_service::UserService;\npub fn handle() {}\n".into(),
            },
            ParsedSourceFile {
                path: "src/user_service.rs".into(),
                source: "pub struct UserService;\npub fn execute() {}\n".into(),
            },
        ];

        let core = CodeLanguageCore::default();
        let graph = core.reverse_architecture(&files);
        let generated = core.generate_code(&graph);

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(generated.len(), 2);
    }
}
