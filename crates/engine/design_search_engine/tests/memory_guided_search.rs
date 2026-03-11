use architecture_knowledge::{ArchitectureKnowledge, ArchitecturePattern, ArchitecturePatternKind};
use architecture_memory::ArchitectureMemory;
use design_search_engine::{BeamSearchController, SearchConfig, SearchContext};
use world_model_core::WorldState;

#[test]
fn test16_memory_guided_search() {
    let controller = BeamSearchController::default();
    let config = SearchConfig {
        max_depth: 8,
        max_candidates: 16,
        beam_width: 8,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let baseline = controller.search_trace(WorldState::new(1, vec![2.0, 1.0]), None, &config);
    let context = SearchContext {
        knowledge: ArchitectureKnowledge {
            patterns: vec![ArchitecturePattern {
                kind: ArchitecturePatternKind::Layered,
                name: "Layered architecture".into(),
                evidence: vec!["seed".into()],
            }],
            anti_patterns: Vec::new(),
        },
        memory: ArchitectureMemory::with_seed_patterns(vec![ArchitecturePattern {
            kind: ArchitecturePatternKind::Microservice,
            name: "Microservice architecture".into(),
            evidence: vec!["seed".into()],
        }]),
    };
    let guided = controller.search_trace_with_context(
        WorldState::new(1, vec![2.0, 1.0]),
        None,
        &config,
        &context,
    );
    let step_reduction =
        1.0 - guided.explored_state_count as f64 / baseline.explored_state_count.max(1) as f64;

    assert!(step_reduction >= 0.3, "step_reduction={step_reduction}");
}
