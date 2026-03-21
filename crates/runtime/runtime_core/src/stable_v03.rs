use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use architecture_evaluator_core::stable_v03::{
    ArchitectureEvaluator, EvaluationResult, WeightedArchitectureEvaluator,
};
use architecture_ir::stable_v03::ArchitectureGraph;
use code_language_core::stable_v03::{
    CodeGenerator, CodeIRBuilder, ContextualCodeIRBuilder, DefaultCodeIRBuilder,
    DefaultContextualCodeIRBuilder, DefaultGeneratorRegistry, DefaultProfileResolver,
    GeneratedFile, GenerationContext, GeneratorRegistry, ProfileResolver, RustGenerator,
};
use constraint_engine::stable_v03::{
    CompositeConstraintEngine, Constraint as GraphConstraint, ConstraintEngine,
    LayerOrderConstraint, MaxNodeConstraint, NoCycleConstraint, NoIsolatedNodesConstraint,
};
use design_search_engine::stable_v03::{
    ArchitectureCandidate, Constraint as RecallConstraint, DesignSearchEngine, RecallContext,
    ReasoningTrace, RecalledPattern, SearchInput,
};
use implementation_core::stable_v03::{
    DefaultProjectGenerator, ExecutionPlan, ProjectGenerator, ProjectLayout,
};
use memory_space_phase14::stable_v03::{MemoryEngine, MemoryRecord, RecallInput};
use test_generation_core::stable_v03::{
    DefaultStructuralTestGenerator, TestGenerator, TestSuite, render_test_file, validate_test_suite,
};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper, DesignGraph};
use world_model::stable_v03::{IntentInput, IntentState};

