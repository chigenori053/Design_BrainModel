use crate::DeterministicCandidateGenerator;
use crate::{
    ArchitectureCandidate, ArchitectureGrammar, ArchitectureTemplateEngine,
    BasicArchitectureEvaluator, BasicConstraintFilter, BeamSearchController, DesignSpaceBuilder,
    IntentModel, IntentProcessor, ParetoSetOptimizer, SearchConfig, SearchSpace, TemplateSelection,
};
use memory_space_phase14::{
    ArchitectureMetadata, DesignIntentRecord, DesignMemorySpace, ReasoningTrace, SearchStep,
    embed_architecture,
};

#[derive(Clone, Debug)]
pub struct ArchitectureSearchEngine {
    pub config: SearchConfig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub design_space: SearchSpace,
    pub template_selection: Option<TemplateSelection>,
    pub candidates: Vec<ArchitectureCandidate>,
    pub pareto_frontier: Vec<ArchitectureCandidate>,
    pub telemetry: crate::SearchTelemetry,
}

impl Default for ArchitectureSearchEngine {
    fn default() -> Self {
        Self {
            config: SearchConfig::default(),
        }
    }
}

impl ArchitectureSearchEngine {
    pub fn run(&self, intent: &IntentModel) -> SearchResult {
        let grammar = crate::ArchitectureGrammar::from_intent(intent);
        self.run_with_grammar(intent, grammar)
    }

    pub fn run_with_memory(
        &self,
        intent: &IntentModel,
        memory: &mut DesignMemorySpace,
    ) -> SearchResult {
        let grammar = crate::ArchitectureGrammar::from_intent(intent);
        self.run_with_grammar_and_memory(intent, grammar, Some(memory))
    }

    pub fn run_with_grammar(
        &self,
        intent: &IntentModel,
        grammar: ArchitectureGrammar,
    ) -> SearchResult {
        self.run_with_grammar_and_memory(intent, grammar, None)
    }

    fn run_with_grammar_and_memory(
        &self,
        intent: &IntentModel,
        grammar: ArchitectureGrammar,
        memory: Option<&mut DesignMemorySpace>,
    ) -> SearchResult {
        let design_space = DesignSpaceBuilder::new(grammar).build(intent);
        if design_space.component_catalog.is_empty() {
            return SearchResult {
                design_space,
                template_selection: None,
                candidates: Vec::new(),
                pareto_frontier: Vec::new(),
                telemetry: crate::SearchTelemetry::default(),
            };
        }
        let design_intent = IntentProcessor.process(intent);
        let required_components = design_intent.required_components.clone();
        let template_engine = ArchitectureTemplateEngine::with_builtin_library();
        let memory_guided = memory
            .as_deref()
            .map(|memory| !memory.template_memory.all().is_empty())
            .unwrap_or(false);
        let template_selection = if let Some(memory) = memory.as_deref() {
            template_engine.select_templates_with_memory(intent, memory)
        } else {
            template_engine.select_templates(intent)
        };
        let seed_template = template_engine.mutate_template(&template_selection.selected, intent);
        let effective_config = if memory_guided {
            SearchConfig {
                beam_width: self.config.beam_width.max(2).div_ceil(2),
                max_depth: self.config.max_depth.saturating_sub(1).max(1),
                max_candidates: self.config.max_candidates.max(4) / 2,
                pareto_limit: self.config.pareto_limit.max(2).div_ceil(2),
                timeout_ms: self.config.timeout_ms,
            }
        } else {
            self.config.clone()
        };

        let generator = DeterministicCandidateGenerator::new(design_space.clone(), design_intent);
        let filter = BasicConstraintFilter::new(design_space.clone());
        let controller = BeamSearchController::new(
            effective_config,
            generator,
            filter,
            BasicArchitectureEvaluator,
            ParetoSetOptimizer,
        );

        let mut initial = template_engine.expand_template(&seed_template, &design_space);
        for constraint in &design_space.constraints {
            if !initial.architecture.constraints.contains(constraint) {
                initial.architecture.constraints.push(constraint.clone());
            }
        }

        let mut outcome = controller.search_with_telemetry(initial);
        let complete = outcome
            .states
            .iter()
            .filter(|state| {
                required_components.iter().all(|component_type| {
                    state
                        .architecture
                        .components
                        .iter()
                        .any(|component| &component.component_type == component_type)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        if !complete.is_empty() {
            outcome.states = complete;
        }

        let candidates = outcome
            .states
            .iter()
            .map(|state| ArchitectureCandidate {
                architecture_ir: state.architecture.clone(),
                evaluation: state.score,
                generation_step: state.depth,
            })
            .collect::<Vec<_>>();

        if let Some(memory) = memory {
            let selected_template = seed_template.template_id.clone();
            let mut candidate_ids = Vec::new();
            for (index, state) in outcome.states.iter().enumerate() {
                let architecture_id = format!("search-{}-{}", selected_template, index + 1);
                let evaluation_score = state.score.desirability();
                let record = DesignMemorySpace::make_architecture_record(
                    architecture_id.clone(),
                    state.architecture.clone(),
                    selected_template.clone(),
                    evaluation_score,
                    ArchitectureMetadata {
                        search_depth: state.depth,
                        generation_time: 0,
                        search_iteration: index + 1,
                    },
                );
                memory.store_architecture(
                    record.clone(),
                    embed_architecture(&record.architecture_ir, record.evaluation_score),
                );
                let _ = memory.learn_template_from_architecture(&record, 0.8);
                candidate_ids.push(architecture_id);
            }
            let trace_id = format!(
                "trace:{}:{}",
                seed_template.template_id,
                intent.system_type.to_ascii_lowercase()
            );
            memory.store_reasoning_trace(
                ReasoningTrace {
                    trace_id,
                    intent: DesignIntentRecord {
                        intent_id: intent.system_type.to_ascii_lowercase(),
                        system_type: intent.system_type.clone(),
                        requirements: intent.requirements.clone(),
                        constraints: intent.constraints.architecture.iter().cloned().collect(),
                    },
                    selected_template: seed_template.template_id.clone(),
                    search_steps: outcome
                        .states
                        .iter()
                        .enumerate()
                        .map(|(index, state)| SearchStep {
                            step_id: index + 1,
                            action: format!("depth:{}", state.depth),
                            score: format!("{:.4}", state.score.desirability()),
                        })
                        .collect(),
                    candidate_architectures: candidate_ids.clone(),
                    final_architecture: candidate_ids.first().cloned().unwrap_or_default(),
                },
                vec![
                    design_space.component_catalog.len() as f32,
                    candidates.len() as f32,
                    self.config.max_depth as f32,
                ],
            );
        }

        SearchResult {
            design_space,
            template_selection: Some(TemplateSelection {
                selected: seed_template,
                alternatives: template_selection.alternatives,
            }),
            pareto_frontier: candidates
                .iter()
                .take(self.config.pareto_limit.max(1))
                .cloned()
                .collect(),
            candidates,
            telemetry: outcome.telemetry,
        }
    }
}
