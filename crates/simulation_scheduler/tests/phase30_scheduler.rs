use architecture_domain::ArchitectureState;
use design_domain::{Architecture, Constraint, Dependency, DependencyKind, DesignUnit, DesignUnitId, Layer};
use simulation_scheduler::{
    DefaultCandidateFilter, DefaultSimulationScheduler, HeuristicKnowledgeEvaluator,
    HeuristicLightSimulationEngine, KnowledgeEvaluator, LightSimulationEngine,
    SchedulerTelemetryEvent, SimulationScheduler, SimulationSchedulerConfig,
};

fn architecture_state(
    nodes: usize,
    dependencies: &[(u64, u64)],
    constraints: Vec<Constraint>,
) -> ArchitectureState {
    let mut architecture = Architecture::seeded();
    for id in 1..=nodes as u64 {
        let layer = match id % 4 {
            1 => Layer::Ui,
            2 => Layer::Service,
            3 => Layer::Repository,
            _ => Layer::Database,
        };
        architecture.add_design_unit(DesignUnit::with_layer(id, format!("Node{id}"), layer));
    }
    for (from, to) in dependencies {
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(*from),
            to: DesignUnitId(*to),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((*from, *to));
    }
    ArchitectureState::from_architecture(&architecture, constraints)
}

#[test]
fn test33_01_candidate_filtering() {
    let filter = DefaultCandidateFilter;
    let valid = architecture_state(3, &[(1, 2)], Vec::new());
    let invalid = architecture_state(
        3,
        &[(1, 2), (2, 1)],
        vec![Constraint {
            name: "deps".into(),
            max_design_units: None,
            max_dependencies: Some(1),
        }],
    );

    let filtered = simulation_scheduler::CandidateFilter::filter(&filter, vec![valid.clone(), invalid]);

    assert_eq!(filtered, vec![valid]);
}

#[test]
fn test33_02_knowledge_evaluation() {
    let evaluator = HeuristicKnowledgeEvaluator;
    let layered = architecture_state(4, &[(1, 2), (2, 3), (3, 4)], Vec::new());
    let tangled = architecture_state(4, &[(4, 1), (3, 1), (2, 1)], Vec::new());

    assert!(evaluator.evaluate(&layered).value > evaluator.evaluate(&tangled).value);
}

#[test]
fn test33_03_light_simulation_accuracy() {
    let engine = HeuristicLightSimulationEngine;
    let healthy = architecture_state(4, &[(1, 2), (2, 3)], Vec::new());
    let unhealthy = architecture_state(4, &[(1, 2), (2, 1), (3, 1)], Vec::new());

    assert!(
        engine.simulate(&healthy).feasibility_score > engine.simulate(&unhealthy).feasibility_score
    );
}

#[test]
fn test33_04_simulation_scheduler_ranking() {
    let scheduler = DefaultSimulationScheduler::with_config(SimulationSchedulerConfig {
        max_full_simulations: 2,
        light_simulation_threshold: 0.2,
        knowledge_threshold: 0.2,
    });
    let best = architecture_state(4, &[(1, 2), (2, 3)], Vec::new());
    let weaker = architecture_state(4, &[(4, 1), (3, 1)], Vec::new());
    let batch = scheduler.rank_candidates(vec![weaker.clone(), best.clone()]);

    assert_eq!(batch.scheduled.len(), 2);
    assert!(
        batch.scheduled[0].ranking_score >= batch.scheduled[1].ranking_score,
        "scores: {:?}",
        batch.scheduled
            .iter()
            .map(|candidate| candidate.ranking_score)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test33_05_cache_hit() {
    let scheduler = DefaultSimulationScheduler::with_config(SimulationSchedulerConfig {
        max_full_simulations: 1,
        light_simulation_threshold: 0.2,
        knowledge_threshold: 0.2,
    });
    let candidate = architecture_state(4, &[(1, 2), (2, 3)], Vec::new());

    let first = scheduler.rank_candidates(vec![candidate.clone()]);
    let second = scheduler.rank_candidates(vec![candidate]);

    assert!(!first.scheduled[0].cache_hit);
    assert!(second.scheduled[0].cache_hit);
}

#[test]
fn test33_06_incremental_simulation() {
    let scheduler = DefaultSimulationScheduler::default();
    let base = architecture_state(3, &[(1, 2)], Vec::new());
    let modified = architecture_state(4, &[(1, 2), (2, 3)], Vec::new());

    let (delta_result, event) = scheduler.simulate_incrementally(&base, &modified);

    assert!(delta_result.total() > 0.0);
    assert!(matches!(
        event,
        SchedulerTelemetryEvent::IncrementalSimulationExecuted(_)
    ));
}

#[test]
fn test33_08_determinism() {
    let scheduler = DefaultSimulationScheduler::with_config(SimulationSchedulerConfig {
        max_full_simulations: 2,
        light_simulation_threshold: 0.2,
        knowledge_threshold: 0.2,
    });
    let candidates = vec![
        architecture_state(4, &[(1, 2), (2, 3)], Vec::new()),
        architecture_state(4, &[(1, 2)], Vec::new()),
    ];

    let left = scheduler.rank_candidates(candidates.clone());
    let right = scheduler.rank_candidates(candidates);

    assert_eq!(left.scheduled.len(), right.scheduled.len());
    assert_eq!(
        left.scheduled
            .iter()
            .map(|candidate| candidate.architecture_hash.clone())
            .collect::<Vec<_>>(),
        right
            .scheduled
            .iter()
            .map(|candidate| candidate.architecture_hash.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test33_09_telemetry_completeness() {
    let scheduler = DefaultSimulationScheduler::with_config(SimulationSchedulerConfig {
        max_full_simulations: 2,
        light_simulation_threshold: 0.2,
        knowledge_threshold: 0.2,
    });
    let batch = scheduler.rank_candidates(vec![
        architecture_state(4, &[(1, 2), (2, 3)], Vec::new()),
        architecture_state(4, &[(1, 2)], Vec::new()),
    ]);

    assert!(batch.trace.knowledge_evaluated >= 2);
    assert!(batch.trace.light_simulated >= 2);
    assert!(batch.trace.scheduled_candidates >= 1);
    assert!(
        batch
            .trace
            .telemetry_events
            .iter()
            .any(|event| matches!(event, SchedulerTelemetryEvent::SimulationScheduled(_)))
    );
}

#[test]
fn test33_10_performance_improvement() {
    let scheduler = DefaultSimulationScheduler::with_config(SimulationSchedulerConfig {
        max_full_simulations: 5,
        light_simulation_threshold: 0.2,
        knowledge_threshold: 0.2,
    });
    let candidates = (0..100)
        .map(|index| {
            architecture_state(
                4 + (index % 3),
                &[(1, 2), (2, 3)],
                if index % 2 == 0 {
                    Vec::new()
                } else {
                    vec![Constraint {
                        name: "budget".into(),
                        max_design_units: Some(5),
                        max_dependencies: Some(3),
                    }]
                },
            )
        })
        .collect::<Vec<_>>();

    let batch = scheduler.rank_candidates(candidates);

    assert!(batch.scheduled.len() <= 5);
    assert!(batch.trace.scheduled_candidates * 10 <= 100);
}

#[test]
fn scheduler_schedule_returns_ranked_results() {
    let scheduler = DefaultSimulationScheduler::default();
    let results = scheduler.schedule(vec![architecture_state(4, &[(1, 2), (2, 3)], Vec::new())]);

    assert_eq!(results.len(), 1);
    assert!(results[0].total() > 0.0);
}
