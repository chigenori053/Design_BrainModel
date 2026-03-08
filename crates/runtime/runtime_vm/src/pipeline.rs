use crate::agent::{
    Agent, ConceptActivationAgent, ConceptAgent, ConceptFieldAgent, DesignSearchAgent,
    EvaluationAgent, IntentAgent, MemoryAgent, ReasoningRuntimeAgent, SearchControllerAgent,
    SemanticAgent,
};
use crate::execution_mode::ExecutionMode;
use crate::runtime_context::RuntimeContext;
use crate::scheduler::ExecutionScheduler;

pub struct Pipeline {
    scheduler: ExecutionScheduler,
}

impl Pipeline {
    pub fn new(agents: Vec<Box<dyn Agent>>) -> Self {
        Self {
            scheduler: ExecutionScheduler::with_agents(agents),
        }
    }

    pub fn execute(&mut self, ctx: &mut RuntimeContext) {
        self.scheduler.run(ctx);
    }

    pub fn len(&self) -> usize {
        self.scheduler.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scheduler.is_empty()
    }
}

pub struct PipelineRuntime {
    scheduler: ExecutionScheduler,
}

impl PipelineRuntime {
    pub fn new(scheduler: ExecutionScheduler) -> Self {
        Self { scheduler }
    }

    pub fn execute(&mut self, ctx: &mut RuntimeContext) {
        self.scheduler.run(ctx);
    }

    pub fn for_mode(mode: ExecutionMode) -> Self {
        match mode {
            ExecutionMode::Analysis => analysis_pipeline(),
            ExecutionMode::Reasoning => reasoning_pipeline(),
            ExecutionMode::Simulation => simulation_pipeline(),
        }
    }
}

pub fn analysis_pipeline() -> PipelineRuntime {
    let mut scheduler = ExecutionScheduler::new();
    scheduler.register(Box::new(SemanticAgent::default()));
    scheduler.register(Box::new(ConceptAgent));
    scheduler.register(Box::new(IntentAgent));
    scheduler.register(Box::new(ConceptActivationAgent::default()));
    scheduler.register(Box::new(ConceptFieldAgent));
    PipelineRuntime::new(scheduler)
}

pub fn reasoning_pipeline() -> PipelineRuntime {
    let mut scheduler = ExecutionScheduler::new();
    scheduler.register(Box::new(SemanticAgent::default()));
    scheduler.register(Box::new(ConceptAgent));
    scheduler.register(Box::new(IntentAgent));
    scheduler.register(Box::new(ConceptActivationAgent::default()));
    scheduler.register(Box::new(ConceptFieldAgent));
    scheduler.register(Box::new(MemoryAgent::default()));
    scheduler.register(Box::new(SearchControllerAgent::default()));
    scheduler.register(Box::new(DesignSearchAgent::default()));
    scheduler.register(Box::new(EvaluationAgent));
    PipelineRuntime::new(scheduler)
}

pub fn simulation_pipeline() -> PipelineRuntime {
    let mut scheduler = ExecutionScheduler::new();
    scheduler.register(Box::new(SemanticAgent::default()));
    scheduler.register(Box::new(ConceptAgent));
    scheduler.register(Box::new(IntentAgent));
    scheduler.register(Box::new(ConceptActivationAgent::default()));
    scheduler.register(Box::new(ConceptFieldAgent));
    scheduler.register(Box::new(SearchControllerAgent::default()));
    scheduler.register(Box::new(ReasoningRuntimeAgent::new(16)));
    scheduler.register(Box::new(EvaluationAgent));
    PipelineRuntime::new(scheduler)
}

#[cfg(test)]
mod tests {
    use crate::agent::{ConceptAgent, SemanticAgent};

    use super::*;

    #[test]
    fn pipeline_executes_agents_in_order() {
        let mut pipeline = Pipeline::new(vec![
            Box::new(SemanticAgent::default()),
            Box::new(ConceptAgent),
        ]);
        let mut ctx = RuntimeContext {
            input_text: "optimize query".to_string(),
            ..Default::default()
        };

        pipeline.execute(&mut ctx);

        assert_eq!(ctx.semantic_units.len(), 1);
        assert_eq!(ctx.concepts.len(), 1);
    }
}
