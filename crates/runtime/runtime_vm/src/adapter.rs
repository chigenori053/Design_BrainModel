use ai_context::{
    AIContext, EvaluationState, ExperienceState, InferredKnowledge, RuntimeState as AiRuntimeState,
    StabilizedKnowledge,
};
use architecture_domain::ArchitectureState;
use evaluation_engine::EvaluationEngine;
use knowledge_engine::{
    KnowledgeEngine, KnowledgeGraph, ValidationScore, WebSearchRetriever,
    integrate_knowledge_into_semantic_graph, knowledge_graph_to_constraints,
};
use knowledge_lifecycle::{
    ConflictContext, KnowledgeLifecycleConfig, KnowledgeLifecycleEngine, KnowledgeLifecycleState,
    LifecycleMetrics,
};
use language_core::{
    LanguageState, language_search, semantic_graph_to_constraints, semantic_parser,
};
use language_reasoning::{
    knowledge_query_from_semantic_graph, knowledge_reasoning_effective_confidence,
    meaning_reasoning_search, reasoning_graph_to_constraints,
};
use memory_graph::DesignExperienceGraph;
use memory_space_core::{
    InMemoryMemoryStore, MemoryEngine, MemoryRecord, ModalityInput, RecallConfig, RecallQuery,
};
use memory_space_phase14::{
    DesignExperience, InMemoryMemorySpace, MemorySpace, architecture_hash,
    layer_sequence_from_state,
};
use runtime_core::{
    Phase9RuntimeContext, RequestId, RuntimeEvent, RuntimeStage, SearchMetrics, SearchSummary,
};
use world_model::DefaultSimulationEngine;
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
        let parsed_language_state = match &modality_input {
            ModalityInput::Text(text) => Some(semantic_parser(text)),
            _ => None,
        };
        let mut reasoned_language_state =
            parsed_language_state.as_ref().map(apply_meaning_reasoning);
        let (
            knowledge_graph,
            inferred_knowledge_graph,
            lifecycle_state,
            knowledge_validation,
            knowledge_retrieved,
        ) = if let Some(state) = reasoned_language_state.as_mut() {
            let knowledge_query =
                knowledge_query_from_semantic_graph(&state.semantic_graph, &state.source_text);
            let knowledge =
                KnowledgeEngine::new(WebSearchRetriever::default()).process_query(knowledge_query);
            let inferred_knowledge_graph = knowledge.knowledge_graph.clone();
            let mut stabilized_knowledge_graph = inferred_knowledge_graph.clone();
            let lifecycle_engine = KnowledgeLifecycleEngine::new(
                KnowledgeLifecycleConfig {
                    current_cycle: ctx.tick,
                    ..KnowledgeLifecycleConfig::default()
                },
                knowledge.validation,
                1,
                !state.semantic_graph.relations.is_empty(),
            );
            let lifecycle_state = lifecycle_engine.process_with_context(
                &mut stabilized_knowledge_graph,
                &ConflictContext {
                    semantic_graph: state.semantic_graph.clone(),
                    architecture_context: ArchitectureState::default(),
                },
            );
            integrate_knowledge_into_semantic_graph(
                &mut state.semantic_graph,
                &stabilized_knowledge_graph,
            );
            (
                stabilized_knowledge_graph,
                inferred_knowledge_graph,
                lifecycle_state,
                knowledge.validation,
                !knowledge.documents.is_empty(),
            )
        } else {
            (
                KnowledgeGraph::default(),
                KnowledgeGraph::default(),
                KnowledgeLifecycleState::default(),
                ValidationScore::default(),
                false,
            )
        };
        let language_state = reasoned_language_state.clone().map(language_search);
        let mut world_state = WorldState::new(ctx.tick, context_vector);
        if let Some(language_state) = &language_state {
            world_state.constraints = semantic_graph_to_constraints(language_state);
            world_state
                .constraints
                .extend(reasoning_graph_to_constraints(
                    &language_state.semantic_graph,
                ));
            world_state
                .constraints
                .extend(knowledge_graph_to_constraints(&knowledge_graph));
        }
        let simulator = DefaultSimulationEngine;
        let traced_simulation = simulator.simulate_with_trace(&world_state, Some(&recall_result));
        let simulation = traced_simulation.result.clone();
        world_state.simulation = Some(simulation.clone());
        world_state.evaluation.simulation_quality = simulation.total();
        let architecture_state = ArchitectureState::from_architecture(
            &world_state.architecture,
            world_state.constraints.clone(),
        );
        let evaluation_result = EvaluationEngine::default().evaluate(&architecture_state);
        world_state.score = ((world_state.evaluation.total() + evaluation_result.total_score)
            / 2.0)
            .clamp(0.0, 1.0);
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

        let request_id = format!("legacy-tick-{}", ctx.tick);
        let mut phase9 = Phase9RuntimeContext {
            request_id: RequestId(request_id.clone()),
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
            search_metrics: Some(SearchMetrics {
                explored_states: 1,
                unique_architectures: 1,
                pattern_matches: 0,
                policy_score_mean: 0.0,
                architecture_similarity: 1.0,
            }),
            ai_context: Some(AIContext::new(
                architecture_state,
                language_state
                    .as_ref()
                    .map(|state| state.semantic_graph.clone())
                    .unwrap_or_default(),
                knowledge_graph.clone(),
                InferredKnowledge {
                    graph: inferred_knowledge_graph.clone(),
                },
                StabilizedKnowledge {
                    graph: knowledge_graph.clone(),
                },
                lifecycle_state.lifecycle_metrics.clone(),
                ExperienceState {
                    graph: DesignExperienceGraph::default(),
                },
                EvaluationState {
                    latest: Some(evaluation_result),
                    history: vec![evaluation_result],
                },
                AiRuntimeState {
                    request_id,
                    stage: format!("{:?}", map_stage(ctx)),
                    event_count: 0,
                },
            )),
        };

        publish_taxonomy_events(
            &mut phase9,
            parsed_language_state.as_ref(),
            reasoned_language_state.as_ref(),
            language_state.as_ref(),
            &lifecycle_state,
            &knowledge_graph,
            knowledge_validation,
            knowledge_retrieved,
        );
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

