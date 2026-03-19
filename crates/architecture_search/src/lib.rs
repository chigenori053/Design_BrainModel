pub mod beam_search;
pub mod candidate;
pub mod config;
pub mod constraints;
pub mod controller;
pub mod design_space;
pub mod engine;
pub mod evaluator;
pub mod generator;
pub mod grammar;
pub mod grammar_engine;
pub mod intent;
pub mod intent_processor;
pub mod pareto;
pub mod search_space;
pub mod search_state;
pub mod telemetry;
pub mod template;
pub mod template_engine;

pub use beam_search::BeamSearchController;
pub use candidate::ArchitectureCandidate;
pub use config::SearchConfig;
pub use constraints::{BasicConstraintFilter, ConstraintFilter};
pub use controller::SearchController;
pub use design_space::DesignSpaceBuilder;
pub use engine::{ArchitectureSearchEngine, SearchResult};
pub use evaluator::{
    ArchitectureEvaluator, ArchitectureScore, BasicArchitectureEvaluator, score_dominates,
};
pub use generator::{CandidateGenerator, DeterministicCandidateGenerator};
pub use grammar::{ArchitectureGrammar, ArchitectureStyle};
pub use grammar::{ComponentRule, ConstraintRule, InterfaceRule, LayerRule};
pub use grammar_engine::{ArchitectureGrammarEngine, GrammarValidation};
pub use intent::{IntentConstraints, IntentModel};
pub use intent_processor::IntentProcessor;
pub use pareto::{ParetoOptimizer, ParetoSetOptimizer};
pub use search_space::{DependencyRule, DesignIntent, SearchSpace};
pub use search_state::{SearchState, create_initial_state};
pub use telemetry::{SearchOutcome, SearchTelemetry};
pub use template::{
    ArchitectureTemplate, ComponentSlot, TemplateLayer, TemplateSelection, Topology,
};
pub use template_engine::ArchitectureTemplateEngine;
