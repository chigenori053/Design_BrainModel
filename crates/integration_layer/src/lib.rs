use std::collections::BTreeSet;

use architecture_ir::stable_v03::{ArchitectureGraph, RelationType as ArchitectureRelationType};
use code_language_core::stable_v03::GeneratedFile;
use unified_design_ir::{
    DesignEdge, DesignGraph, DesignGraphBuilder, DesignNode, DesignNodeId, DesignNodeKind,
    DesignRelation,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Entity(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Predicate {
    DependsOn,
    Implements,
    Requires,
    ConflictsWith,
    BelongsTo,
    Violates,
    Satisfies,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalGoal {
    DependencyExists(Entity, Entity),
    ConstraintSatisfied(Entity),
    ArchitectureValid(String),
    ChangeImpactSafe(Entity),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceType {
    DesignNode,
    DesignEdge,
    CodeFile,
    CodeImport,
    Analysis,
    ArchitectureNode,
    ArchitectureEdge,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Provenance {
    pub source_type: SourceType,
    pub source_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalRelation {
    pub id: String,
    pub predicate: Predicate,
    pub subject: Entity,
    pub object: Entity,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceLink {
    pub relation_id: String,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisInput {
    pub system_id: String,
    pub entities: Vec<String>,
    pub has_cycle: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemInput {
    Design(DesignGraph),
    Code(Vec<GeneratedFile>),
    Analyze(AnalysisInput),
    Architecture(ArchitectureGraph),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignAction {
    pub title: String,
    pub relation_id: String,
    pub suggestion: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiRelationView {
    pub relation: CanonicalRelation,
    pub trace_link: Option<TraceLink>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiStateModel {
    pub relations: Vec<UiRelationView>,
    pub explanation_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SystemOutput {
    Design(DesignGraph),
    Actions(Vec<DesignAction>),
    Ui(UiStateModel),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub relation_id: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationReport {
    pub is_valid: bool,
    pub issues: Vec<ValidationIssue>,
}

pub fn to_relations(input: SystemInput) -> Vec<CanonicalRelation> {
    match input {
        SystemInput::Design(graph) => design_to_relations(&graph),
        SystemInput::Code(files) => code_to_relations(&files),
        SystemInput::Analyze(input) => goals_to_relations(&analysis_to_goals(&input), &input.system_id),
        SystemInput::Architecture(graph) => architecture_to_relations(&graph),
    }
}

pub fn to_system_output(relations: Vec<CanonicalRelation>) -> SystemOutput {
    let has_only_design_predicates = relations.iter().all(|relation| {
        matches!(
            relation.predicate,
            Predicate::DependsOn | Predicate::Implements | Predicate::BelongsTo
        )
    });
    if has_only_design_predicates {
        return SystemOutput::Design(relations_to_design_graph(&relations));
    }

    let actions = relations
        .iter()
        .map(|relation| DesignAction {
            title: format!("{:?} {} -> {}", relation.predicate, relation.subject.0, relation.object.0),
            relation_id: relation.id.clone(),
            suggestion: suggestion_for_relation(relation),
        })
        .collect::<Vec<_>>();
    SystemOutput::Actions(actions)
}

pub fn to_ui_state(
    relations: &[CanonicalRelation],
    explanation_text: Option<String>,
) -> UiStateModel {
    UiStateModel {
        relations: relations
            .iter()
            .cloned()
            .map(|relation| UiRelationView {
                trace_link: Some(TraceLink {
                    relation_id: relation.id.clone(),
                    provenance: relation.provenance.clone(),
                }),
                relation,
            })
            .collect(),
        explanation_text,
    }
}

pub fn trace_links(relations: &[CanonicalRelation]) -> Vec<TraceLink> {
    relations
        .iter()
        .map(|relation| TraceLink {
            relation_id: relation.id.clone(),
            provenance: relation.provenance.clone(),
        })
        .collect()
}

pub fn validate_mapping(input: &SystemInput, relations: &[CanonicalRelation]) -> ValidationReport {
    let known_entities = entities_from_input(input);
    let mut issues = relations
        .iter()
        .flat_map(|relation| {
            let mut relation_issues = Vec::new();
            if relation.subject.0.is_empty() || relation.object.0.is_empty() {
                relation_issues.push(ValidationIssue {
                    relation_id: relation.id.clone(),
                    message: "entity must be non-empty".to_string(),
                });
            }
            if !known_entities.is_empty()
                && (!known_entities.contains(&relation.subject) || !known_entities.contains(&relation.object))
            {
                relation_issues.push(ValidationIssue {
                    relation_id: relation.id.clone(),
                    message: "entity missing from source domain".to_string(),
                });
            }
            relation_issues
        })
        .collect::<Vec<_>>();
    issues.sort_by(|lhs, rhs| lhs.relation_id.cmp(&rhs.relation_id).then_with(|| lhs.message.cmp(&rhs.message)));
    ValidationReport {
        is_valid: issues.is_empty(),
        issues,
    }
}

pub fn validate_round_trip_design(graph: &DesignGraph) -> ValidationReport {
    let relations = design_to_relations(graph);
    let rebuilt = relations_to_design_graph(&relations);
    let mut issues = Vec::new();
    if graph.nodes().len() != rebuilt.nodes().len() {
        issues.push(ValidationIssue {
            relation_id: "design:nodes".to_string(),
            message: "node count changed during round trip".to_string(),
        });
    }
    if graph.edges().len() != rebuilt.edges().len() {
        issues.push(ValidationIssue {
            relation_id: "design:edges".to_string(),
            message: "edge count changed during round trip".to_string(),
        });
    }
    ValidationReport {
        is_valid: issues.is_empty(),
        issues,
    }
}

pub fn analysis_to_goals(input: &AnalysisInput) -> Vec<CanonicalGoal> {
    let mut goals = vec![CanonicalGoal::ArchitectureValid(input.system_id.clone())];
    if input.has_cycle {
        goals.push(CanonicalGoal::ConstraintSatisfied(Entity("no_cycle".to_string())));
    }
    if let Some(first) = input.entities.first() {
        goals.push(CanonicalGoal::ChangeImpactSafe(Entity(first.clone())));
    }
    goals
}

pub fn relation_trace_links(
    relations: &[CanonicalRelation],
    explanation_text: Option<&str>,
) -> UiStateModel {
    to_ui_state(relations, explanation_text.map(ToString::to_string))
}

fn design_to_relations(graph: &DesignGraph) -> Vec<CanonicalRelation> {
    let mut relations = graph
        .edges()
        .iter()
        .map(|edge| CanonicalRelation {
            id: stable_id(&format!("design:{}:{}:{:?}", edge.source.0, edge.target.0, edge.relation)),
            predicate: predicate_from_design_relation(&edge.relation),
            subject: Entity(edge.source.0.clone()),
            object: Entity(edge.target.0.clone()),
            provenance: Provenance {
                source_type: SourceType::DesignEdge,
                source_id: format!("{}->{}", edge.source.0, edge.target.0),
            },
        })
        .collect::<Vec<_>>();
    relations.sort_by(relation_order);
    relations
}

fn architecture_to_relations(graph: &ArchitectureGraph) -> Vec<CanonicalRelation> {
    let mut relations = graph
        .edges()
        .iter()
        .map(|edge| CanonicalRelation {
            id: stable_id(&format!("arch:{}:{}:{:?}", edge.source.0, edge.target.0, edge.relation)),
            predicate: match edge.relation {
                ArchitectureRelationType::DependsOn
                | ArchitectureRelationType::Calls
                | ArchitectureRelationType::Reads
                | ArchitectureRelationType::Writes => Predicate::DependsOn,
                ArchitectureRelationType::Contains => Predicate::BelongsTo,
                ArchitectureRelationType::Custom(_) => Predicate::Implements,
            },
            subject: Entity(edge.source.0.clone()),
            object: Entity(edge.target.0.clone()),
            provenance: Provenance {
                source_type: SourceType::ArchitectureEdge,
                source_id: format!("{}->{}", edge.source.0, edge.target.0),
            },
        })
        .collect::<Vec<_>>();
    relations.sort_by(relation_order);
    relations
}

fn code_to_relations(files: &[GeneratedFile]) -> Vec<CanonicalRelation> {
    let mut relations = Vec::new();
    for file in files {
        let module = module_name_for_path(&file.path);
        for import in parse_imports(&file.content) {
            relations.push(CanonicalRelation {
                id: stable_id(&format!("code:{module}:{import}")),
                predicate: Predicate::DependsOn,
                subject: Entity(module.clone()),
                object: Entity(import.clone()),
                provenance: Provenance {
                    source_type: SourceType::CodeImport,
                    source_id: format!("{}::{import}", file.path),
                },
            });
        }
    }
    relations.sort_by(relation_order);
    relations
}

fn goals_to_relations(goals: &[CanonicalGoal], system_id: &str) -> Vec<CanonicalRelation> {
    let mut relations = goals
        .iter()
        .map(|goal| match goal {
            CanonicalGoal::DependencyExists(lhs, rhs) => CanonicalRelation {
                id: stable_id(&format!("goal:dep:{}:{}", lhs.0, rhs.0)),
                predicate: Predicate::Requires,
                subject: lhs.clone(),
                object: rhs.clone(),
                provenance: Provenance {
                    source_type: SourceType::Analysis,
                    source_id: system_id.to_string(),
                },
            },
            CanonicalGoal::ConstraintSatisfied(entity) => CanonicalRelation {
                id: stable_id(&format!("goal:satisfied:{}", entity.0)),
                predicate: Predicate::Satisfies,
                subject: entity.clone(),
                object: Entity(system_id.to_string()),
                provenance: Provenance {
                    source_type: SourceType::Analysis,
                    source_id: system_id.to_string(),
                },
            },
            CanonicalGoal::ArchitectureValid(system) => CanonicalRelation {
                id: stable_id(&format!("goal:valid:{system}")),
                predicate: Predicate::Satisfies,
                subject: Entity(system.clone()),
                object: Entity("architecture".to_string()),
                provenance: Provenance {
                    source_type: SourceType::Analysis,
                    source_id: system.clone(),
                },
            },
            CanonicalGoal::ChangeImpactSafe(entity) => CanonicalRelation {
                id: stable_id(&format!("goal:safe:{}", entity.0)),
                predicate: Predicate::Satisfies,
                subject: entity.clone(),
                object: Entity("change_impact".to_string()),
                provenance: Provenance {
                    source_type: SourceType::Analysis,
                    source_id: system_id.to_string(),
                },
            },
        })
        .collect::<Vec<_>>();
    relations.sort_by(relation_order);
    relations
}

fn relations_to_design_graph(relations: &[CanonicalRelation]) -> DesignGraph {
    let node_names = relations
        .iter()
        .flat_map(|relation| [relation.subject.0.clone(), relation.object.0.clone()])
        .collect::<BTreeSet<_>>();
    let mut builder = DesignGraphBuilder::new();
    for node_name in node_names.iter() {
        builder = builder.add_node(DesignNode {
            id: DesignNodeId(node_name.clone()),
            name: node_name.clone(),
            kind: infer_node_kind(node_name),
            metadata: Default::default(),
        });
    }
    for relation in relations {
        if let Some(design_relation) = design_relation_from_predicate(&relation.predicate) {
            builder = builder.add_edge(DesignEdge {
                source: DesignNodeId(relation.subject.0.clone()),
                target: DesignNodeId(relation.object.0.clone()),
                relation: design_relation,
            });
        }
    }
    builder.build()
}

fn entities_from_input(input: &SystemInput) -> BTreeSet<Entity> {
    match input {
        SystemInput::Design(graph) => graph
            .nodes()
            .iter()
            .map(|node| Entity(node.id.0.clone()))
            .collect(),
        SystemInput::Code(files) => {
            let mut entities = BTreeSet::new();
            for file in files {
                entities.insert(Entity(module_name_for_path(&file.path)));
                for import in parse_imports(&file.content) {
                    entities.insert(Entity(import));
                }
            }
            entities
        }
        SystemInput::Analyze(input) => input
            .entities
            .iter()
            .cloned()
            .map(Entity)
            .chain(std::iter::once(Entity(input.system_id.clone())))
            .collect(),
        SystemInput::Architecture(graph) => graph
            .nodes()
            .iter()
            .map(|node| Entity(node.id.0.clone()))
            .collect(),
    }
}

fn predicate_from_design_relation(relation: &DesignRelation) -> Predicate {
    match relation {
        DesignRelation::DependsOn | DesignRelation::Calls => Predicate::DependsOn,
        DesignRelation::Owns => Predicate::BelongsTo,
        DesignRelation::Implements => Predicate::Implements,
    }
}

fn design_relation_from_predicate(predicate: &Predicate) -> Option<DesignRelation> {
    match predicate {
        Predicate::DependsOn => Some(DesignRelation::DependsOn),
        Predicate::BelongsTo => Some(DesignRelation::Owns),
        Predicate::Implements => Some(DesignRelation::Implements),
        _ => None,
    }
}

fn infer_node_kind(name: &str) -> DesignNodeKind {
    let lower = name.to_ascii_lowercase();
    if lower.contains("api") || lower.contains("ui") {
        DesignNodeKind::API
    } else if lower.contains("service") {
        DesignNodeKind::Service
    } else if lower.contains("db") || lower.contains("repo") {
        DesignNodeKind::Database
    } else if lower.contains("interface") {
        DesignNodeKind::Interface
    } else {
        DesignNodeKind::Module
    }
}

fn parse_imports(content: &str) -> Vec<String> {
    let mut imports = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("use ") {
                return rest
                    .split("::")
                    .next()
                    .map(str::trim)
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| segment.trim_end_matches(';').to_string());
            }
            if let Some(rest) = trimmed.strip_prefix("import ") {
                return rest
                    .split_whitespace()
                    .next()
                    .map(|segment| segment.trim_matches('"').trim_matches('\'').to_string());
            }
            if let Some(rest) = trimmed.strip_prefix("from ") {
                return rest
                    .split_whitespace()
                    .next()
                    .map(|segment| segment.trim_matches('"').trim_matches('\'').to_string());
            }
            None
        })
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    imports
}

fn module_name_for_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .split('.')
        .next()
        .unwrap_or(path)
        .to_string()
}

fn suggestion_for_relation(relation: &CanonicalRelation) -> String {
    match relation.predicate {
        Predicate::Violates | Predicate::ConflictsWith => format!(
            "Remove or isolate the dependency between {} and {}",
            relation.subject.0, relation.object.0
        ),
        Predicate::DependsOn => format!(
            "Review whether {} should depend on {}",
            relation.subject.0, relation.object.0
        ),
        Predicate::Implements => format!(
            "Keep {} aligned with {}",
            relation.subject.0, relation.object.0
        ),
        Predicate::BelongsTo => format!(
            "Confirm {} remains owned by {}",
            relation.subject.0, relation.object.0
        ),
        Predicate::Requires | Predicate::Satisfies => format!(
            "Validate {} against {}",
            relation.subject.0, relation.object.0
        ),
    }
}

fn relation_order(lhs: &CanonicalRelation, rhs: &CanonicalRelation) -> std::cmp::Ordering {
    lhs.predicate
        .cmp(&rhs.predicate)
        .then_with(|| lhs.subject.0.cmp(&rhs.subject.0))
        .then_with(|| lhs.object.0.cmp(&rhs.object.0))
        .then_with(|| lhs.id.cmp(&rhs.id))
}

fn stable_id(value: &str) -> String {
    let mut hash = 1469598103934665603_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("rel-{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType};

    fn design_graph() -> DesignGraph {
        DesignGraphBuilder::new()
            .add_node(DesignNode {
                id: DesignNodeId("api".to_string()),
                name: "api".to_string(),
                kind: DesignNodeKind::API,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("db".to_string()),
                name: "db".to_string(),
                kind: DesignNodeKind::Database,
                metadata: Default::default(),
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("api".to_string()),
                target: DesignNodeId("db".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .build()
    }

    #[test]
    fn design_round_trip_preserves_structure() {
        let graph = design_graph();
        let relations = to_relations(SystemInput::Design(graph.clone()));
        let report = validate_round_trip_design(&graph);
        let output = to_system_output(relations);

        assert!(report.is_valid);
        match output {
            SystemOutput::Design(rebuilt) => {
                assert_eq!(rebuilt.nodes().len(), graph.nodes().len());
                assert_eq!(rebuilt.edges().len(), graph.edges().len());
            }
            other => panic!("expected design output, got {other:?}"),
        }
    }

    #[test]
    fn trace_links_preserve_provenance() {
        let relations = to_relations(SystemInput::Design(design_graph()));
        let trace_links = trace_links(&relations);

        assert_eq!(trace_links.len(), relations.len());
        assert_eq!(trace_links[0].relation_id, relations[0].id);
        assert_eq!(trace_links[0].provenance, relations[0].provenance);
    }

    #[test]
    fn analyze_maps_to_expected_goals_and_relations() {
        let input = AnalysisInput {
            system_id: "system".to_string(),
            entities: vec!["api".to_string()],
            has_cycle: true,
        };
        let goals = analysis_to_goals(&input);
        let relations = to_relations(SystemInput::Analyze(input));

        assert!(goals.iter().any(|goal| matches!(goal, CanonicalGoal::ConstraintSatisfied(_))));
        assert!(relations.iter().any(|relation| relation.predicate == Predicate::Satisfies));
    }

    #[test]
    fn code_mapping_is_deterministic() {
        let files = vec![GeneratedFile {
            path: "src/api.rs".to_string(),
            content: "use db::client;\nuse service::core;".to_string(),
        }];

        let lhs = to_relations(SystemInput::Code(files.clone()));
        let rhs = to_relations(SystemInput::Code(files));

        assert_eq!(lhs, rhs);
    }

    #[test]
    fn validation_detects_unknown_entities() {
        let input = SystemInput::Analyze(AnalysisInput {
            system_id: "system".to_string(),
            entities: vec!["api".to_string()],
            has_cycle: false,
        });
        let invalid = vec![CanonicalRelation {
            id: "r1".to_string(),
            predicate: Predicate::DependsOn,
            subject: Entity("api".to_string()),
            object: Entity("db".to_string()),
            provenance: Provenance {
                source_type: SourceType::Analysis,
                source_id: "system".to_string(),
            },
        }];

        let report = validate_mapping(&input, &invalid);
        assert!(!report.is_valid);
    }

    #[test]
    fn architecture_mapping_converts_edges_to_relations() {
        let graph = ArchitectureGraphBuilder::new()
            .add_node(Node::new("api", NodeType::Interface))
            .add_node(Node::new("service", NodeType::Service))
            .add_edge(Edge::new("api", "service", ArchitectureRelationType::Calls))
            .build()
            .expect("valid graph");

        let relations = to_relations(SystemInput::Architecture(graph));

        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].predicate, Predicate::DependsOn);
    }
}
