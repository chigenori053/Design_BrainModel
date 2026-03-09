use design_domain::{Architecture, DesignUnit, Layer};
use design_search_engine::BeamSearchController;
use memory_space_phase14::{DesignExperience, MemorySpace};
use search_verification::{
    bad_pattern_frequency, run_all_scenarios, scenario_states, seed_good_experience,
    update_policy_from_memory, verification_config,
};

#[test]
fn experience_poisoning_is_rejected_or_negligible() {
    let controller = BeamSearchController::default();
    for state in scenario_states() {
        seed_good_experience(&controller, &state, 0.9);
    }
    {
        let mut memory = controller.memory.lock().expect("memory lock");
        let mut architecture = Architecture::seeded();
        architecture.classes[0].structures[0].design_units.clear();
        architecture.add_design_unit(DesignUnit::with_layer(1, "DbOnly", Layer::Database));
        memory.store_experience(DesignExperience {
            semantic_context: Default::default(),
            inferred_semantics: Default::default(),
            architecture: architecture.clone(),
            architecture_hash: 404,
            causal_graph: architecture.causal_graph(),
            dependency_edges: architecture.graph.edges.clone(),
            layer_sequence: vec![Layer::Database],
            score: 0.1,
            search_depth: 1,
        });
    }
    update_policy_from_memory(&controller);

    let states = run_all_scenarios(&controller, &verification_config(0.2));

    assert!(bad_pattern_frequency(&states, &[Layer::Database]) <= 0.1);
}
