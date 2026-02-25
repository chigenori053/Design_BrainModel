use design_reasoning::{
    DesignFactor, DesignHypothesis, FactorType, HypothesisEngine, IssueType, LanguageEngine,
    LanguageState, LanguageStateV2, MeaningEngine, ModelConfig, OverallState, ProjectionEngine,
    RealizationMode, ReasoningAxis, ScsInputs, SnapshotEngine, StructuredReasoningEngine,
    StructuredReasoningInput, StructuredReasoningTrace, TEMPLATE_SELECTION_EPSILON, TemplateId,
    ValidationError, canonical_srt_hash, compute_dependency_consistency, compute_scs_v1_1,
    is_ambiguous_margin, normalize_realized_explanation_for_output, sanitize_factors,
    validate_llm_output,
};
use semantic_dhm::{
    ConceptId, ConceptUnit, ConceptUnitV2, DEFAULT_L2_CONFIG, DerivedRequirement, L1Id, L2Config,
    MeaningLayerState, RequirementKind, RequirementRole, SemanticUnitL1, SemanticUnitL1V2,
    Snapshotable, compare_snapshots,
};

fn mk_l1(
    id: u128,
    text: &str,
    role: RequirementRole,
    abstraction: f32,
    polarity: i8,
) -> SemanticUnitL1 {
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
    assert_eq!(
        engine.infer_requirement_role("クラウド依存を禁止"),
        RequirementRole::Prohibition
    );
}

#[test]
fn role_constraint_keywords() {
    let engine = MeaningEngine;
    assert_eq!(
        engine.infer_requirement_role("メモリ512MB以下"),
        RequirementRole::Constraint
    );
}

#[test]
fn role_optimization_keywords() {
    let engine = MeaningEngine;
    assert_eq!(
        engine.infer_requirement_role("できるだけ速く"),
        RequirementRole::Optimization
    );
}

#[test]
fn role_goal_default() {
    let engine = MeaningEngine;
    assert_eq!(
        engine.infer_requirement_role("高性能にする"),
        RequirementRole::Goal
    );
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
    let engine = LanguageEngine::new();
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
    let engine = LanguageEngine::new();
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
    let engine = LanguageEngine::new();
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
    let engine = LanguageEngine::new();
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
    let engine = LanguageEngine::new();
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
    let engine = LanguageEngine::new();
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
    let engine = LanguageEngine::new();
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
    let d = engine
        .compare(&s1, &s2)
        .expect("snapshot compare should succeed");
    assert!(d.identical);
}

