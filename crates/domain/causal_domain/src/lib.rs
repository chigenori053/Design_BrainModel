use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum IntentType {
    Refactor,
    FixBug,
    Rename,
}

impl IntentType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Refactor => "refactor",
            Self::FixBug => "fix_bug",
            Self::Rename => "rename",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ActionType {
    ExtractFunction,
    Inline,
    Rename,
    FixBug,
}

impl ActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExtractFunction => "extract_function",
            Self::Inline => "inline",
            Self::Rename => "rename",
            Self::FixBug => "fix_bug",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub action_type: ActionType,
    pub target: String,
    pub params: BTreeMap<String, String>,
    pub confidence: f32,
}

impl Action {
    pub fn new(action_type: ActionType) -> Self {
        Self {
            action_type,
            target: "*".to_string(),
            params: BTreeMap::new(),
            confidence: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Constraint {
    NoBehaviorChange,
    ScopeLimited,
}

impl Constraint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoBehaviorChange => "no_behavior_change",
            Self::ScopeLimited => "scope_limited",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CausalRelationKind {
    Enables,
    Inhibits,
    Requires,
    Emits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct CausalRelation {
    pub target: u64,
    pub kind: CausalRelationKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct CausalEdge {
    pub from: u64,
    pub to: u64,
    pub kind: CausalRelationKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CausalGraph {
    pub intent_id: String,
    pub intent_type: Option<IntentType>,
    pub causes: Vec<String>,
    pub goals: Vec<String>,
    pub constraints: Vec<Constraint>,
    pub actions: Vec<Action>,
    nodes: BTreeSet<u64>,
    edges: Vec<CausalEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalValidation {
    pub valid: bool,
    pub issues: Vec<String>,
}

impl CausalGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_natural_language(input: &str) -> Vec<Self> {
        initial_causal_graphs_from_nl(input)
    }

    pub fn is_ir_convertible_minimal(&self) -> bool {
        !self.intent_id.is_empty()
            && self.intent_type.is_some()
            && !self.goals.is_empty()
            && !self.actions.is_empty()
            && !self.constraints.is_empty()
    }

    pub fn add_node(&mut self, node: u64) {
        self.nodes.insert(node);
    }

    pub fn add_edge(&mut self, from: u64, to: u64, kind: CausalRelationKind) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.edges.push(CausalEdge { from, to, kind });
    }

    pub fn nodes(&self) -> impl Iterator<Item = &u64> {
        self.nodes.iter()
    }

    pub fn edges(&self) -> &[CausalEdge] {
        &self.edges
    }

    pub fn closure_map(&self) -> BTreeMap<u64, BTreeSet<u64>> {
        self.nodes
            .iter()
            .map(|node| (*node, self.causal_closure(*node)))
            .collect()
    }

    pub fn causal_closure(&self, source: u64) -> BTreeSet<u64> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([source]);

        while let Some(current) = queue.pop_front() {
            for edge in self.edges.iter().filter(|edge| edge.from == current) {
                if visited.insert(edge.to) {
                    queue.push_back(edge.to);
                }
            }
        }

        visited
    }

    pub fn validate(&self) -> CausalValidation {
        let mut issues = Vec::new();

        for edge in &self.edges {
            if edge.from == edge.to {
                issues.push(format!("self causal edge detected at node {}", edge.from));
            }
        }

        for edge in &self.edges {
            if !self.nodes.contains(&edge.from) || !self.nodes.contains(&edge.to) {
                issues.push(format!(
                    "edge {} -> {} references an unknown node",
                    edge.from, edge.to
                ));
            }
        }

        for edge in &self.edges {
            if self.edges.iter().any(|other| {
                edge.from == other.to && edge.to == other.from && edge.kind != other.kind
            }) {
                issues.push(format!(
                    "conflicting causal edges detected between {} and {}",
                    edge.from, edge.to
                ));
            }
        }

        let closure = self.closure_map();
        for node in self.nodes() {
            if closure
                .get(node)
                .map(|reachable| reachable.contains(node))
                .unwrap_or(false)
            {
                issues.push(format!("causal cycle detected at node {}", node));
            }
        }

        issues.sort();
        issues.dedup();

        CausalValidation {
            valid: issues.is_empty(),
            issues,
        }
    }
}

pub fn initial_causal_graphs_from_nl(input: &str) -> Vec<CausalGraph> {
    let intent_type = extract_intent(input);
    let mut causes = infer_causes(intent_type);
    let mut goals = generate_goals(intent_type);
    let mut actions = generate_actions(intent_type);
    let mut constraints = default_constraints();

    if causes.is_empty() {
        causes.push("readability_low".to_string());
    }
    if goals.is_empty() {
        goals.push("improve_readability".to_string());
    }
    if actions.is_empty() {
        actions.push(Action::new(ActionType::ExtractFunction));
    }
    if constraints.is_empty() {
        constraints = default_constraints();
    }

    vec![CausalGraph {
        intent_id: deterministic_intent_id(input, intent_type),
        intent_type: Some(intent_type),
        causes,
        goals,
        constraints,
        actions,
        nodes: BTreeSet::new(),
        edges: Vec::new(),
    }]
}

pub fn extract_intent(input: &str) -> IntentType {
    if input.contains("リファクタ") || input.contains("きれいに") {
        IntentType::Refactor
    } else if input.contains("直して") {
        IntentType::FixBug
    } else if input.contains("名前変更") {
        IntentType::Rename
    } else {
        IntentType::Refactor
    }
}

fn infer_causes(intent_type: IntentType) -> Vec<String> {
    match intent_type {
        IntentType::Refactor | IntentType::Rename => vec!["readability_low".to_string()],
        IntentType::FixBug => vec!["bug_present".to_string()],
    }
}

fn generate_goals(intent_type: IntentType) -> Vec<String> {
    match intent_type {
        IntentType::Refactor | IntentType::Rename => vec!["improve_readability".to_string()],
        IntentType::FixBug => vec!["remove_bug".to_string()],
    }
}

fn generate_actions(intent_type: IntentType) -> Vec<Action> {
    match intent_type {
        IntentType::Refactor => vec![
            Action::new(ActionType::ExtractFunction),
            Action::new(ActionType::Inline),
            Action::new(ActionType::Rename),
        ],
        IntentType::FixBug => vec![Action::new(ActionType::FixBug)],
        IntentType::Rename => vec![Action::new(ActionType::Rename)],
    }
}

fn default_constraints() -> Vec<Constraint> {
    vec![Constraint::NoBehaviorChange, Constraint::ScopeLimited]
}

fn deterministic_intent_id(input: &str, intent_type: IntentType) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in input
        .as_bytes()
        .iter()
        .chain(intent_type.as_str().as_bytes().iter())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let high = hash;
    let low = hash.rotate_left(17) ^ 0xa5a5a5a55a5a5a5a;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (high >> 32) as u32,
        (high >> 16) as u16,
        high as u16,
        (low >> 48) as u16,
        low & 0x0000_ffff_ffff_ffff
    )
}

impl Default for CausalValidation {
    fn default() -> Self {
        Self {
            valid: true,
            issues: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_step1_refactor_from_clean_up_text() {
        let graphs = initial_causal_graphs_from_nl("きれいにして");

        assert_eq!(graphs.len(), 1);
        let graph = &graphs[0];
        assert_eq!(graph.intent_type, Some(IntentType::Refactor));
        assert!(graph.goals.iter().any(|goal| goal == "improve_readability"));
        assert!(
            graph
                .actions
                .iter()
                .any(|action| action.action_type == ActionType::ExtractFunction)
        );
        assert!(graph.is_ir_convertible_minimal());
    }

    #[test]
    fn phase1_step1_fix_bug_from_fix_text() {
        let graphs = CausalGraph::from_natural_language("バグを直して");

        let graph = &graphs[0];
        assert_eq!(graph.intent_type, Some(IntentType::FixBug));
        assert!(graph.goals.iter().any(|goal| goal == "remove_bug"));
        assert!(
            graph
                .actions
                .iter()
                .any(|action| action.action_type == ActionType::FixBug)
        );
        assert!(graph.constraints.contains(&Constraint::NoBehaviorChange));
        assert!(graph.constraints.contains(&Constraint::ScopeLimited));
    }

    #[test]
    fn phase1_step1_unknown_input_falls_back_to_refactor() {
        let graphs = initial_causal_graphs_from_nl("何とかして");

        assert_eq!(graphs.len(), 1);
        assert_eq!(graphs[0].intent_type, Some(IntentType::Refactor));
        assert!(graphs[0].is_ir_convertible_minimal());
    }

    #[test]
    fn phase1_step1_is_deterministic_and_never_empty_for_blank_input() {
        let lhs = initial_causal_graphs_from_nl("");
        let rhs = initial_causal_graphs_from_nl("");

        assert_eq!(lhs, rhs);
        assert_eq!(lhs.len(), 1);
        assert!(lhs[0].is_ir_convertible_minimal());
    }

    #[test]
    fn computes_transitive_closure() {
        let mut graph = CausalGraph::new();
        graph.add_edge(1, 2, CausalRelationKind::Requires);
        graph.add_edge(2, 3, CausalRelationKind::Enables);

        let closure = graph.causal_closure(1);

        assert!(closure.contains(&2));
        assert!(closure.contains(&3));
    }

    #[test]
    fn rejects_cycles() {
        let mut graph = CausalGraph::new();
        graph.add_edge(1, 2, CausalRelationKind::Requires);
        graph.add_edge(2, 1, CausalRelationKind::Requires);

        let validation = graph.validate();

        assert!(!validation.valid);
        assert!(
            validation
                .issues
                .iter()
                .any(|issue| issue.contains("causal cycle"))
        );
    }
}
