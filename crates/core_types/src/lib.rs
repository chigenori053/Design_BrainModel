#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveVector {
    pub f_struct: f64,
    pub f_field: f64,
    pub f_risk: f64,
    pub f_shape: f64,
}

impl ObjectiveVector {
    pub fn clamped(self) -> Self {
        Self {
            f_struct: self.f_struct.clamp(0.0, 1.0),
            f_field: self.f_field.clamp(0.0, 1.0),
            f_risk: self.f_risk.clamp(0.0, 1.0),
            f_shape: self.f_shape.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileVector {
    pub struct_weight: f64,
    pub field_weight: f64,
    pub risk_weight: f64,
    pub cost_weight: f64,
}

impl ProfileVector {
    pub fn normalized(self) -> Self {
        let sum = (self.struct_weight + self.field_weight + self.risk_weight + self.cost_weight)
            .max(1e-12);
        Self {
            struct_weight: self.struct_weight / sum,
            field_weight: self.field_weight / sum,
            risk_weight: self.risk_weight / sum,
            cost_weight: self.cost_weight / sum,
        }
    }

    pub fn score(&self, obj: &ObjectiveVector) -> f64 {
        let n = self.clone().normalized();
        (n.struct_weight * obj.f_struct
            + n.field_weight * obj.f_field
            + n.risk_weight * obj.f_risk
            + n.cost_weight * obj.f_shape)
            .clamp(0.0, 1.0)
    }
}

pub const P_INFER_ALPHA: f64 = 0.4;
pub const P_INFER_BETA: f64 = 0.3;
pub const P_INFER_GAMMA: f64 = 0.3;

pub fn stability_index(
    high_reliability: f64,
    safety_critical: f64,
    experimental: f64,
    rapid_prototype: f64,
) -> f64 {
    (high_reliability + safety_critical - experimental - rapid_prototype).clamp(-1.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Orchestration,
    Design,
    Semantic,
    Execution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassUnit {
    pub id: String,
    pub name: String,
    pub fields: Vec<String>,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureUnit {
    pub id: String,
    pub name: String,
    pub classes: Vec<ClassUnit>,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignUnit {
    pub id: String,
    pub name: String,
    pub structures: Vec<StructureUnit>,
}

pub type DesignId = String;
pub type ClassId = String;
pub type StructureId = String;
pub type UnitId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitRole {
    Interface,
    Implementation,
    Domain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyKind {
    Uses,
    Owns,
    Extends,
    Constrains,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNode {
    pub id: UnitId,
    pub role: UnitRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureNode {
    pub id: StructureId,
    pub units: Vec<UnitNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassNode {
    pub id: ClassId,
    pub structures: Vec<StructureNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignHierarchy {
    pub classes: Vec<ClassNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyEdge {
    pub from: UnitId,
    pub to: UnitId,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyGraph {
    pub edges: Vec<DependencyEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceSpec {
    pub resource: String,
    pub limit: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSpec {
    pub target: String,
    pub required_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constraint {
    Invariant(String),
    ResourceLimit(ResourceSpec),
    TypeRequirement(TypeSpec),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    Performance,
    Safety,
    Readability,
    MemoryEfficiency,
    Determinism,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignIntent {
    pub objective: ObjectiveKind,
    pub description: String,
    pub priority: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeFrontier {
    pub mutable_units: Vec<UnitId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignIR {
    pub id: DesignId,
    pub hierarchy: DesignHierarchy,
    pub dependencies: DependencyGraph,
    pub constraints: Vec<Constraint>,
    pub intent: DesignIntent,
    pub frontier: ChangeFrontier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticIR {
    pub concepts: Vec<String>,
    pub dependency_graph: Vec<(usize, usize)>,
    pub constraints: Vec<String>,
    pub objective: Option<ObjectiveKind>,
    pub mutable_concepts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericIR {
    pub features: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericResult {
    pub values: Vec<f64>,
}

pub trait DesignCompiler {
    fn to_ir(&self, design: &DesignUnit) -> DesignIR;
}

pub trait SemanticLowering {
    fn to_semantic_ir(&self, design_ir: &DesignIR) -> SemanticIR;
}

pub trait NumericLowering {
    fn to_numeric_ir(&self, semantic_ir: &SemanticIR) -> NumericIR;
}

pub trait NumericEvaluator {
    fn evaluate(&self, input: &NumericIR) -> NumericResult;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignIRDiff {
    pub added_units: Vec<UnitId>,
    pub removed_units: Vec<UnitId>,
    pub changed_intent: bool,
}

pub fn diff_design_ir(previous: &DesignIR, current: &DesignIR) -> DesignIRDiff {
    let mut prev_units = previous.frontier.mutable_units.clone();
    prev_units.sort();
    prev_units.dedup();

    let mut cur_units = current.frontier.mutable_units.clone();
    cur_units.sort();
    cur_units.dedup();

    let added_units = cur_units
        .iter()
        .filter(|u| !prev_units.contains(u))
        .cloned()
        .collect::<Vec<_>>();
    let removed_units = prev_units
        .iter()
        .filter(|u| !cur_units.contains(u))
        .cloned()
        .collect::<Vec<_>>();

    DesignIRDiff {
        added_units,
        removed_units,
        changed_intent: previous.intent != current.intent,
    }
}

pub fn lower_design_to_numeric<C, S, N>(
    compiler: &C,
    semantic_lowering: &S,
    numeric_lowering: &N,
    design: &DesignUnit,
) -> NumericIR
where
    C: DesignCompiler,
    S: SemanticLowering,
    N: NumericLowering,
{
    let design_ir = compiler.to_ir(design);
    let semantic_ir = semantic_lowering.to_semantic_ir(&design_ir);
    numeric_lowering.to_numeric_ir(&semantic_ir)
}

#[cfg(test)]
mod tests {
    use super::{
        ChangeFrontier, ClassNode, Constraint, DependencyGraph, DesignCompiler, DesignHierarchy,
        DesignIR, DesignIntent, DesignUnit, NumericIR, NumericLowering, ObjectiveKind, SemanticIR,
        SemanticLowering, StructureNode, StructureUnit, UnitNode, UnitRole, diff_design_ir,
        lower_design_to_numeric,
    };

    #[derive(Default)]
    struct DummyDesignCompiler;

    impl DesignCompiler for DummyDesignCompiler {
        fn to_ir(&self, design: &DesignUnit) -> DesignIR {
            DesignIR {
                id: design.id.clone(),
                hierarchy: DesignHierarchy {
                    classes: vec![ClassNode {
                        id: format!("class:{}", design.name),
                        structures: vec![StructureNode {
                            id: format!("structure:{}", design.name),
                            units: vec![UnitNode {
                                id: format!("unit:{}", design.name),
                                role: UnitRole::Implementation,
                            }],
                        }],
                    }],
                },
                dependencies: DependencyGraph { edges: Vec::new() },
                constraints: vec![Constraint::Invariant("dummy".to_string())],
                intent: DesignIntent {
                    objective: ObjectiveKind::Readability,
                    description: design.name.clone(),
                    priority: 1.0,
                },
                frontier: ChangeFrontier {
                    mutable_units: vec![format!("unit:{}", design.name)],
                },
            }
        }
    }

    #[derive(Default)]
    struct DummySemanticLowering;

    impl SemanticLowering for DummySemanticLowering {
        fn to_semantic_ir(&self, design_ir: &DesignIR) -> SemanticIR {
            SemanticIR {
                concepts: design_ir.frontier.mutable_units.clone(),
                dependency_graph: Vec::new(),
                constraints: vec!["dummy".to_string()],
                objective: Some(design_ir.intent.objective),
                mutable_concepts: design_ir.frontier.mutable_units.clone(),
            }
        }
    }

    #[derive(Default)]
    struct DummyNumericLowering;

    impl NumericLowering for DummyNumericLowering {
        fn to_numeric_ir(&self, semantic_ir: &SemanticIR) -> NumericIR {
            NumericIR {
                features: vec![semantic_ir.concepts.len() as f64],
            }
        }
    }

    #[test]
    fn design_to_numeric_requires_two_stage_lowering() {
        let design = DesignUnit {
            id: "d1".to_string(),
            name: "ServiceDesign".to_string(),
            structures: vec![StructureUnit {
                id: "s1".to_string(),
                name: "Core".to_string(),
                classes: Vec::new(),
                dependencies: Vec::new(),
            }],
        };

        let numeric = lower_design_to_numeric(
            &DummyDesignCompiler,
            &DummySemanticLowering,
            &DummyNumericLowering,
            &design,
        );

        assert_eq!(numeric.features, vec![1.0]);
    }

    #[test]
    fn design_ir_diff_tracks_frontier_and_intent() {
        let mut a = DummyDesignCompiler.to_ir(&DesignUnit {
            id: "d1".to_string(),
            name: "A".to_string(),
            structures: vec![StructureUnit {
                id: "s1".to_string(),
                name: "core".to_string(),
                classes: Vec::new(),
                dependencies: Vec::new(),
            }],
        });
        let mut b = a.clone();
        b.frontier.mutable_units.push("unit:new".to_string());
        b.intent.objective = ObjectiveKind::Performance;
        a.frontier.mutable_units.push("unit:old".to_string());

        let diff = diff_design_ir(&a, &b);
        assert_eq!(diff.added_units, vec!["unit:new".to_string()]);
        assert_eq!(diff.removed_units, vec!["unit:old".to_string()]);
        assert!(diff.changed_intent);
    }
}
use std::collections::{BTreeMap, BTreeSet};

pub type CanonicalId = String;
pub type AliasId = String;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(pub String);

impl Fingerprint {
    pub fn from_parts(parts: &[impl AsRef<str>]) -> Self {
        let mut hash = FNV_OFFSET_BASIS;
        for part in parts {
            hash = fnv1a_update(hash, part.as_ref().as_bytes());
            hash = fnv1a_update(hash, &[0xff]);
        }
        Self(format!("{hash:016x}"))
    }
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReuseDomain {
    FollowupContext,
    HolographicMemory,
    StateGraph,
    Replay,
    WorldModel,
    Plan,
    Validation,
    Preview,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReuseScope {
    Global,
    Session(String),
    Domain(ReuseDomain),
    Local(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReuseLifecycle {
    Active,
    Merged,
    Superseded,
    Archived,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalReuseRef {
    pub canonical_id: CanonicalId,
    pub domain: ReuseDomain,
    pub alias_ids: Vec<AliasId>,
    pub source_fingerprint: Fingerprint,
    pub semantic_fingerprint: Fingerprint,
    pub trajectory_fingerprint: Fingerprint,
    pub scope: ReuseScope,
    pub lifecycle: ReuseLifecycle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalReuseInput {
    pub domain: ReuseDomain,
    pub source: String,
    pub semantic_terms: Vec<String>,
    pub trajectory_terms: Vec<String>,
    pub scope: ReuseScope,
}

impl CanonicalReuseInput {
    pub fn new(domain: ReuseDomain, source: impl Into<String>) -> Self {
        Self {
            domain,
            source: source.into(),
            semantic_terms: Vec::new(),
            trajectory_terms: Vec::new(),
            scope: ReuseScope::Domain(domain),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseDecision {
    Reuse,
    Create,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalMatchKind {
    ExactSource,
    Semantic,
    TrajectoryContinuation,
    Novel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalReuseEvent {
    ReuseObserved,
    ReuseResolved,
    CanonicalCreated,
    AliasRegistered,
    DuplicateMerged,
    ContinuationResolved,
    ExactDuplicateMerged,
    SemanticAliasRegistered,
    CanonicalMemorySelected,
    CanonicalMemoryReused,
    CanonicalClusterExpanded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalResolution {
    pub decision: ReuseDecision,
    pub canonical_ref: CanonicalReuseRef,
    pub match_kind: CanonicalMatchKind,
    pub events: Vec<CanonicalReuseEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct CanonicalReuseResolver {
    refs: BTreeMap<CanonicalId, CanonicalReuseRef>,
    source_index: BTreeMap<(ReuseDomain, ReuseScope, Fingerprint), CanonicalId>,
    semantic_index: BTreeMap<(ReuseDomain, ReuseScope, Fingerprint), CanonicalId>,
    trajectory_index: BTreeMap<(ReuseDomain, ReuseScope, Fingerprint), CanonicalId>,
    aliases: BTreeMap<AliasId, CanonicalId>,
    next_id: u64,
}

impl CanonicalReuseResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resolve(&mut self, input: CanonicalReuseInput) -> CanonicalResolution {
        let normalized_source = normalize_text(&input.source);
        let source_fingerprint = Fingerprint::from_parts(&[
            domain_key(input.domain).to_string(),
            scope_key(&input.scope),
            normalized_source,
        ]);
        let semantic_fingerprint = semantic_fingerprint(input.domain, &input.scope, &input);
        let trajectory_fingerprint = trajectory_fingerprint(input.domain, &input.scope, &input);

        if let Some(canonical_id) = self.source_index.get(&(
            input.domain,
            input.scope.clone(),
            source_fingerprint.clone(),
        )) {
            return self.reuse_existing(
                canonical_id.clone(),
                source_fingerprint,
                semantic_fingerprint,
                trajectory_fingerprint,
                CanonicalMatchKind::ExactSource,
            );
        }

        if let Some(canonical_id) = self.semantic_index.get(&(
            input.domain,
            input.scope.clone(),
            semantic_fingerprint.clone(),
        )) {
            return self.reuse_existing(
                canonical_id.clone(),
                source_fingerprint,
                semantic_fingerprint,
                trajectory_fingerprint,
                CanonicalMatchKind::Semantic,
            );
        }

        if let Some(canonical_id) = self.trajectory_index.get(&(
            input.domain,
            input.scope.clone(),
            trajectory_fingerprint.clone(),
        )) {
            return self.reuse_existing(
                canonical_id.clone(),
                source_fingerprint,
                semantic_fingerprint,
                trajectory_fingerprint,
                CanonicalMatchKind::TrajectoryContinuation,
            );
        }

        self.create_new(
            input.domain,
            input.scope,
            source_fingerprint,
            semantic_fingerprint,
            trajectory_fingerprint,
        )
    }

    pub fn get(&self, canonical_id: &str) -> Option<&CanonicalReuseRef> {
        self.refs.get(canonical_id)
    }

    pub fn canonical_for_alias(&self, alias_id: &str) -> Option<&CanonicalId> {
        self.aliases.get(alias_id)
    }

    fn create_new(
        &mut self,
        domain: ReuseDomain,
        scope: ReuseScope,
        source_fingerprint: Fingerprint,
        semantic_fingerprint: Fingerprint,
        trajectory_fingerprint: Fingerprint,
    ) -> CanonicalResolution {
        self.next_id = self.next_id.saturating_add(1);
        let canonical_id = format!("canonical:{:016x}", self.next_id);
        let canonical_ref = CanonicalReuseRef {
            canonical_id: canonical_id.clone(),
            domain,
            alias_ids: Vec::new(),
            source_fingerprint: source_fingerprint.clone(),
            semantic_fingerprint: semantic_fingerprint.clone(),
            trajectory_fingerprint: trajectory_fingerprint.clone(),
            scope: scope.clone(),
            lifecycle: ReuseLifecycle::Active,
        };
        self.refs
            .insert(canonical_id.clone(), canonical_ref.clone());
        self.source_index.insert(
            (domain, scope.clone(), source_fingerprint),
            canonical_id.clone(),
        );
        self.semantic_index.insert(
            (domain, scope.clone(), semantic_fingerprint),
            canonical_id.clone(),
        );
        self.trajectory_index
            .insert((domain, scope, trajectory_fingerprint), canonical_id);
        CanonicalResolution {
            decision: ReuseDecision::Create,
            canonical_ref,
            match_kind: CanonicalMatchKind::Novel,
            events: vec![
                CanonicalReuseEvent::ReuseObserved,
                CanonicalReuseEvent::CanonicalCreated,
                CanonicalReuseEvent::ReuseResolved,
            ],
        }
    }

    fn reuse_existing(
        &mut self,
        canonical_id: CanonicalId,
        source_fingerprint: Fingerprint,
        semantic_fingerprint: Fingerprint,
        trajectory_fingerprint: Fingerprint,
        match_kind: CanonicalMatchKind,
    ) -> CanonicalResolution {
        let alias_id = format!(
            "alias:{}:{}:{}",
            source_fingerprint.0, semantic_fingerprint.0, trajectory_fingerprint.0
        );
        let canonical_ref = self
            .refs
            .get_mut(&canonical_id)
            .expect("indexed ref exists");
        if !canonical_ref.alias_ids.contains(&alias_id) {
            canonical_ref.alias_ids.push(alias_id.clone());
        }
        self.aliases.insert(alias_id, canonical_id);

        let mut events = vec![
            CanonicalReuseEvent::ReuseObserved,
            CanonicalReuseEvent::AliasRegistered,
        ];
        match match_kind {
            CanonicalMatchKind::ExactSource => {
                events.extend([
                    CanonicalReuseEvent::DuplicateMerged,
                    CanonicalReuseEvent::ExactDuplicateMerged,
                    CanonicalReuseEvent::CanonicalMemoryReused,
                ]);
            }
            CanonicalMatchKind::Semantic => {
                events.extend([
                    CanonicalReuseEvent::SemanticAliasRegistered,
                    CanonicalReuseEvent::CanonicalClusterExpanded,
                ]);
            }
            CanonicalMatchKind::TrajectoryContinuation => {
                events.push(CanonicalReuseEvent::ContinuationResolved);
            }
            CanonicalMatchKind::Novel => {}
        }
        events.push(CanonicalReuseEvent::ReuseResolved);

        CanonicalResolution {
            decision: ReuseDecision::Reuse,
            canonical_ref: canonical_ref.clone(),
            match_kind,
            events,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FollowupResolution {
    pub reused: bool,
    pub canonical_ref: CanonicalReuseRef,
    pub confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayRecord {
    pub canonical_id: CanonicalId,
    pub trajectory_fingerprint: Fingerprint,
    pub parent_canonical_id: Option<CanonicalId>,
}

pub fn resolve_followup_context(
    resolver: &mut CanonicalReuseResolver,
    input: impl Into<String>,
    history: &[String],
    scope: ReuseScope,
) -> FollowupResolution {
    let resolution = resolve_followup_context_with_events(resolver, input, history, scope);
    FollowupResolution {
        reused: resolution.decision == ReuseDecision::Reuse,
        confidence: match resolution.match_kind {
            CanonicalMatchKind::ExactSource => 1.0,
            CanonicalMatchKind::Semantic => 0.86,
            CanonicalMatchKind::TrajectoryContinuation => 0.78,
            CanonicalMatchKind::Novel => 0.0,
        },
        canonical_ref: resolution.canonical_ref,
    }
}

pub fn resolve_followup_context_with_events(
    resolver: &mut CanonicalReuseResolver,
    input: impl Into<String>,
    history: &[String],
    scope: ReuseScope,
) -> CanonicalResolution {
    let input = input.into();
    let mut semantic_terms = concept_terms(&input);
    semantic_terms.extend(history.iter().flat_map(|item| concept_terms(item)));
    resolver.resolve(CanonicalReuseInput {
        domain: ReuseDomain::FollowupContext,
        source: input,
        semantic_terms,
        trajectory_terms: history.to_vec(),
        scope,
    })
}

pub fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn concept_terms(value: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    normalize_text(value)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|term| term.len() > 2)
        .filter_map(|term| {
            if seen.insert(term.to_string()) {
                Some(term.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn semantic_fingerprint(
    domain: ReuseDomain,
    scope: &ReuseScope,
    input: &CanonicalReuseInput,
) -> Fingerprint {
    let mut terms = if input.semantic_terms.is_empty() {
        concept_terms(&input.source)
    } else {
        input
            .semantic_terms
            .iter()
            .map(|term| normalize_text(term))
            .collect()
    };
    terms.sort();
    terms.dedup();
    Fingerprint::from_parts(&[
        domain_key(domain).to_string(),
        scope_key(scope),
        terms.join("|"),
    ])
}

fn trajectory_fingerprint(
    domain: ReuseDomain,
    scope: &ReuseScope,
    input: &CanonicalReuseInput,
) -> Fingerprint {
    let trajectory = if input.trajectory_terms.is_empty() {
        normalize_text(&input.source)
    } else {
        input
            .trajectory_terms
            .iter()
            .map(|term| normalize_text(term))
            .collect::<Vec<_>>()
            .join("->")
    };
    Fingerprint::from_parts(&[domain_key(domain).to_string(), scope_key(scope), trajectory])
}

fn domain_key(domain: ReuseDomain) -> &'static str {
    match domain {
        ReuseDomain::FollowupContext => "followup_context",
        ReuseDomain::HolographicMemory => "holographic_memory",
        ReuseDomain::StateGraph => "state_graph",
        ReuseDomain::Replay => "replay",
        ReuseDomain::WorldModel => "world_model",
        ReuseDomain::Plan => "plan",
        ReuseDomain::Validation => "validation",
        ReuseDomain::Preview => "preview",
    }
}

fn scope_key(scope: &ReuseScope) -> String {
    match scope {
        ReuseScope::Global => "global".to_string(),
        ReuseScope::Session(session) => format!("session:{session}"),
        ReuseScope::Domain(domain) => format!("domain:{}", domain_key(*domain)),
        ReuseScope::Local(local) => format!("local:{local}"),
    }
}

#[cfg(test)]
mod canonical_reuse_tests {
    use super::{
        CanonicalMatchKind, CanonicalReuseInput, CanonicalReuseResolver, ReuseDecision,
        ReuseDomain, ReuseScope, resolve_followup_context,
    };

    #[test]
    fn exact_duplicate_reuses_existing_canonical_identity() {
        let mut resolver = CanonicalReuseResolver::new();
        let first = resolver.resolve(CanonicalReuseInput::new(
            ReuseDomain::HolographicMemory,
            "Save canonical memory",
        ));
        let second = resolver.resolve(CanonicalReuseInput::new(
            ReuseDomain::HolographicMemory,
            "  save   canonical MEMORY ",
        ));

        assert_eq!(first.decision, ReuseDecision::Create);
        assert_eq!(second.decision, ReuseDecision::Reuse);
        assert_eq!(second.match_kind, CanonicalMatchKind::ExactSource);
        assert_eq!(
            first.canonical_ref.canonical_id,
            second.canonical_ref.canonical_id
        );
        assert_eq!(second.canonical_ref.alias_ids.len(), 1);
    }

    #[test]
    fn semantic_duplicate_registers_alias_without_new_identity() {
        let mut resolver = CanonicalReuseResolver::new();
        let first = resolver.resolve(CanonicalReuseInput {
            domain: ReuseDomain::WorldModel,
            source: "entity user service".to_string(),
            semantic_terms: vec!["user".to_string(), "service".to_string()],
            trajectory_terms: Vec::new(),
            scope: ReuseScope::Global,
        });
        let second = resolver.resolve(CanonicalReuseInput {
            domain: ReuseDomain::WorldModel,
            source: "service for users".to_string(),
            semantic_terms: vec!["service".to_string(), "user".to_string()],
            trajectory_terms: Vec::new(),
            scope: ReuseScope::Global,
        });

        assert_eq!(first.decision, ReuseDecision::Create);
        assert_eq!(second.decision, ReuseDecision::Reuse);
        assert_eq!(second.match_kind, CanonicalMatchKind::Semantic);
        assert_eq!(
            first.canonical_ref.canonical_id,
            second.canonical_ref.canonical_id
        );
    }

    #[test]
    fn followup_resolution_reports_reuse_and_confidence() {
        let mut resolver = CanonicalReuseResolver::new();
        let history = vec!["build canonical reuse".to_string()];
        let first = resolve_followup_context(
            &mut resolver,
            "continue memory integration",
            &history,
            ReuseScope::Session("s1".to_string()),
        );
        let second = resolve_followup_context(
            &mut resolver,
            "continue memory integration",
            &history,
            ReuseScope::Session("s1".to_string()),
        );

        assert!(!first.reused);
        assert!(second.reused);
        assert_eq!(
            first.canonical_ref.canonical_id,
            second.canonical_ref.canonical_id
        );
        assert_eq!(second.confidence, 1.0);
    }
}
mod workspace_root;

pub use workspace_root::{
    DBM_WORKSPACE_ROOT, WorkspaceMigrationReport, WorkspaceRoot,
};
