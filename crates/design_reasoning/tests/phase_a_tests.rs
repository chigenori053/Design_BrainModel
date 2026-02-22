use design_reasoning::{
    DesignHypothesis, HypothesisEngine, LanguageEngine, LanguageState, MeaningEngine,
    ProjectionEngine, SnapshotEngine,
};
use semantic_dhm::{
    compare_snapshots, ConceptId, ConceptUnit, DerivedRequirement, L1Id, L2Config, MeaningLayerState,
    RequirementKind, RequirementRole, SemanticUnitL1, Snapshotable, DEFAULT_L2_CONFIG,
};

fn mk_l1(id: u128, text: &str, role: RequirementRole, abstraction: f32, polarity: i8) -> SemanticUnitL1 {
    SemanticUnitL1 {
        id: L1Id(id),
        role,
        polarity,
        abstraction,
        vector: vec![1.0; semantic_dhm::D_SEM],
        source_text: text.to_string(),
    }
}

fn mk_l2(id: u64, refs: Vec<L1Id>) -> ConceptUnit {
    ConceptUnit {
        id: ConceptId(id),
        l1_refs: refs,
        integrated_vector: vec![1.0; semantic_dhm::D_SEM],
        a: 0.5,
        s: vec![0.5; semantic_dhm::D_STRUCT],
        polarity: 1,
        timestamp: 0,
    }
}

#[test]
fn empty_input_fragment_is_single_empty_trimmed() {
    let engine = MeaningEngine;
    let f = engine.extract_l1_fragments("   ");
    assert_eq!(f.len(), 1);
}

#[test]
fn split_japanese_punctuation() {
    let engine = MeaningEngine;
    let f = engine.extract_l1_fragments("高速化したい。クラウド依存は避ける、メモリは512MB以下");
    assert!(f.len() >= 2);
}

#[test]
fn split_english_conjunctions() {
    let engine = MeaningEngine;
    let f = engine.extract_l1_fragments("fast api and low memory but no cloud");
    assert!(f.len() >= 2);
}

#[test]
fn role_prohibition_keywords() {
    let engine = MeaningEngine;
    assert_eq!(engine.infer_requirement_role("クラウド依存を禁止"), RequirementRole::Prohibition);
}

#[test]
fn role_constraint_keywords() {
    let engine = MeaningEngine;
    assert_eq!(engine.infer_requirement_role("メモリ512MB以下"), RequirementRole::Constraint);
}

#[test]
fn role_optimization_keywords() {
    let engine = MeaningEngine;
    assert_eq!(engine.infer_requirement_role("できるだけ速く"), RequirementRole::Optimization);
}

#[test]
fn role_goal_default() {
    let engine = MeaningEngine;
    assert_eq!(engine.infer_requirement_role("高性能にする"), RequirementRole::Goal);
}

#[test]
fn polarity_by_role() {
    let engine = MeaningEngine;
    assert_eq!(engine.infer_polarity(RequirementRole::Goal), 1);
    assert_eq!(engine.infer_polarity(RequirementRole::Constraint), -1);
}

#[test]
fn abstraction_range_is_clamped() {
    let engine = MeaningEngine;
    let a = engine.infer_abstraction("メモリ512MB以下");
    assert!((0.0..=1.0).contains(&a));
}

#[test]
fn abstraction_prefers_qualitative_sentence() {
    let engine = MeaningEngine;
    let a1 = engine.infer_abstraction("メモリ512MB以下");
    let a2 = engine.infer_abstraction("できるだけ高速");
    assert!(a2 >= a1);
}

#[test]
fn language_state_stability_label_stable() {
    let engine = LanguageEngine;
    let e = engine.explain_state(&LanguageState {
        selected_objective: Some("高速化".to_string()),
        requirement_count: 3,
        stability_score: 0.9,
        ambiguity_score: 0.2,
    });
    assert!(e.summary.contains("構造安定性: 安定"));
}

#[test]
fn language_state_stability_label_mid() {
    let engine = LanguageEngine;
    let e = engine.explain_state(&LanguageState {
        selected_objective: Some("高速化".to_string()),
        requirement_count: 3,
        stability_score: 0.7,
        ambiguity_score: 0.2,
    });
    assert!(e.summary.contains("構造安定性: 概ね安定"));
}

#[test]
fn language_state_stability_label_unstable() {
    let engine = LanguageEngine;
    let e = engine.explain_state(&LanguageState {
        selected_objective: Some("高速化".to_string()),
        requirement_count: 3,
        stability_score: 0.2,
        ambiguity_score: 0.2,
    });
    assert!(e.summary.contains("構造安定性: 不安定"));
}

#[test]
fn language_state_ambiguity_label_high() {
    let engine = LanguageEngine;
    let e = engine.explain_state(&LanguageState {
        selected_objective: None,
        requirement_count: 0,
        stability_score: 0.9,
        ambiguity_score: 0.9,
    });
    assert!(e.summary.contains("曖昧性: 不明確"));
}

