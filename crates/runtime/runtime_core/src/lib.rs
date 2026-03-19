pub mod agent;
pub mod context;
pub mod event;
pub mod explanation;
pub mod intent_refiner;
pub mod modality;
pub mod ports;
pub mod stable_v03;

pub use agent::{AgentInput, AgentOutput, RuntimeAgent};
pub use ai_context::AIContext;
pub use context::{Phase9RuntimeContext, RequestId, RuntimeStage, SearchMetrics, SearchSummary};
pub use event::{RuntimeEvent, RuntimeEventBus};
pub use explanation::{
    DecisionExplanation, DefaultExplanationBuilder, Explanation, ExplanationBuilder,
    SlotExplanation, source_to_message,
};
pub use intent_refiner::{
    ChatContext, Clarification, CoreSlot, DefaultIntentRefiner, InferenceEngine, IntentError,
    IntentExecution, IntentRefiner, IntentTrace, OptionalSlot, QualitySlot, SlotMap, SlotSource,
    SlotValue, StructuredIntent, SystemSlot,
};
pub use modality::{AudioBuffer, ImageBuffer, ModalityInput, ModalityKind};
pub use ports::{
    DecisionPolicy, GeometryEvaluator, LanguageRenderer, MemoryRecallEngine, MultimodalEncoder,
    ReasoningEngine, RuntimeError, RuntimeResult,
};
pub use stable_v03::CoreRuntime;
pub use stable_v03::RuntimeExecutionResult;
