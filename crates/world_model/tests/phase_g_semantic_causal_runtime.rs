use design_domain::Constraint;
use world_model::semantic_causal_runtime::{
    CausalEdge, CausalGraph, CausalState, EntityState, EnvironmentSync, IdentityPersistence,
    RiskLevel, SemanticCausalEngine, SemanticRuntimeError, SemanticWorldCompression, WorldState,
};

fn sample_world() -> WorldState {
    WorldState::new(
        vec![
            EntityState {
                entity_id: "runtime".into(),
                semantic_role: "executor".into(),
                current_state: "ready".into(),
            },
            EntityState {
                entity_id: "database".into(),
                semantic_role: "executor".into(),
                current_state: "ready".into(),
            },
        ],
        vec![Constraint {
            name: "bounded_environment".into(),
            max_design_units: Some(4),
            max_dependencies: Some(4),
        }],
        CausalState {
            edges: vec![CausalEdge {
                source_state: "ready".into(),
                target_state: "validated".into(),
                causal_weight: 0.95,
            }],
        },
    )
}

#[test]
fn phase_g_same_causal_state_produces_same_future_prediction() {
    let world = sample_world();
    let sync = EnvironmentSync::synchronized(&world, 1);
    let engine = SemanticCausalEngine::new(CausalGraph {
        edges: world.causal_state.edges.clone(),
    });

    let left = engine
        .predict(&world, "ready", "runtime_identity", &sync)
        .expect("prediction");
    let right = engine
        .predict(&world, "ready", "runtime_identity", &sync)
        .expect("prediction");

    assert_eq!(left, right);
    assert_eq!(left.projected_risk, RiskLevel::Low);
    assert_eq!(
        left.projected_world_state.entities[0].current_state,
        "validated"
    );
}

#[test]
fn phase_g_stale_world_state_denies_execution() {
    let world = sample_world();
    let engine = SemanticCausalEngine::new(CausalGraph {
        edges: world.causal_state.edges.clone(),
    });

    let denied = engine
        .predict(
            &world,
            "ready",
            "runtime_identity",
            &EnvironmentSync::stale(0),
        )
        .expect_err("stale world must deny execution");

    assert_eq!(denied, SemanticRuntimeError::WorldStateStale);
}

#[test]
fn phase_g_world_compression_preserves_semantic_causality() {
    let world = sample_world();
    let compression = SemanticWorldCompression::compress(&world);

    assert_eq!(compression.compressed_world_groups.len(), 1);
    assert!(compression.preserves_semantic_causality(&world));
}

#[test]
fn phase_g_identity_persists_across_world_mutation_lineage() {
    let world = sample_world();
    let base_identity = IdentityPersistence::establish("runtime_seed", &world);
    let sync = EnvironmentSync::synchronized(&world, 1);
    let engine = SemanticCausalEngine::new(CausalGraph {
        edges: world.causal_state.edges.clone(),
    });
    let simulation = engine
        .predict(
            &world,
            "ready",
            &base_identity.persistent_identity_hash,
            &sync,
        )
        .expect("prediction");

    let transitioned = base_identity.transition(&simulation.projected_world_state);

    assert!(base_identity.is_continuous_with(&transitioned));
    assert_eq!(
        base_identity.persistent_identity_hash,
        transitioned.persistent_identity_hash
    );
}

#[test]
fn phase_g_future_instability_overflow_halts_prediction() {
    let world = sample_world();
    let sync = EnvironmentSync::synchronized(&world, 1);
    let engine = SemanticCausalEngine::new(CausalGraph {
        edges: vec![
            CausalEdge {
                source_state: "ready".into(),
                target_state: "validated".into(),
                causal_weight: 0.1,
            },
            CausalEdge {
                source_state: "ready".into(),
                target_state: "collapsed".into(),
                causal_weight: 0.1,
            },
        ],
    });

    let error = engine
        .predict(&world, "ready", "runtime_identity", &sync)
        .expect_err("contradictory propagation must halt");

    match error {
        SemanticRuntimeError::FutureInstabilityOverflow { stability } => {
            assert!(stability.requires_halt());
        }
        SemanticRuntimeError::WorldStateStale => panic!("expected instability halt"),
    }
}
