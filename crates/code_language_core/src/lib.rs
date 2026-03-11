use architecture_reasoner::{ArchitectureGraph, ReverseArchitectureReasoner};
use code_ir::CodeIr;
use design_domain::{
    Architecture, ClassUnit, Dependency, DependencyKind, DesignUnit, DesignUnitId, StructureUnit,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSourceFile {
    pub path: String,
    pub source: String,
}

#[derive(Clone, Debug, Default)]
pub struct CodeLanguageCore {
    reasoner: ReverseArchitectureReasoner,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoundTripReport {
    pub node_recall: f64,
    pub dependency_recall: f64,
    pub consistency_rate: f64,
}

impl CodeLanguageCore {
    pub fn parse_sources(&self, files: &[ParsedSourceFile]) -> CodeIr {
        let mut units = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let fallback_name = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(file.path.as_str())
                .trim_end_matches(".rs")
                .trim_end_matches(".ts")
                .trim_end_matches(".py")
                .to_string();
            let mut discovered_name = None;
            let mut unit = DesignUnit::new(index as u64 + 1, fallback_name.clone());
            for line in file.source.lines() {
                let trimmed = line.trim();
                if let Some(name) = parse_function_name(trimmed) {
                    unit.outputs.push(name.to_string());
                }
                if let Some(name) = parse_type_name(trimmed) {
                    if discovered_name.is_none() {
                        discovered_name = Some(name.to_string());
                    }
                    unit.semantics.push(format!("owns {}", name));
                }
                for name in parse_use_identifiers(trimmed) {
                    unit.inputs.push(name);
                }
            }
            if let Some(name) = discovered_name {
                unit.name = name;
            } else {
                unit.name = fallback_name;
            }
            units.push(unit);
        }

        let known_symbols = units
            .iter()
            .map(|unit| (unit.name.clone(), unit.id))
            .collect::<Vec<_>>();
        for unit in &mut units {
            let inferred = unit
                .inputs
                .iter()
                .filter_map(|input| {
                    known_symbols
                        .iter()
                        .find(|(name, _)| name == input)
                        .map(|(_, id)| *id)
                })
                .filter(|dependency| *dependency != unit.id)
                .collect::<BTreeSet<_>>();
            unit.dependencies.extend(inferred);
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
        let node_names = graph
            .nodes
            .iter()
            .map(|node| (node.id, node.name.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        graph.nodes
            .iter()
            .map(|node| {
                let file_name = format!("{}.rs", to_snake_case(&node.name));
                let dependencies = graph
                    .dependency_edges()
                    .filter(|edge| edge.from == node.id)
                    .filter_map(|edge| node_names.get(&edge.to))
                    .map(|dependency| {
                        format!(
                            "use crate::{}::{};",
                            to_snake_case(dependency),
                            dependency
                        )
                    })
                    .collect::<Vec<_>>();
                let dependency_block = if dependencies.is_empty() {
                    String::new()
                } else {
                    format!("{}\n\n", dependencies.join("\n"))
                };
                let source = format!(
                    "{}pub struct {};\n\nimpl {} {{\n    pub fn responsibility(&self) -> &'static str {{\n        \"{}\"\n    }}\n}}\n",
                    dependency_block, node.name, node.name, node.responsibility
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

    pub fn evaluate_generation_quality(&self, graph: &ArchitectureGraph) -> f64 {
        let generated = self.generate_code(graph);
        if generated.is_empty() {
            return 0.0;
        }
        let score = generated.iter().fold(0.0, |sum, (_, source)| {
            let mut local = 0.0;
            if source.contains("pub struct ") {
                local += 0.4;
            }
            if source.contains("impl ") {
                local += 0.3;
            }
            if source.contains("responsibility") {
                local += 0.3;
            }
            sum + local
        });
        (score / generated.len() as f64).clamp(0.0, 1.0)
    }

    pub fn evaluate_roundtrip_consistency(&self, architecture: &Architecture) -> RoundTripReport {
        let original_ir = self.architecture_to_code_ir(architecture);
        let original_graph = self.reasoner.infer_from_code_ir(&original_ir);
        let generated = self.generate_code(&original_graph);
        let parsed_files = generated
            .into_iter()
            .map(|(path, source)| ParsedSourceFile { path, source })
            .collect::<Vec<_>>();
        let recovered_graph = self.reverse_architecture(&parsed_files);

        let original_nodes = original_graph
            .nodes
            .iter()
            .map(|node| node.name.clone())
            .collect::<BTreeSet<_>>();
        let recovered_nodes = recovered_graph
            .nodes
            .iter()
            .map(|node| node.name.clone())
            .collect::<BTreeSet<_>>();
        let matched_nodes = original_nodes.intersection(&recovered_nodes).count();

        let original_edges = original_graph
            .dependency_edges()
            .map(|edge| (edge.from, edge.to))
            .collect::<BTreeSet<_>>();
        let recovered_edges = recovered_graph
            .dependency_edges()
            .map(|edge| (edge.from, edge.to))
            .collect::<BTreeSet<_>>();
        let matched_edges = original_edges.intersection(&recovered_edges).count();

        let node_recall = if original_nodes.is_empty() {
            1.0
        } else {
            matched_nodes as f64 / original_nodes.len() as f64
        };
        let dependency_recall = if original_edges.is_empty() {
            1.0
        } else {
            matched_edges as f64 / original_edges.len() as f64
        };

        RoundTripReport {
            node_recall,
            dependency_recall,
            consistency_rate: ((node_recall + dependency_recall) / 2.0).clamp(0.0, 1.0),
        }
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

fn parse_type_name(line: &str) -> Option<&str> {
    parse_after_keywords(line, &["pub struct ", "struct ", "pub enum ", "enum "])
}

fn parse_function_name(line: &str) -> Option<&str> {
    parse_after_keywords(
        line,
        &[
            "pub async fn ",
            "async fn ",
            "pub fn ",
            "fn ",
            "pub(crate) fn ",
            "pub(crate) async fn ",
        ],
    )
}

fn parse_after_keywords<'a>(line: &'a str, keywords: &[&str]) -> Option<&'a str> {
    keywords.iter().find_map(|keyword| {
        line.strip_prefix(keyword)
            .and_then(|rest| {
                rest.split(|ch: char| {
                    ch == '(' || ch == '{' || ch == ';' || ch == ':' || ch.is_whitespace()
                })
                .next()
            })
            .filter(|name| !name.is_empty())
    })
}

fn parse_use_identifiers(line: &str) -> Vec<String> {
    let Some(rest) = line.strip_prefix("use ") else {
        return Vec::new();
    };
    let path = rest.trim_end_matches(';').trim();
    if let Some((prefix, group)) = path.split_once("::{") {
        let root = prefix.split("::").last().unwrap_or(prefix).trim();
        let mut names = if root.is_empty() {
            Vec::new()
        } else {
            vec![root.to_string()]
        };
        names.extend(
            group
                .trim_end_matches('}')
                .split(',')
                .map(|segment| segment.trim())
                .filter(|segment| !segment.is_empty() && *segment != "self")
                .map(|segment| {
                    segment
                        .split_whitespace()
                        .last()
                        .unwrap_or(segment)
                        .trim_matches('{')
                        .trim_matches('}')
                        .to_string()
                }),
        );
        return names;
    }

    path.split("::")
        .last()
        .map(|segment| segment.trim().to_string())
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
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
