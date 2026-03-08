use brain_core::{CoreResult, CoreState, ReasoningEngine};

use crate::execution_mode::ExecutionMode;
use crate::pipeline::PipelineRuntime;
use crate::runtime_context::RuntimeContext;

#[derive(Debug, Clone)]
pub struct RuntimeVm {
    state: CoreState,
    engine: ReasoningEngine,
}

impl Default for RuntimeVm {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeVm {
    pub fn new() -> Self {
        Self {
            state: CoreState::default(),
            engine: ReasoningEngine,
        }
    }

    pub fn tick(&mut self) -> CoreResult<()> {
        self.state = self.engine.step(&self.state)?;
        Ok(())
    }

    pub fn state(&self) -> &CoreState {
        &self.state
    }
}

pub struct HybridVm {
    mode: ExecutionMode,
    runtime: PipelineRuntime,
    context: RuntimeContext,
}

impl HybridVm {
    pub fn new(mode: ExecutionMode) -> Self {
        Self {
            mode,
            runtime: PipelineRuntime::for_mode(mode),
            context: RuntimeContext::default(),
        }
    }

    pub fn with_context(mode: ExecutionMode, context: RuntimeContext) -> Self {
        Self {
            mode,
            runtime: PipelineRuntime::for_mode(mode),
            context,
        }
    }

    pub fn set_mode(&mut self, mode: ExecutionMode) {
        self.mode = mode;
        self.runtime = PipelineRuntime::for_mode(mode);
    }

    pub fn mode(&self) -> ExecutionMode {
        self.mode
    }

    pub fn set_input_text(&mut self, text: impl Into<String>) {
        self.context.input_text = text.into();
    }

    pub fn execute(&mut self) {
        self.runtime.execute(&mut self.context);
    }

    pub fn context(&self) -> &RuntimeContext {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut RuntimeContext {
        &mut self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_vm_runs_reasoning_pipeline() {
        let mut vm = HybridVm::new(ExecutionMode::Reasoning);
        vm.set_input_text("optimize database query performance");

        vm.execute();

        assert!(!vm.context().semantic_units.is_empty());
        assert!(!vm.context().concepts.is_empty());
        assert!(vm.context().tick > 0);
    }

    #[test]
    fn changing_mode_rebuilds_pipeline() {
        let mut vm = HybridVm::new(ExecutionMode::Analysis);
        vm.set_mode(ExecutionMode::Simulation);
        assert_eq!(vm.mode(), ExecutionMode::Simulation);
    }
}
