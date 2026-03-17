use architecture_ir::stable_v03::ArchitectureGraph;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentInput {
    pub raw: String,
}

impl IntentInput {
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntentState {
    pub raw: String,
    pub tokens: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureState {
    pub graph: ArchitectureGraph,
    pub candidate_id: Option<String>,
    pub score: Option<f64>,
}

impl Default for ArchitectureState {
    fn default() -> Self {
        Self {
            graph: ArchitectureGraph::default(),
            candidate_id: None,
            score: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldModel {
    pub intent: Option<IntentState>,
    pub architecture: Option<ArchitectureState>,
}

impl WorldModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_intent(&self, intent: IntentState) -> Self {
        let mut next = self.clone();
        next.intent = Some(intent);
        next
    }

    pub fn with_architecture(&self, architecture: ArchitectureState) -> Self {
        let mut next = self.clone();
        next.architecture = Some(architecture);
        next
    }
}
