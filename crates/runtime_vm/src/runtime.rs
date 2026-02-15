use brain_core::{CoreResult, CoreState, ReasoningEngine};

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