#[test]
fn language_engine_build_state_with_empty_l1() {
    let engine = LanguageEngine::new();
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

#[test]
fn snapshot_v2_same_input_same_hash() {
    let engine = SnapshotEngine;
    let l1 = vec![mk_l1(1, "goal", RequirementRole::Goal, 0.7, 1)];
    let l2 = semantic_dhm::build_l2_cache(&l1);
    let s1 = engine
        .make_snapshot_v2(&l1, &l2)
        .expect("snapshot v2 should succeed");
    let s2 = engine
        .make_snapshot_v2(&l1, &l2)
        .expect("snapshot v2 should succeed");
    let diff = engine.compare_snapshots_v2(&s1, &s2);
    assert!(diff.identical);
    assert!(!diff.l1_changed);
    assert!(!diff.l2_changed);
}

#[test]
fn snapshot_v2_l1_only_change_detected() {
    let engine = SnapshotEngine;
    let l1_a = vec![mk_l1(1, "goal", RequirementRole::Goal, 0.7, 1)];
    let l1_b = vec![mk_l1(1, "goal changed", RequirementRole::Goal, 0.7, 1)];
    let l2 = semantic_dhm::build_l2_cache(&l1_a);
    let s1 = engine
        .make_snapshot_v2(&l1_a, &l2)
        .expect("snapshot v2 should succeed");
    let s2 = engine
        .make_snapshot_v2(&l1_b, &l2)
        .expect("snapshot v2 should succeed");
    let diff = engine.compare_snapshots_v2(&s1, &s2);
    assert!(diff.l1_changed);
    assert!(!diff.l2_changed);
}

#[test]
fn snapshot_v2_l2_change_detected() {
    let engine = SnapshotEngine;
    let l1 = vec![mk_l1(1, "goal", RequirementRole::Goal, 0.7, 1)];
    let mut l2_a = semantic_dhm::build_l2_cache(&l1);
    let mut l2_b = l2_a.clone();
    l2_b[0].integrated_vector[0] = 0.123;
    let s1 = engine
        .make_snapshot_v2(&l1, &l2_a)
        .expect("snapshot v2 should succeed");
    let s2 = engine
        .make_snapshot_v2(&l1, &l2_b)
        .expect("snapshot v2 should succeed");
    let diff = engine.compare_snapshots_v2(&s1, &s2);
    assert!(!diff.l1_changed);
    assert!(diff.l2_changed);
    l2_a[0].integrated_vector[0] = 0.123;
}

#[test]
fn snapshot_v2_timestamp_is_ignored() {
    let engine = SnapshotEngine;
    let a = design_reasoning::MeaningLayerSnapshotV2 {
        l1_hash: 10,
        l2_hash: 20,
        timestamp_ms: 1000,
        version: 2,
    };
    let b = design_reasoning::MeaningLayerSnapshotV2 {
        l1_hash: 10,
        l2_hash: 20,
        timestamp_ms: 9999,
        version: 2,
    };
    let diff = engine.compare_snapshots_v2(&a, &b);
    assert!(diff.identical);
    assert!(!diff.version_changed);
}

#[test]
fn snapshot_v2_version_difference_detected() {
    let engine = SnapshotEngine;
    let a = design_reasoning::MeaningLayerSnapshotV2 {
        l1_hash: 10,
        l2_hash: 20,
        timestamp_ms: 1,
        version: 2,
    };
    let b = design_reasoning::MeaningLayerSnapshotV2 {
        l1_hash: 10,
        l2_hash: 20,
        timestamp_ms: 2,
        version: 3,
    };
    let diff = engine.compare_snapshots_v2(&a, &b);
    assert!(!diff.identical);
    assert!(diff.version_changed);
}

#[test]
fn semantic_unit_l1_v2_clamps_ambiguity() {
    let l1 = mk_l1(10, "  FAST API  ", RequirementRole::Goal, 1.5, 1);
    let v2 = SemanticUnitL1V2::try_from(l1).expect("l1 v2");
    assert!((0.0..=1.0).contains(&v2.ambiguity_score));
    assert!(v2.objective.is_some());
}

#[test]
fn concept_unit_v2_clamps_stability() {
    let c = ConceptUnit {
        id: ConceptId(1),
        l1_refs: vec![L1Id(1), L1Id(2)],
        integrated_vector: vec![0.2; semantic_dhm::D_SEM],
        a: 5.0,
        s: vec![0.0; semantic_dhm::D_STRUCT],
        polarity: -1,
        timestamp: 0,
    };
    let v2 = ConceptUnitV2::try_from(c).expect("l2 v2");
    assert!((0.0..=1.0).contains(&v2.stability_score));
}

#[test]
fn template_selection_is_deterministic() {
    let engine = LanguageEngine::new();
    let state = LanguageStateV2 {
        selected_objective: Some("高速化".to_string()),
        requirement_count: 4,
        stability_score: 0.9,
        ambiguity_score: 0.2,
    };
    let h = engine.build_h_state(&state);
    let a = engine.select_template(&h).expect("template a");
    let b = engine.select_template(&h).expect("template b");
    assert_eq!(a, b);
}

#[test]
fn template_selection_fallback_for_ambiguous_margin() {
    let engine = LanguageEngine::new();
    let h = vec![0.0f32; language_dhm::EMBEDDING_DIM];
    let selected = engine.select_template(&h).expect("template");
    assert_eq!(selected, TemplateId::Fallback);
}

#[test]
fn template_margin_epsilon_boundary() {
    assert!(is_ambiguous_margin(TEMPLATE_SELECTION_EPSILON));
    assert!(!is_ambiguous_margin(TEMPLATE_SELECTION_EPSILON * 2.0));
}

#[test]
fn stability_label_boundary_is_consistent() {
    let engine = LanguageEngine::new();
    let s1 = LanguageState {
        selected_objective: Some("obj".to_string()),
        requirement_count: 1,
        stability_score: 0.6000001,
        ambiguity_score: 0.3,
    };
    let s2 = LanguageState {
        selected_objective: Some("obj".to_string()),
        requirement_count: 1,
        stability_score: 0.6,
        ambiguity_score: 0.3,
    };
    let e1 = engine.explain_state(&s1);
    let e2 = engine.explain_state(&s2);
    assert!(e1.summary.contains("構造安定性: 概ね安定"));
    assert!(e2.summary.contains("構造安定性: 概ね安定"));
}

#[test]
fn dependency_consistency_empty_defaults_to_half() {
    let score = compute_dependency_consistency(&[]);
    assert!((score - 0.5).abs() < 1e-9);
}

#[test]
fn dependency_consistency_why_missing_connectivity_zero() {
    let factors = vec![
        DesignFactor {
            id: "A".to_string(),
            factor_type: FactorType::What,
            depends_on: vec![],
        },
        DesignFactor {
            id: "B".to_string(),
            factor_type: FactorType::How,
            depends_on: vec!["A".to_string()],
        },
    ];
    let score = compute_dependency_consistency(&factors);
    let expected = 0.5 * 0.0 + 0.3 * (1.0 - 0.0) + 0.2 * (1.0 - 0.0);
    assert!((score - expected).abs() < 1e-9);
}

#[test]
fn dependency_consistency_cycle_penalty_applies() {
    let factors = vec![
        DesignFactor {
            id: "WHY".to_string(),
            factor_type: FactorType::Why,
            depends_on: vec!["A".to_string()],
        },
        DesignFactor {
            id: "A".to_string(),
            factor_type: FactorType::What,
            depends_on: vec!["B".to_string()],
        },
        DesignFactor {
            id: "B".to_string(),
            factor_type: FactorType::How,
            depends_on: vec!["A".to_string()],
        },
    ];
    let score = compute_dependency_consistency(&factors);
    assert!(score < 1.0);
}

#[test]
fn dependency_consistency_detects_orphan_non_why() {
    let factors = vec![
        DesignFactor {
            id: "WHY".to_string(),
            factor_type: FactorType::Why,
            depends_on: vec!["A".to_string()],
        },
        DesignFactor {
            id: "A".to_string(),
            factor_type: FactorType::What,
            depends_on: vec![],
        },
        DesignFactor {
            id: "ORPHAN".to_string(),
            factor_type: FactorType::Constraint,
            depends_on: vec![],
        },
    ];
    let score = compute_dependency_consistency(&factors);
    let expected_orphan_rate = 1.0 / 3.0;
    let expected = 0.5 * (2.0 / 3.0) + 0.3 * (1.0 - 0.0) + 0.2 * (1.0 - expected_orphan_rate);
    assert!((score - expected).abs() < 1e-9);
}

#[test]
fn dependency_consistency_unmeasurable_graph_falls_back() {
    let factors = vec![
        DesignFactor {
            id: "X".to_string(),
            factor_type: FactorType::Why,
            depends_on: vec![],
        },
        DesignFactor {
            id: "X".to_string(),
            factor_type: FactorType::What,
            depends_on: vec![],
        },
    ];
    let score = compute_dependency_consistency(&factors);
    assert!((score - 0.5).abs() < 1e-9);
}

#[test]
fn srt_build_is_deterministic_and_bounded() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "教育向けに最適化された設計".to_string(),
        selected_objective: Some("教育向け最適化".to_string()),
        requirement_count: 0,
        stability_score: 0.31,
        ambiguity_score: 0.79,
        evidence_spans: vec![
            "教育向けに最適化された設計".to_string(),
            "大規模対応".to_string(),
            "  大規模対応 ".to_string(),
        ],
    };
    let s1 = engine.build_srt(&input);
    let s2 = engine.build_srt(&input);
    assert_eq!(s1, s2);
    assert_eq!(s1.evaluation_version, "v1.0");
    assert!(s1.strengths.len() <= 3);
    assert!(s1.issues.len() <= 5);
    assert!(s1.issues.iter().all(|i| (0.0..=1.0).contains(&i.severity)));
}

