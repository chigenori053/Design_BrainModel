pub mod agent;
pub mod context;
pub mod event;
pub mod modality;
pub mod ports;
pub mod search_core;
pub mod search_domain;
pub mod search_runtime;

pub use agent::{AgentInput, AgentOutput, RuntimeAgent};
pub use ai_context::AIContext;
pub use context::{Phase9RuntimeContext, RequestId, RuntimeStage, SearchMetrics, SearchSummary};
pub use event::{RuntimeEvent, RuntimeEventBus};
pub use modality::{AudioBuffer, ImageBuffer, ModalityInput, ModalityKind};
pub use ports::{
    DecisionPolicy, GeometryEvaluator, LanguageRenderer, MemoryRecallEngine, MultimodalEncoder,
    ReasoningEngine, RuntimeError, RuntimeResult,
};
pub use search_domain::{SearchInput, SearchPolicy, SearchResult, ScoredState};
pub use search_runtime::search;