use crate::explanation::{DefaultExplanationBuilder, Explanation, ExplanationBuilder};
use crate::intent_refiner::{
    ChatContext, Clarification, DefaultIntentRefiner, IntentExecution, IntentRefiner, IntentTrace,
    StructuredIntent,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    InvalidInput,
    SearchFailed,
    MemoryError,
}

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionTrace {
    pub recall_used: bool,
    pub candidate_count: usize,
    pub selected_score: f64,
    pub generated_hypotheses: usize,
    pub search_depth: usize,
    pub recall_hit_rate: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeResult {
    pub architecture: ArchitectureGraph,
    pub design: DesignGraph,
    pub files: Vec<GeneratedFile>,
    pub test_suites: Vec<TestSuite>,
    pub project_layout: ProjectLayout,
    pub execution_plan: ExecutionPlan,
    pub generation_contexts: Vec<GenerationContext>,
    pub trace: ExecutionTrace,
    pub reasoning_trace: Option<ReasoningTrace>,
    pub intent_trace: Option<IntentTrace>,
    pub explanation: Option<Explanation>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeExecutionResult {
    Executed(RuntimeResult),
    Clarification(Clarification),
}

pub struct CoreRuntime {
    pub executor: RuntimeExecutor,
    refiner: Arc<dyn IntentRefiner>,
}

impl CoreRuntime {
    pub fn new(
        memory: Arc<dyn MemoryEngine>,
        search: Arc<dyn DesignSearchEngine>,
        constraint: Arc<dyn ConstraintEngine>,
        evaluator: Arc<dyn ArchitectureEvaluator>,
        mapper: Arc<dyn ArchitectureMapper>,
        code_ir_builder: Arc<dyn CodeIRBuilder>,
        generator: Arc<dyn CodeGenerator>,
    ) -> Self {
        let refiner: Arc<dyn IntentRefiner> = Arc::new(DefaultIntentRefiner::new(memory.clone()));
        Self {
            executor: RuntimeExecutor {
                memory,
                search,
                constraint,
                evaluator,
                mapper,
                code_ir_builder,
                generator,
                profile_resolver: Arc::new(DefaultProfileResolver),
                contextual_code_ir_builder: Arc::new(DefaultContextualCodeIRBuilder),
                generator_registry: Arc::new(DefaultGeneratorRegistry),
                project_generator: Arc::new(DefaultProjectGenerator),
                test_generator: Arc::new(DefaultStructuralTestGenerator),
                explanation_builder: Arc::new(DefaultExplanationBuilder),
            },
            refiner,
        }
    }

    pub fn new_with_defaults(
        memory: Arc<dyn MemoryEngine>,
        search: Arc<dyn DesignSearchEngine>,
    ) -> Self {
        let constraints: Vec<Arc<dyn GraphConstraint>> = vec![
            Arc::new(NoIsolatedNodesConstraint),
            Arc::new(LayerOrderConstraint),
            Arc::new(NoCycleConstraint),
            Arc::new(MaxNodeConstraint { max_nodes: 12 }),
        ];
        Self::new(
            memory,
            search,
            Arc::new(CompositeConstraintEngine::new(constraints)),
            Arc::new(WeightedArchitectureEvaluator::default()),
            Arc::new(DefaultArchitectureMapper),
            Arc::new(DefaultCodeIRBuilder),
            Arc::new(RustGenerator),
        )
    }

    pub fn execute_from_text(
        &self,
        input: &str,
        context: &ChatContext,
    ) -> CoreResult<RuntimeExecutionResult> {
        let (execution, trace) = self.refiner.refine_with_trace(input, context)?;
        match execution {
            IntentExecution::Ready(intent) => {
                let result = self.executor.execute_structured(intent, Some(trace))?;
                Ok(RuntimeExecutionResult::Executed(result))
            }
            IntentExecution::NeedClarification(clarification) => {
                Ok(RuntimeExecutionResult::Clarification(clarification))
            }
        }
    }
}

pub struct RuntimeExecutor {
    memory: Arc<dyn MemoryEngine>,
    search: Arc<dyn DesignSearchEngine>,
    constraint: Arc<dyn ConstraintEngine>,
    evaluator: Arc<dyn ArchitectureEvaluator>,
    mapper: Arc<dyn ArchitectureMapper>,
    code_ir_builder: Arc<dyn CodeIRBuilder>,
    generator: Arc<dyn CodeGenerator>,
    profile_resolver: Arc<dyn ProfileResolver>,
    contextual_code_ir_builder: Arc<dyn ContextualCodeIRBuilder>,
    generator_registry: Arc<dyn GeneratorRegistry>,
    project_generator: Arc<dyn ProjectGenerator>,
    test_generator: Arc<dyn TestGenerator>,
    explanation_builder: Arc<dyn ExplanationBuilder>,
}

impl RuntimeExecutor {
    pub fn execute(&self, input: IntentInput) -> CoreResult<RuntimeResult> {
        let intent = parse(input)?;
        self.execute_intent_state(intent, None)
    }

    pub fn execute_structured(
        &self,
        intent: StructuredIntent,
        intent_trace: Option<IntentTrace>,
    ) -> CoreResult<RuntimeResult> {
        self.execute_intent_state(intent_state_from_structured(&intent), intent_trace)
    }

    fn execute_intent_state(
        &self,
        intent: IntentState,
        intent_trace: Option<IntentTrace>,
    ) -> CoreResult<RuntimeResult> {
        let recall_result = self.memory.recall(RecallInput {
            intent: intent.clone(),
            limit: 5,
        });
        let recall = to_recall_context(&intent, &recall_result);
        let search_result = self.search.search_with_trace(SearchInput {
            intent: intent.clone(),
            recall: recall.clone(),
        });
        let filtered = self.constraint.filter(search_result.candidates);
        let candidate_count = filtered.len();
        let scored = filtered
            .into_iter()
            .map(|candidate| {
                let evaluation = self.evaluator.evaluate(&candidate.architecture);
                (candidate, evaluation)
            })
            .collect::<Vec<_>>();
        let (selected, evaluation) = select_best(scored).ok_or(CoreError::SearchFailed)?;
        let design = self.mapper.map(&selected.architecture);
        let units = design.to_implementation_units();
        let generation_contexts = units
            .iter()
            .map(|unit| self.profile_resolver.resolve(unit, self.memory.as_ref()))
            .collect::<Vec<_>>();
        let test_suites = units
            .iter()
            .zip(generation_contexts.iter())
            .map(|(unit, ctx)| {
                let suite = self.test_generator.generate(unit, ctx);
                validate_test_suite(&suite, unit, ctx)
                    .expect("generated test suite should be valid");
                suite
            })
            .collect::<Vec<_>>();
        let modules = self.code_ir_builder.build(units.clone());
        let legacy_files = self.generator.generate(modules);
        let specialized_modules = self
            .contextual_code_ir_builder
            .build_with_context(units.into_iter().zip(generation_contexts.clone()).collect());
        let files = specialized_modules
            .into_iter()
            .flat_map(|module| {
                self.generator_registry
                    .get_generator(&module.context)
                    .generate(vec![module])
            })
            .collect::<Vec<_>>();
        let final_files = if files.is_empty() {
            legacy_files
        } else {
            files
        };
        let generated_test_files = test_suites
            .iter()
            .zip(generation_contexts.iter())
            .map(|(suite, ctx)| render_test_file(suite, ctx))
            .collect::<Vec<_>>();
        let (project_layout, execution_plan) = self.project_generator.generate(
            "generated_project",
            final_files.clone(),
            generation_contexts.clone(),
            generated_test_files,
        );

        self.memory.store(MemoryRecord {
            id: stable_id(&format!("{}:{}", intent.raw, selected.id)),
            text: intent.raw,
            tags: intent.tokens,
            embedding: None,
            architecture: Some(selected.architecture.clone()),
            relations: vec!["selected".to_string()],
        });

        let mut runtime_result = RuntimeResult {
            architecture: selected.architecture,
            design,
            files: final_files,
            test_suites,
            project_layout,
            execution_plan,
            generation_contexts,
            trace: ExecutionTrace {
                recall_used: recall.is_some(),
                candidate_count,
                selected_score: evaluation.score,
                generated_hypotheses: search_result.trace.generated_hypotheses,
                search_depth: search_result.trace.search_depth,
                recall_hit_rate: search_result.trace.recall_hit_rate,
            },
            reasoning_trace: Some(search_result.trace),
            intent_trace,
            explanation: None,
        };

        if let Some(trace) = runtime_result.intent_trace.clone() {
            let explanation = self.explanation_builder.build(&trace, &runtime_result);
            runtime_result.explanation = Some(explanation);
        }

        Ok(runtime_result)
    }
}

fn parse(input: IntentInput) -> CoreResult<IntentState> {
    let tokens = input
        .raw
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(CoreError::InvalidInput);
    }
    Ok(IntentState {
        raw: input.raw,
        tokens,
    })
}

fn to_recall_context(
    intent: &IntentState,
    recall: &memory_space_phase14::stable_v03::RecallResult,
) -> Option<RecallContext> {
    if recall.records.is_empty() {
        return None;
    }
    Some(RecallContext {
        patterns: recall
            .records
            .iter()
            .filter_map(|record| {
                record
                    .record
                    .architecture
                    .clone()
                    .map(|architecture| RecalledPattern {
                        record_id: record.record.id.clone(),
                        architecture,
                        score: record.score,
                        tags: record.record.tags.clone(),
                    })
            })
            .collect(),
        constraints: intent
            .tokens
            .iter()
            .filter(|token| token.contains("must") || token.contains("only"))
            .map(|token| RecallConstraint {
                key: "intent".to_string(),
                value: token.clone(),
            })
            .collect(),
        confidence: recall.confidence,
    })
}

fn select_best(
    candidates: Vec<(ArchitectureCandidate, EvaluationResult)>,
) -> Option<(ArchitectureCandidate, EvaluationResult)> {
    candidates.into_iter().max_by(|lhs, rhs| {
        lhs.1
            .score
            .partial_cmp(&rhs.1.score)
            .expect("evaluation score should be finite")
            .then_with(|| lhs.0.id.cmp(&rhs.0.id))
    })
}

fn stable_id(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("intent-{:016x}", hasher.finish())
}

fn intent_state_from_structured(intent: &StructuredIntent) -> IntentState {
    let mut tokens = intent
        .goal
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();

    for value in intent.slots.core.values() {
        for token in slot_tokens(&value.value) {
            tokens.insert(token);
        }
    }
    for value in intent.slots.system.values() {
        for token in slot_tokens(&value.value) {
            tokens.insert(token);
        }
    }
    for value in intent.slots.quality.values() {
        for token in slot_tokens(&value.value) {
            tokens.insert(token);
        }
    }
    for value in intent.slots.optional.values() {
        for token in slot_tokens(&value.value) {
            tokens.insert(token);
        }
    }

    IntentState {
        raw: intent.goal.clone(),
        tokens: tokens.into_iter().collect(),
    }
}

fn slot_tokens(value: &str) -> Vec<String> {
    let mut tokens = value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();
    match value {
        "postgres" | "mysql" | "sqlite" | "redis" => tokens.push("db".to_string()),
        _ => {}
    }
    tokens.sort();
    tokens.dedup();
    tokens
}