#[test]
fn srt_realization_cache_key_is_stable() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "大規模対応".to_string(),
        selected_objective: None,
        requirement_count: 0,
        stability_score: 0.4,
        ambiguity_score: 0.8,
        evidence_spans: vec!["大規模対応".to_string()],
    };
    let a = engine.realize(&input, RealizationMode::LlmControlled);
    let b = engine.realize(&input, RealizationMode::LlmControlled);
    assert_eq!(a.cache_key, b.cache_key);
    assert_eq!(a.output, b.output);
}

#[test]
fn srt_rule_based_realization_is_deterministic() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "処理速度を上げる".to_string(),
        selected_objective: Some("処理速度向上".to_string()),
        requirement_count: 2,
        stability_score: 0.8,
        ambiguity_score: 0.2,
        evidence_spans: vec!["処理速度を上げる".to_string()],
    };
    let a = engine.realize(&input, RealizationMode::RuleBased);
    let b = engine.realize(&input, RealizationMode::RuleBased);
    assert_eq!(a.output, b.output);
}

#[test]
fn srt_severity_formula_applies_to_missing_success_metric() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "短い".to_string(),
        selected_objective: Some("obj".to_string()),
        requirement_count: 0,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["短い".to_string()],
    };
    let srt = engine.build_srt(&input);
    let issue = srt
        .issues
        .iter()
        .find(|i| i.axis == ReasoningAxis::SuccessMetric && i.issue_type == IssueType::Missing)
        .expect("missing success metric issue");
    assert!((issue.severity - 1.0).abs() < 1e-9);
}

