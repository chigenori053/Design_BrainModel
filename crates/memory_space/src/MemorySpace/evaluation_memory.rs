use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationScores {
    pub layering_score: f64,
    pub coupling_score: f64,
    pub cohesion_score: f64,
    pub complexity_score: f64,
    pub modularity_score: f64,
    pub overall_score: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationMetricsV2 {
    pub component_count: usize,
    pub dependency_count: usize,
    pub layer_count: usize,
    pub cycle_count: usize,
    pub average_degree: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationDiagnostics {
    pub layer_violations: Vec<String>,
    pub circular_dependencies: Vec<Vec<String>>,
    pub high_coupling_components: Vec<String>,
    pub interface_mismatch: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationRecord {
    pub architecture_hash: String,
    pub evaluation_scores: EvaluationScores,
    pub evaluation_metrics: EvaluationMetricsV2,
    pub diagnostics: EvaluationDiagnostics,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationMemoryDomain {
    records: BTreeMap<String, EvaluationRecord>,
}

impl EvaluationMemoryDomain {
    pub fn upsert(&mut self, record: EvaluationRecord) {
        self.records.insert(record.architecture_hash.clone(), record);
    }

    pub fn get(&self, architecture_hash: &str) -> Option<&EvaluationRecord> {
        self.records.get(architecture_hash)
    }

    pub fn all(&self) -> Vec<&EvaluationRecord> {
        self.records.values().collect()
    }
}
