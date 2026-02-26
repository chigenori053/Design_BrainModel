use hybrid_vm::{ConceptId, ConceptUnitV2, DerivedRequirement, L1Id, RequirementKind, SemanticObjectiveCase, rank_frontier_by_semantic};
use semantic_dhm::CausalEdge;

const EPS: f64 = 1e-12;

fn mk_l2(seed: u64, edge_weight: f64) -> ConceptUnitV2 {
    ConceptUnitV2 {
        id: ConceptId(seed),
        derived_requirements: vec![
            DerivedRequirement {
                kind: RequirementKind::Performance,
                strength: 1.0,
            },
            DerivedRequirement {
                kind: RequirementKind::Memory,
                strength: 1.0,
            },
            DerivedRequirement {
                kind: RequirementKind::Security,
                strength: 1.0,
            },
            DerivedRequirement {
                kind: RequirementKind::Reliability,
                strength: 1.0,
            },
            DerivedRequirement {
                kind: RequirementKind::NoCloud,
                strength: 1.0,
            },
        ],
        causal_links: vec![CausalEdge {
            from: L1Id(seed as u128 + 1),
            to: L1Id(seed as u128 + 2),
            weight: edge_weight,
        }],
        stability_score: 1.0,
    }
}

#[test]
fn semantic_ranking_is_deterministic() {
    let frontier = vec![
        SemanticObjectiveCase {
            case_id: "C-002".to_string(),
            pareto_rank: 1,
            total_score: 0.8,
            l2: mk_l2(2, -1.0),
        },
        SemanticObjectiveCase {
            case_id: "C-001".to_string(),
            pareto_rank: 1,
            total_score: 0.8,
            l2: mk_l2(1, 1.0),
        },
    ];

    let a = rank_frontier_by_semantic(frontier.clone());
    let b = rank_frontier_by_semantic(frontier);
    assert_eq!(a.len(), b.len());
    for (left, right) in a.iter().zip(b.iter()) {
        assert_eq!(left.objective.case_id, right.objective.case_id);
        assert!((left.coherence.total_score - right.coherence.total_score).abs() <= EPS);
    }
}

#[test]
fn ranking_does_not_change_frontier_size() {
    let frontier = vec![
        SemanticObjectiveCase {
            case_id: "A".to_string(),
            pareto_rank: 1,
            total_score: 0.9,
            l2: mk_l2(1, 1.0),
        },
        SemanticObjectiveCase {
            case_id: "B".to_string(),
            pareto_rank: 1,
            total_score: 0.7,
            l2: mk_l2(2, 1.0),
        },
    ];
    let ranked = rank_frontier_by_semantic(frontier.clone());
    assert_eq!(ranked.len(), frontier.len());
}

#[test]
fn ranking_respects_pareto_rank() {
    let frontier = vec![
        SemanticObjectiveCase {
            case_id: "A".to_string(),
            pareto_rank: 2,
            total_score: 1.0,
            l2: mk_l2(1, 1.0),
        },
        SemanticObjectiveCase {
            case_id: "B".to_string(),
            pareto_rank: 1,
            total_score: 0.1,
            l2: mk_l2(2, -1.0),
        },
    ];
    let ranked = rank_frontier_by_semantic(frontier);
    assert_eq!(ranked[0].objective.case_id, "B");
    assert_eq!(ranked[1].objective.case_id, "A");
}

#[test]
fn higher_sc_ranks_higher_when_equal_objective() {
    let low_sc = SemanticObjectiveCase {
        case_id: "low".to_string(),
        pareto_rank: 1,
        total_score: 0.5,
        l2: mk_l2(1, -1.0),
    };
    let high_sc = SemanticObjectiveCase {
        case_id: "high".to_string(),
        pareto_rank: 1,
        total_score: 0.5,
        l2: mk_l2(2, 1.0),
    };
    let ranked = rank_frontier_by_semantic(vec![low_sc, high_sc]);
    assert_eq!(ranked[0].objective.case_id, "high");
    assert_eq!(ranked[1].objective.case_id, "low");
}