#[test]
fn srt_overall_state_thresholds_follow_spec() {
    let engine = StructuredReasoningEngine::default();
    let ready = engine.build_srt(&StructuredReasoningInput {
        source_text: "教育向け学習支援の設計で学校現場の運用に適用する".to_string(),
        selected_objective: Some("教育向け最適化".to_string()),
        requirement_count: 3,
        stability_score: 0.95,
        ambiguity_score: 0.1,
        evidence_spans: vec!["教育向け".to_string()],
    });
    assert_eq!(ready.overall_state, OverallState::Ready);

    let partial = engine.build_srt(&StructuredReasoningInput {
        source_text: "教育向け学習支援の設計で学校現場の運用に適用する計画".to_string(),
        selected_objective: Some("教育向け最適化".to_string()),
        requirement_count: 1,
        stability_score: 0.95,
        ambiguity_score: 0.1,
        evidence_spans: vec!["教育向け".to_string()],
    });
    assert_eq!(partial.overall_state, OverallState::PartialReady);

    let insufficient = engine.build_srt(&StructuredReasoningInput {
        source_text: "短い".to_string(),
        selected_objective: None,
        requirement_count: 0,
        stability_score: 0.2,
        ambiguity_score: 0.9,
        evidence_spans: vec!["短い".to_string()],
    });
    assert_eq!(insufficient.overall_state, OverallState::Insufficient);
}

#[test]
fn srt_ambiguity_score_formula_is_reflected_in_issues() {
    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&StructuredReasoningInput {
        source_text: "大規模対応する予定".to_string(),
        selected_objective: Some("性能改善と運用最適化".to_string()),
        requirement_count: 3,
        stability_score: 0.9,
        ambiguity_score: 0.0,
        evidence_spans: vec!["大規模対応する予定".to_string()],
    });
    // 大規模(0.1) + 対応(0.2) + する予定(0.15) = 0.45
    // スコア閾値により AMBIGUOUS issue が生成されることを確認する。
    let ambiguous_count = srt
        .issues
        .iter()
        .filter(|i| i.issue_type == IssueType::Ambiguous)
        .count();
    assert!(ambiguous_count > 0);
}

#[test]
fn srt_ambiguity_span_contains_quant_condition_reason() {
    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&StructuredReasoningInput {
        source_text: "スケール可能で大規模対応する予定".to_string(),
        selected_objective: Some("性能改善".to_string()),
        requirement_count: 2,
        stability_score: 0.9,
        ambiguity_score: 0.0,
        evidence_spans: vec!["スケール可能で大規模対応する予定".to_string()],
    });
    let has_quant = srt.issues.iter().any(|i| {
        i.issue_type == IssueType::Ambiguous && i.reason.as_deref() == Some("定量条件未指定")
    });
    let has_condition = srt
        .issues
        .iter()
        .any(|i| i.issue_type == IssueType::Ambiguous && i.reason.as_deref() == Some("条件未定義"));
    assert!(has_quant);
    assert!(has_condition);
}

#[test]
fn scs_v1_1_formula_is_applied() {
    let inputs = ScsInputs {
        completeness: 1.0,
        ambiguity_mean: 0.2,
        dependency_consistency: 0.6,
        inconsistency: 0.3,
    };
    let scs = compute_scs_v1_1(inputs);
    let expected = 0.40 * 1.0 + 0.25 * (1.0 - 0.2) + 0.20 * 0.6 + 0.15 * (1.0 - 0.3);
    assert!((scs - expected).abs() < 1e-9);
}