fn publish_taxonomy_events(
    ctx: &mut Phase9RuntimeContext,
    parsed_language_state: Option<&LanguageState>,
    reasoned_language_state: Option<&LanguageState>,
    language_state: Option<&LanguageState>,
    lifecycle_state: &KnowledgeLifecycleState,
    knowledge_graph: &KnowledgeGraph,
    knowledge_validation: ValidationScore,
    knowledge_retrieved: bool,
) {
    ctx.event_bus.publish(RuntimeEvent::InputAccepted);
    ctx.advance(RuntimeStage::Normalize);
    ctx.event_bus.publish(RuntimeEvent::ModalityNormalized);
    if ctx.ai_context.is_some() {
        ctx.event_bus.publish(RuntimeEvent::AIContextInitialized);
    }
    if parsed_language_state.is_some() {
        ctx.event_bus.publish(RuntimeEvent::LanguageParsingStarted);
        ctx.event_bus
            .publish(RuntimeEvent::LanguageParsingCompleted);
    }
    if reasoned_language_state.is_some() {
        ctx.event_bus.publish(RuntimeEvent::MeaningReasoningStarted);
        ctx.event_bus
            .publish(RuntimeEvent::SemanticInferenceApplied);
        ctx.event_bus
            .publish(RuntimeEvent::MeaningReasoningCompleted);
        ctx.event_bus.publish(RuntimeEvent::KnowledgeQueryIssued);
        if knowledge_retrieved {
            ctx.event_bus.publish(RuntimeEvent::KnowledgeRetrieved);
        }
        if !knowledge_graph.entities.is_empty() {
            ctx.event_bus.publish(RuntimeEvent::KnowledgeParsed);
        }
        if knowledge_validation.confidence > 0.0 {
            ctx.event_bus.publish(RuntimeEvent::KnowledgeValidated);
        }
        if knowledge_reasoning_effective_confidence(knowledge_graph) > 0.0 {
            ctx.event_bus
                .publish(RuntimeEvent::KnowledgeEffectiveConfidenceCalculated);
        }
        ctx.event_bus
            .publish(RuntimeEvent::KnowledgeProvenanceRecorded);
        if lifecycle_state.reliability_evaluated > 0 {
            ctx.event_bus
                .publish(RuntimeEvent::KnowledgeSourceReliabilityEvaluated);
        }
        if lifecycle_state.embeddings_generated > 0 {
            ctx.event_bus
                .publish(RuntimeEvent::KnowledgeEmbeddingGenerated);
        }
        ctx.event_bus.publish(RuntimeEvent::KnowledgeAgingApplied);
        ctx.event_bus.publish(RuntimeEvent::KnowledgeReinforced);
        ctx.event_bus
            .publish(RuntimeEvent::KnowledgeSemanticClustered);
        if lifecycle_state.pruned_relations > 0 {
            ctx.event_bus.publish(RuntimeEvent::KnowledgePruned);
        }
        ctx.event_bus
            .publish(RuntimeEvent::KnowledgeQualityAnalyzed);
        ctx.event_bus
            .publish(RuntimeEvent::KnowledgeEntropyCalculated);
        let _ = lifecycle_state.half_life.half_life;
        ctx.event_bus
            .publish(RuntimeEvent::KnowledgeHalfLifeCalculated);
        let _ = lifecycle_state.lifecycle_metrics != LifecycleMetrics::default();
        ctx.event_bus.publish(RuntimeEvent::LifecycleMetricsUpdated);
        ctx.event_bus
            .publish(RuntimeEvent::KnowledgeTurnoverAnalyzed);
        if lifecycle_state.conflicts_resolved > 0 {
            ctx.event_bus
                .publish(RuntimeEvent::KnowledgeConflictResolved);
            ctx.event_bus
                .publish(RuntimeEvent::KnowledgeConflictResolvedWithContext);
        }
        if lifecycle_state.diversification_triggered {
            ctx.event_bus
                .publish(RuntimeEvent::KnowledgeDiversificationTriggered);
        }
        if ctx
            .ai_context
            .as_ref()
            .map(|ai_context| !ai_context.knowledge_graph.entities.is_empty())
            .unwrap_or(false)
        {
            ctx.event_bus.publish(RuntimeEvent::KnowledgeIntegrated);
        }
    }
    if language_state.is_some() {
        ctx.event_bus.publish(RuntimeEvent::LanguageSearchStarted);
        ctx.event_bus.publish(RuntimeEvent::LanguageSearchCompleted);
        ctx.event_bus
            .publish(RuntimeEvent::ArchitectureStateCreated);
    }

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
        ctx.event_bus.publish(RuntimeEvent::PatternMatchStarted);
        ctx.event_bus.publish(RuntimeEvent::PolicyEvaluationStarted);
        ctx.advance(RuntimeStage::Simulation);
        ctx.event_bus.publish(RuntimeEvent::SimulationStarted);
        let simulation_steps = ctx
            .world_state
            .as_ref()
            .map(|world_state| {
                DefaultSimulationEngine
                    .simulate_with_trace(world_state, ctx.recall_result.as_ref())
                    .traces
                    .simulation_trace
                    .step_count
            })
            .unwrap_or(0);
        for _ in 0..simulation_steps {
            ctx.event_bus.publish(RuntimeEvent::SimulationStep);
        }
        ctx.event_bus.publish(RuntimeEvent::SimulationCompleted);
        ctx.event_bus.publish(RuntimeEvent::CausalAnalysisStarted);
        ctx.event_bus.publish(RuntimeEvent::EvaluationStarted);
        if let Some(world_state) = ctx.world_state.as_ref() {
            let mut memory = InMemoryMemorySpace::with_bootstrap_patterns();
            let matched = memory.recall_patterns(world_state);
            if !matched.is_empty() {
                ctx.event_bus.publish(RuntimeEvent::PatternMatched);
            }
            ctx.event_bus.publish(RuntimeEvent::PatternMatchCompleted);
            let causal_validation = world_state.architecture.causal_graph().validate();
            ctx.event_bus.publish(RuntimeEvent::CausalClosureComputed);
            if causal_validation.valid {
                ctx.event_bus.publish(RuntimeEvent::CausalValidationPassed);
            } else {
                ctx.event_bus.publish(RuntimeEvent::CausalValidationFailed);
            }
            let before = memory.experience_count();
            memory.store_experience(DesignExperience {
                semantic_context: parsed_language_state
                    .map(|state| state.to_meaning_graph())
                    .unwrap_or_default(),
                inferred_semantics: reasoned_language_state
                    .map(|state| state.to_meaning_graph())
                    .unwrap_or_default(),
                architecture: world_state.architecture.clone(),
                architecture_hash: architecture_hash(world_state),
                causal_graph: world_state.architecture.causal_graph(),
                dependency_edges: world_state.architecture.graph.edges.clone(),
                layer_sequence: layer_sequence_from_state(world_state),
                score: world_state
                    .score
                    .max(
                        world_state
                            .simulation
                            .as_ref()
                            .map(|simulation| simulation.total())
                            .unwrap_or(0.0),
                    )
                    .max(0.8),
                search_depth: world_state.depth,
            });
            if memory.experience_count() > before {
                ctx.event_bus.publish(RuntimeEvent::ExperienceStored);
                ctx.event_bus.publish(RuntimeEvent::PolicyUpdated);
            }
            if let Some(ai_context) = ctx.ai_context.as_mut() {
                if let Some(result) = ai_context.evaluation_state.latest {
                    ai_context
                        .experience_state
                        .graph
                        .record_experience_with_lifecycle(
                            reasoned_language_state
                                .map(|state| state.to_meaning_graph())
                                .unwrap_or_default(),
                            Some(ai_context.knowledge_graph.clone()),
                            Some(knowledge_validation),
                            Some(ai_context.inferred_knowledge.graph.clone()),
                            Some(ai_context.stabilized_knowledge.graph.clone()),
                            Some(KnowledgeLifecycleState {
                                cycle: world_state.state_id,
                                quality_metrics: Default::default(),
                                lifecycle_metrics: ai_context.lifecycle_metrics.clone(),
                                source_reliabilities: Vec::new(),
                                half_life: Default::default(),
                                provenance_recorded: ai_context
                                    .stabilized_knowledge
                                    .graph
                                    .relations
                                    .len(),
                                reliability_evaluated: 0,
                                embeddings_generated: 0,
                                aged_relations: 0,
                                reinforced_relations: 0,
                                pruned_relations: 0,
                                semantic_clusters: 0,
                                semantic_pruned_relations: 0,
                                conflicts_resolved: 0,
                                diversification_triggered: false,
                                exploration_weight: 1.0,
                                reinforcement_rate_applied: 0.0,
                                turnover_metrics: Default::default(),
                            }),
                            Some(ai_context.lifecycle_metrics.clone()),
                            architecture_hash(world_state),
                            ai_context.architecture_state.clone(),
                            result,
                        );
                    ctx.event_bus.publish(RuntimeEvent::ExperienceGraphUpdated);
                }
            }
            ctx.event_bus
                .publish(RuntimeEvent::PolicyEvaluationCompleted);
            ctx.event_bus.publish(RuntimeEvent::EvaluationCompleted);
            ctx.search_metrics = Some(SearchMetrics {
                explored_states: 1,
                unique_architectures: 1,
                pattern_matches: matched.len(),
                policy_score_mean: if matched.is_empty() {
                    0.0
                } else {
                    matched
                        .iter()
                        .map(|pattern| pattern.average_score)
                        .sum::<f64>()
                        / matched.len() as f64
                },
                architecture_similarity: 1.0,
            });
        }
        ctx.advance(RuntimeStage::TransitionEvaluation);
        ctx.event_bus.publish(RuntimeEvent::TransitionEvaluated);
    }

    ctx.advance(RuntimeStage::ConsistencyEvaluation);
    ctx.evaluation = ctx.evaluation.or(Some(ConsistencyScore { value: 0.5 }));
    ctx.event_bus.publish(RuntimeEvent::ConsistencyScored);

    ctx.advance(RuntimeStage::Output);
    ctx.event_bus.publish(RuntimeEvent::OutputProduced);
    if let Some(ai_context) = ctx.ai_context.as_mut() {
        ai_context.runtime_state.stage = format!("{:?}", ctx.stage);
        ai_context.runtime_state.event_count = ctx.event_bus.len();
    }
}

fn apply_meaning_reasoning(state: &LanguageState) -> LanguageState {
    let mut next = state.clone();
    next.semantic_graph = meaning_reasoning_search(state.semantic_graph.clone());
    next.generated_sentence = None;
    next
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
