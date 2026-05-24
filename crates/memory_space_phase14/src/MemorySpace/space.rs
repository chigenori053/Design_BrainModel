use std::collections::BTreeMap;

use architecture_ir::{ArchitectureConstraint, ArchitectureIR, ComponentType, Layer};

use super::architecture_memory::{
    ArchitectureMemoryDomain, ArchitectureMetadata, ArchitectureRecord, architecture_hash_string,
};
use super::evaluation_memory::{
    EvaluationDiagnostics, EvaluationMemoryDomain, EvaluationMetricsV2, EvaluationRecord,
    EvaluationScores,
};
use super::graph::MemoryGraph;
use super::index::MemoryIndex;
use super::reasoning_trace_memory::{ReasoningTrace, ReasoningTraceMemoryDomain};
use super::template_memory::{
    DependencyRuleRecord, TemplateMemoryDomain, TemplateMetadata, TemplateRecord, TopologyType,
};
use super::types::{DesignIntentRecord, MemoryId, MemoryMetadata, MemoryType, RelationType};

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateLearningEvent {
    pub architecture_id: String,
    pub template_id: String,
    pub score: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesignMemorySpace {
    pub graph: MemoryGraph,
    pub template_memory: TemplateMemoryDomain,
    pub architecture_memory: ArchitectureMemoryDomain,
    pub evaluation_memory: EvaluationMemoryDomain,
    pub reasoning_trace_memory: ReasoningTraceMemoryDomain,
    pub index: MemoryIndex,
    node_by_key: BTreeMap<String, MemoryId>,
}

impl DesignMemorySpace {
    pub fn recall(&self, embedding: &[f32], top_k: usize) -> Vec<super::types::MemoryNode> {
        let initial = self.graph.nearest_search(embedding, top_k.max(3));
        self.graph
            .activation_propagation(&initial, top_k, 2)
            .into_iter()
            .filter_map(|(node_id, _)| self.graph.get(node_id).cloned())
            .collect()
    }

    pub fn recall_templates_for_intent(
        &self,
        intent: &DesignIntentRecord,
        top_k: usize,
    ) -> Vec<TemplateRecord> {
        let embedding = embed_intent(intent);
        self.recall(&embedding, top_k.max(3))
            .into_iter()
            .filter(|node| node.node_type == MemoryType::Template)
            .filter_map(|node| self.template_memory.get(&node.metadata.label).cloned())
            .collect()
    }

    pub fn store_template(
        &mut self,
        record: TemplateRecord,
        embedding: Vec<f32>,
        relations: &[(String, RelationType, f32)],
    ) -> MemoryId {
        let key = format!("template:{}", record.template_id);
        let node_id = self.upsert_node(
            key.clone(),
            MemoryType::Template,
            embedding.clone(),
            record.template_id.clone(),
            BTreeMap::from([("topology".to_string(), topology_label(&record.topology))]),
        );
        self.index.index_embedding(node_id, embedding);
        self.template_memory.upsert(record.clone());
        for (target_key, relation, weight) in relations {
            if let Some(target) = self.node_by_key.get(target_key).copied() {
                self.graph
                    .add_edge(node_id, target, relation.clone(), *weight);
            }
        }
        node_id
    }

    pub fn store_architecture(
        &mut self,
        record: ArchitectureRecord,
        embedding: Vec<f32>,
    ) -> MemoryId {
        let key = format!("architecture:{}", record.architecture_id);
        let hash = architecture_hash_string(&record.architecture_ir);
        let node_id = self.upsert_node(
            key.clone(),
            MemoryType::Architecture,
            embedding.clone(),
            record.architecture_id.clone(),
            BTreeMap::from([
                (
                    "template_origin".to_string(),
                    record.template_origin.clone(),
                ),
                ("architecture_hash".to_string(), hash.clone()),
            ]),
        );
        self.index.index_embedding(node_id, embedding);
        self.index.index_hash(hash.clone(), node_id);
        if let Some(template_id) = self
            .node_by_key
            .get(&format!("template:{}", record.template_origin))
            .copied()
        {
            self.graph
                .add_edge(node_id, template_id, RelationType::DerivedFrom, 0.9);
            self.graph
                .add_edge(template_id, node_id, RelationType::Implements, 0.9);
        }
        self.architecture_memory.upsert(record);
        node_id
    }

    pub fn store_evaluation(&mut self, record: EvaluationRecord, embedding: Vec<f32>) -> MemoryId {
        let key = format!("evaluation:{}", record.architecture_hash);
        let node_id = self.upsert_node(
            key.clone(),
            MemoryType::Evaluation,
            embedding.clone(),
            record.architecture_hash.clone(),
            BTreeMap::from([(
                "architecture_hash".to_string(),
                record.architecture_hash.clone(),
            )]),
        );
        self.index.index_embedding(node_id, embedding);
        self.index
            .index_hash(record.architecture_hash.clone(), node_id);
        self.evaluation_memory.upsert(record.clone());
        if let Some(architecture_node) = self.index.resolve_hash(&record.architecture_hash) {
            self.graph
                .add_edge(architecture_node, node_id, RelationType::EvaluatedAs, 1.0);
        }
        node_id
    }

    pub fn store_reasoning_trace(
        &mut self,
        record: ReasoningTrace,
        embedding: Vec<f32>,
    ) -> MemoryId {
        let key = format!("trace:{}", record.trace_id);
        let selected_template = record.selected_template.clone();
        let final_architecture = record.final_architecture.clone();
        let node_id = self.upsert_node(
            key,
            MemoryType::Trace,
            embedding.clone(),
            record.trace_id.clone(),
            BTreeMap::from([("selected_template".to_string(), selected_template.clone())]),
        );
        self.index.index_embedding(node_id, embedding);
        self.reasoning_trace_memory.upsert(record);
        if let Some(template_node) = self
            .node_by_key
            .get(&format!("template:{selected_template}"))
            .copied()
        {
            self.graph
                .add_edge(node_id, template_node, RelationType::DependsOn, 0.8);
        }
        if let Some(architecture_node) = self
            .node_by_key
            .get(&format!("architecture:{final_architecture}"))
            .copied()
        {
            self.graph
                .add_edge(node_id, architecture_node, RelationType::DerivedFrom, 0.8);
        }
        node_id
    }

    pub fn find_evaluation(&self, architecture_hash: &str) -> Option<&EvaluationRecord> {
        self.evaluation_memory.get(architecture_hash)
    }

    pub fn find_similar_architectures(
        &self,
        architecture: &ArchitectureIR,
        top_k: usize,
    ) -> Vec<ArchitectureRecord> {
        self.architecture_memory.find_similar(architecture, top_k)
    }

    pub fn learn_template_from_architecture(
        &mut self,
        architecture: &ArchitectureRecord,
        threshold: f32,
    ) -> Option<TemplateLearningEvent> {
        if architecture.evaluation_score < threshold {
            return None;
        }

        let template_id = format!("learned:{}", architecture.architecture_id);
        if self.template_memory.get(&template_id).is_some() {
            return Some(TemplateLearningEvent {
                architecture_id: architecture.architecture_id.clone(),
                template_id,
                score: architecture.evaluation_score,
            });
        }

        let layers = architecture.architecture_ir.layers.clone();
        let dependency_rules = architecture
            .architecture_ir
            .dependencies
            .iter()
            .filter_map(|edge| {
                let from = component_type_for_node(&architecture.architecture_ir, edge.source)?;
                let to = component_type_for_node(&architecture.architecture_ir, edge.target)?;
                Some(DependencyRuleRecord { from, to })
            })
            .collect::<Vec<_>>();
        let constraints = inferred_constraints(&architecture.architecture_ir);
        let record = TemplateRecord {
            template_id: template_id.clone(),
            topology: infer_topology(&layers),
            layers,
            dependency_rules,
            constraints,
            metadata: TemplateMetadata {
                usage_count: 1,
                success_rate: 1.0,
                average_score: architecture.evaluation_score,
                created_from_architecture: Some(architecture.architecture_id.clone()),
            },
        };
        let embedding =
            embed_architecture(&architecture.architecture_ir, architecture.evaluation_score);
        self.store_template(
            record,
            embedding,
            &[(
                format!("architecture:{}", architecture.architecture_id),
                RelationType::DerivedFrom,
                0.95,
            )],
        );
        Some(TemplateLearningEvent {
            architecture_id: architecture.architecture_id.clone(),
            template_id,
            score: architecture.evaluation_score,
        })
    }

    fn upsert_node(
        &mut self,
        key: String,
        node_type: MemoryType,
        embedding: Vec<f32>,
        label: String,
        attributes: BTreeMap<String, String>,
    ) -> MemoryId {
        if let Some(node_id) = self.node_by_key.get(&key).copied() {
            return node_id;
        }
        let node_id = self.graph.add_node(
            node_type,
            embedding,
            MemoryMetadata {
                key: key.clone(),
                label,
                version: 1,
                attributes,
            },
        );
        self.node_by_key.insert(key, node_id);
        self.index.index_graph_neighbors(node_id, Vec::new());
        node_id
    }

    pub fn make_architecture_record(
        architecture_id: impl Into<String>,
        architecture_ir: ArchitectureIR,
        template_origin: impl Into<String>,
        evaluation_score: f32,
        metadata: ArchitectureMetadata,
    ) -> ArchitectureRecord {
        ArchitectureRecord {
            architecture_id: architecture_id.into(),
            architecture_ir,
            template_origin: template_origin.into(),
            evaluation_score,
            metadata,
        }
    }

    pub fn make_evaluation_record(
        architecture_hash: impl Into<String>,
        evaluation_scores: EvaluationScores,
        evaluation_metrics: EvaluationMetricsV2,
        diagnostics: EvaluationDiagnostics,
    ) -> EvaluationRecord {
        EvaluationRecord {
            architecture_hash: architecture_hash.into(),
            evaluation_scores,
            evaluation_metrics,
            diagnostics,
        }
    }
}

pub fn embed_intent(intent: &DesignIntentRecord) -> Vec<f32> {
    let requirement_signal = intent.requirements.len() as f32;
    let constraint_signal = intent.constraints.len() as f32;
    let system_signal = normalized_keyword_score(&intent.system_type);
    vec![
        system_signal,
        requirement_signal,
        constraint_signal,
        requirement_signal + constraint_signal,
    ]
}

pub fn embed_template(record: &TemplateRecord) -> Vec<f32> {
    vec![
        record.layers.len() as f32,
        record.dependency_rules.len() as f32,
        record.constraints.len() as f32,
        record.metadata.average_score,
    ]
}

pub fn embed_architecture(architecture: &ArchitectureIR, evaluation_score: f32) -> Vec<f32> {
    vec![
        architecture.components.len() as f32,
        architecture.dependencies.len() as f32,
        architecture.layers.len() as f32,
        evaluation_score,
    ]
}

pub fn embed_evaluation(record: &EvaluationRecord) -> Vec<f32> {
    vec![
        record.evaluation_scores.overall_score as f32,
        record.evaluation_metrics.component_count as f32,
        record.evaluation_metrics.dependency_count as f32,
        record.evaluation_metrics.cycle_count as f32,
    ]
}

fn component_type_for_node(
    architecture: &ArchitectureIR,
    node: architecture_ir::NodeId,
) -> Option<ComponentType> {
    match node {
        architecture_ir::NodeId::Component(id) => architecture
            .components
            .iter()
            .find(|component| component.id == id)
            .map(|component| component.component_type.clone()),
        _ => None,
    }
}

fn inferred_constraints(architecture: &ArchitectureIR) -> Vec<ArchitectureConstraint> {
    architecture.constraints.clone()
}

fn infer_topology(layers: &[Layer]) -> TopologyType {
    match layers.len() {
        0..=2 => TopologyType::Custom("minimal".to_string()),
        3 => TopologyType::Pipeline,
        _ => TopologyType::Layered,
    }
}

fn topology_label(topology: &TopologyType) -> String {
    match topology {
        TopologyType::Layered => "layered".to_string(),
        TopologyType::Hexagonal => "hexagonal".to_string(),
        TopologyType::Microservice => "microservice".to_string(),
        TopologyType::EventDriven => "event_driven".to_string(),
        TopologyType::Pipeline => "pipeline".to_string(),
        TopologyType::Custom(value) => value.clone(),
    }
}

fn normalized_keyword_score(value: &str) -> f32 {
    match value.to_ascii_lowercase().replace('_', "").as_str() {
        "webapi" | "api" => 1.0,
        "datapipeline" | "pipeline" => 0.8,
        "eventdriven" => 0.7,
        "microservice" => 0.6,
        _ => 0.4,
    }
}
