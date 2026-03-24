use std::collections::{BTreeMap, BTreeSet};

use architecture_ir::stable_v03::{ArchitectureGraph, RelationType as ArchitectureRelationType};
use code_language_core::stable_v03::GeneratedFile;
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    pub relation_id: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub is_valid: bool,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArchitectureIr {
    pub nodes: Vec<ArchitectureIrNode>,
    pub edges: Vec<ArchitectureIrEdge>,
    pub metadata: ArchitectureIrMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArchitectureIrMetadata {
    pub graph_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ArchitectureIrNode {
    pub id: String,
    pub name: String,
    pub kind: ArchitectureNodeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ArchitectureNodeKind {
    Module,
    File,
    Crate,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ArchitectureIrEdge {
    pub from: String,
    pub to: String,
    pub kind: ArchitectureEdgeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ArchitectureEdgeKind {
    DependsOn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CycleReport {
    pub has_cycle: bool,
    pub cycles: Vec<Cycle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Cycle {
    pub nodes: Vec<String>,
    pub size: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Layer {
    pub level: usize,
    pub nodes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LayerModel {
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LayerViolation {
    pub from: String,
    pub to: String,
    pub from_level: usize,
    pub to_level: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DesignInsights {
    pub has_cycle: bool,
    pub layer_count: usize,
    pub violations: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum NodeRole {
    Core,
    Service,
    Infrastructure,
    Interface,
    Presentation,
    Utility,
    Test,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RoleAssignment {
    pub node_id: String,
    pub node_name: String,
    pub role: NodeRole,
    pub confidence_milli: u16,
    pub score: RoleScore,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RoleScore {
    pub core_milli: u16,
    pub service_milli: u16,
    pub infra_milli: u16,
    pub interface_milli: u16,
    pub presentation_milli: u16,
    pub utility_milli: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum LayerType {
    CoreLayer,
    DomainLayer,
    ApplicationLayer,
    InterfaceLayer,
    InfrastructureLayer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SemanticLayer {
    pub level: usize,
    pub layer_type: LayerType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Pattern {
    Layered,
    Cyclic { nodes: Vec<String> },
    Hub { node: String },
    GodObject { node: String },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum RefactorAction {
    IntroduceAbstraction,
    InvertDependency,
    SplitModule,
    ExtractInterface,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RefactorSuggestion {
    pub target: String,
    pub action: RefactorAction,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SemanticInsights {
    pub roles: Vec<RoleAssignment>,
    pub layers: Vec<SemanticLayer>,
    pub patterns: Vec<Pattern>,
    pub suggestions: Vec<RefactorSuggestion>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum IssueType {
    Cycle,
    LayerViolation,
    OrphanNode,
    GodObject,
    Hub,
    DataFlowAnomaly,
    RoleMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum IssueScope {
    Node(String),
    Edge(String, String),
    Subgraph(Vec<String>),
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum EvidenceType {
    Nodes,
    Edge,
    Metric,
    Role,
    Pattern,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Evidence {
    pub kind: EvidenceType,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub id: String,
    pub kind: IssueType,
    pub severity: Severity,
    pub scope: IssueScope,
    pub description: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DataFlowGraph {
    pub flows: Vec<DataEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DataEdge {
    pub from: String,
    pub to: String,
    pub weight_milli: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RefactorPlanAction {
    IntroduceInterface {
        between: (String, String),
    },
    RemoveDependency {
        from: String,
        to: String,
    },
    SplitModule {
        target: String,
    },
    MoveDependency {
        from: String,
        to: String,
        via: Option<String>,
    },
    ExtractComponent {
        from: String,
    },
    IsolateNode {
        node: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSet {
    pub actions: Vec<RefactorPlanAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PhaseType {
    BreakCycle,
    FixLayering,
    RestructureModules,
    OptimizeFlow,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorPhase {
    pub phase_type: PhaseType,
    pub actions: Vec<RefactorPlanAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsDelta {
    pub cycle_count: isize,
    pub layer_violations: isize,
    pub coupling_score_milli: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSummary {
    pub total_actions: usize,
    pub phase_count: usize,
    pub expected_improvement: MetricsDelta,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchOperation {
    CreateInterface {
        name: String,
        between: (String, String),
    },
    UpdateDependency {
        from: String,
        to: String,
        via: Option<String>,
    },
    SplitModule {
        module: String,
        new_modules: Vec<String>,
    },
    ExtractComponent {
        from: String,
        component: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodePatch {
    pub patch_id: String,
    pub action: RefactorPlanAction,
    pub operations: Vec<PatchOperation>,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorPlan {
    pub phases: Vec<RefactorPhase>,
    pub summary: PlanSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationMetrics {
    pub cycle_count: usize,
    pub layer_violations: usize,
    pub coupling_score_milli: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationResult {
    pub before: SimulationMetrics,
    pub after: SimulationMetrics,
    pub delta: MetricsDelta,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiagnosticAnalysis {
    pub ir: ArchitectureIr,
    pub cycle_report: CycleReport,
    pub layer_model: LayerModel,
    pub violations: Vec<LayerViolation>,
    pub integrity: ValidationReport,
    pub insights: DesignInsights,
    pub semantic: SemanticInsights,
    pub data_flow: DataFlowGraph,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StructuralAnalysis {
    pub ir: ArchitectureIr,
    pub cycle_report: CycleReport,
    pub layer_model: LayerModel,
    pub violations: Vec<LayerViolation>,
    pub integrity: ValidationReport,
    pub insights: DesignInsights,
    pub semantic: SemanticInsights,
    pub data_flow: DataFlowGraph,
    pub action_set: ActionSet,
    pub refactor_plan: RefactorPlan,
    pub code_patches: Vec<CodePatch>,
    pub simulation: SimulationResult,
}

pub fn to_relations(input: SystemInput) -> Vec<CanonicalRelation> {
    match input {
        SystemInput::Design(graph) => design_to_relations(&graph),
        SystemInput::Code(files) => code_to_relations(&files),
        SystemInput::Analyze(input) => {
            goals_to_relations(&analysis_to_goals(&input), &input.system_id)
        }
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
            title: format!(
                "{:?} {} -> {}",
                relation.predicate, relation.subject.0, relation.object.0
            ),
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
                && (!known_entities.contains(&relation.subject)
                    || !known_entities.contains(&relation.object))
            {
                relation_issues.push(ValidationIssue {
                    relation_id: relation.id.clone(),
                    message: "entity missing from source domain".to_string(),
                });
            }
            relation_issues
        })
        .collect::<Vec<_>>();
    issues.sort_by(|lhs, rhs| {
        lhs.relation_id
            .cmp(&rhs.relation_id)
            .then_with(|| lhs.message.cmp(&rhs.message))
    });
    ValidationReport {
        is_valid: issues.is_empty(),
        issues,
    }
}

pub fn validate_round_trip_design(graph: &DesignGraph) -> ValidationReport {
    let ir = architecture_ir_from_design(graph);
    let rebuilt = design_graph_from_ir(&ir);
    let mut issues = Vec::new();
    if graph.nodes().len() != rebuilt.nodes().len() {
        issues.push(ValidationIssue {
            relation_id: "design:nodes".to_string(),
            message: "NodeCountMismatch".to_string(),
        });
    }
    if graph.edges().len() != rebuilt.edges().len() {
        issues.push(ValidationIssue {
            relation_id: "design:edges".to_string(),
            message: "EdgeMismatch".to_string(),
        });
    }
    let original_nodes = graph
        .nodes()
        .iter()
        .map(|node| node.id.0.clone())
        .collect::<BTreeSet<_>>();
    let rebuilt_nodes = rebuilt
        .nodes()
        .iter()
        .map(|node| node.id.0.clone())
        .collect::<BTreeSet<_>>();
    if original_nodes != rebuilt_nodes {
        issues.push(ValidationIssue {
            relation_id: "design:node-set".to_string(),
            message: "NodeSetMismatch".to_string(),
        });
    }
    let original_edges = graph
        .edges()
        .iter()
        .map(|edge| {
            (
                edge.source.0.clone(),
                edge.target.0.clone(),
                edge.relation.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    let rebuilt_edges = rebuilt
        .edges()
        .iter()
        .map(|edge| {
            (
                edge.source.0.clone(),
                edge.target.0.clone(),
                edge.relation.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    if original_edges != rebuilt_edges {
        issues.push(ValidationIssue {
            relation_id: "design:edge-set".to_string(),
            message: "EdgeMismatch".to_string(),
        });
    }
    issues.sort_by(|lhs, rhs| {
        lhs.relation_id
            .cmp(&rhs.relation_id)
            .then_with(|| lhs.message.cmp(&rhs.message))
    });
    ValidationReport {
        is_valid: issues.is_empty(),
        issues,
    }
}

pub fn architecture_ir_from_design(graph: &DesignGraph) -> ArchitectureIr {
    let mut nodes = graph
        .nodes()
        .iter()
        .map(|node| ArchitectureIrNode {
            id: stable_node_id(&node.name),
            name: node.name.clone(),
            kind: architecture_node_kind(&node.kind),
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id).then_with(|| lhs.name.cmp(&rhs.name)));

    let name_to_id = nodes
        .iter()
        .map(|node| (node.name.clone(), node.id.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut edges = graph
        .edges()
        .iter()
        .filter_map(|edge| {
            Some(ArchitectureIrEdge {
                from: name_to_id.get(&edge.source.0)?.clone(),
                to: name_to_id.get(&edge.target.0)?.clone(),
                kind: ArchitectureEdgeKind::DependsOn,
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|lhs, rhs| {
        lhs.from
            .cmp(&rhs.from)
            .then_with(|| lhs.to.cmp(&rhs.to))
            .then_with(|| lhs.kind.cmp(&rhs.kind))
    });

    ArchitectureIr {
        metadata: ArchitectureIrMetadata {
            graph_id: stable_id(&format!(
                "graph:{}",
                nodes
                    .iter()
                    .map(|node| node.name.as_str())
                    .collect::<Vec<_>>()
                    .join("|")
            )),
        },
        nodes,
        edges,
    }
}

pub fn diagnostic_analysis(graph: &DesignGraph) -> DiagnosticAnalysis {
    let ir = architecture_ir_from_design(graph);
    let cycle_report = cycle_report(&ir);
    let layer_model = infer_layers(&ir);
    let violations = layer_violations(&ir, &layer_model);
    let mut integrity = validate_round_trip_design(graph);
    for cycle in &cycle_report.cycles {
        if cycle.size >= 2 {
            integrity.issues.push(ValidationIssue {
                relation_id: format!("cycle:{}", cycle.nodes.join("->")),
                message: "CycleDetected".to_string(),
            });
        }
    }
    for edge in &ir.edges {
        if edge.from == edge.to {
            integrity.issues.push(ValidationIssue {
                relation_id: format!("self-loop:{}", edge.from),
                message: "SelfLoop".to_string(),
            });
        }
    }
    for violation in &violations {
        integrity.issues.push(ValidationIssue {
            relation_id: format!("violation:{}->{}", violation.from, violation.to),
            message: "LayerViolation".to_string(),
        });
    }
    for node in orphan_nodes(&ir) {
        integrity.issues.push(ValidationIssue {
            relation_id: format!("orphan:{node}"),
            message: "OrphanNode".to_string(),
        });
    }
    integrity.issues.sort_by(|lhs, rhs| {
        lhs.relation_id
            .cmp(&rhs.relation_id)
            .then_with(|| lhs.message.cmp(&rhs.message))
    });
    integrity.issues.dedup();
    integrity.is_valid = integrity.issues.is_empty();

    let insights = DesignInsights {
        has_cycle: cycle_report.has_cycle,
        layer_count: layer_model.layers.len(),
        violations: violations.len(),
    };
    let semantic_layers = semantic_layers(&layer_model, &ir, &violations);
    let roles = infer_roles(&ir, &layer_model);
    let patterns = detect_patterns(&ir, &cycle_report, &violations);
    let data_flow = data_flow_graph(&ir);
    let semantic = SemanticInsights {
        roles,
        layers: semantic_layers,
        patterns,
        suggestions: Vec::new(),
    };
    let issues = generate_issues(
        &ir,
        &cycle_report,
        &violations,
        &semantic,
        &data_flow,
        &integrity,
    );

    DiagnosticAnalysis {
        ir,
        cycle_report,
        layer_model,
        violations,
        integrity,
        insights,
        semantic,
        data_flow,
        issues,
    }
}

pub fn structural_analysis(graph: &DesignGraph) -> StructuralAnalysis {
    let mut diagnostics = diagnostic_analysis(graph);
    diagnostics.semantic.suggestions = refactor_suggestions(
        &diagnostics.ir,
        &diagnostics.semantic.roles,
        &diagnostics.semantic.patterns,
        &diagnostics.violations,
    );
    let action_set = action_set_from_issues(&diagnostics.issues);
    let mut refactor_plan = refactor_plan(&action_set);
    let code_patches = generate_patches(&refactor_plan);
    let simulation = simulate_refactor(&diagnostics.ir, &refactor_plan);
    refactor_plan.summary.expected_improvement = simulation.delta.clone();

    StructuralAnalysis {
        ir: diagnostics.ir,
        cycle_report: diagnostics.cycle_report,
        layer_model: diagnostics.layer_model,
        violations: diagnostics.violations,
        integrity: diagnostics.integrity,
        insights: diagnostics.insights,
        semantic: diagnostics.semantic,
        data_flow: diagnostics.data_flow,
        action_set,
        refactor_plan,
        code_patches,
        simulation,
    }
}

pub fn analysis_to_goals(input: &AnalysisInput) -> Vec<CanonicalGoal> {
    let mut goals = vec![CanonicalGoal::ArchitectureValid(input.system_id.clone())];
    if input.has_cycle {
        goals.push(CanonicalGoal::ConstraintSatisfied(Entity(
            "no_cycle".to_string(),
        )));
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
        .nodes()
        .iter()
        .map(|node| CanonicalRelation {
            id: stable_id(&format!("design-node:{}", node.id.0)),
            predicate: Predicate::BelongsTo,
            subject: Entity(node.id.0.clone()),
            object: Entity("__design_graph__".to_string()),
            provenance: Provenance {
                source_type: SourceType::DesignNode,
                source_id: node.id.0.clone(),
            },
        })
        .collect::<Vec<_>>();
    relations.extend(
        graph
            .edges()
            .iter()
            .map(|edge| CanonicalRelation {
                id: stable_id(&format!(
                    "design:{}:{}:{:?}",
                    edge.source.0, edge.target.0, edge.relation
                )),
                predicate: predicate_from_design_relation(&edge.relation),
                subject: Entity(edge.source.0.clone()),
                object: Entity(edge.target.0.clone()),
                provenance: Provenance {
                    source_type: SourceType::DesignEdge,
                    source_id: format!("{}->{}", edge.source.0, edge.target.0),
                },
            })
            .collect::<Vec<_>>(),
    );
    relations.sort_by(relation_order);
    relations
}

fn architecture_to_relations(graph: &ArchitectureGraph) -> Vec<CanonicalRelation> {
    let mut relations = graph
        .edges()
        .iter()
        .map(|edge| CanonicalRelation {
            id: stable_id(&format!(
                "arch:{}:{}:{:?}",
                edge.source.0, edge.target.0, edge.relation
            )),
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
        .flat_map(|relation| match relation.provenance.source_type {
            SourceType::DesignNode => vec![relation.subject.0.clone()],
            _ => vec![relation.subject.0.clone(), relation.object.0.clone()],
        })
        .filter(|name| name != "__design_graph__")
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
        if matches!(relation.provenance.source_type, SourceType::DesignNode) {
            continue;
        }
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

fn design_graph_from_ir(ir: &ArchitectureIr) -> DesignGraph {
    let id_to_name = ir
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut builder = DesignGraphBuilder::new();
    for node in &ir.nodes {
        builder = builder.add_node(DesignNode {
            id: DesignNodeId(node.name.clone()),
            name: node.name.clone(),
            kind: design_node_kind(&node.kind),
            metadata: Default::default(),
        });
    }
    for edge in &ir.edges {
        let Some(from) = id_to_name.get(&edge.from) else {
            continue;
        };
        let Some(to) = id_to_name.get(&edge.to) else {
            continue;
        };
        builder = builder.add_edge(DesignEdge {
            source: DesignNodeId(from.clone()),
            target: DesignNodeId(to.clone()),
            relation: DesignRelation::DependsOn,
        });
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

fn architecture_node_kind(kind: &DesignNodeKind) -> ArchitectureNodeKind {
    match kind {
        DesignNodeKind::API
        | DesignNodeKind::Service
        | DesignNodeKind::Database
        | DesignNodeKind::Interface
        | DesignNodeKind::Domain
        | DesignNodeKind::Module => ArchitectureNodeKind::Module,
    }
}

fn design_node_kind(kind: &ArchitectureNodeKind) -> DesignNodeKind {
    match kind {
        ArchitectureNodeKind::Module => DesignNodeKind::Module,
        ArchitectureNodeKind::File => DesignNodeKind::Module,
        ArchitectureNodeKind::Crate => DesignNodeKind::Module,
    }
}

fn stable_node_id(name: &str) -> String {
    stable_id(&format!(
        "node:{}",
        name.replace('\\', "/").to_ascii_lowercase()
    ))
}

pub fn cycle_report(ir: &ArchitectureIr) -> CycleReport {
    let components = strongly_connected_components(ir);
    let mut cycles = components
        .into_iter()
        .filter_map(|component| {
            if component.len() >= 2 {
                let mut names = component
                    .iter()
                    .filter_map(|id| node_name_by_id(ir, id))
                    .collect::<Vec<_>>();
                names.sort();
                Some(Cycle {
                    size: names.len(),
                    nodes: names,
                })
            } else {
                let node = component.first()?;
                if ir
                    .edges
                    .iter()
                    .any(|edge| edge.from == *node && edge.to == *node)
                {
                    Some(Cycle {
                        size: 1,
                        nodes: vec![node_name_by_id(ir, node)?],
                    })
                } else {
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    cycles.sort_by(|lhs, rhs| lhs.nodes.cmp(&rhs.nodes));
    CycleReport {
        has_cycle: !cycles.is_empty(),
        cycles,
    }
}

pub fn infer_layers(ir: &ArchitectureIr) -> LayerModel {
    let components = strongly_connected_components(ir);
    let mut component_by_node = BTreeMap::new();
    for (index, component) in components.iter().enumerate() {
        for node in component {
            component_by_node.insert(node.clone(), index);
        }
    }

    let mut dag_edges = BTreeSet::new();
    let mut out_degree = vec![0usize; components.len()];
    for edge in &ir.edges {
        let Some(&from_component) = component_by_node.get(&edge.from) else {
            continue;
        };
        let Some(&to_component) = component_by_node.get(&edge.to) else {
            continue;
        };
        if from_component != to_component && dag_edges.insert((from_component, to_component)) {
            out_degree[from_component] += 1;
        }
    }

    let mut children = vec![Vec::<usize>::new(); components.len()];
    for (from, to) in dag_edges {
        children[from].push(to);
    }
    for targets in &mut children {
        targets.sort();
    }

    let mut levels = vec![None; components.len()];
    fn assign_level(index: usize, children: &[Vec<usize>], levels: &mut [Option<usize>]) -> usize {
        if let Some(level) = levels[index] {
            return level;
        }
        let level = if children[index].is_empty() {
            0
        } else {
            children[index]
                .iter()
                .map(|child| assign_level(*child, children, levels) + 1)
                .max()
                .unwrap_or(0)
        };
        levels[index] = Some(level);
        level
    }
    for index in 0..components.len() {
        assign_level(index, &children, &mut levels);
    }

    let mut grouped = BTreeMap::<usize, Vec<String>>::new();
    for (component_index, component) in components.iter().enumerate() {
        let level = levels[component_index].unwrap_or(0);
        let nodes = component
            .iter()
            .filter_map(|id| node_name_by_id(ir, id))
            .collect::<Vec<_>>();
        grouped.entry(level).or_default().extend(nodes);
    }
    let mut layers = grouped
        .into_iter()
        .map(|(level, mut nodes)| {
            nodes.sort();
            nodes.dedup();
            Layer { level, nodes }
        })
        .collect::<Vec<_>>();
    layers.sort_by(|lhs, rhs| lhs.level.cmp(&rhs.level));
    LayerModel { layers }
}

pub fn layer_violations(ir: &ArchitectureIr, model: &LayerModel) -> Vec<LayerViolation> {
    let node_to_level = model
        .layers
        .iter()
        .flat_map(|layer| {
            layer
                .nodes
                .iter()
                .cloned()
                .map(move |node| (node, layer.level))
        })
        .collect::<BTreeMap<_, _>>();
    let mut violations = ir
        .edges
        .iter()
        .filter_map(|edge| {
            let from = node_name_by_id(ir, &edge.from)?;
            let to = node_name_by_id(ir, &edge.to)?;
            let from_level = *node_to_level.get(&from)?;
            let to_level = *node_to_level.get(&to)?;
            (from_level <= to_level).then_some(LayerViolation {
                from,
                to,
                from_level,
                to_level,
            })
        })
        .collect::<Vec<_>>();
    violations.sort_by(|lhs, rhs| {
        lhs.from
            .cmp(&rhs.from)
            .then_with(|| lhs.to.cmp(&rhs.to))
            .then_with(|| lhs.from_level.cmp(&rhs.from_level))
            .then_with(|| lhs.to_level.cmp(&rhs.to_level))
    });
    violations
}

pub fn infer_roles(ir: &ArchitectureIr, model: &LayerModel) -> Vec<RoleAssignment> {
    let node_to_level = model
        .layers
        .iter()
        .flat_map(|layer| {
            layer
                .nodes
                .iter()
                .cloned()
                .map(move |node| (node, layer.level))
        })
        .collect::<BTreeMap<_, _>>();
    let fan_in = fan_in_map(ir);
    let fan_out = fan_out_map(ir);

    let max_level = model
        .layers
        .iter()
        .map(|layer| layer.level)
        .max()
        .unwrap_or(0);
    let cycle_nodes = cycle_report(ir)
        .cycles
        .into_iter()
        .flat_map(|cycle| cycle.nodes)
        .collect::<BTreeSet<_>>();
    let mut roles = ir
        .nodes
        .iter()
        .map(|node| {
            let lower = node.name.to_ascii_lowercase();
            let level = *node_to_level.get(&node.name).unwrap_or(&0);
            let inbound = *fan_in.get(&node.name).unwrap_or(&0);
            let outbound = *fan_out.get(&node.name).unwrap_or(&0);
            let dependency_directionality = if inbound + outbound == 0 {
                0.0
            } else {
                outbound as f32 / (inbound + outbound) as f32
            };
            let level_ratio = if max_level == 0 {
                0.0
            } else {
                level as f32 / max_level as f32
            };
            let in_ratio = inbound as f32 / (inbound.max(outbound).max(1) as f32);
            let out_ratio = outbound as f32 / (inbound.max(outbound).max(1) as f32);
            let cycle_factor = if cycle_nodes.contains(&node.name) {
                1.0
            } else {
                0.0
            };

            let mut score = RoleScore {
                core_milli: score_to_milli(in_ratio * (1.0 - out_ratio) * (1.0 - level_ratio)),
                service_milli: score_to_milli(
                    ((in_ratio + out_ratio) / 2.0) * (0.4 + level_ratio * 0.4),
                ),
                infra_milli: score_to_milli(
                    out_ratio * (1.0 - level_ratio) * dependency_directionality,
                ),
                interface_milli: score_to_milli(
                    level_ratio
                        * (0.3 + out_ratio * 0.7)
                        * (1.0 - in_ratio * 0.85)
                        * (1.0 - cycle_factor * 0.75),
                ),
                presentation_milli: score_to_milli(
                    level_ratio * out_ratio * (1.0 - cycle_factor * 0.2),
                ),
                utility_milli: score_to_milli(
                    (1.0 - in_ratio) * out_ratio * (0.5 + cycle_factor * 0.3),
                ),
            };

            // Minimal deterministic name hints remain as tie-break bias only.
            if lower.contains("renderer") || lower.contains("ui") {
                score.presentation_milli = score.presentation_milli.saturating_add(260).min(1000);
                score.interface_milli = score.interface_milli.saturating_sub(80);
            }
            if lower.contains("debug") || lower.contains("util") {
                score.utility_milli = score.utility_milli.saturating_add(340).min(1000);
                score.interface_milli = score.interface_milli.saturating_sub(120);
                score.presentation_milli = score.presentation_milli.saturating_sub(220);
            }
            if lower.contains("world") {
                score.core_milli = score.core_milli.saturating_add(140).min(1000);
            }
            if lower.contains("diagnostic") || lower.contains("service") {
                score.service_milli = score.service_milli.saturating_add(120).min(1000);
            }
            if lower.contains("test") {
                score.utility_milli = score.utility_milli.saturating_add(60).min(1000);
            }

            let candidates = [
                (NodeRole::Core, score.core_milli),
                (NodeRole::Service, score.service_milli),
                (NodeRole::Infrastructure, score.infra_milli),
                (NodeRole::Interface, score.interface_milli),
                (NodeRole::Presentation, score.presentation_milli),
                (NodeRole::Utility, score.utility_milli),
            ];
            let (role, confidence_milli) = candidates
                .into_iter()
                .max_by(|lhs, rhs| {
                    lhs.1
                        .cmp(&rhs.1)
                        .then_with(|| role_rank(&lhs.0).cmp(&role_rank(&rhs.0)).reverse())
                })
                .unwrap_or((NodeRole::Unknown, 500));
            let (role, confidence_milli) = if lower.contains("test") {
                (NodeRole::Test, 940)
            } else {
                (role, confidence_milli)
            };
            RoleAssignment {
                node_id: node.id.clone(),
                node_name: node.name.clone(),
                role,
                confidence_milli,
                score,
            }
        })
        .collect::<Vec<_>>();
    roles.sort_by(|lhs, rhs| {
        lhs.node_name
            .cmp(&rhs.node_name)
            .then_with(|| lhs.node_id.cmp(&rhs.node_id))
    });
    roles
}

pub fn semantic_layers(
    model: &LayerModel,
    ir: &ArchitectureIr,
    violations: &[LayerViolation],
) -> Vec<SemanticLayer> {
    let roles = infer_roles(ir, model)
        .into_iter()
        .map(|role| (role.node_name, role.role))
        .collect::<BTreeMap<_, _>>();
    let max_level = model
        .layers
        .iter()
        .map(|layer| layer.level)
        .max()
        .unwrap_or(0);
    let violation_nodes = violations
        .iter()
        .flat_map(|violation| [violation.from.clone(), violation.to.clone()])
        .collect::<BTreeSet<_>>();

    let mut result = model
        .layers
        .iter()
        .map(|layer| {
            let layer_type = if layer
                .nodes
                .iter()
                .filter(|node| roles.get(*node) == Some(&NodeRole::Core))
                .count()
                >= 1
                && layer.level == 0
            {
                LayerType::CoreLayer
            } else if layer.level == max_level {
                LayerType::InterfaceLayer
            } else if layer
                .nodes
                .iter()
                .any(|node| violation_nodes.contains(node))
            {
                LayerType::ApplicationLayer
            } else if layer
                .nodes
                .iter()
                .any(|node| roles.get(node) == Some(&NodeRole::Infrastructure))
            {
                LayerType::InfrastructureLayer
            } else {
                LayerType::DomainLayer
            };
            SemanticLayer {
                level: layer.level,
                layer_type,
            }
        })
        .collect::<Vec<_>>();
    result.sort_by(|lhs, rhs| lhs.level.cmp(&rhs.level));
    result
}

pub fn detect_patterns(
    ir: &ArchitectureIr,
    cycles: &CycleReport,
    violations: &[LayerViolation],
) -> Vec<Pattern> {
    let fan_in = fan_in_map(ir);
    let fan_out = fan_out_map(ir);
    let mut patterns = Vec::new();

    if !cycles.has_cycle && violations.is_empty() {
        patterns.push(Pattern::Layered);
    }
    for cycle in &cycles.cycles {
        if cycle.size >= 2 {
            patterns.push(Pattern::Cyclic {
                nodes: cycle.nodes.clone(),
            });
        }
    }
    for node in &ir.nodes {
        let inbound = *fan_in.get(&node.name).unwrap_or(&0);
        let outbound = *fan_out.get(&node.name).unwrap_or(&0);
        if inbound >= 3 {
            patterns.push(Pattern::Hub {
                node: node.name.clone(),
            });
        }
        if inbound >= 2 && outbound >= 2 {
            patterns.push(Pattern::GodObject {
                node: node.name.clone(),
            });
        }
    }
    patterns.sort_by(|lhs, rhs| pattern_sort_key(lhs).cmp(&pattern_sort_key(rhs)));
    patterns.dedup();
    patterns
}

pub fn refactor_suggestions(
    ir: &ArchitectureIr,
    roles: &[RoleAssignment],
    patterns: &[Pattern],
    violations: &[LayerViolation],
) -> Vec<RefactorSuggestion> {
    let role_by_name = roles
        .iter()
        .map(|role| (role.node_name.clone(), role.role.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut suggestions = Vec::new();

    for pattern in patterns {
        match pattern {
            Pattern::Cyclic { nodes } => {
                for node in nodes {
                    suggestions.push(RefactorSuggestion {
                        target: node.clone(),
                        action: RefactorAction::InvertDependency,
                        reason: format!(
                            "Break cycle involving {node} by introducing an abstraction"
                        ),
                    });
                }
            }
            Pattern::GodObject { node } => suggestions.push(RefactorSuggestion {
                target: node.clone(),
                action: RefactorAction::SplitModule,
                reason: format!("{node} has high fan-in and fan-out; split responsibilities"),
            }),
            Pattern::Hub { node } => suggestions.push(RefactorSuggestion {
                target: node.clone(),
                action: RefactorAction::ExtractInterface,
                reason: format!("{node} is a dependency hub; extract a narrow interface"),
            }),
            Pattern::Layered => {}
        }
    }

    for violation in violations {
        suggestions.push(RefactorSuggestion {
            target: violation.from.clone(),
            action: if role_by_name.get(&violation.to) == Some(&NodeRole::Presentation) {
                RefactorAction::IntroduceAbstraction
            } else {
                RefactorAction::InvertDependency
            },
            reason: format!(
                "{} depends on {} across the same or higher layer; introduce an abstraction boundary",
                violation.from, violation.to
            ),
        });
    }

    for node in orphan_nodes(ir) {
        suggestions.push(RefactorSuggestion {
            target: node.clone(),
            action: RefactorAction::ExtractInterface,
            reason: format!("{node} is isolated; either connect it intentionally or remove it"),
        });
    }

    suggestions.sort_by(|lhs, rhs| {
        lhs.target
            .cmp(&rhs.target)
            .then_with(|| lhs.reason.cmp(&rhs.reason))
    });
    suggestions.dedup();
    suggestions
}

pub fn data_flow_graph(ir: &ArchitectureIr) -> DataFlowGraph {
    let fan_in = fan_in_map(ir);
    let fan_out = fan_out_map(ir);
    let id_to_name = ir
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut flows = ir
        .edges
        .iter()
        .filter_map(|edge| {
            let from = id_to_name.get(&edge.from)?.clone();
            let to = id_to_name.get(&edge.to)?.clone();
            let inbound = *fan_in.get(&to).unwrap_or(&0) as f32;
            let outbound = *fan_out.get(&from).unwrap_or(&0) as f32;
            let weight = (0.4 + (inbound + outbound) / 10.0).clamp(0.1, 1.0);
            Some(DataEdge {
                from,
                to,
                weight_milli: score_to_milli(weight),
            })
        })
        .collect::<Vec<_>>();
    flows.sort_by(|lhs, rhs| lhs.from.cmp(&rhs.from).then_with(|| lhs.to.cmp(&rhs.to)));
    DataFlowGraph { flows }
}

pub fn action_set_from_issues(issues: &[Issue]) -> ActionSet {
    const MAX_ACTIONS: usize = 5;

    let mut actions = issues
        .iter()
        .filter(|issue| should_emit_action(issue))
        .flat_map(map_issue_to_actions)
        .collect::<Vec<_>>();
    actions.sort_by(|lhs, rhs| action_sort_key(lhs).cmp(&action_sort_key(rhs)));
    actions.dedup();
    actions = resolve_conflicts(actions);
    actions.truncate(MAX_ACTIONS);
    ActionSet { actions }
}

pub fn refactor_plan(action_set: &ActionSet) -> RefactorPlan {
    let mut grouped = BTreeMap::<PhaseType, Vec<RefactorPlanAction>>::new();
    for action in &action_set.actions {
        grouped
            .entry(phase_for_action(action))
            .or_default()
            .push(action.clone());
    }

    let mut phases = Vec::new();
    for phase_type in [
        PhaseType::BreakCycle,
        PhaseType::FixLayering,
        PhaseType::RestructureModules,
        PhaseType::OptimizeFlow,
    ] {
        if let Some(actions) = grouped.get_mut(&phase_type) {
            actions.sort_by(|lhs, rhs| action_sort_key(lhs).cmp(&action_sort_key(rhs)));
            actions.dedup();
            if !actions.is_empty() {
                phases.push(RefactorPhase {
                    phase_type,
                    actions: actions.clone(),
                });
            }
        }
    }

    RefactorPlan {
        summary: PlanSummary {
            total_actions: phases.iter().map(|phase| phase.actions.len()).sum(),
            phase_count: phases.len(),
            expected_improvement: MetricsDelta {
                cycle_count: 0,
                layer_violations: 0,
                coupling_score_milli: 0,
            },
        },
        phases,
    }
}

pub fn generate_patches(plan: &RefactorPlan) -> Vec<CodePatch> {
    let mut patches = Vec::new();
    for phase in &plan.phases {
        for action in &phase.actions {
            patches.push(map_action_to_patch(action));
        }
    }
    patches
}

fn map_action_to_patch(action: &RefactorPlanAction) -> CodePatch {
    let (operations, description) = match action {
        RefactorPlanAction::IntroduceInterface { between } => {
            let interface_name = interface_name(&between.0, &between.1);
            (
                vec![
                    PatchOperation::CreateInterface {
                        name: interface_name.clone(),
                        between: (between.0.clone(), between.1.clone()),
                    },
                    PatchOperation::UpdateDependency {
                        from: between.1.clone(),
                        to: between.0.clone(),
                        via: Some(interface_name.clone()),
                    },
                ],
                format!(
                    "Introduce {} as an abstract boundary between {} and {}",
                    interface_name, between.0, between.1
                ),
            )
        }
        RefactorPlanAction::RemoveDependency { from, to } => (
            vec![PatchOperation::UpdateDependency {
                from: from.clone(),
                to: to.clone(),
                via: None,
            }],
            format!("Update dependency specification for {} -> {}", from, to),
        ),
        RefactorPlanAction::MoveDependency { from, to, via } => (
            vec![PatchOperation::UpdateDependency {
                from: from.clone(),
                to: to.clone(),
                via: via.clone(),
            }],
            format!("Redirect dependency from {} to {}", from, to),
        ),
        RefactorPlanAction::SplitModule { target } => (
            vec![PatchOperation::SplitModule {
                module: target.clone(),
                new_modules: vec![format!("{target}_core"), format!("{target}_api")],
            }],
            format!("Split {} into narrower modules", target),
        ),
        RefactorPlanAction::ExtractComponent { from } => (
            vec![PatchOperation::ExtractComponent {
                from: from.clone(),
                component: format!("{}_service", from),
            }],
            format!("Extract a component from {}", from),
        ),
        RefactorPlanAction::IsolateNode { node } => (
            vec![PatchOperation::ExtractComponent {
                from: node.clone(),
                component: format!("{}_isolated", node),
            }],
            format!("Isolate {}", node),
        ),
    };

    CodePatch {
        patch_id: stable_id(&format!("patch:{action:?}")),
        action: action.clone(),
        operations,
        description,
    }
}

pub fn map_issue_to_actions(issue: &Issue) -> Vec<RefactorPlanAction> {
    match issue.kind {
        IssueType::Cycle => match &issue.scope {
            IssueScope::Subgraph(nodes) if nodes.len() >= 2 => {
                let pair = stable_nodes(nodes);
                vec![RefactorPlanAction::IntroduceInterface {
                    between: (pair[0].clone(), pair[1].clone()),
                }]
            }
            _ => Vec::new(),
        },
        IssueType::LayerViolation => match &issue.scope {
            IssueScope::Edge(from, to) => vec![RefactorPlanAction::MoveDependency {
                from: from.clone(),
                to: to.clone(),
                via: None,
            }],
            _ => Vec::new(),
        },
        IssueType::GodObject => match &issue.scope {
            IssueScope::Node(node) => vec![RefactorPlanAction::SplitModule {
                target: node.clone(),
            }],
            _ => Vec::new(),
        },
        IssueType::Hub => match &issue.scope {
            IssueScope::Node(node) => {
                vec![RefactorPlanAction::ExtractComponent { from: node.clone() }]
            }
            _ => Vec::new(),
        },
        IssueType::DataFlowAnomaly => match &issue.scope {
            IssueScope::Edge(_, to) => {
                vec![RefactorPlanAction::ExtractComponent { from: to.clone() }]
            }
            IssueScope::Node(node) => {
                vec![RefactorPlanAction::ExtractComponent { from: node.clone() }]
            }
            _ => Vec::new(),
        },
        IssueType::OrphanNode => match &issue.scope {
            IssueScope::Node(node) => vec![RefactorPlanAction::IsolateNode { node: node.clone() }],
            _ => Vec::new(),
        },
        IssueType::RoleMismatch => match &issue.scope {
            IssueScope::Edge(from, to) => vec![RefactorPlanAction::MoveDependency {
                from: from.clone(),
                to: to.clone(),
                via: Some(virtual_interface_node(from, to)),
            }],
            _ => Vec::new(),
        },
    }
}

fn should_emit_action(issue: &Issue) -> bool {
    match issue.severity {
        Severity::Critical => true,
        Severity::High => true,
        Severity::Medium => true,
        Severity::Low => false,
    }
}

fn resolve_conflicts(actions: Vec<RefactorPlanAction>) -> Vec<RefactorPlanAction> {
    let cycle_pairs = actions
        .iter()
        .filter_map(|action| match action {
            RefactorPlanAction::IntroduceInterface { between } => {
                Some(canonical_pair(&between.0, &between.1))
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let move_pairs = actions
        .iter()
        .filter_map(|action| match action {
            RefactorPlanAction::MoveDependency {
                from,
                to,
                via: None,
            } => Some((from.clone(), to.clone())),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let bidirectional_pairs = move_pairs
        .iter()
        .filter_map(|(from, to)| {
            if move_pairs.contains(&(to.clone(), from.clone())) {
                Some(canonical_pair(from, to))
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();

    let split_targets = actions
        .iter()
        .filter_map(|action| match action {
            RefactorPlanAction::SplitModule { target } => Some(target.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let mut resolved = Vec::new();
    for action in actions {
        let conflicted = match &action {
            RefactorPlanAction::MoveDependency {
                from,
                to,
                via: None,
            } => {
                let pair = canonical_pair(from, to);
                cycle_pairs.contains(&pair) || bidirectional_pairs.contains(&pair)
            }
            RefactorPlanAction::ExtractComponent { from } => split_targets.contains(from),
            _ => false,
        };
        if !conflicted {
            resolved.push(action);
        }
    }
    resolved
}

fn phase_for_action(action: &RefactorPlanAction) -> PhaseType {
    match action {
        RefactorPlanAction::IntroduceInterface { .. } => PhaseType::BreakCycle,
        RefactorPlanAction::MoveDependency { .. } | RefactorPlanAction::RemoveDependency { .. } => {
            PhaseType::FixLayering
        }
        RefactorPlanAction::SplitModule { .. } => PhaseType::RestructureModules,
        RefactorPlanAction::ExtractComponent { .. } | RefactorPlanAction::IsolateNode { .. } => {
            PhaseType::OptimizeFlow
        }
    }
}

pub fn simulate_refactor(ir: &ArchitectureIr, plan: &RefactorPlan) -> SimulationResult {
    let before = simulation_metrics(ir);
    let mut new_ir = ir.clone();
    for phase in &plan.phases {
        for action in &phase.actions {
            match action {
                RefactorPlanAction::IntroduceInterface { between } => {
                    let interface_name = format!("{}_{}_interface", between.0, between.1);
                    let interface_id = stable_node_id(&interface_name);
                    if !new_ir.nodes.iter().any(|node| node.id == interface_id) {
                        new_ir.nodes.push(ArchitectureIrNode {
                            id: interface_id.clone(),
                            name: interface_name.clone(),
                            kind: ArchitectureNodeKind::Module,
                        });
                    }
                    let id_to_name = new_ir
                        .nodes
                        .iter()
                        .map(|node| (node.id.clone(), node.name.clone()))
                        .collect::<BTreeMap<_, _>>();
                    new_ir.edges.retain(|edge| {
                    let from = id_to_name.get(&edge.from);
                    let to = id_to_name.get(&edge.to);
                    !matches!(
                        (from.map(String::as_str), to.map(String::as_str)),
                        (Some(a), Some(b))
                            if (a == between.0 && b == between.1) || (a == between.1 && b == between.0)
                    )
                });
                    for endpoint in [between.0.clone(), between.1.clone()] {
                        if let Some(endpoint_id) = new_ir
                            .nodes
                            .iter()
                            .find(|node| node.name == endpoint)
                            .map(|node| node.id.clone())
                        {
                            new_ir.edges.push(ArchitectureIrEdge {
                                from: endpoint_id,
                                to: interface_id.clone(),
                                kind: ArchitectureEdgeKind::DependsOn,
                            });
                        }
                    }
                }
                RefactorPlanAction::RemoveDependency { from, to } => {
                    if let (Some(from_id), Some(to_id)) = (
                        new_ir
                            .nodes
                            .iter()
                            .find(|node| &node.name == from)
                            .map(|node| node.id.clone()),
                        new_ir
                            .nodes
                            .iter()
                            .find(|node| &node.name == to)
                            .map(|node| node.id.clone()),
                    ) {
                        new_ir
                            .edges
                            .retain(|edge| !(edge.from == from_id && edge.to == to_id));
                    }
                }
                RefactorPlanAction::SplitModule { target } => {
                    if let Some(source) = new_ir
                        .nodes
                        .iter()
                        .find(|node| &node.name == target)
                        .cloned()
                    {
                        let split_name = format!("{target}_extracted");
                        let split_id = stable_node_id(&split_name);
                        if !new_ir.nodes.iter().any(|node| node.id == split_id) {
                            new_ir.nodes.push(ArchitectureIrNode {
                                id: split_id.clone(),
                                name: split_name,
                                kind: source.kind,
                            });
                        }
                        if let Some(first_edge) = new_ir
                            .edges
                            .iter_mut()
                            .find(|edge| edge.from == source.id || edge.to == source.id)
                        {
                            if first_edge.from == source.id {
                                first_edge.from = split_id.clone();
                            } else {
                                first_edge.to = split_id.clone();
                            }
                        }
                    }
                }
                RefactorPlanAction::MoveDependency { from, to, via } => {
                    if let (Some(from_id), Some(to_id)) = (
                        new_ir
                            .nodes
                            .iter()
                            .find(|node| &node.name == from)
                            .map(|node| node.id.clone()),
                        new_ir
                            .nodes
                            .iter()
                            .find(|node| &node.name == to)
                            .map(|node| node.id.clone()),
                    ) {
                        new_ir
                            .edges
                            .retain(|edge| !(edge.from == from_id && edge.to == to_id));
                        if let Some(via) = via {
                            let via_id = stable_node_id(via);
                            if !new_ir.nodes.iter().any(|node| node.id == via_id) {
                                new_ir.nodes.push(ArchitectureIrNode {
                                    id: via_id.clone(),
                                    name: via.clone(),
                                    kind: ArchitectureNodeKind::Module,
                                });
                            }
                            new_ir.edges.push(ArchitectureIrEdge {
                                from: from_id,
                                to: via_id.clone(),
                                kind: ArchitectureEdgeKind::DependsOn,
                            });
                            new_ir.edges.push(ArchitectureIrEdge {
                                from: via_id,
                                to: to_id,
                                kind: ArchitectureEdgeKind::DependsOn,
                            });
                        }
                    }
                }
                RefactorPlanAction::ExtractComponent { from } => {
                    let extracted = format!("{from}_component");
                    let extracted_id = stable_node_id(&extracted);
                    if !new_ir.nodes.iter().any(|node| node.id == extracted_id) {
                        new_ir.nodes.push(ArchitectureIrNode {
                            id: extracted_id.clone(),
                            name: extracted,
                            kind: ArchitectureNodeKind::Module,
                        });
                    }
                }
                RefactorPlanAction::IsolateNode { node } => {
                    if let Some(node_id) = new_ir
                        .nodes
                        .iter()
                        .find(|candidate| &candidate.name == node)
                        .map(|candidate| candidate.id.clone())
                    {
                        new_ir
                            .edges
                            .retain(|edge| edge.from != node_id && edge.to != node_id);
                    }
                }
            }
        }
    }
    new_ir
        .nodes
        .sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id).then_with(|| lhs.name.cmp(&rhs.name)));
    new_ir
        .edges
        .sort_by(|lhs, rhs| lhs.from.cmp(&rhs.from).then_with(|| lhs.to.cmp(&rhs.to)));
    new_ir.edges.dedup();
    let after = simulation_metrics(&new_ir);
    SimulationResult {
        before: before.clone(),
        after: after.clone(),
        delta: MetricsDelta {
            cycle_count: after.cycle_count as isize - before.cycle_count as isize,
            layer_violations: after.layer_violations as isize - before.layer_violations as isize,
            coupling_score_milli: i32::from(after.coupling_score_milli)
                - i32::from(before.coupling_score_milli),
        },
    }
}

fn simulation_metrics(ir: &ArchitectureIr) -> SimulationMetrics {
    let cycles = cycle_report(ir)
        .cycles
        .iter()
        .filter(|cycle| cycle.size >= 2)
        .count();
    let layers = infer_layers(ir);
    let violations = layer_violations(ir, &layers).len();
    let coupling = if ir.nodes.is_empty() {
        0.0
    } else {
        ir.edges.len() as f32 / ir.nodes.len() as f32
    };
    SimulationMetrics {
        cycle_count: cycles,
        layer_violations: violations,
        coupling_score_milli: score_to_milli((coupling / 4.0).clamp(0.0, 1.0)),
    }
}

fn generate_issues(
    ir: &ArchitectureIr,
    cycle_report: &CycleReport,
    violations: &[LayerViolation],
    semantic: &SemanticInsights,
    data_flow: &DataFlowGraph,
    integrity: &ValidationReport,
) -> Vec<Issue> {
    let fan_in = fan_in_map(ir);
    let fan_out = fan_out_map(ir);
    let role_map = semantic
        .roles
        .iter()
        .map(|role| (role.node_name.clone(), role.role.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut issues = Vec::new();

    for cycle in &cycle_report.cycles {
        if cycle.size >= 2 {
            let nodes = stable_nodes(&cycle.nodes);
            issues.push(Issue {
                id: issue_id(
                    IssueType::Cycle,
                    &IssueScope::Subgraph(nodes.clone()),
                    &[Evidence {
                        kind: EvidenceType::Nodes,
                        value: nodes.join(", "),
                    }],
                ),
                kind: IssueType::Cycle,
                severity: Severity::Critical,
                scope: IssueScope::Subgraph(nodes.clone()),
                description: "Cyclic dependency detected".to_string(),
                evidence: vec![Evidence {
                    kind: EvidenceType::Nodes,
                    value: nodes.join(", "),
                }],
            });
        }
    }

    for violation in violations {
        issues.push(Issue {
            id: issue_id(
                IssueType::LayerViolation,
                &IssueScope::Edge(violation.from.clone(), violation.to.clone()),
                &[Evidence {
                    kind: EvidenceType::Metric,
                    value: format!("levels:{}->{}", violation.from_level, violation.to_level),
                }],
            ),
            kind: IssueType::LayerViolation,
            severity: Severity::High,
            scope: IssueScope::Edge(violation.from.clone(), violation.to.clone()),
            description: "Layer ordering violation observed".to_string(),
            evidence: vec![Evidence {
                kind: EvidenceType::Metric,
                value: format!("levels:{}->{}", violation.from_level, violation.to_level),
            }],
        });
    }

    for node in orphan_nodes(ir) {
        issues.push(Issue {
            id: issue_id(
                IssueType::OrphanNode,
                &IssueScope::Node(node.clone()),
                &[Evidence {
                    kind: EvidenceType::Metric,
                    value: "connected_edges:0".to_string(),
                }],
            ),
            kind: IssueType::OrphanNode,
            severity: Severity::Low,
            scope: IssueScope::Node(node),
            description: "Node is isolated from the dependency graph".to_string(),
            evidence: vec![Evidence {
                kind: EvidenceType::Metric,
                value: "connected_edges:0".to_string(),
            }],
        });
    }

    for pattern in &semantic.patterns {
        match pattern {
            Pattern::GodObject { node } => {
                let inbound = *fan_in.get(node).unwrap_or(&0);
                let outbound = *fan_out.get(node).unwrap_or(&0);
                let total = inbound + outbound;
                issues.push(Issue {
                    id: issue_id(
                        IssueType::GodObject,
                        &IssueScope::Node(node.clone()),
                        &[Evidence {
                            kind: EvidenceType::Metric,
                            value: format!("fan_in:{inbound};fan_out:{outbound}"),
                        }],
                    ),
                    kind: IssueType::GodObject,
                    severity: if total > 4 {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                    scope: IssueScope::Node(node.clone()),
                    description: "Responsibility concentration detected".to_string(),
                    evidence: vec![Evidence {
                        kind: EvidenceType::Metric,
                        value: format!("fan_in:{inbound};fan_out:{outbound}"),
                    }],
                });
            }
            Pattern::Hub { node } => issues.push(Issue {
                id: issue_id(
                    IssueType::Hub,
                    &IssueScope::Node(node.clone()),
                    &[Evidence {
                        kind: EvidenceType::Pattern,
                        value: "hub".to_string(),
                    }],
                ),
                kind: IssueType::Hub,
                severity: Severity::Medium,
                scope: IssueScope::Node(node.clone()),
                description: "Dependency hub observed".to_string(),
                evidence: vec![Evidence {
                    kind: EvidenceType::Pattern,
                    value: "hub".to_string(),
                }],
            }),
            Pattern::Cyclic { .. } | Pattern::Layered => {}
        }
    }

    if let Some(max_flow) = data_flow.flows.iter().max_by(|lhs, rhs| {
        lhs.weight_milli
            .cmp(&rhs.weight_milli)
            .then_with(|| lhs.from.cmp(&rhs.from))
            .then_with(|| lhs.to.cmp(&rhs.to))
    }) {
        if max_flow.weight_milli >= 850 {
            issues.push(Issue {
                id: issue_id(
                    IssueType::DataFlowAnomaly,
                    &IssueScope::Edge(max_flow.from.clone(), max_flow.to.clone()),
                    &[Evidence {
                        kind: EvidenceType::Metric,
                        value: format!("weight:{}", max_flow.weight_milli),
                    }],
                ),
                kind: IssueType::DataFlowAnomaly,
                severity: Severity::Medium,
                scope: IssueScope::Edge(max_flow.from.clone(), max_flow.to.clone()),
                description: "High data-flow concentration observed".to_string(),
                evidence: vec![Evidence {
                    kind: EvidenceType::Metric,
                    value: format!("weight:{}", max_flow.weight_milli),
                }],
            });
        }
    }

    for edge in &ir.edges {
        let Some(from) = node_name_by_id(ir, &edge.from) else {
            continue;
        };
        let Some(to) = node_name_by_id(ir, &edge.to) else {
            continue;
        };
        if matches!(role_map.get(&from), Some(NodeRole::Presentation))
            && matches!(role_map.get(&to), Some(NodeRole::Core))
        {
            issues.push(Issue {
                id: issue_id(
                    IssueType::RoleMismatch,
                    &IssueScope::Edge(from.clone(), to.clone()),
                    &[Evidence {
                        kind: EvidenceType::Role,
                        value: "Presentation->Core".to_string(),
                    }],
                ),
                kind: IssueType::RoleMismatch,
                severity: Severity::Medium,
                scope: IssueScope::Edge(from.clone(), to.clone()),
                description: "Role boundary mismatch observed".to_string(),
                evidence: vec![Evidence {
                    kind: EvidenceType::Role,
                    value: "Presentation->Core".to_string(),
                }],
            });
        }
    }

    for issue in &integrity.issues {
        if issue.message == "OrphanNode" {
            continue;
        }
    }

    issues.sort_by(|lhs, rhs| issue_sort_key(lhs).cmp(&issue_sort_key(rhs)));
    issues.dedup_by(|lhs, rhs| lhs.id == rhs.id);
    issues
}

fn score_to_milli(score: f32) -> u16 {
    (score.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn role_rank(role: &NodeRole) -> u8 {
    match role {
        NodeRole::Core => 0,
        NodeRole::Service => 1,
        NodeRole::Infrastructure => 2,
        NodeRole::Interface => 3,
        NodeRole::Presentation => 4,
        NodeRole::Utility => 5,
        NodeRole::Test => 6,
        NodeRole::Unknown => 7,
    }
}

fn fan_in_map(ir: &ArchitectureIr) -> BTreeMap<String, usize> {
    let id_to_name = ir
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut fan_in = ir
        .nodes
        .iter()
        .map(|node| (node.name.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for edge in &ir.edges {
        if let Some(name) = id_to_name.get(&edge.to) {
            *fan_in.entry(name.clone()).or_default() += 1;
        }
    }
    fan_in
}

fn fan_out_map(ir: &ArchitectureIr) -> BTreeMap<String, usize> {
    let id_to_name = ir
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut fan_out = ir
        .nodes
        .iter()
        .map(|node| (node.name.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for edge in &ir.edges {
        if let Some(name) = id_to_name.get(&edge.from) {
            *fan_out.entry(name.clone()).or_default() += 1;
        }
    }
    fan_out
}

fn pattern_sort_key(pattern: &Pattern) -> (u8, String) {
    match pattern {
        Pattern::Layered => (0, "layered".to_string()),
        Pattern::Cyclic { nodes } => (1, nodes.join("|")),
        Pattern::Hub { node } => (2, node.clone()),
        Pattern::GodObject { node } => (3, node.clone()),
    }
}

fn issue_id(kind: IssueType, scope: &IssueScope, evidence: &[Evidence]) -> String {
    let mut evidence_parts = evidence
        .iter()
        .map(|item| format!("{:?}:{}", item.kind, item.value))
        .collect::<Vec<_>>();
    evidence_parts.sort();
    stable_id(&format!(
        "{kind:?}|{}|{}",
        issue_scope_key(scope),
        evidence_parts.join("|")
    ))
}

fn issue_scope_key(scope: &IssueScope) -> String {
    match scope {
        IssueScope::Node(node) => format!("node:{node}"),
        IssueScope::Edge(from, to) => format!("edge:{from}->{to}"),
        IssueScope::Subgraph(nodes) => format!("subgraph:{}", stable_nodes(nodes).join("|")),
        IssueScope::Global => "global".to_string(),
    }
}

fn stable_nodes(nodes: &[String]) -> Vec<String> {
    let mut nodes = nodes.to_vec();
    nodes.sort();
    nodes.dedup();
    nodes
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Critical => 0,
        Severity::High => 1,
        Severity::Medium => 2,
        Severity::Low => 3,
    }
}

fn issue_sort_key(issue: &Issue) -> (u8, String, String) {
    (
        severity_rank(&issue.severity),
        issue_scope_key(&issue.scope),
        issue.id.clone(),
    )
}

fn action_priority(action: &RefactorPlanAction) -> u8 {
    match action {
        RefactorPlanAction::IntroduceInterface { .. } => 0,
        RefactorPlanAction::MoveDependency { .. } => 1,
        RefactorPlanAction::RemoveDependency { .. } => 1,
        RefactorPlanAction::SplitModule { .. } => 2,
        RefactorPlanAction::ExtractComponent { .. } => 3,
        RefactorPlanAction::IsolateNode { .. } => 4,
    }
}

fn action_sort_key(action: &RefactorPlanAction) -> (u8, String) {
    (action_priority(action), format!("{action:?}"))
}

fn virtual_interface_node(from: &str, to: &str) -> String {
    let mut pair = vec![from.to_string(), to.to_string()];
    pair.sort();
    format!("{}_{}_interface", pair[0], pair[1])
}

fn interface_name(from: &str, to: &str) -> String {
    let mut pair = vec![pascal_case(from), pascal_case(to)];
    pair.sort();
    format!("{}{}Interface", pair[0], pair[1])
}

fn pascal_case(value: &str) -> String {
    value
        .split(['_', '-', ' '])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn canonical_pair(lhs: &str, rhs: &str) -> (String, String) {
    if lhs <= rhs {
        (lhs.to_string(), rhs.to_string())
    } else {
        (rhs.to_string(), lhs.to_string())
    }
}

fn orphan_nodes(ir: &ArchitectureIr) -> Vec<String> {
    let mut connected = BTreeSet::new();
    for edge in &ir.edges {
        connected.insert(edge.from.clone());
        connected.insert(edge.to.clone());
    }
    ir.nodes
        .iter()
        .filter(|node| !connected.contains(&node.id))
        .map(|node| node.name.clone())
        .collect()
}

fn node_name_by_id(ir: &ArchitectureIr, node_id: &str) -> Option<String> {
    ir.nodes
        .iter()
        .find(|node| node.id == node_id)
        .map(|node| node.name.clone())
}

fn strongly_connected_components(ir: &ArchitectureIr) -> Vec<Vec<String>> {
    let mut adjacency = BTreeMap::<String, Vec<String>>::new();
    for node in &ir.nodes {
        adjacency.entry(node.id.clone()).or_default();
    }
    for edge in &ir.edges {
        adjacency
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }
    for targets in adjacency.values_mut() {
        targets.sort();
        targets.dedup();
    }

    struct TarjanState {
        index: usize,
        stack: Vec<String>,
        on_stack: BTreeSet<String>,
        indices: BTreeMap<String, usize>,
        lowlink: BTreeMap<String, usize>,
        components: Vec<Vec<String>>,
    }

    fn strong_connect(
        node: &str,
        adjacency: &BTreeMap<String, Vec<String>>,
        state: &mut TarjanState,
    ) {
        state.indices.insert(node.to_string(), state.index);
        state.lowlink.insert(node.to_string(), state.index);
        state.index += 1;
        state.stack.push(node.to_string());
        state.on_stack.insert(node.to_string());

        if let Some(targets) = adjacency.get(node) {
            for target in targets {
                if !state.indices.contains_key(target) {
                    strong_connect(target, adjacency, state);
                    let lowlink = state.lowlink[node].min(state.lowlink[target]);
                    state.lowlink.insert(node.to_string(), lowlink);
                } else if state.on_stack.contains(target) {
                    let lowlink = state.lowlink[node].min(state.indices[target]);
                    state.lowlink.insert(node.to_string(), lowlink);
                }
            }
        }

        if state.lowlink[node] == state.indices[node] {
            let mut component = Vec::new();
            while let Some(candidate) = state.stack.pop() {
                state.on_stack.remove(&candidate);
                component.push(candidate.clone());
                if candidate == node {
                    break;
                }
            }
            component.sort();
            state.components.push(component);
        }
    }

    let mut state = TarjanState {
        index: 0,
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        indices: BTreeMap::new(),
        lowlink: BTreeMap::new(),
        components: Vec::new(),
    };
    let nodes = adjacency.keys().cloned().collect::<Vec<_>>();
    for node in nodes {
        if !state.indices.contains_key(&node) {
            strong_connect(&node, &adjacency, &mut state);
        }
    }
    state.components.sort();
    state.components
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

    fn cyclic_graph_with_orphan() -> DesignGraph {
        DesignGraphBuilder::new()
            .add_node(DesignNode {
                id: DesignNodeId("debug".to_string()),
                name: "debug".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("renderer".to_string()),
                name: "renderer".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("world".to_string()),
                name: "world".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("src".to_string()),
                name: "src".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("debug".to_string()),
                target: DesignNodeId("renderer".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("renderer".to_string()),
                target: DesignNodeId("debug".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("debug".to_string()),
                target: DesignNodeId("world".to_string()),
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

        assert!(
            goals
                .iter()
                .any(|goal| matches!(goal, CanonicalGoal::ConstraintSatisfied(_)))
        );
        assert!(
            relations
                .iter()
                .any(|relation| relation.predicate == Predicate::Satisfies)
        );
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

    #[test]
    fn round_trip_preserves_isolated_nodes() {
        let graph = cyclic_graph_with_orphan();
        let report = validate_round_trip_design(&graph);
        assert!(report.is_valid, "{report:?}");
    }

    #[test]
    fn cycle_detection_reports_scc() {
        let ir = architecture_ir_from_design(&cyclic_graph_with_orphan());
        let report = cycle_report(&ir);
        assert!(report.has_cycle);
        assert_eq!(report.cycles.len(), 1);
        assert_eq!(
            report.cycles[0].nodes,
            vec!["debug".to_string(), "renderer".to_string()]
        );
    }

    #[test]
    fn layer_inference_assigns_leaf_to_zero() {
        let ir = architecture_ir_from_design(&cyclic_graph_with_orphan());
        let model = infer_layers(&ir);
        let level_of = model
            .layers
            .iter()
            .flat_map(|layer| {
                layer
                    .nodes
                    .iter()
                    .cloned()
                    .map(move |node| (node, layer.level))
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(level_of.get("world"), Some(&0));
        assert_eq!(level_of.get("debug"), Some(&1));
        assert_eq!(level_of.get("renderer"), Some(&1));
    }

    #[test]
    fn structural_analysis_surfaces_cycle_and_violation() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        assert!(analysis.cycle_report.has_cycle);
        assert!(!analysis.violations.is_empty());
        assert!(
            analysis
                .integrity
                .issues
                .iter()
                .any(|issue| issue.message == "CycleDetected")
        );
    }

    #[test]
    fn role_inference_basic() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        let roles = analysis
            .semantic
            .roles
            .iter()
            .map(|role| (role.node_name.as_str(), &role.role))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(roles.get("world"), Some(&&NodeRole::Core));
        assert_eq!(roles.get("renderer"), Some(&&NodeRole::Presentation));
        assert_eq!(roles.get("debug"), Some(&&NodeRole::Utility));
    }

    #[test]
    fn detect_god_object_pattern() {
        let graph = DesignGraphBuilder::new()
            .add_node(DesignNode {
                id: DesignNodeId("god".to_string()),
                name: "god".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("a".to_string()),
                name: "a".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("b".to_string()),
                name: "b".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("c".to_string()),
                name: "c".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("god".to_string()),
                target: DesignNodeId("a".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("god".to_string()),
                target: DesignNodeId("b".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("c".to_string()),
                target: DesignNodeId("god".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("b".to_string()),
                target: DesignNodeId("god".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .build();
        let analysis = structural_analysis(&graph);
        assert!(
            analysis
                .semantic
                .patterns
                .iter()
                .any(|pattern| matches!(pattern, Pattern::GodObject { node } if node == "god"))
        );
    }

    #[test]
    fn cycle_break_suggestion_is_present() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        assert!(analysis.semantic.suggestions.iter().any(|suggestion| {
            suggestion.reason.contains("Break cycle")
                && matches!(suggestion.action, RefactorAction::InvertDependency)
        }));
    }

    #[test]
    fn role_scoring_is_present() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        let renderer = analysis
            .semantic
            .roles
            .iter()
            .find(|role| role.node_name == "renderer")
            .expect("renderer role");
        assert!(renderer.score.presentation_milli >= renderer.score.core_milli);
        assert!(renderer.confidence_milli > 0);
    }

    #[test]
    fn data_flow_extraction_produces_edges() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        assert!(!analysis.data_flow.flows.is_empty());
        assert!(
            analysis
                .data_flow
                .flows
                .iter()
                .any(|flow| flow.from == "debug" && flow.to == "renderer")
        );
    }

    #[test]
    fn cycle_break_plan_contains_interface_step() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        assert!(
            analysis
                .refactor_plan
                .phases
                .iter()
                .flat_map(|phase| phase.actions.iter())
                .any(|action| matches!(action, RefactorPlanAction::IntroduceInterface { .. }))
        );
    }

    #[test]
    fn simulation_reduces_cycles() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        assert!(analysis.simulation.after.cycle_count <= analysis.simulation.before.cycle_count);
    }

    #[test]
    fn issue_from_cycle_is_generated() {
        let analysis = diagnostic_analysis(&cyclic_graph_with_orphan());
        assert!(analysis.issues.iter().any(|issue| {
            issue.kind == IssueType::Cycle && issue.severity == Severity::Critical
        }));
    }

    #[test]
    fn issue_ids_are_deterministic() {
        let lhs = diagnostic_analysis(&cyclic_graph_with_orphan());
        let rhs = diagnostic_analysis(&cyclic_graph_with_orphan());
        let lhs_ids = lhs
            .issues
            .into_iter()
            .map(|issue| issue.id)
            .collect::<Vec<_>>();
        let rhs_ids = rhs
            .issues
            .into_iter()
            .map(|issue| issue.id)
            .collect::<Vec<_>>();
        assert_eq!(lhs_ids, rhs_ids);
    }

    #[test]
    fn issue_json_schema_is_stable() {
        let analysis = diagnostic_analysis(&cyclic_graph_with_orphan());
        let json = serde_json::to_value(&analysis.issues).expect("issue json");
        let issues = json.as_array().expect("issues array");
        assert!(!issues.is_empty());
        let first = &issues[0];
        assert!(first["id"].is_string());
        assert!(first["kind"].is_string());
        assert!(first["severity"].is_string());
        assert!(first["scope"].is_object() || first["scope"].is_string());
        assert!(first["description"].is_string());
        assert!(first["evidence"].is_array());
    }

    #[test]
    fn severity_mapping_matches_issue_kind() {
        let analysis = diagnostic_analysis(&cyclic_graph_with_orphan());
        assert!(analysis
            .issues
            .iter()
            .any(|issue| issue.kind == IssueType::Cycle && issue.severity == Severity::Critical));
        assert!(analysis.issues.iter().any(
            |issue| issue.kind == IssueType::LayerViolation && issue.severity == Severity::High
        ));
        assert!(analysis
            .issues
            .iter()
            .any(|issue| issue.kind == IssueType::OrphanNode && issue.severity == Severity::Low));
    }

    #[test]
    fn cycle_to_interface() {
        let issue = diagnostic_analysis(&cyclic_graph_with_orphan())
            .issues
            .into_iter()
            .find(|issue| issue.kind == IssueType::Cycle)
            .expect("cycle issue");
        let actions = map_issue_to_actions(&issue);
        assert_eq!(
            actions,
            vec![RefactorPlanAction::IntroduceInterface {
                between: ("debug".to_string(), "renderer".to_string()),
            }]
        );
    }

    #[test]
    fn violation_to_move() {
        let issue = diagnostic_analysis(&cyclic_graph_with_orphan())
            .issues
            .into_iter()
            .find(|issue| issue.kind == IssueType::LayerViolation)
            .expect("violation issue");
        let actions = map_issue_to_actions(&issue);
        assert!(matches!(
            actions.first(),
            Some(RefactorPlanAction::MoveDependency { via: None, .. })
        ));
    }

    #[test]
    fn godobject_to_split() {
        let graph = DesignGraphBuilder::new()
            .add_node(DesignNode {
                id: DesignNodeId("god".to_string()),
                name: "god".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("a".to_string()),
                name: "a".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_node(DesignNode {
                id: DesignNodeId("b".to_string()),
                name: "b".to_string(),
                kind: DesignNodeKind::Module,
                metadata: Default::default(),
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("god".to_string()),
                target: DesignNodeId("a".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("god".to_string()),
                target: DesignNodeId("b".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("a".to_string()),
                target: DesignNodeId("god".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .add_edge(DesignEdge {
                source: DesignNodeId("b".to_string()),
                target: DesignNodeId("god".to_string()),
                relation: DesignRelation::DependsOn,
            })
            .build();
        let issue = diagnostic_analysis(&graph)
            .issues
            .into_iter()
            .find(|issue| issue.kind == IssueType::GodObject)
            .expect("god object issue");
        let actions = map_issue_to_actions(&issue);
        assert_eq!(
            actions,
            vec![RefactorPlanAction::SplitModule {
                target: "god".to_string(),
            }]
        );
    }

    #[test]
    fn mapping_is_deterministic() {
        let lhs = action_set_from_issues(&diagnostic_analysis(&cyclic_graph_with_orphan()).issues);
        let rhs = action_set_from_issues(&diagnostic_analysis(&cyclic_graph_with_orphan()).issues);
        assert_eq!(lhs, rhs);
        assert_eq!(
            lhs.actions.len(),
            lhs.actions.iter().collect::<BTreeSet<_>>().len()
        );
    }

    #[test]
    fn low_severity_filtered() {
        let issues = vec![Issue {
            id: "low".to_string(),
            kind: IssueType::OrphanNode,
            severity: Severity::Low,
            scope: IssueScope::Node("isolated".to_string()),
            description: "low".to_string(),
            evidence: Vec::new(),
        }];
        let actions = action_set_from_issues(&issues);
        assert!(actions.actions.is_empty());
    }

    #[test]
    fn critical_always_emitted() {
        let issues = diagnostic_analysis(&cyclic_graph_with_orphan()).issues;
        let actions = action_set_from_issues(&issues);
        assert!(
            actions
                .actions
                .iter()
                .any(|action| matches!(action, RefactorPlanAction::IntroduceInterface { .. }))
        );
    }

    #[test]
    fn remove_bidirectional_move_dependency() {
        let actions = resolve_conflicts(vec![
            RefactorPlanAction::MoveDependency {
                from: "a".to_string(),
                to: "b".to_string(),
                via: None,
            },
            RefactorPlanAction::MoveDependency {
                from: "b".to_string(),
                to: "a".to_string(),
                via: None,
            },
        ]);
        assert!(actions.is_empty());
    }

    #[test]
    fn cycle_overrides_move_dependency() {
        let actions =
            action_set_from_issues(&diagnostic_analysis(&cyclic_graph_with_orphan()).issues);
        assert!(
            actions
                .actions
                .iter()
                .any(|action| matches!(action, RefactorPlanAction::IntroduceInterface { .. }))
        );
        assert!(!actions.actions.iter().any(|action| {
            matches!(
                action,
                RefactorPlanAction::MoveDependency { from, to, via: None }
                    if (from == "debug" && to == "renderer") || (from == "renderer" && to == "debug")
            )
        }));
    }

    #[test]
    fn split_over_extract() {
        let actions = resolve_conflicts(vec![
            RefactorPlanAction::SplitModule {
                target: "renderer".to_string(),
            },
            RefactorPlanAction::ExtractComponent {
                from: "renderer".to_string(),
            },
        ]);
        assert_eq!(
            actions,
            vec![RefactorPlanAction::SplitModule {
                target: "renderer".to_string(),
            }]
        );
    }

    #[test]
    fn action_to_phase_mapping() {
        assert_eq!(
            phase_for_action(&RefactorPlanAction::IntroduceInterface {
                between: ("a".to_string(), "b".to_string()),
            }),
            PhaseType::BreakCycle
        );
        assert_eq!(
            phase_for_action(&RefactorPlanAction::MoveDependency {
                from: "a".to_string(),
                to: "b".to_string(),
                via: None,
            }),
            PhaseType::FixLayering
        );
        assert_eq!(
            phase_for_action(&RefactorPlanAction::SplitModule {
                target: "x".to_string(),
            }),
            PhaseType::RestructureModules
        );
        assert_eq!(
            phase_for_action(&RefactorPlanAction::ExtractComponent {
                from: "x".to_string(),
            }),
            PhaseType::OptimizeFlow
        );
    }

    #[test]
    fn phase_ordering() {
        let actions = ActionSet {
            actions: vec![
                RefactorPlanAction::ExtractComponent {
                    from: "world".to_string(),
                },
                RefactorPlanAction::SplitModule {
                    target: "renderer".to_string(),
                },
                RefactorPlanAction::MoveDependency {
                    from: "renderer".to_string(),
                    to: "world".to_string(),
                    via: Some("renderer_world_interface".to_string()),
                },
                RefactorPlanAction::IntroduceInterface {
                    between: ("debug".to_string(), "renderer".to_string()),
                },
            ],
        };
        let plan = refactor_plan(&actions);
        let order = plan
            .phases
            .iter()
            .map(|phase| &phase.phase_type)
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            vec![
                &PhaseType::BreakCycle,
                &PhaseType::FixLayering,
                &PhaseType::RestructureModules,
                &PhaseType::OptimizeFlow,
            ]
        );
    }

    #[test]
    fn interface_before_move() {
        let plan = refactor_plan(&ActionSet {
            actions: vec![
                RefactorPlanAction::MoveDependency {
                    from: "renderer".to_string(),
                    to: "world".to_string(),
                    via: Some("renderer_world_interface".to_string()),
                },
                RefactorPlanAction::IntroduceInterface {
                    between: ("debug".to_string(), "renderer".to_string()),
                },
            ],
        });
        let break_cycle_index = plan
            .phases
            .iter()
            .position(|phase| phase.phase_type == PhaseType::BreakCycle)
            .expect("break cycle phase");
        let fix_layering_index = plan
            .phases
            .iter()
            .position(|phase| phase.phase_type == PhaseType::FixLayering)
            .expect("fix layering phase");
        assert!(break_cycle_index < fix_layering_index);
    }

    #[test]
    fn plan_improves_metrics() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        assert!(analysis.simulation.delta.cycle_count <= 0);
        assert!(analysis.simulation.delta.layer_violations <= 0);
        assert!(analysis.simulation.delta.coupling_score_milli <= 0);
    }

    #[test]
    fn plan_deterministic() {
        let lhs = structural_analysis(&cyclic_graph_with_orphan()).refactor_plan;
        let rhs = structural_analysis(&cyclic_graph_with_orphan()).refactor_plan;
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn interface_patch_generation() {
        let patch = map_action_to_patch(&RefactorPlanAction::IntroduceInterface {
            between: ("debug".to_string(), "renderer".to_string()),
        });
        assert!(matches!(
            patch.operations.first(),
            Some(PatchOperation::CreateInterface { name, .. }) if name == "DebugRendererInterface"
        ));
        assert!(patch
            .operations
            .iter()
            .any(|operation| matches!(operation, PatchOperation::UpdateDependency { via: Some(via), .. } if via == "DebugRendererInterface")));
    }

    #[test]
    fn interface_naming() {
        assert_eq!(
            interface_name("debug", "renderer"),
            "DebugRendererInterface"
        );
        assert_eq!(
            interface_name("renderer", "world"),
            "RendererWorldInterface"
        );
    }

    #[test]
    fn patch_deterministic() {
        let plan = structural_analysis(&cyclic_graph_with_orphan()).refactor_plan;
        let lhs = generate_patches(&plan);
        let rhs = generate_patches(&plan);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn patch_matches_plan() {
        let analysis = structural_analysis(&cyclic_graph_with_orphan());
        let action_count = analysis
            .refactor_plan
            .phases
            .iter()
            .map(|phase| phase.actions.len())
            .sum::<usize>();
        assert_eq!(analysis.code_patches.len(), action_count);
    }
}
