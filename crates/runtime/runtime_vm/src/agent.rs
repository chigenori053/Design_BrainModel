use concept_engine::{ActivationEngine, ConceptEdge, ConceptGraph, ConceptId, RelationType};
use concept_field::{ConceptVector, build_field_from_vectors, concept_vector_from_id};
use memory_space_api::{ConceptMemorySpace as MemorySpace, MemoryEntry};
use reasoning_agent::hypothesis::generate_bound_concept_pairs;
use semantic_dhm::SemanticEngine;

use crate::runtime_context::{IntentGraph, IntentNode, RuntimeContext, RuntimeHypothesis};

pub trait Agent {
    fn execute(&mut self, ctx: &mut RuntimeContext);
}

#[derive(Default)]
pub struct SemanticAgent {
    semantic_engine: SemanticEngine,
}

impl SemanticAgent {
    pub fn new(semantic_engine: SemanticEngine) -> Self {
        Self { semantic_engine }
    }
}

impl Agent for SemanticAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        if ctx.input_text.trim().is_empty() {
            ctx.semantic_units.clear();
            return;
        }

        let embedding = embed_text(&ctx.input_text);
        let unit = self
            .semantic_engine
            .text_to_semantic_unit(&ctx.input_text, &embedding);
        ctx.semantic_units = vec![unit];
    }
}

#[derive(Default)]
pub struct ConceptAgent;

impl Agent for ConceptAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        let mut concepts = ctx
            .semantic_units
            .iter()
            .map(|unit| unit.concept)
            .collect::<Vec<_>>();
        concepts.sort_by_key(|id| id.0);
        concepts.dedup();
        ctx.concepts = concepts;
    }
}

#[derive(Default)]
pub struct IntentAgent;

impl Agent for IntentAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        ctx.intent_nodes = ctx
            .concepts
            .iter()
            .map(|concept| IntentNode {
                concept: *concept,
                weight: 1,
            })
            .collect();
        ctx.intent_graph = Some(build_intent_graph(&ctx.concepts));
    }
}

pub struct ConceptActivationAgent {
    engine: ActivationEngine,
}

impl Default for ConceptActivationAgent {
    fn default() -> Self {
        Self {
            engine: ActivationEngine {
                propagation_steps: 2,
                decay: 0.7,
            },
        }
    }
}

impl Agent for ConceptActivationAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        if ctx.concepts.is_empty() {
            ctx.concept_activation.clear();
            return;
        }

        let mut graph = ConceptGraph::default();
        for edge in ctx.concepts.windows(2) {
            graph.add_edge(ConceptEdge {
                source: edge[0],
                relation: RelationType::DependsOn,
                target: edge[1],
            });
        }

        let intent_concepts = ctx
            .intent_nodes
            .iter()
            .map(|n| n.concept)
            .collect::<Vec<_>>();
        let scores = self.engine.run(&graph, &intent_concepts);

        let mut activation = scores.into_iter().collect::<Vec<_>>();
        activation.sort_by(|lhs, rhs| rhs.1.total_cmp(&lhs.1).then_with(|| lhs.0.0.cmp(&rhs.0.0)));
        ctx.concept_activation = activation;
    }
}

#[derive(Default)]
pub struct ConceptFieldAgent;

impl Agent for ConceptFieldAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        let activation_map = ctx
            .concept_activation
            .iter()
            .copied()
            .collect::<std::collections::HashMap<_, _>>();

        let vectors = ctx
            .concepts
            .iter()
            .map(|concept| {
                let mut v = concept_vector_from_id(*concept, 16);
                v.weight = activation_map.get(concept).copied().unwrap_or(1.0).max(0.1);
                v
            })
            .collect::<Vec<ConceptVector>>();

        if vectors.is_empty() {
            ctx.concept_field = None;
            return;
        }

        ctx.concept_field = Some(build_field_from_vectors(&vectors));
    }
}

pub struct MemoryAgent {
    memory: MemorySpace,
    top_k: usize,
}

impl MemoryAgent {
    pub fn new(memory: MemorySpace, top_k: usize) -> Self {
        Self {
            memory,
            top_k: top_k.max(1),
        }
    }

    pub fn with_seeded_memory(mut self, entries: Vec<MemoryEntry>) -> Self {
        for entry in entries {
            self.memory.insert(entry);
        }
        self
    }
}

