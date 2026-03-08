use memory_space_core::{
    InMemoryMemoryStore, MemoryEngine, MemoryRecord, ModalityInput, RecallConfig, RecallQuery,
};
use runtime_core::{
    Phase9RuntimeContext, RequestId, RuntimeEvent, RuntimeStage, SearchSummary,
};
use world_model::{DefaultSimulationEngine, SimulationEngine};
use world_model_core::{
    ConsistencyEvaluator, ConsistencyScore, DeltaConsistencyEvaluator, DeterministicWorldModel,
    Hypothesis, HypothesisGenerator, SimpleHypothesisGenerator, WorldModel, WorldState,
};

use crate::runtime_context::RuntimeContext;

#[derive(Debug, Clone, Default)]
pub struct Phase9RuntimeSnapshot {
    pub request_id: String,
    pub modality: &'static str,
    pub stage: RuntimeStage,
    pub recalled_memories: usize,
    pub hypotheses: usize,
    pub simulation_score: f64,
    pub events: Vec<RuntimeEvent>,
}

pub struct Phase9RuntimeAdapter;

impl Phase9RuntimeAdapter {
    pub fn from_legacy(ctx: &RuntimeContext) -> Phase9RuntimeContext {
        let modality_input = if ctx.input_text.trim().is_empty() {
            ModalityInput::Structured(serde_json::Value::Null)
        } else {
            ModalityInput::Text(ctx.input_text.clone())
        };
        let context_vector = lift_context_vector(ctx);
        let query_text = match &modality_input {
            ModalityInput::Text(text) => Some(text.clone()),
            _ => None,
        };
        let recall_query = RecallQuery {
            modality: modality_input.clone(),
            context_vector: context_vector.clone(),
            query_text,
        };
        let memory_engine = MemoryEngine::new(seed_memory_store(ctx));
        let recall_result = memory_engine.recall(&recall_query, RecallConfig { top_k: 3 });
        let mut world_state = WorldState::new(ctx.tick, context_vector);
        let simulator = DefaultSimulationEngine;
        let simulation = simulator.simulate(&world_state, Some(&recall_result));
        world_state.simulation = Some(simulation.clone());
        world_state.evaluation.simulation_quality = simulation.total();
        world_state.score = world_state.evaluation.total();
        let world_score = world_state.score;
        let simulation_score = simulation.total();
        let hypothesis_generator = SimpleHypothesisGenerator;
        let hypotheses = hypothesis_generator
            .generate(&world_state, Some(&recall_result))
            .unwrap_or_else(|_| fallback_hypotheses(ctx));
        let evaluator = DeltaConsistencyEvaluator;
        let evaluation = hypotheses
            .first()
            .and_then(|hypothesis| {
                DeterministicWorldModel
                    .transition(&world_state, hypothesis)
                    .ok()
                    .and_then(|predicted| evaluator.evaluate(&world_state, &predicted).ok())
            })
            .or(Some(ConsistencyScore { value: 0.5 }));

        let mut phase9 = Phase9RuntimeContext {
            request_id: RequestId(format!("legacy-tick-{}", ctx.tick)),
            modality_input,
            recall_result: Some(recall_result),
            world_state: Some(world_state),
            hypotheses,
            evaluation,
            stage: map_stage(ctx),
            event_bus: Default::default(),
            search_summary: Some(SearchSummary {
                search_states: 1,
                best_score: world_score,
                best_simulation_score: simulation_score,
            }),
        };

        publish_taxonomy_events(&mut phase9);
        phase9
    }

    pub fn snapshot(ctx: &RuntimeContext) -> Phase9RuntimeSnapshot {
        let phase9 = Self::from_legacy(ctx);
        Phase9RuntimeSnapshot {
            request_id: phase9.request_id.0,
            modality: phase9.modality_input.kind().as_str(),
            stage: phase9.stage,
            recalled_memories: phase9
                .recall_result
                .as_ref()
                .map(|result| result.candidates.len())
                .unwrap_or(0),
            hypotheses: phase9.hypotheses.len(),
            simulation_score: phase9
                .world_state
                .as_ref()
                .and_then(|world_state| world_state.simulation.as_ref())
                .map(|simulation| simulation.total())
                .unwrap_or(0.0),
            events: phase9.event_bus.events().cloned().collect(),
        }
    }

    pub fn event_sequence(ctx: &RuntimeContext) -> Vec<RuntimeEvent> {
        let phase9 = Self::from_legacy(ctx);
        phase9.event_bus.events().cloned().collect()
    }

    pub fn consistency(ctx: &RuntimeContext) -> Option<ConsistencyScore> {
        Self::from_legacy(ctx).evaluation
    }
}

