use std::collections::HashMap;

use memory_space_phase14::PatternId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Role {
    LayerA,
    LayerB,
    LayerC,
    LayerD,
    Generic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphPattern {
    pub node_count: usize,
    pub dependency_edges: usize,
    pub causal_edges: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AbstractPattern {
    pub pattern_id: PatternId,
    pub node_roles: Vec<Role>,
    pub relation_structure: GraphPattern,
    pub average_score: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ActionType {
    AddUi,
    AddService,
    AddRepository,
    AddDatabase,
    RemoveDesignUnit,
    ConnectDependency,
    SplitStructure,
    MergeStructure,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SearchPolicy {
    pub action_weights: HashMap<ActionType, f64>,
    pub pattern_weights: HashMap<PatternId, f64>,
}
