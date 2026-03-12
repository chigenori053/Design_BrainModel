use design_search_engine::{
    ArchitectureCognitionSearchIntegration, BeamSearchController, SearchConfig,
};
use world_model_core::WorldState;

#[test]
fn test8_knowledge_constrained_search() {
    let controller = BeamSearchController::default();
    let base = SearchConfig {
        max_depth: 6,
        max_candidates: 16,
        beam_width: 8,
        diversity_threshold: 0.85,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let planner = ArchitectureCognitionSearchIntegration::default();
    let constrained = planner.knowledge_constrained_plan("scalable rest api cache", base);
    let baseline_trace = controller.search_trace(WorldState::new(1, vec![2.0, 1.0]), None, &base);
    let knowledge_trace = controller.search_trace(
        WorldState::new(1, vec![2.0, 1.0]),
        None,
        &constrained.config,
    );
    let baseline_best = baseline_trace
        .final_beam
        .iter()
        .map(|state| state.score)
        .fold(0.0_f64, f64::max);
    let knowledge_best = knowledge_trace
        .final_beam
        .iter()
        .map(|state| state.score)
        .fold(0.0_f64, f64::max);
    let step_reduction = if baseline_trace.explored_state_count == 0 {
        0.0
    } else {
        1.0 - knowledge_trace.explored_state_count as f64
            / baseline_trace.explored_state_count as f64
    };

    println!(
        "Test8 Knowledge Constrained Search\nbaseline_steps: {}\nknowledge_steps: {}\nstep_reduction: {:.2}\nbest_score_baseline: {:.3}\nbest_score_knowledge: {:.3}",
        baseline_trace.explored_state_count,
        knowledge_trace.explored_state_count,
        step_reduction,
        baseline_best,
        knowledge_best
    );

    assert!(constrained.constraint_count > 0);
    assert!(step_reduction >= 0.15, "step_reduction={step_reduction}");
    assert!(
        knowledge_best >= baseline_best,
        "knowledge_best={knowledge_best}, baseline_best={baseline_best}"
    );
}
