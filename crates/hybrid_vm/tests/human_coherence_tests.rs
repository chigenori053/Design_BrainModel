use hybrid_vm::semantic::coherence::{compute_human_coherence, human_coherence_from_components};
use hybrid_vm::{
    ConceptId, ConceptUnitV2, DerivedRequirement, L1Id, RequirementKind, SemanticObjectiveCase,
    rank_frontier_by_human_coherence,
};
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
fn worst_factor_dominates() {
    let (_, score_balanced) = human_coherence_from_components(0.9, 0.9, 0.9);
    let (_, score_lowest) = human_coherence_from_components(0.9, 0.9, 0.2);
    assert!(score_lowest < score_balanced);
    assert!(score_lowest <= 0.2 + 1e-9);
}

#[test]
fn penalty_applies_when_spread_large() {
    let (_, score_balanced) = human_coherence_from_components(0.8, 0.8, 0.8);
    let (_, score_spread) = human_coherence_from_components(0.8, 0.8, 0.2);
    assert!(score_spread < score_balanced);
}

#[test]
fn nonlinear_effect_visible() {
    let (raw_a, score_a) = human_coherence_from_components(0.6, 0.6, 0.6);
    let (raw_b, score_b) = human_coherence_from_components(0.7, 0.7, 0.7);
    let (raw_c, score_c) = human_coherence_from_components(0.8, 0.8, 0.8);
    assert!(score_a < raw_a);
    assert!(score_b < raw_b);
    assert!(score_c < raw_c);
    let delta_score_1 = score_b - score_a;
    let delta_score_2 = score_c - score_b;
    assert!(delta_score_1 > 0.0);
    assert!(delta_score_2 > 0.0);
}

#[test]
fn deterministic_100_runs() {
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
    let baseline = rank_frontier_by_human_coherence(frontier.clone());
    for _ in 0..100 {
        let current = rank_frontier_by_human_coherence(frontier.clone());
        assert_eq!(baseline.len(), current.len());
        for (a, b) in baseline.iter().zip(current.iter()) {
            assert_eq!(a.objective.case_id, b.objective.case_id);
            assert!((a.human_coherence.score - b.human_coherence.score).abs() <= EPS);
        }
    }
}

#[test]
fn hc_is_applied_on_second_layer() {
    let high_hc = SemanticObjectiveCase {
        case_id: "B".to_string(),
        pareto_rank: 2,
        total_score: 0.5,
        l2: mk_l2(10, 1.0),
    };
    let low_hc = SemanticObjectiveCase {
        case_id: "A".to_string(),
        pareto_rank: 2,
        total_score: 0.5,
        l2: mk_l2(11, -1.0),
    };
    let ranked = rank_frontier_by_human_coherence(vec![high_hc.clone(), low_hc.clone()]);
    let high_score = compute_human_coherence(&high_hc.l2).score;
    let low_score = compute_human_coherence(&low_hc.l2).score;
    if high_score > low_score {
        assert_eq!(ranked[0].objective.case_id, "B");
        assert_eq!(ranked[1].objective.case_id, "A");
    } else if low_score > high_score {
        assert_eq!(ranked[0].objective.case_id, "A");
        assert_eq!(ranked[1].objective.case_id, "B");
    } else {
        // HC tie falls back to case_id.
        assert_eq!(ranked[0].objective.case_id, "A");
        assert_eq!(ranked[1].objective.case_id, "B");
    }
}

#[test]
fn hc_is_ignored_on_third_layer_and_below() {
    let high_hc = SemanticObjectiveCase {
        case_id: "B".to_string(),
        pareto_rank: 3,
        total_score: 0.5,
        l2: mk_l2(12, 1.0),
    };
    let low_hc = SemanticObjectiveCase {
        case_id: "A".to_string(),
        pareto_rank: 3,
        total_score: 0.5,
        l2: mk_l2(13, -1.0),
    };
    let ranked = rank_frontier_by_human_coherence(vec![high_hc, low_hc]);
    assert_eq!(ranked[0].objective.case_id, "A");
    assert_eq!(ranked[1].objective.case_id, "B");
}

#[test]
fn compression_reduces_extremes() {
    let (_, score_low) = human_coherence_from_components(0.05, 0.05, 0.05);
    let (_, score_high) = human_coherence_from_components(0.95, 0.95, 0.95);
    assert!(score_low > 0.0);
    assert!(score_high < 1.0);
    assert!(score_low < 0.05);
    assert!(score_high < 0.95);
}

#[test]
fn compression_is_monotonic() {
    let (_, a) = human_coherence_from_components(0.3, 0.3, 0.3);
    let (_, b) = human_coherence_from_components(0.5, 0.5, 0.5);
    let (_, c) = human_coherence_from_components(0.7, 0.7, 0.7);
    assert!(a < b);
    assert!(b < c);
}