impl Default for MemoryAgent {
    fn default() -> Self {
        Self::new(MemorySpace::new(), 3)
    }
}

impl Agent for MemoryAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        let query = ctx
            .semantic_units
            .first()
            .map(|unit| unit.context_vector.as_slice())
            .unwrap_or(&[]);
        ctx.memory_candidates = self.memory.recall_concepts(query, self.top_k);
    }
}

/// Replaces legacy SearchControllerAgent + DesignSearchAgent.
/// Computes a search score from runtime signals and marks design search as complete.
#[derive(Default)]
pub struct RuntimeCoreSearchAgent;

impl Agent for RuntimeCoreSearchAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        let memory_signal = if ctx.memory_candidates.is_empty() {
            0.0
        } else {
            ctx.memory_candidates
                .iter()
                .map(|m| f64::from(m.score))
                .sum::<f64>()
                / ctx.memory_candidates.len() as f64
        };
        let concept_signal = (ctx.concepts.len() as f64 / 10.0).clamp(0.0, 1.0);
        let intent_signal = (ctx
            .intent_graph
            .as_ref()
            .map(|g| g.edges.len())
            .unwrap_or(0) as f64
            / 10.0)
            .clamp(0.0, 1.0);

        let score = (memory_signal + concept_signal + intent_signal) / 3.0;
        ctx.search_score = Some(score);
        ctx.design_search_done = true;
    }
}

pub struct ReasoningRuntimeAgent {
    max_pairs: usize,
}

impl ReasoningRuntimeAgent {
    pub fn new(max_pairs: usize) -> Self {
        Self {
            max_pairs: max_pairs.max(1),
        }
    }
}

impl Default for ReasoningRuntimeAgent {
    fn default() -> Self {
        Self::new(8)
    }
}

impl Agent for ReasoningRuntimeAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        let pairs = generate_bound_concept_pairs(&ctx.concepts, self.max_pairs);
        ctx.hypotheses = pairs
            .into_iter()
            .map(|(a, b)| RuntimeHypothesis {
                concept_a: a,
                concept_b: b,
            })
            .collect();
    }
}

#[derive(Default)]
pub struct EvaluationAgent;

impl Agent for EvaluationAgent {
    fn execute(&mut self, ctx: &mut RuntimeContext) {
        if let Some(score) = ctx.search_score
            && score < 0.2
        {
            ctx.hypotheses.truncate(1);
        }

        ctx.hypotheses
            .sort_by_key(|h| (h.concept_a.0, h.concept_b.0));
    }
}

pub fn build_intent_graph(concepts: &[ConceptId]) -> IntentGraph {
    let mut unique = concepts.to_vec();
    unique.sort_by_key(|id| id.0);
    unique.dedup();

    let edges = unique
        .windows(2)
        .map(|window| (window[0], window[1]))
        .collect::<Vec<_>>();

    IntentGraph { edges }
}

fn embed_text(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; 16];
    for (i, byte) in text.bytes().enumerate() {
        v[i % 16] += f32::from(byte) / 255.0;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_agent_generates_hypotheses_from_concepts() {
        let mut agent = ReasoningRuntimeAgent::new(4);
        let mut ctx = RuntimeContext {
            concepts: vec![
                ConceptId::from_name("DATABASE"),
                ConceptId::from_name("CACHE"),
            ],
            ..Default::default()
        };

        agent.execute(&mut ctx);

        assert!(!ctx.hypotheses.is_empty());
    }

    #[test]
    fn semantic_agent_produces_semantic_units() {
        let mut agent = SemanticAgent::default();
        let mut ctx = RuntimeContext {
            input_text: "optimize query".to_string(),
            ..Default::default()
        };

        agent.execute(&mut ctx);

        assert_eq!(ctx.semantic_units.len(), 1);
    }

    #[test]
    fn runtime_core_search_agent_sets_search_score() {
        let mut agent = RuntimeCoreSearchAgent;
        let mut ctx = RuntimeContext {
            concepts: vec![
                ConceptId::from_name("DATABASE"),
                ConceptId::from_name("CACHE"),
            ],
            ..Default::default()
        };

        agent.execute(&mut ctx);
        assert!(ctx.search_score.is_some());
        assert!(ctx.design_search_done);
    }
}