#[test]
fn language_state_ambiguity_label_mid() {
    let engine = LanguageEngine;
    let e = engine.explain_state(&LanguageState {
        selected_objective: None,
        requirement_count: 0,
        stability_score: 0.9,
        ambiguity_score: 0.5,
    });
    assert!(e.summary.contains("曖昧性: 部分的に不明確"));
}

#[test]
fn language_state_ambiguity_label_low() {
    let engine = LanguageEngine;
    let e = engine.explain_state(&LanguageState {
        selected_objective: None,
        requirement_count: 0,
        stability_score: 0.9,
        ambiguity_score: 0.1,
    });
    assert!(e.summary.contains("曖昧性: 明確"));
}

#[test]
fn language_output_is_deterministic() {
    let engine = LanguageEngine;
    let state = LanguageState {
        selected_objective: Some("高速化".to_string()),
        requirement_count: 2,
        stability_score: 0.66,
        ambiguity_score: 0.41,
    };
    let a = engine.explain_state(&state);
    let b = engine.explain_state(&state);
    assert_eq!(a, b);
}

#[test]
fn hypothesis_engine_constraint_violation() {
    let engine = HypothesisEngine;
    let projection = semantic_dhm::DesignProjection {
        source_l2_ids: vec![ConceptId(1)],
        derived: vec![DerivedRequirement {
            kind: RequirementKind::Memory,
            strength: 0.8,
        }],
    };
    let h = engine
        .evaluate_hypothesis(&projection)
        .expect("hypothesis should evaluate");
    assert!(h.constraint_violation);
}

#[test]
fn hypothesis_engine_no_violation_negative_constraint() {
    let engine = HypothesisEngine;
    let projection = semantic_dhm::DesignProjection {
        source_l2_ids: vec![ConceptId(1)],
        derived: vec![DerivedRequirement {
            kind: RequirementKind::Memory,
            strength: -0.8,
        }],
    };
    let h = engine
        .evaluate_hypothesis(&projection)
        .expect("hypothesis should evaluate");
    assert!(!h.constraint_violation);
}

#[test]
fn projection_engine_is_deterministic() {
    let l1 = vec![
        mk_l1(1, "security", RequirementRole::Goal, 0.8, 1),
        mk_l1(2, "no cloud", RequirementRole::Prohibition, 0.9, -1),
    ];
    let l2 = vec![mk_l2(10, vec![L1Id(1), L1Id(2)])];
    let engine = ProjectionEngine;
    let p1 = engine.project_phase_a(&l2, &l1);
    let p2 = engine.project_phase_a(&l2, &l1);
    assert_eq!(p1, p2);
}

#[test]
fn snapshot_compare_zero_diff_after_rebuild_like_cycle() {
    let l1 = vec![mk_l1(1, "goal", RequirementRole::Goal, 0.7, 1)];
    let l2 = semantic_dhm::build_l2_cache_with_config(&l1, DEFAULT_L2_CONFIG);
    let state = MeaningLayerState {
        algorithm_version: L2Config {
            similarity_threshold: DEFAULT_L2_CONFIG.similarity_threshold,
            algorithm_version: DEFAULT_L2_CONFIG.algorithm_version,
        }
        .algorithm_version,
        l1_units: l1.clone(),
        l2_units: l2.clone(),
    };
    let s1 = state.snapshot();
    let s2 = state.snapshot();
    let diff = compare_snapshots(&s1, &s2).expect("snapshot compare should succeed");
    assert!(diff.identical);
}

#[test]
fn snapshot_engine_compare_is_stable() {
    let engine = SnapshotEngine;
    let l1 = vec![mk_l1(1, "goal", RequirementRole::Goal, 0.7, 1)];
    let l2 = semantic_dhm::build_l2_cache(&l1);
    let s1 = engine
        .snapshot(DEFAULT_L2_CONFIG.algorithm_version, l1.clone(), l2.clone())
        .expect("snapshot should succeed");
    let s2 = engine
        .snapshot(DEFAULT_L2_CONFIG.algorithm_version, l1, l2)
        .expect("snapshot should succeed");
    let d = engine.compare(&s1, &s2).expect("snapshot compare should succeed");
    assert!(d.identical);
}

#[test]
fn language_engine_build_state_with_empty_l1() {
    let engine = LanguageEngine;
    let projection = semantic_dhm::DesignProjection {
        source_l2_ids: vec![],
        derived: vec![],
    };
    let hypothesis = DesignHypothesis {
        requirements: vec![],
        total_score: 0.0,
        normalized_score: 0.0,
        constraint_violation: false,
    };
    let state = engine.build_state(&projection, &[], &hypothesis);
    assert_eq!(state.selected_objective, None);
    assert_eq!(state.requirement_count, 0);
}
