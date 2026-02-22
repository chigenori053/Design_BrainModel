use crate::agent::AgentContext;
use crate::domain::{AgentEvent, AgentOutput, AgentRequest, DomainError, RuntimeState};
use crate::runtime::{AgentLifecycle, AgentRegistry, Dispatcher, NoopLifecycle};

pub struct Orchestrator {
    registry: AgentRegistry,
    dispatcher: Dispatcher,
    lifecycle: Box<dyn AgentLifecycle + Send>,
    state: RuntimeState,
}

impl Orchestrator {
    pub fn new(registry: AgentRegistry) -> Self {
        Self {
            registry,
            dispatcher: Dispatcher,
            lifecycle: Box::new(NoopLifecycle),
            state: RuntimeState::default(),
        }
    }

    pub fn with_lifecycle(
        registry: AgentRegistry,
        lifecycle: Box<dyn AgentLifecycle + Send>,
    ) -> Self {
        Self {
            registry,
            dispatcher: Dispatcher,
            lifecycle,
            state: RuntimeState::default(),
        }
    }

    pub fn registry_mut(&mut self) -> &mut AgentRegistry {
        &mut self.registry
    }

    pub fn dispatch(
        &mut self,
        req: AgentRequest,
        ctx: &AgentContext<'_>,
    ) -> Result<AgentOutput, DomainError> {
        self.lifecycle.on_dispatch_start(&req);
        let mut out = self.dispatcher.dispatch(&mut self.registry, req.clone(), ctx)?;
        self.process_events(&out.events, ctx)?;
        self.state.dispatch_count += 1;
        self.lifecycle.on_dispatch_end(&req);
        out.events.retain(|e| !matches!(e, AgentEvent::EmitTelemetry(_)));
        Ok(out)
    }

    pub fn state(&self) -> &RuntimeState {
        &self.state
    }

    fn process_events(
        &self,
        events: &[AgentEvent],
        ctx: &AgentContext<'_>,
    ) -> Result<(), DomainError> {
        for event in events {
            match event {
                AgentEvent::RequestSearch { .. } => {}
                AgentEvent::PersistMemory { key, value } => {
                    ctx.memory.put(key, value)?;
                }
                AgentEvent::WriteRawObjectives {
                    path,
                    depth,
                    objectives,
                } => {
                    crate::adapters::file_storage::append_raw_objectives(
                        path.as_path(),
                        *depth,
                        objectives,
                    )?;
                }
                AgentEvent::EmitTelemetry(event) => {
                    ctx.telemetry.emit(event);
                }
            }
        }
        Ok(())
    }
}

pub fn execute_soft_trace(
    config: crate::TraceRunConfig,
    params: crate::SoftTraceParams,
) -> Vec<crate::TraceRow> {
    let result = crate::capability::search::execute_soft_search_core(config, params);
    for event in result.events {
        if let AgentEvent::WriteRawObjectives {
            path,
            depth,
            objectives,
        } = event
        {
            crate::adapters::file_storage::append_raw_objectives(path.as_path(), depth, &objectives)
                .expect("failed to append raw trace rows");
        }
    }
    result.trace
}
