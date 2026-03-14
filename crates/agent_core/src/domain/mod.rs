pub mod command;
pub mod event;
pub mod hash;
pub mod history;
pub mod hypothesis;
pub mod interaction;
pub mod metrics;
pub mod state;
pub mod target;
pub mod transaction;

pub use command::{AgentInput, AgentOutput, AgentRequest, DomainError};
pub use event::{AgentEvent, TelemetryEvent};
pub use history::{SessionHistory, SessionSnapshot};
pub use hypothesis::{Hypothesis, Score};
pub use interaction::{
    ActionHistory, ArchitectureAction, ArchitectureState, CodeState, Edge, EvaluationMetrics,
    GeometryState, InteractionLayer, InteractionResult, Node, NodeId, NodeType, Position,
    Property, ReasoningEvent, ReasoningTrace, Specification, UiEvent, WorldState,
};
pub use metrics::{
    chm_density, need_from_objective, p_inferred, profile_modulation, stability_index,
};
pub use state::{
    AnalyzeError, AppState, DeltaVector, DesignScoreVector, EvalError, EvaluationResult,
    NodeIdState, ParetoResult, PromotionError, PromotionReport, RuntimeState, StateVector,
    SuggestError, UnifiedDesignState,
};
pub use target::{build_target_field, build_target_field_with_diversity};
pub use transaction::{ActiveTransaction, ProposedDiff, TransactionEngine, TxError, TxStatus};