#[test]
fn sanitize_factors_repairs_ids_and_dangling_dependencies() {
    let factors = vec![
        DesignFactor {
            id: "".to_string(),
            factor_type: FactorType::Why,
            depends_on: vec!["MISSING".to_string()],
        },
        DesignFactor {
            id: "A".to_string(),
            factor_type: FactorType::What,
            depends_on: vec![],
        },
        DesignFactor {
            id: "A".to_string(),
            factor_type: FactorType::How,
            depends_on: vec!["A".to_string(), "MISSING2".to_string()],
        },
    ];
    let (sanitized, stats) = sanitize_factors(&factors);
    assert_eq!(sanitized.len(), 3);
    assert!(stats.empty_id_fixes >= 1);
    assert!(stats.duplicate_id_fixes >= 1);
    assert!(stats.unknown_dependency_drops >= 1);
}

#[test]
fn llm_cache_key_changes_when_temperature_changes() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "明確な要件と制約を定義する".to_string(),
        selected_objective: Some("要件定義".to_string()),
        requirement_count: 3,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["要件定義".to_string()],
    };
    let mut cfg_a = ModelConfig::default();
    cfg_a.temperature = 0.1;
    let mut cfg_b = cfg_a.clone();
    cfg_b.temperature = 0.2;

    let a = engine.realize_with_model_config(&input, RealizationMode::LlmControlled, &cfg_a);
    let b = engine.realize_with_model_config(&input, RealizationMode::LlmControlled, &cfg_b);
    assert_ne!(a.llm_cache_key, b.llm_cache_key);
}

#[test]
fn llm_cache_key_changes_when_model_name_changes() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "明確な要件と制約を定義する".to_string(),
        selected_objective: Some("要件定義".to_string()),
        requirement_count: 3,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["要件定義".to_string()],
    };
    let mut cfg_a = ModelConfig::default();
    cfg_a.model_name = "model-a".to_string();
    let mut cfg_b = cfg_a.clone();
    cfg_b.model_name = "model-b".to_string();

    let a = engine.realize_with_model_config(&input, RealizationMode::LlmControlled, &cfg_a);
    let b = engine.realize_with_model_config(&input, RealizationMode::LlmControlled, &cfg_b);
    assert_ne!(a.llm_cache_key, b.llm_cache_key);
}

#[test]
fn llm_cache_key_changes_when_prompt_version_changes() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "明確な要件と制約を定義する".to_string(),
        selected_objective: Some("要件定義".to_string()),
        requirement_count: 3,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["要件定義".to_string()],
    };
    let mut cfg_a = ModelConfig::default();
    cfg_a.system_prompt_version = "v1".to_string();
    let mut cfg_b = cfg_a.clone();
    cfg_b.system_prompt_version = "v2".to_string();

    let a = engine.realize_with_model_config(&input, RealizationMode::LlmControlled, &cfg_a);
    let b = engine.realize_with_model_config(&input, RealizationMode::LlmControlled, &cfg_b);
    assert_ne!(a.llm_cache_key, b.llm_cache_key);
}

#[test]
fn rounded_hash_treats_small_severity_delta_as_equal() {
    let mut srt_a = StructuredReasoningTrace {
        evaluation_version: "v1.0".to_string(),
        input_digest: "abc".to_string(),
        overall_state: OverallState::PartialReady,
        strengths: vec![],
        issues: vec![design_reasoning::SrtIssue {
            issue_type: IssueType::Weak,
            axis: ReasoningAxis::Constraint,
            span: None,
            reason: Some("x".to_string()),
            severity: 0.6000001,
        }],
        consistency_warnings: vec![],
        next_priority_axis: ReasoningAxis::Constraint,
    };
    let mut srt_b = srt_a.clone();
    srt_b.issues[0].severity = 0.6000000;
    srt_a.strengths = vec![design_reasoning::SrtStrength {
        axis: ReasoningAxis::TargetUser,
        evidence_span: "e".to_string(),
        confidence: 0.5000001,
    }];
    srt_b.strengths = vec![design_reasoning::SrtStrength {
        axis: ReasoningAxis::TargetUser,
        evidence_span: "e".to_string(),
        confidence: 0.5000000,
    }];

    assert_eq!(canonical_srt_hash(&srt_a), canonical_srt_hash(&srt_b));
}

#[test]
fn validator_rejects_too_many_sentences() {
    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&StructuredReasoningInput {
        source_text: "要件を明確にする".to_string(),
        selected_objective: Some("要件定義".to_string()),
        requirement_count: 2,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["要件定義".to_string()],
    });
    let text = "a。b。c。d。e。f。";
    let err = validate_llm_output(text, &srt).expect_err("must reject sentence overflow");
    assert_eq!(err, ValidationError::TooManySentences);
}