fn map_stage(ctx: &RuntimeContext) -> RuntimeStage {
    if ctx.design_state.is_some() || ctx.hypothesis_graph.is_some() {
        RuntimeStage::Output
    } else if !ctx.hypotheses.is_empty() {
        RuntimeStage::HypothesisGeneration
    } else if !ctx.memory_candidates.is_empty() {
        RuntimeStage::Recall
    } else if !ctx.semantic_units.is_empty() || !ctx.concepts.is_empty() {
        RuntimeStage::Normalize
    } else {
        RuntimeStage::Input
    }
}

fn publish_taxonomy_events(ctx: &mut Phase9RuntimeContext) {
    ctx.event_bus.publish(RuntimeEvent::InputAccepted);
    ctx.advance(RuntimeStage::Normalize);
    ctx.event_bus.publish(RuntimeEvent::ModalityNormalized);

    ctx.advance(RuntimeStage::Recall);
    ctx.event_bus.publish(RuntimeEvent::MemoryRecallRequested);
    if ctx
        .recall_result
        .as_ref()
        .map(|result| !result.candidates.is_empty())
        .unwrap_or(false)
    {
        ctx.event_bus.publish(RuntimeEvent::MemoryRecallCompleted);
    }

    if !ctx.hypotheses.is_empty() {
        ctx.advance(RuntimeStage::HypothesisGeneration);
        ctx.event_bus.publish(RuntimeEvent::HypothesisGenerated);
        ctx.advance(RuntimeStage::Simulation);
        ctx.event_bus.publish(RuntimeEvent::SimulationStarted);
        ctx.event_bus.publish(RuntimeEvent::SimulationCompleted);
        ctx.advance(RuntimeStage::TransitionEvaluation);
        ctx.event_bus.publish(RuntimeEvent::TransitionEvaluated);
    }

    ctx.advance(RuntimeStage::ConsistencyEvaluation);
    ctx.evaluation = ctx.evaluation.or(Some(ConsistencyScore { value: 0.5 }));
    ctx.event_bus.publish(RuntimeEvent::ConsistencyScored);

    ctx.advance(RuntimeStage::Output);
    ctx.event_bus.publish(RuntimeEvent::OutputProduced);
}

fn lift_context_vector(ctx: &RuntimeContext) -> Vec<f64> {
    vec![
        ctx.semantic_units.len() as f64,
        ctx.concepts.len() as f64,
        ctx.memory_candidates.len() as f64,
    ]
}

fn seed_memory_store(ctx: &RuntimeContext) -> InMemoryMemoryStore {
    let base = lift_context_vector(ctx);
    InMemoryMemoryStore::with_records(vec![
        MemoryRecord {
            memory_id: 1,
            feature_vector: base.iter().map(|value| value + 1.0).collect(),
            metadata: serde_json::json!({"kind":"semantic"}),
        },
        MemoryRecord {
            memory_id: 2,
            feature_vector: base.iter().map(|value| value + 2.0).collect(),
            metadata: serde_json::json!({"kind":"design"}),
        },
        MemoryRecord {
            memory_id: 3,
            feature_vector: base.iter().map(|value| value + 3.0).collect(),
            metadata: serde_json::json!({"kind":"knowledge"}),
        },
    ])
}

fn fallback_hypotheses(ctx: &RuntimeContext) -> Vec<Hypothesis> {
    ctx.hypotheses
        .iter()
        .enumerate()
        .map(|(idx, hypothesis)| Hypothesis {
            hypothesis_id: idx as u64,
            predicted_state: WorldState::new(
                hypothesis.concept_a.0 as u64,
                vec![hypothesis.concept_a.0 as f64, hypothesis.concept_b.0 as f64],
            ),
            score: 0.5,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_context::RuntimeHypothesis;
    use concept_engine::ConceptId;

    #[test]
    fn adapter_lifts_legacy_context() {
        let ctx = RuntimeContext {
            input_text: "phase9".to_string(),
            hypotheses: vec![RuntimeHypothesis {
                concept_a: ConceptId(1),
                concept_b: ConceptId(2),
            }],
            tick: 3,
            ..Default::default()
        };

        let phase9 = Phase9RuntimeAdapter::from_legacy(&ctx);

        assert_eq!(phase9.request_id.0, "legacy-tick-3");
        assert_eq!(phase9.modality_input.kind().as_str(), "text");
        assert_eq!(phase9.recall_result.as_ref().unwrap().candidates.len(), 3);
        assert_eq!(phase9.hypotheses.len(), 1);
        assert!(phase9.evaluation.is_some());
        assert_eq!(phase9.stage, RuntimeStage::Output);
    }
}
