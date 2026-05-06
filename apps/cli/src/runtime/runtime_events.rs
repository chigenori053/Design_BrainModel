#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    Route(RouteEvent),
    StateTransition(StateTransitionEvent),
    Git(GitEvent),
    Validation(ValidationEvent),
    Replay(ReplayEvent),
    Debug(DebugEvent),
    Render(RenderEvent),
    BootstrapStarted,
    LoopStarted,
    InputAccepted(String),
    ShutdownRequested,
    RuntimeFailed(String),
}

impl RuntimeEvent {
    pub fn message(&self) -> String {
        match self {
            Self::Route(event) => format!("route {} -> {}", event.input, event.route),
            Self::StateTransition(event) => format!("state {} -> {}", event.from, event.to),
            Self::Git(event) => event.command.clone(),
            Self::Validation(event) => event.message.clone(),
            Self::Replay(event) => event.command.clone(),
            Self::Debug(event) => event.message.clone(),
            Self::Render(event) => event.reason.clone(),
            Self::BootstrapStarted => "bootstrap started".to_string(),
            Self::LoopStarted => "loop started".to_string(),
            Self::InputAccepted(input) => format!("input accepted: {input}"),
            Self::ShutdownRequested => "shutdown requested".to_string(),
            Self::RuntimeFailed(reason) => format!("runtime failed: {reason}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEvent {
    pub input: String,
    pub route: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransitionEvent {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitEvent {
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationEvent {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEvent {
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugEvent {
    pub timestamp: u64,
    pub source: String,
    pub message: String,
    pub level: DebugLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderEvent {
    pub reason: String,
}