#[test]
fn validator_rejects_forbidden_numbers() {
    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&StructuredReasoningInput {
        source_text: "要件を明確にする".to_string(),
        selected_objective: Some("要件定義".to_string()),
        requirement_count: 2,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["要件定義".to_string()],
    });
    let err = validate_llm_output("three issues exist.", &srt).expect_err("must reject numbers");
    assert_eq!(err, ValidationError::ContainsForbiddenNumber);
}

#[test]
fn validator_rejects_axis_out_of_scope() {
    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&StructuredReasoningInput {
        source_text: "十分に長い説明文で対象ユーザーを明示する。".to_string(),
        selected_objective: Some("定義".to_string()),
        requirement_count: 2,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["説明".to_string()],
    });
    let text = "summary:\nSUCCESS_METRIC を定義。\nnext_action:\n対応。";
    let err = validate_llm_output(text, &srt).expect_err("must reject external axis");
    assert!(matches!(err, ValidationError::AxisOutOfScope(_)));
}

#[test]
fn validator_rejects_unwanted_proposal_outside_next_action() {
    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&StructuredReasoningInput {
        source_text: "説明文で課題を明確化する".to_string(),
        selected_objective: Some("定義".to_string()),
        requirement_count: 2,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["説明".to_string()],
    });
    let text = "summary:\nwe must adopt this.\nnext_action:\n対応。";
    let err = validate_llm_output(text, &srt).expect_err("must reject proposal outside next_action");
    assert_eq!(err, ValidationError::UnwantedProposalOutsideNextAction);
}

#[test]
fn fallback_reason_is_set_when_llm_output_validation_fails() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "must 大規模".to_string(),
        selected_objective: Some("設計最適化".to_string()),
        requirement_count: 2,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["must 大規模".to_string()],
    };
    let result = engine.realize(&input, RealizationMode::LlmControlled);
    assert_eq!(result.mode, RealizationMode::RuleBased);
    assert_eq!(
        result.fallback_reason,
        Some("UnwantedProposalOutsideNextAction".to_string())
    );
}

#[test]
fn load_100_runs_is_stable_and_cache_hit_ratio_is_high() {
    let engine = StructuredReasoningEngine::default();
    engine.reset_llm_call_count();
    let input = StructuredReasoningInput {
        source_text: "十分に長い説明文で対象ユーザーと価値と制約を明示する。".to_string(),
        selected_objective: Some("価値定義".to_string()),
        requirement_count: 3,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["価値定義".to_string()],
    };

    let mut hit_count = 0usize;
    for _ in 0..100 {
        let result = engine.realize(&input, RealizationMode::LlmControlled);
        if result.cache_hit {
            hit_count += 1;
        }
    }
    let hit_ratio = hit_count as f64 / 100.0;
    assert!(hit_ratio > 0.95);
    assert_eq!(engine.llm_call_count(), 1);
    assert_eq!(engine.cache_len(), 1);
}

#[test]
fn summary_normalizer_clamps_to_100_chars() {
    let long_summary = "設計は概ね整理されています。以下の点を明確にすると安定します。さらに補足として詳細背景を長く説明します。これは冗長な部分です。";
    let normalized = normalize_realized_explanation_for_output(design_reasoning::RealizedExplanation {
        summary: long_summary.to_string(),
        key_issues: vec!["課題があります。".to_string()],
        next_action: "次の対応を整理します。".to_string(),
    });
    assert!(normalized.summary.chars().count() <= 100);
}

#[test]
fn summary_normalizer_clamps_to_two_sentences() {
    let normalized = normalize_realized_explanation_for_output(design_reasoning::RealizedExplanation {
        summary: "一文目です。二文目です。三文目です。".to_string(),
        key_issues: vec!["課題があります。".to_string()],
        next_action: "次の対応を整理します。".to_string(),
    });
    let sentence_count = normalized
        .summary
        .split(['。', '.', '!', '?'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .count();
    assert!(sentence_count <= 2);
}

#[test]
fn llm_and_rule_based_summary_style_is_aligned() {
    let engine = StructuredReasoningEngine::default();
    let input = StructuredReasoningInput {
        source_text: "十分に長い説明文で対象ユーザーと価値と制約を明示する。".to_string(),
        selected_objective: Some("価値定義".to_string()),
        requirement_count: 3,
        stability_score: 0.9,
        ambiguity_score: 0.1,
        evidence_spans: vec!["価値定義".to_string()],
    };
    let a = engine.realize(&input, RealizationMode::LlmControlled);
    let b = engine.realize(&input, RealizationMode::RuleBased);
    assert_eq!(a.output.summary, b.output.summary);
}
