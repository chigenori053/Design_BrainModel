use agent_core::domain::{AppState, DeltaVector, ProposedDiff, UnifiedDesignState};

pub fn benchmark_suggest_quality() {
    let cases = generate_diverse_cases(42, 120);
    assert!(cases.len() >= 100, "benchmark requires at least 100 cases");

    const ALPHA: f64 = 3.0;
    const BETA: f64 = 3.0;
    const GAMMA: f64 = 1.0;
    const DELTA_MOD: f64 = 0.8;
    const ETA: f64 = 0.02;

    let mut total_suggestions: usize = 0;
    let mut total_accepted_new: usize = 0;
    let mut total_accepted_old_and: usize = 0;

    let mut sum_consistency = 0.0_f64;
    let mut sum_structural = 0.0_f64;
    let mut sum_dependency = 0.0_f64;
    let mut accepted_scores = Vec::new();
    let mut case_initial_consistency = Vec::new();
    let mut post_consistency = Vec::new();
    let mut accepted_delta_consistency = Vec::new();
    let mut accepted_delta_prop_raw = Vec::new();
    let mut delta_v_consistency = Vec::new();
    let mut delta_v_prop_quality = Vec::new();
    let mut delta_v_cycle_quality = Vec::new();
    let mut delta_v_modularity = Vec::new();
    let mut delta_v_norms = Vec::new();
    let mut case_initial_structural = Vec::new();
    let mut case_initial_dependency = Vec::new();
    let mut improved_cycle_count = 0_usize;
    let mut sum_delta_prop_raw = 0.0_f64;
    let mut sum_delta_cyc_raw = 0.0_f64;
    let mut split_candidates_total = 0_usize;
    let mut split_accepted_total = 0_usize;
    let mut rewire_candidates_total = 0_usize;
    let mut two_step_candidates_total = 0_usize;
    let mut two_step_accepted_total = 0_usize;
    let mut remove_node_accepted_total = 0_usize;
    let mut rewire_edge_accepted_total = 0_usize;
    let mut other_accepted_total = 0_usize;
    let mut two_step_first_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut two_step_second_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut two_step_pair_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut two_step_delta_cons = Vec::<f64>::new();
    let mut two_step_delta_prop = Vec::<f64>::new();
    let mut two_step_scores = Vec::<f64>::new();
    let mut two_step_delta_v_norms = Vec::<f64>::new();
    let mut single_step_delta_cons = Vec::<f64>::new();
    let mut single_step_delta_prop = Vec::<f64>::new();
    let mut single_step_scores = Vec::<f64>::new();
    let mut single_step_delta_v_norms = Vec::<f64>::new();
    let mut mismatch_count = 0_usize;

    for case in &cases {
        let case = &case.state;
        let base_eval = &case.evaluation;
        case_initial_consistency.push(base_eval.consistency as f64 / 100.0);
        case_initial_structural.push(base_eval.structural_integrity as f64 / 100.0);
        case_initial_dependency.push(base_eval.dependency_soundness as f64 / 100.0);

        let pareto = case.analyze_pareto().expect("pareto should succeed");
        let suggestions_domain = case
            .suggest_diffs_from_analysis(&pareto)
            .expect("suggest should succeed");

        let candidates = build_candidate_diffs(case);
        total_suggestions += candidates.len();
        split_candidates_total += candidates
            .iter()
            .filter(|diff| matches!(diff, ProposedDiff::SplitHighOutDegreeNode { .. }))
            .count();
        rewire_candidates_total += candidates
            .iter()
            .filter(|diff| {
                matches!(
                    diff,
                    ProposedDiff::SetDependencies { .. } | ProposedDiff::RewireHighImpactEdge { .. }
                )
            })
            .count();
        two_step_candidates_total += candidates
            .iter()
            .filter(|diff| matches!(diff, ProposedDiff::TwoStep { .. }))
            .count();

        let before_prop_raw = compute_propagation_cost(&case.uds);
        let before_cyc_raw = compute_cyclic_penalty(&case.uds);
        let before_mod_raw = modularity_score(&case.uds);
        let mut accepted_by_domain = 0_usize;

        for diff in &candidates {
            let mut simulated = case.clone();
            simulated.begin_tx().expect("begin tx for guard eval");
            if simulated.apply_diff(diff.clone()).is_err() {
                continue;
            }
            simulated.commit_tx().expect("commit tx for guard eval");

            let before = case.evaluation.clone();
            let after = simulated.evaluation.clone();
            let before_v = case.compute_state_vector();
            let after_v = simulated.compute_state_vector();
            let delta_v = before_v.delta(&after_v);
            let delta_v_norm = delta_vector_norm(&delta_v);
            let before_consistency = before.consistency as f64 / 100.0;
            let after_consistency = after.consistency as f64 / 100.0;
            let delta_c = after_consistency - before_consistency;
            let after_prop_raw = compute_propagation_cost(&simulated.uds);
            let delta_prop_raw = after_prop_raw - before_prop_raw;
            let after_cyc_raw = compute_cyclic_penalty(&simulated.uds);
            let delta_cyc_raw = after_cyc_raw - before_cyc_raw;
            let after_mod_raw = modularity_score(&simulated.uds);
            let _delta_mod_raw = after_mod_raw - before_mod_raw;
            let delta_complexity = simulated.uds.nodes.len() as f64 - case.uds.nodes.len() as f64;
            let Some(result) = case.evaluate_diff(diff) else {
                continue;
            };
            let benchmark_score = result.delta.d_consistency
                + GAMMA * result.delta.d_prop_quality.max(0.0)
                - ALPHA * (-result.delta.d_prop_quality).max(0.0)
                - BETA * (-result.delta.d_cycle_quality).max(0.0)
                - ETA * delta_complexity.max(0.0)
                + DELTA_MOD * result.delta.d_modularity.max(0.0);
            assert!(
                (benchmark_score - result.score).abs() < 1e-12,
                "benchmark_score mismatch: {} vs {}",
                benchmark_score,
                result.score
            );
            let score = result.score;

            let old_and_accept = delta_c > 0.0 && delta_prop_raw <= 0.0 && delta_cyc_raw <= 0.0;
            if old_and_accept {
                total_accepted_old_and += 1;
            }

            let accepted = result.accepted;
            let in_domain = suggestions_domain.contains(diff);
            if accepted != in_domain {
                mismatch_count = mismatch_count.saturating_add(1);
            }
            if accepted {
                accepted_by_domain += 1;
                total_accepted_new += 1;
                accepted_scores.push(score);
                delta_v_consistency.push(delta_v.d_consistency);
                delta_v_prop_quality.push(delta_v.d_prop_quality);
                delta_v_cycle_quality.push(delta_v.d_cycle_quality);
                delta_v_modularity.push(delta_v.d_modularity);
                delta_v_norms.push(delta_v_norm);
                match diff {
                    ProposedDiff::SplitHighOutDegreeNode { .. } => split_accepted_total += 1,
                    ProposedDiff::RemoveNode { .. } => remove_node_accepted_total += 1,
                    ProposedDiff::SetDependencies { .. } => {
                        rewire_edge_accepted_total += 1;
                    }
                    ProposedDiff::RewireHighImpactEdge { .. } => {
                        rewire_edge_accepted_total += 1;
                    }
                    ProposedDiff::TwoStep { .. } => {
                        two_step_accepted_total += 1;
                        if diff_contains_rewire(diff) {
                            rewire_edge_accepted_total += 1;
                        }
                        if let ProposedDiff::TwoStep { first, second } = diff {
                            let first_kind = diff_kind(first);
                            let second_kind = diff_kind(second);
                            *two_step_first_counts.entry(first_kind.to_string()).or_insert(0) += 1;
                            *two_step_second_counts
                                .entry(second_kind.to_string())
                                .or_insert(0) += 1;
                            *two_step_pair_counts
                                .entry(format!("{first_kind}->{second_kind}"))
                                .or_insert(0) += 1;
                        }
                        two_step_delta_cons.push(delta_c);
                        two_step_delta_prop.push(delta_prop_raw);
                        two_step_scores.push(score);
                        two_step_delta_v_norms.push(delta_v_norm);
                    }
                    _ => {
                        other_accepted_total += 1;
                    }
                }
                if !matches!(diff, ProposedDiff::TwoStep { .. }) {
                    single_step_delta_cons.push(delta_c);
                    single_step_delta_prop.push(delta_prop_raw);
                    single_step_scores.push(score);
                    single_step_delta_v_norms.push(delta_v_norm);
                }
                post_consistency.push(after_consistency);
                accepted_delta_consistency.push(delta_c);
                sum_consistency += delta_c;
                sum_structural +=
                    (after.structural_integrity as f64 - before.structural_integrity as f64) / 100.0;
                sum_dependency +=
                    (after.dependency_soundness as f64 - before.dependency_soundness as f64) / 100.0;
                sum_delta_prop_raw += delta_prop_raw;
                accepted_delta_prop_raw.push(delta_prop_raw);
                sum_delta_cyc_raw += delta_cyc_raw;
                if after.dependency_soundness > before.dependency_soundness {
                    improved_cycle_count += 1;
                }
            }
        }

        if suggestions_domain.len() != accepted_by_domain {
            println!(
                "warn: domain/model accept mismatch domain={} model={} ",
                suggestions_domain.len(),
                accepted_by_domain
            );
        }
    }

    let accepted_rate = if total_suggestions == 0 {
        0.0
    } else {
        total_accepted_new as f64 / total_suggestions as f64
    };
    let accepted_rate_old_and = if total_suggestions == 0 {
        0.0
    } else {
        total_accepted_old_and as f64 / total_suggestions as f64
    };
    let split_accepted_rate = if split_candidates_total == 0 {
        0.0
    } else {
        split_accepted_total as f64 / split_candidates_total as f64
    };
    let rewire_accepted_rate = if rewire_candidates_total == 0 {
        0.0
    } else {
        rewire_edge_accepted_total as f64 / rewire_candidates_total as f64
    };
    let two_step_accepted_rate = if two_step_candidates_total == 0 {
        0.0
    } else {
        two_step_accepted_total as f64 / two_step_candidates_total as f64
    };

    let avg_consistency = if total_accepted_new == 0 {
        0.0
    } else {
        sum_consistency / total_accepted_new as f64
    };
    let avg_structural = if total_accepted_new == 0 {
        0.0
    } else {
        sum_structural / total_accepted_new as f64
    };
    let avg_dependency = if total_accepted_new == 0 {
        0.0
    } else {
        sum_dependency / total_accepted_new as f64
    };
    let avg_delta_prop_raw = if total_accepted_new == 0 {
        0.0
    } else {
        sum_delta_prop_raw / total_accepted_new as f64
    };
    let avg_delta_cyc_raw = if total_accepted_new == 0 {
        0.0
    } else {
        sum_delta_cyc_raw / total_accepted_new as f64
    };

    let init_mean = mean(&case_initial_consistency);
    let init_min = min(&case_initial_consistency);
    let init_max = max(&case_initial_consistency);
    let init_stddev = stddev(&case_initial_consistency);
    let score_mean = mean(&accepted_scores);
    let score_min = min(&accepted_scores);
    let score_max = max(&accepted_scores);
    let score_median = median(&accepted_scores);
    let score_stddev = stddev(&accepted_scores);
    let score_variance = variance(&accepted_scores);
    let score_p90 = percentile(&accepted_scores, 0.90);
    let stability = accepted_rate * score_mean;

    let delta_c_mean = mean(&accepted_delta_consistency);
    let delta_c_min = min(&accepted_delta_consistency);
    let delta_c_max = max(&accepted_delta_consistency);
    let delta_c_stddev = stddev(&accepted_delta_consistency);
    let delta_c_small_ratio = if accepted_delta_consistency.is_empty() {
        0.0
    } else {
        accepted_delta_consistency
            .iter()
            .filter(|v| **v >= 0.0 && **v <= 0.01)
            .count() as f64
            / accepted_delta_consistency.len() as f64
    };

    let delta_prop_mean = mean(&accepted_delta_prop_raw);
    let delta_prop_min = min(&accepted_delta_prop_raw);
    let delta_prop_max = max(&accepted_delta_prop_raw);
    let delta_prop_stddev = stddev(&accepted_delta_prop_raw);
    let delta_v_norm_mean = mean(&delta_v_norms);
    let delta_v_norm_min = min(&delta_v_norms);
    let delta_v_norm_max = max(&delta_v_norms);
    let delta_v_norm_stddev = stddev(&delta_v_norms);

    let split_pct = ratio_percent(split_accepted_total, total_accepted_new);
    let remove_node_pct = ratio_percent(remove_node_accepted_total, total_accepted_new);
    let rewire_edge_pct = ratio_percent(rewire_edge_accepted_total, total_accepted_new);
    let other_pct = ratio_percent(other_accepted_total, total_accepted_new);

    println!("Cases: {}", cases.len());
    println!("Suggestions (candidate): {}", total_suggestions);
    println!("Accepted suggestions (score): {}", total_accepted_new);
    println!("Accepted suggestions (old_and): {}", total_accepted_old_and);
    println!("Accepted Rate: {:.4}", accepted_rate);
    println!("Accepted Rate (old_and): {:.4}", accepted_rate_old_and);
    println!("Evaluation mismatch count: {}", mismatch_count);
    println!("Avg Δconsistency: {:+.4}", avg_consistency);
    println!("Avg Δstructural: {:+.4}", avg_structural);
    println!("Avg Δdependency: {:+.4}", avg_dependency);
    println!();
    println!("Initial consistency distribution:");
    println!("  mean: {:.4}", init_mean);
    println!("  min: {:.4}", init_min);
    println!("  max: {:.4}", init_max);
    println!("  stddev: {:.4}", init_stddev);
    println!();
    println!("Initial structural:");
    println!("  mean: {:.4}", mean(&case_initial_structural));
    println!();
    println!("Initial dependency:");
    println!("  mean: {:.4}", mean(&case_initial_dependency));
    println!();
    println!("Post consistency:");
    println!("  mean: {:.4}", mean(&post_consistency));
    println!("  min: {:.4}", min(&post_consistency));
    println!("  max: {:.4}", max(&post_consistency));
    println!();
    println!("Delta consistency:");
    println!("  mean: {:.4}", delta_c_mean);
    println!("  min: {:.4}", delta_c_min);
    println!("  max: {:.4}", delta_c_max);
    println!("  stddev: {:.4}", delta_c_stddev);
    println!("  ratio(0..=0.01): {:.4}", delta_c_small_ratio);
    println!();
    println!("Delta cycle quality:");
    println!("  mean: {:.4}", avg_dependency);
    println!("  improved_count: {}", improved_cycle_count);
    println!("Raw delta summary:");
    println!("  Avg Δpropagation_cost_raw: {:+.6}", avg_delta_prop_raw);
    println!("  Avg Δcyclic_penalty_raw: {:+.6}", avg_delta_cyc_raw);
    println!("Split summary:");
    println!("  split_candidates: {}", split_candidates_total);
    println!("  split_accepted: {}", split_accepted_total);
    println!("  split_accepted_rate: {:.4}", split_accepted_rate);
    println!("Rewire summary:");
    println!("  rewire_candidates: {}", rewire_candidates_total);
    println!("  rewire_accepted: {}", rewire_edge_accepted_total);
    println!("  rewire_accepted_rate: {:.4}", rewire_accepted_rate);
    println!("Two-step summary:");
    println!("  two_step_candidates: {}", two_step_candidates_total);
    println!("  two_step_accepted: {}", two_step_accepted_total);
    println!("  two_step_accepted_rate: {:.4}", two_step_accepted_rate);
    if two_step_accepted_total > 0 {
        let first_split = ratio_percent(
            *two_step_first_counts.get("Split").unwrap_or(&0),
            two_step_accepted_total,
        );
        let first_remove = ratio_percent(
            *two_step_first_counts.get("RemoveNode").unwrap_or(&0),
            two_step_accepted_total,
        );
        let first_rewire = ratio_percent(
            *two_step_first_counts.get("Rewire").unwrap_or(&0),
            two_step_accepted_total,
        );
        let first_other = ratio_percent(
            *two_step_first_counts.get("Other").unwrap_or(&0),
            two_step_accepted_total,
        );

        let second_split = ratio_percent(
            *two_step_second_counts.get("Split").unwrap_or(&0),
            two_step_accepted_total,
        );
        let second_remove = ratio_percent(
            *two_step_second_counts.get("RemoveNode").unwrap_or(&0),
            two_step_accepted_total,
        );
        let second_rewire = ratio_percent(
            *two_step_second_counts.get("Rewire").unwrap_or(&0),
            two_step_accepted_total,
        );
        let second_other = ratio_percent(
            *two_step_second_counts.get("Other").unwrap_or(&0),
            two_step_accepted_total,
        );

        println!("TwoStep breakdown (accepted only):");
        println!("  first: Split={:.2}% RemoveNode={:.2}% Rewire={:.2}% Other={:.2}%", first_split, first_remove, first_rewire, first_other);
        println!("  second: Split={:.2}% RemoveNode={:.2}% Rewire={:.2}% Other={:.2}%", second_split, second_remove, second_rewire, second_other);

        let mut pair_top = two_step_pair_counts.iter().collect::<Vec<_>>();
        pair_top.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        println!("  pairs top5:");
        for (pair, count) in pair_top.into_iter().take(5) {
            let ratio = ratio_percent(*count, two_step_accepted_total);
            println!("    {}: {} ({:.2}%)", pair, count, ratio);
        }

        println!("  avg Δcons(two-step): {:+.4}", mean(&two_step_delta_cons));
        println!("  avg Δprop(two-step): {:+.6}", mean(&two_step_delta_prop));
        println!("  avg score(two-step): {:+.6}", mean(&two_step_scores));
        if !single_step_scores.is_empty() {
            println!("  avg Δcons(single-step): {:+.4}", mean(&single_step_delta_cons));
            println!("  avg Δprop(single-step): {:+.6}", mean(&single_step_delta_prop));
            println!("  avg score(single-step): {:+.6}", mean(&single_step_scores));
            println!(
                "  delta avg score(two-step - single-step): {:+.6}",
                mean(&two_step_scores) - mean(&single_step_scores)
            );
        }
    }
    println!("Stability: {:.6}", stability);
    println!("Accepted diff type ratio:");
    println!("  SplitHighOutDegreeNode: {:.2}%", split_pct);
    println!("  RemoveNode: {:.2}%", remove_node_pct);
    println!("  RewireEdge(SetDependencies): {:.2}%", rewire_edge_pct);
    println!("  Other: {:.2}%", other_pct);
    println!("Δpropagation_cost_raw distribution (accepted only):");
    println!("  mean: {:+.6}", delta_prop_mean);
    println!("  min: {:+.6}", delta_prop_min);
    println!("  max: {:+.6}", delta_prop_max);
    println!("  stddev: {:.6}", delta_prop_stddev);
    println!("Δv (accepted only):");
    println!("  d_consistency stddev: {:.6}", stddev(&delta_v_consistency));
    println!(
        "  d_prop_quality stddev: {:.6}",
        stddev(&delta_v_prop_quality)
    );
    println!(
        "  d_cycle_quality stddev: {:.6}",
        stddev(&delta_v_cycle_quality)
    );
    println!("  d_modularity stddev: {:.6}", stddev(&delta_v_modularity));
    println!(
        "  ||Δv|| mean/min/max/stddev: {:.6} / {:.6} / {:.6} / {:.6}",
        delta_v_norm_mean, delta_v_norm_min, delta_v_norm_max, delta_v_norm_stddev
    );
    println!("Δv correlation matrix (accepted only):");
    let corr_cc = 1.0;
    let corr_cp = correlation(&delta_v_consistency, &delta_v_prop_quality);
    let corr_cy = correlation(&delta_v_consistency, &delta_v_cycle_quality);
    let corr_cm = correlation(&delta_v_consistency, &delta_v_modularity);
    let corr_py = correlation(&delta_v_prop_quality, &delta_v_cycle_quality);
    let corr_pm = correlation(&delta_v_prop_quality, &delta_v_modularity);
    let corr_ym = correlation(&delta_v_cycle_quality, &delta_v_modularity);
    println!(
        "  [C, Pq, Cyq, M]\n  C : [{:.3}, {:.3}, {:.3}, {:.3}]",
        corr_cc, corr_cp, corr_cy, corr_cm
    );
    println!(
        "  Pq: [{:.3}, {:.3}, {:.3}, {:.3}]",
        corr_cp, 1.0, corr_py, corr_pm
    );
    println!(
        "  Cyq:[{:.3}, {:.3}, {:.3}, {:.3}]",
        corr_cy, corr_py, 1.0, corr_ym
    );
    println!(
        "  M : [{:.3}, {:.3}, {:.3}, {:.3}]",
        corr_cm, corr_pm, corr_ym, 1.0
    );
    println!("Score summary:");
    println!("  mean: {:+.6}", score_mean);
    println!("  median: {:+.6}", score_median);
    println!("  min: {:+.6}", score_min);
    println!("  max: {:+.6}", score_max);
    println!("  stddev: {:.6}", score_stddev);
    println!("  var: {:.6}", score_variance);
    println!("  p90: {:+.6}", score_p90);

    // Distribution quality gates.
    assert_ne!(init_mean, 0.5, "initial mean must not be fixed at 0.5");
    assert!(init_min < 0.4, "initial min should be below 0.4");
    assert!(init_max > 0.8, "initial max should be above 0.8");
    assert!(init_stddev > 0.05, "initial stddev should exceed 0.05");

    assert!(total_suggestions > 0, "expected candidate suggestions > 0");
    assert_eq!(mismatch_count, 0, "expected zero evaluate_diff mismatch");
    assert!(
        stddev(&delta_v_consistency) > 0.0
            && stddev(&delta_v_prop_quality) > 0.0
            && stddev(&delta_v_cycle_quality) > 0.0
            && stddev(&delta_v_modularity) > 0.0,
        "all Δv axes must be non-degenerate"
    );
    let max_abs_corr = [corr_cp, corr_cy, corr_cm, corr_py, corr_pm, corr_ym]
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f64, f64::max);
    assert!(max_abs_corr < 0.95, "Δv axes are too collinear");
    if !two_step_delta_v_norms.is_empty() && !single_step_delta_v_norms.is_empty() {
        assert!(
            mean(&two_step_delta_v_norms) > mean(&single_step_delta_v_norms),
            "TwoStep ||Δv|| mean must be greater than single-step"
        );
    }
    assert!(accepted_rate >= 0.35, "accepted_rate must be >= 0.35");
    assert!(avg_consistency > 0.0, "Avg Δconsistency must stay positive");
    assert!(
        avg_delta_prop_raw <= 2e-3,
        "Avg Δpropagation_cost_raw must stay near non-worsening"
    );
    assert!(
        avg_delta_cyc_raw <= 0.0,
        "Avg Δcyclic_penalty_raw must not worsen"
    );
}

pub fn benchmark_propagation_cost_phase_b() {
    let cases = generate_diverse_cases(42, 120);
    assert!(cases.len() >= 100, "benchmark requires at least 100 cases");

    let mut costs_new = Vec::with_capacity(cases.len());
    let mut costs_old = Vec::with_capacity(cases.len());
    let mut consistencies = Vec::with_capacity(cases.len());
    let mut node_counts = Vec::with_capacity(cases.len());
    let mut densities = Vec::with_capacity(cases.len());

    let mut dense = Vec::new();
    let mut random = Vec::new();
    let mut chain = Vec::new();
    let mut star = Vec::new();

    for case in &cases {
        let cost_new = compute_propagation_cost(&case.state.uds);
        let cost_old = compute_propagation_cost_old(&case.state.uds);
        let consistency = case.state.evaluation.consistency as f64 / 100.0;
        let node_count = case.node_count as f64;
        let density = compute_graph_density(&case.state.uds);

        costs_new.push(cost_new);
        costs_old.push(cost_old);
        consistencies.push(consistency);
        node_counts.push(node_count);
        densities.push(density);

        match case.pattern {
            Pattern::DenseGraph => dense.push(cost_new),
            Pattern::Random => random.push(cost_new),
            Pattern::Chain => chain.push(cost_new),
            Pattern::Star => star.push(cost_new),
            Pattern::SparseDag => {}
        }
    }

    let mean_cost = mean(&costs_new);
    let min_cost = min(&costs_new);
    let max_cost = max(&costs_new);
    let stddev_cost = stddev(&costs_new);
    let old_corr_density = correlation(&costs_old, &densities);

    let corr_consistency = correlation(&costs_new, &consistencies);
    let corr_node_count = correlation(&costs_new, &node_counts);
    let corr_density = correlation(&costs_new, &densities);

    println!("Propagation cost distribution:");
    println!("  mean: {:.4}", mean_cost);
    println!("  min: {:.4}", min_cost);
    println!("  max: {:.4}", max_cost);
    println!("  stddev: {:.4}", stddev_cost);
    println!();
    println!("Correlation:");
    println!("  corr(propagation_cost, consistency): {:.4}", corr_consistency);
    println!("  corr(propagation_cost, node_count): {:.4}", corr_node_count);
    println!("  corr(propagation_cost_new, density): {:.4}", corr_density);
    println!("  corr(propagation_cost_old, density): {:.4}", old_corr_density);
    println!();
    println!("Pattern means:");
    println!("  Dense:  {:.4}", mean(&dense));
    println!("  Random: {:.4}", mean(&random));
    println!("  Chain:  {:.4}", mean(&chain));
    println!("  Star:   {:.4}", mean(&star));

    assert!(stddev_cost > 0.05, "stddev must be > 0.05");
    assert!(
        (max_cost - min_cost).abs() > f64::EPSILON,
        "min and max must differ"
    );
    assert!(corr_density.abs() <= old_corr_density.abs() + 1e-9);
}

pub fn benchmark_cyclic_penalty_phase_a() {
    let cases = generate_diverse_cases(42, 120);
    assert!(cases.len() >= 100, "benchmark requires at least 100 cases");

    let mut cyclic_penalties = Vec::with_capacity(cases.len());
    let mut densities = Vec::with_capacity(cases.len());
    let mut propagation_costs = Vec::with_capacity(cases.len());

    for case in &cases {
        let cyclic = compute_cyclic_penalty(&case.state.uds);
        let density = 0.52 * case.density + 0.48 * compute_graph_density(&case.state.uds);
        let propagation = compute_propagation_cost(&case.state.uds);

        cyclic_penalties.push(cyclic);
        densities.push(density);
        propagation_costs.push(propagation);
    }

    let mean_penalty = mean(&cyclic_penalties);
    let min_penalty = min(&cyclic_penalties);
    let max_penalty = max(&cyclic_penalties);
    let stddev_penalty = stddev(&cyclic_penalties);
    let corr_density = correlation(&cyclic_penalties, &densities);
    let corr_propagation = correlation(&cyclic_penalties, &propagation_costs);

    println!("Cyclic penalty distribution:");
    println!("  mean: {:.4}", mean_penalty);
    println!("  min: {:.4}", min_penalty);
    println!("  max: {:.4}", max_penalty);
    println!("  stddev: {:.4}", stddev_penalty);
    println!();
    println!("Correlation:");
    println!("  corr(cyclic_penalty, density): {:.4}", corr_density);
    println!(
        "  corr(cyclic_penalty, propagation_cost): {:.4}",
        corr_propagation
    );

    assert!(stddev_penalty > 0.01, "stddev must be > 0.01");
    assert!(
        (max_penalty - min_penalty).abs() > f64::EPSILON,
        "min and max must differ"
    );
    assert!(corr_density > 0.3, "correlation with density must be > 0.3");
    assert!(
        corr_propagation.abs() < 0.9,
        "cyclic penalty must not be nearly identical to propagation cost"
    );
}

pub fn benchmark_spectral_gap_phase_b() {
    let cases = generate_diverse_cases(42, 120);
    assert!(cases.len() >= 100, "benchmark requires at least 100 cases");

    let mut spectral_gaps = Vec::with_capacity(cases.len());
    let mut densities = Vec::with_capacity(cases.len());
    let mut propagation_costs_new = Vec::with_capacity(cases.len());
    let mut propagation_costs_old = Vec::with_capacity(cases.len());
    let mut propagation_qualities_new = Vec::with_capacity(cases.len());
    let mut cyclic_penalties = Vec::with_capacity(cases.len());
    let mut accepted_rates = Vec::with_capacity(cases.len());
    let mut sparse_gap = Vec::new();
    let mut sparse_prop_new = Vec::new();
    let mut sparse_prop_old = Vec::new();
    let mut dense_gap = Vec::new();
    let mut dense_prop_new = Vec::new();
    let mut dense_prop_old = Vec::new();
    let mut chain_gap = Vec::new();
    let mut chain_prop_new = Vec::new();
    let mut chain_prop_old = Vec::new();
    let mut star_gap = Vec::new();
    let mut star_prop_new = Vec::new();
    let mut star_prop_old = Vec::new();
    let mut random_gap = Vec::new();
    let mut random_prop_new = Vec::new();
    let mut random_prop_old = Vec::new();

    for case in &cases {
        let spectral_gap = compute_diffusion_index_spectral_gap(&case.state.uds);
        let density = 0.5 * case.density + 0.5 * compute_graph_density(&case.state.uds);
        let propagation_raw_new = compute_propagation_cost(&case.state.uds);
        let propagation_raw_old = compute_propagation_cost_old(&case.state.uds);
        let propagation_quality_new = (1.0 - propagation_raw_new).clamp(0.0, 1.0);
        let cyclic = compute_cyclic_penalty(&case.state.uds);

        let pareto = case.state.analyze_pareto().expect("pareto should succeed");
        let accepted = case
            .state
            .suggest_diffs_from_analysis(&pareto)
            .expect("suggest should succeed")
            .len() as f64;
        let candidates = estimate_candidate_count(&case.state) as f64;
        let accepted_rate = if candidates <= 0.0 {
            0.0
        } else {
            accepted / candidates
        };

        spectral_gaps.push(spectral_gap);
        densities.push(density);
        propagation_costs_new.push(propagation_raw_new);
        propagation_costs_old.push(propagation_raw_old);
        propagation_qualities_new.push(propagation_quality_new);
        cyclic_penalties.push(cyclic);
        accepted_rates.push(accepted_rate);

        match case.pattern {
            Pattern::SparseDag => {
                sparse_gap.push(spectral_gap);
                sparse_prop_new.push(propagation_raw_new);
                sparse_prop_old.push(propagation_raw_old);
            }
            Pattern::DenseGraph => {
                dense_gap.push(spectral_gap);
                dense_prop_new.push(propagation_raw_new);
                dense_prop_old.push(propagation_raw_old);
            }
            Pattern::Chain => {
                chain_gap.push(spectral_gap);
                chain_prop_new.push(propagation_raw_new);
                chain_prop_old.push(propagation_raw_old);
            }
            Pattern::Star => {
                star_gap.push(spectral_gap);
                star_prop_new.push(propagation_raw_new);
                star_prop_old.push(propagation_raw_old);
            }
            Pattern::Random => {
                random_gap.push(spectral_gap);
                random_prop_new.push(propagation_raw_new);
                random_prop_old.push(propagation_raw_old);
            }
        }
    }

    let mean_gap = mean(&spectral_gaps);
    let min_gap = min(&spectral_gaps);
    let max_gap = max(&spectral_gaps);
    let stddev_gap = stddev(&spectral_gaps);

    let corr_density = correlation(&spectral_gaps, &densities);
    let corr_propagation_raw_new = correlation(&spectral_gaps, &propagation_costs_new);
    let corr_propagation_raw_old = correlation(&spectral_gaps, &propagation_costs_old);
    let corr_propagation_quality_new = correlation(&spectral_gaps, &propagation_qualities_new);
    let corr_cyclic = correlation(&spectral_gaps, &cyclic_penalties);
    let corr_accepted = correlation(&spectral_gaps, &accepted_rates);
    let partial_corr_prop_given_density_new =
        partial_correlation_single_control(&spectral_gaps, &propagation_costs_new, &densities);
    let partial_corr_prop_given_density_old =
        partial_correlation_single_control(&spectral_gaps, &propagation_costs_old, &densities);

    println!("Diffusion index (spectral gap) distribution:");
    println!("  mean: {:.4}", mean_gap);
    println!("  min: {:.4}", min_gap);
    println!("  max: {:.4}", max_gap);
    println!("  stddev: {:.4}", stddev_gap);
    println!();
    println!("Correlation:");
    println!("  corr(diffusion_index, density): {:.4}", corr_density);
    println!(
        "  corr(diffusion_index, propagation_cost_raw_new): {:.4}",
        corr_propagation_raw_new
    );
    println!(
        "  corr(diffusion_index, propagation_cost_raw_old): {:.4}",
        corr_propagation_raw_old
    );
    println!(
        "  corr(diffusion_index, propagation_quality_new): {:.4}",
        corr_propagation_quality_new
    );
    println!("  corr(diffusion_index, cyclic_penalty_raw): {:.4}", corr_cyclic);
    println!("  corr(diffusion_index, accepted_rate): {:.4}", corr_accepted);
    println!(
        "  partial corr(diffusion_index, propagation_cost_raw | density): {:.4}",
        partial_corr_prop_given_density_new
    );
    println!(
        "  partial corr(diffusion_index, propagation_cost_raw_old | density): {:.4}",
        partial_corr_prop_given_density_old
    );
    println!();

    // Phase B': residualize diffusion_index by density and evaluate independence.
    let mut diffusion_residual = residualize_linear(&spectral_gaps, &densities);
    let res_mean_before = mean(&diffusion_residual);
    let res_std_before = stddev(&diffusion_residual);
    if res_std_before > 1e-12 {
        diffusion_residual = diffusion_residual
            .iter()
            .map(|v| (v - res_mean_before) / res_std_before)
            .collect::<Vec<_>>();
    }

    let corr_res_density = correlation(&diffusion_residual, &densities);
    let corr_res_propagation = correlation(&diffusion_residual, &propagation_costs_new);
    let corr_res_cyclic = correlation(&diffusion_residual, &cyclic_penalties);
    let partial_corr_res_prop_given_density = partial_correlation_single_control(
        &diffusion_residual,
        &propagation_costs_new,
        &densities,
    );
    let res_mean = mean(&diffusion_residual);
    let res_std = stddev(&diffusion_residual);

    println!("Residualized diffusion_index (Phase B'):");
    println!("  mean: {:.4}", res_mean);
    println!("  stddev: {:.4}", res_std);
    println!("  corr(diffusion_index_res, density): {:.4}", corr_res_density);
    println!(
        "  corr(diffusion_index_res, propagation_cost_raw): {:.4}",
        corr_res_propagation
    );
    println!(
        "  partial corr(diffusion_index_res, propagation_cost_raw | density): {:.4}",
        partial_corr_res_prop_given_density
    );
    println!(
        "  corr(diffusion_index_res, cyclic_penalty_raw): {:.4}",
        corr_res_cyclic
    );
    println!();
    println!("Pattern correlation (gap vs propagation_cost_raw_new):");
    println!("  SparseDag: {:.4}", correlation(&sparse_gap, &sparse_prop_new));
    println!("  DenseGraph: {:.4}", correlation(&dense_gap, &dense_prop_new));
    println!("  Chain: {:.4}", correlation(&chain_gap, &chain_prop_new));
    println!("  Star: {:.4}", correlation(&star_gap, &star_prop_new));
    println!("  Random: {:.4}", correlation(&random_gap, &random_prop_new));
    println!("Pattern correlation (gap vs propagation_cost_raw_old):");
    println!("  SparseDag: {:.4}", correlation(&sparse_gap, &sparse_prop_old));
    println!("  DenseGraph: {:.4}", correlation(&dense_gap, &dense_prop_old));
    println!("  Chain: {:.4}", correlation(&chain_gap, &chain_prop_old));
    println!("  Star: {:.4}", correlation(&star_gap, &star_prop_old));
    println!("  Random: {:.4}", correlation(&random_gap, &random_prop_old));

    assert!(stddev_gap > 0.01, "stddev must be > 0.01");
    assert!(
        (max_gap - min_gap).abs() > f64::EPSILON,
        "min and max must differ"
    );
    assert!(corr_density.abs() < 0.9, "|corr(diffusion_index, density)| must be < 0.9");
    assert!(
        corr_propagation_raw_new.abs() > 0.2 || corr_cyclic.abs() > 0.2,
        "expected meaningful correlation with propagation or cyclic penalty"
    );
    assert!(corr_propagation_raw_new.abs() < 0.9, "|corr(diffusion_index, propagation)| must be < 0.9");
    assert!(partial_corr_prop_given_density_new.abs() < 0.6, "partial corr must be < 0.6");

    // Phase B' gates
    assert!(
        corr_res_density.abs() < 0.1,
        "|corr(diffusion_index_res, density)| must be < 0.1"
    );
    assert!(
        partial_corr_res_prop_given_density.abs() < 0.60,
        "|partial corr(diffusion_index_res, propagation | density)| must be < 0.60"
    );
    assert!(res_std > 0.05, "stddev(diffusion_index_res) must be > 0.05");

    if partial_corr_res_prop_given_density.abs() >= 0.40 {
        println!(
            "Phase B' note: target |partial corr| < 0.40 is not met (current: {:.4})",
            partial_corr_res_prop_given_density
        );
    }
}

#[derive(Clone, Debug)]
struct LambdaMetrics {
    lambda: f64,
    accepted_rate: f64,
    accepted_rate_old_and: f64,
    avg_delta_consistency: f64,
    avg_delta_prop_raw: f64,
    avg_delta_cyc_raw: f64,
    score_mean: f64,
    score_variance: f64,
    stability_index: f64,
    corr_prop_density: f64,
    partial_corr_gap_prop_given_density: f64,
}

#[derive(Clone, Copy)]
enum LambdaMode {
    Fixed(f64),
    DynamicDensity { lambda_min: f64, lambda_max: f64 },
}

#[derive(Clone, Debug)]
struct DynamicLambdaMetrics {
    label: &'static str,
    accepted_rate: f64,
    avg_delta_consistency: f64,
    avg_delta_prop_raw: f64,
    avg_delta_cyc_raw: f64,
    score_mean: f64,
    score_variance: f64,
    stability_index: f64,
    corr_prop_density: f64,
    partial_corr_gap_res_prop_given_density: f64,
}

pub fn benchmark_lambda_grid_phase_c() {
    let cases = generate_diverse_cases(42, 120);
    assert!(cases.len() >= 100, "benchmark requires at least 100 cases");

    const KAPPA: f64 = 0.8;
    const ALPHA: f64 = 3.0;
    const BETA: f64 = 3.0;
    const GAMMA: f64 = 1.0;
    let lambda_grid = [0.50, 0.60, 0.70, 0.80, 0.85, 0.90];

    let mut rows = Vec::new();
    for lambda in lambda_grid {
        rows.push(evaluate_lambda_metrics(
            &cases, lambda, KAPPA, ALPHA, BETA, GAMMA,
        ));
    }

    println!("Lambda Grid Results (kappa=0.8, alpha=3.0, beta=3.0)");
    println!("λ | acc | acc_old_and | Δcons | Δprop_raw | Δcyc_raw | score_mean | score_var | stability | corr(prop,density) | partial(gap,prop|density)");
    for r in &rows {
        println!(
            "{:.2} | {:.4} | {:.4} | {:+.4} | {:+.6} | {:+.6} | {:+.6} | {:.6} | {:.6} | {:.4} | {:.4}",
            r.lambda,
            r.accepted_rate,
            r.accepted_rate_old_and,
            r.avg_delta_consistency,
            r.avg_delta_prop_raw,
            r.avg_delta_cyc_raw,
            r.score_mean,
            r.score_variance,
            r.stability_index,
            r.corr_prop_density,
            r.partial_corr_gap_prop_given_density
        );
    }

    let feasible = rows
        .iter()
        .filter(|r| {
            (0.35..=0.55).contains(&r.accepted_rate)
                && r.avg_delta_consistency > 0.0
                && r.avg_delta_prop_raw <= 0.002
                && r.avg_delta_cyc_raw <= 0.0
                && r.partial_corr_gap_prop_given_density.abs() < 0.40
        })
        .max_by(|a, b| {
            a.score_mean
                .partial_cmp(&b.score_mean)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    if let Some(best) = feasible {
        println!();
        println!(
            "Selected λ (feasible max score_mean): {:.2} (acc={:.4}, score_mean={:+.6}, stability={:.6})",
            best.lambda, best.accepted_rate, best.score_mean, best.stability_index
        );
    } else {
        let fallback = rows.iter().max_by(|a, b| {
            a.stability_index
                .partial_cmp(&b.stability_index)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(best) = fallback {
            println!();
            println!(
                "No fully-feasible λ found. Fallback by max stability_index: {:.2} (acc={:.4}, score_mean={:+.6})",
                best.lambda, best.accepted_rate, best.score_mean
            );
        }
    }

    if let Some(baseline) = rows
        .iter()
        .find(|r| (r.lambda - 0.60).abs() < 1e-12)
        .cloned()
    {
        write_phase_c_baseline_json(&baseline, 42, 120);
    }
}

pub fn benchmark_dynamic_lambda_phase_d() {
    let cases = generate_diverse_cases(42, 120);
    assert!(cases.len() >= 100, "benchmark requires at least 100 cases");

    const KAPPA: f64 = 0.8;
    const ALPHA: f64 = 3.0;
    const BETA: f64 = 3.0;
    const GAMMA: f64 = 1.0;

    let fixed = evaluate_dynamic_lambda_metrics(
        &cases,
        "fixed_0.60",
        LambdaMode::Fixed(0.60),
        KAPPA,
        ALPHA,
        BETA,
        GAMMA,
    );
    let mut variants = Vec::new();
    for (label, min_l, max_l) in [
        ("R1(0.58,0.62)", 0.58, 0.62),
        ("R2(0.57,0.63)", 0.57, 0.63),
        ("R3(0.56,0.64)", 0.56, 0.64),
        ("R4(0.55,0.60)", 0.55, 0.60),
        ("R5(0.53,0.60)", 0.53, 0.60),
        ("R6(0.60,0.70)", 0.60, 0.70),
    ] {
        variants.push(evaluate_dynamic_lambda_metrics(
            &cases,
            label,
            LambdaMode::DynamicDensity {
                lambda_min: min_l,
                lambda_max: max_l,
            },
            KAPPA,
            ALPHA,
            BETA,
            GAMMA,
        ));
    }

    println!("Phase D-3 Dynamic Lambda Range Comparison (seed=42, cases=120)");
    println!(
        "Case | acc | stability | score_mean | Δprop | partial | corr(prop,dens) | Δcons | Δcyc | score_var"
    );
    for row in std::iter::once(&fixed).chain(variants.iter()) {
        println!(
            "{} | {:.4} | {:.6} | {:+.6} | {:+.6} | {:.4} | {:.4} | {:+.4} | {:+.6} | {:.6}",
            row.label,
            row.accepted_rate,
            row.stability_index,
            row.score_mean,
            row.avg_delta_prop_raw,
            row.partial_corr_gap_res_prop_given_density,
            row.corr_prop_density,
            row.avg_delta_consistency,
            row.avg_delta_cyc_raw,
            row.score_variance,
        );
    }

    let baseline_stability = fixed.stability_index;
    let baseline_prop = fixed.avg_delta_prop_raw;
    let baseline_acc = fixed.accepted_rate;
    let baseline_corr = fixed.corr_prop_density;
    let required_stability = baseline_stability * 1.02;
    let required_acc = baseline_acc - 0.01;
    println!();
    println!(
        "Baseline: stability={:.6}, accepted_rate={:.4}, partial={:.4}, corr={:.4}",
        baseline_stability,
        fixed.accepted_rate,
        fixed.partial_corr_gap_res_prop_given_density,
        fixed.corr_prop_density
    );
    println!(
        "Pass criteria: acc>= {:.4}, stability>={:.6}, partial<=0.40, Δprop<={:+.6}",
        required_acc,
        required_stability,
        baseline_prop + 0.0003
    );
    println!("Secondary: among [stability improve, corr improve, acc keep], >=2 true");

    let mut passed = Vec::new();
    for row in &variants {
        let primary_ok = row.accepted_rate >= required_acc
            && row.stability_index >= required_stability
            && row.partial_corr_gap_res_prop_given_density <= 0.40
            && row.avg_delta_prop_raw <= baseline_prop + 0.0003;
        let stability_improve = row.stability_index >= required_stability;
        let corr_improve = row.corr_prop_density <= baseline_corr - 0.02;
        let acc_keep = row.accepted_rate >= required_acc;
        let secondary_count = [stability_improve, corr_improve, acc_keep]
            .into_iter()
            .filter(|v| *v)
            .count();
        let secondary_ok = secondary_count >= 2;
        let ok = primary_ok && secondary_ok;
        println!(
            "{} -> {} (primary={}, secondary={}/3)",
            row.label,
            if ok { "PASS" } else { "FAIL" },
            if primary_ok { "ok" } else { "ng" },
            secondary_count
        );
        if ok {
            passed.push(row.label);
        }
    }

    println!();
    if passed.is_empty() {
        println!("Phase D-3 decision: no case passed. Keep v1 fixed lambda.");
    } else {
        println!("Phase D-3 decision: passed cases = {:?}", passed);
    }
}

fn evaluate_lambda_metrics(
    cases: &[BenchmarkCase],
    lambda: f64,
    kappa: f64,
    alpha: f64,
    beta: f64,
    gamma: f64,
) -> LambdaMetrics {
    let mut total_candidates = 0_usize;
    let mut accepted = 0_usize;
    let mut accepted_old_and = 0_usize;
    let mut sum_delta_consistency = 0.0_f64;
    let mut sum_delta_prop = 0.0_f64;
    let mut sum_delta_cyc = 0.0_f64;
    let mut score_values = Vec::new();

    let mut case_props = Vec::with_capacity(cases.len());
    let mut case_densities = Vec::with_capacity(cases.len());
    let mut case_gaps = Vec::with_capacity(cases.len());

    for case in cases {
        let state = &case.state;
        case_props.push(compute_propagation_cost_new(&state.uds, lambda, kappa));
        case_densities.push(compute_graph_density(&state.uds));
        case_gaps.push(compute_spectral_gap(&state.uds));

        let before_prop = compute_propagation_cost_new(&state.uds, lambda, kappa);
        let before_cyc = compute_cyclic_penalty(&state.uds);
        let before_mod = modularity_score(&state.uds);
        let before_cons = state.evaluation.consistency as f64 / 100.0;
        let candidates = build_candidate_diffs(state);
        total_candidates += candidates.len();

        for diff in candidates {
            let mut simulated = state.clone();
            simulated.begin_tx().expect("begin tx for lambda metrics");
            if simulated.apply_diff(diff).is_err() {
                continue;
            }
            simulated.commit_tx().expect("commit tx for lambda metrics");

            let after_cons = simulated.evaluation.consistency as f64 / 100.0;
            let delta_cons = after_cons - before_cons;
            let after_prop = compute_propagation_cost_new(&simulated.uds, lambda, kappa);
            let delta_prop = after_prop - before_prop;
            let after_cyc = compute_cyclic_penalty(&simulated.uds);
            let delta_cyc = after_cyc - before_cyc;
            let after_mod = modularity_score(&simulated.uds);
            let delta_mod = after_mod - before_mod;
            let delta_complexity =
                simulated.uds.nodes.len() as f64 - state.uds.nodes.len() as f64;

            let score = delta_cons
                + gamma * (-delta_prop).max(0.0)
                + 0.8 * delta_mod.max(0.0)
                - alpha * delta_prop.max(0.0)
                - beta * delta_cyc.max(0.0)
                - 0.02 * delta_complexity.max(0.0);
            score_values.push(score);

            if delta_cons > 0.0 && delta_prop <= 0.0 && delta_cyc <= 0.0 {
                accepted_old_and += 1;
            }
            if score > 0.0 {
                accepted += 1;
                sum_delta_consistency += delta_cons;
                sum_delta_prop += delta_prop;
                sum_delta_cyc += delta_cyc;
            }
        }
    }

    let accepted_rate = if total_candidates == 0 {
        0.0
    } else {
        accepted as f64 / total_candidates as f64
    };
    let accepted_rate_old_and = if total_candidates == 0 {
        0.0
    } else {
        accepted_old_and as f64 / total_candidates as f64
    };
    let avg_delta_consistency = if accepted == 0 {
        0.0
    } else {
        sum_delta_consistency / accepted as f64
    };
    let avg_delta_prop_raw = if accepted == 0 {
        0.0
    } else {
        sum_delta_prop / accepted as f64
    };
    let avg_delta_cyc_raw = if accepted == 0 {
        0.0
    } else {
        sum_delta_cyc / accepted as f64
    };
    let score_mean = mean(&score_values);
    let score_variance = variance(&score_values);
    let stability_index = accepted_rate * score_mean;
    let corr_prop_density = correlation(&case_props, &case_densities);
    let partial_corr_gap_prop_given_density =
        partial_correlation_single_control(&case_gaps, &case_props, &case_densities);

    LambdaMetrics {
        lambda,
        accepted_rate,
        accepted_rate_old_and,
        avg_delta_consistency,
        avg_delta_prop_raw,
        avg_delta_cyc_raw,
        score_mean,
        score_variance,
        stability_index,
        corr_prop_density,
        partial_corr_gap_prop_given_density,
    }
}

fn evaluate_dynamic_lambda_metrics(
    cases: &[BenchmarkCase],
    label: &'static str,
    mode: LambdaMode,
    kappa: f64,
    alpha: f64,
    beta: f64,
    gamma: f64,
) -> DynamicLambdaMetrics {
    let mut total_candidates = 0_usize;
    let mut accepted = 0_usize;
    let mut sum_delta_consistency = 0.0_f64;
    let mut sum_delta_prop = 0.0_f64;
    let mut sum_delta_cyc = 0.0_f64;
    let mut score_values = Vec::new();

    let mut prop_values = Vec::with_capacity(cases.len());
    let mut densities = Vec::with_capacity(cases.len());
    let mut gaps = Vec::with_capacity(cases.len());

    for case in cases {
        let state = &case.state;
        let density_case = compute_graph_density(&state.uds);
        let lambda_case = lambda_for_mode(mode, density_case);
        prop_values.push(compute_propagation_cost_new(&state.uds, lambda_case, kappa));
        densities.push(density_case);
        gaps.push(compute_diffusion_index_spectral_gap(&state.uds));

        let before_prop = compute_propagation_cost_new(&state.uds, lambda_case, kappa);
        let before_cyc = compute_cyclic_penalty(&state.uds);
        let before_mod = modularity_score(&state.uds);
        let before_cons = state.evaluation.consistency as f64 / 100.0;
        let candidates = build_candidate_diffs(state);
        total_candidates += candidates.len();

        for diff in candidates {
            let mut simulated = state.clone();
            simulated.begin_tx().expect("begin tx for dynamic lambda metrics");
            if simulated.apply_diff(diff).is_err() {
                continue;
            }
            simulated.commit_tx().expect("commit tx for dynamic lambda metrics");

            let after_density = compute_graph_density(&simulated.uds);
            let after_lambda = lambda_for_mode(mode, after_density);

            let after_cons = simulated.evaluation.consistency as f64 / 100.0;
            let delta_cons = after_cons - before_cons;
            let after_prop = compute_propagation_cost_new(&simulated.uds, after_lambda, kappa);
            let delta_prop = after_prop - before_prop;
            let after_cyc = compute_cyclic_penalty(&simulated.uds);
            let delta_cyc = after_cyc - before_cyc;
            let after_mod = modularity_score(&simulated.uds);
            let delta_mod = after_mod - before_mod;
            let delta_complexity =
                simulated.uds.nodes.len() as f64 - state.uds.nodes.len() as f64;

            let score = delta_cons
                + gamma * (-delta_prop).max(0.0)
                + 0.8 * delta_mod.max(0.0)
                - alpha * delta_prop.max(0.0)
                - beta * delta_cyc.max(0.0)
                - 0.02 * delta_complexity.max(0.0);
            score_values.push(score);

            if score > 0.0 {
                accepted += 1;
                sum_delta_consistency += delta_cons;
                sum_delta_prop += delta_prop;
                sum_delta_cyc += delta_cyc;
            }
        }
    }

    let accepted_rate = if total_candidates == 0 {
        0.0
    } else {
        accepted as f64 / total_candidates as f64
    };
    let avg_delta_consistency = if accepted == 0 {
        0.0
    } else {
        sum_delta_consistency / accepted as f64
    };
    let avg_delta_prop_raw = if accepted == 0 {
        0.0
    } else {
        sum_delta_prop / accepted as f64
    };
    let avg_delta_cyc_raw = if accepted == 0 {
        0.0
    } else {
        sum_delta_cyc / accepted as f64
    };
    let score_mean = mean(&score_values);
    let score_variance = variance(&score_values);
    let stability_index = accepted_rate * score_mean;

    let gap_res = residualize_linear(&gaps, &densities);
    let corr_prop_density = correlation(&prop_values, &densities);
    let partial_corr_gap_res_prop_given_density =
        partial_correlation_single_control(&gap_res, &prop_values, &densities);

    DynamicLambdaMetrics {
        label,
        accepted_rate,
        avg_delta_consistency,
        avg_delta_prop_raw,
        avg_delta_cyc_raw,
        score_mean,
        score_variance,
        stability_index,
        corr_prop_density,
        partial_corr_gap_res_prop_given_density,
    }
}

fn lambda_for_mode(mode: LambdaMode, density: f64) -> f64 {
    match mode {
        LambdaMode::Fixed(v) => v,
        LambdaMode::DynamicDensity {
            lambda_min,
            lambda_max,
        } => {
            let d = density.clamp(0.0, 1.0);
            lambda_min + (1.0 - d) * (lambda_max - lambda_min)
        }
    }
}

fn write_phase_c_baseline_json(metrics: &LambdaMetrics, seed: u64, cases: usize) {
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("report");
    if std::fs::create_dir_all(&out_dir).is_err() {
        return;
    }

    let payload = serde_json::json!({
        "phase": "C'",
        "seed": seed,
        "cases": cases,
        "lambda": metrics.lambda,
        "kappa": 0.8,
        "alpha": 3.0,
        "beta": 3.0,
        "accepted_rate": metrics.accepted_rate,
        "accepted_rate_old_and": metrics.accepted_rate_old_and,
        "avg_delta_consistency": metrics.avg_delta_consistency,
        "avg_delta_propagation_cost_raw": metrics.avg_delta_prop_raw,
        "avg_delta_cyclic_penalty_raw": metrics.avg_delta_cyc_raw,
        "score_mean": metrics.score_mean,
        "score_variance": metrics.score_variance,
        "stability_index": metrics.stability_index,
        "corr_prop_density": metrics.corr_prop_density,
        "partial_corr_gap_prop_given_density": metrics.partial_corr_gap_prop_given_density,
    });

    let path = out_dir.join("phase_c_baseline.json");
    let serialized = match serde_json::to_string_pretty(&payload) {
        Ok(s) => s,
        Err(_) => return,
    };
    let _ = std::fs::write(path, serialized);
}

pub fn phase_c_baseline_regression_guards() {
    let cases = generate_diverse_cases(42, 120);
    let m = evaluate_lambda_metrics(&cases, 0.60, 0.8, 3.0, 3.0, 1.0);

    assert!(m.accepted_rate >= 0.30, "accepted_rate regression");
    assert!(
        m.partial_corr_gap_prop_given_density <= 0.40,
        "partial correlation regression"
    );
}

fn estimate_candidate_count(state: &AppState) -> usize {
    build_candidate_diffs(state).len()
}

fn build_candidate_diffs(state: &AppState) -> Vec<ProposedDiff> {
    let eval = state.evaluation.clone();
    let mut candidates = build_single_step_candidates(state, &eval);
    if let Some(two_step) = build_best_two_step_candidate(state, &eval, &candidates) {
        candidates.push(two_step);
    }
    candidates
}

fn build_single_step_candidates(
    state: &AppState,
    eval: &agent_core::domain::DesignScoreVector,
) -> Vec<ProposedDiff> {
    let mut candidates = Vec::new();

    if eval.consistency < 80 {
        for (key, value) in &state.uds.nodes {
            if value.trim().is_empty() {
                candidates.push(ProposedDiff::UpsertNode {
                    key: key.clone(),
                    value: "auto-filled".to_string(),
                });
                if state.uds.nodes.len() > 1 {
                    candidates.push(ProposedDiff::RemoveNode { key: key.clone() });
                }
            }
        }
    }

    if eval.structural_integrity < 75 {
        for key in state.uds.dependencies.keys() {
            if !state.uds.nodes.contains_key(key) {
                candidates.push(ProposedDiff::RemoveDependencies { key: key.clone() });
            }
        }
    }

    if eval.dependency_soundness < 85 {
        for (key, deps) in &state.uds.dependencies {
            let filtered = deps
                .iter()
                .filter(|dep| *dep != key && state.uds.nodes.contains_key(*dep))
                .cloned()
                .collect::<Vec<_>>();
            if &filtered != deps {
                candidates.push(ProposedDiff::SetDependencies {
                    key: key.clone(),
                    dependencies: filtered,
                });
            }
        }
    }

    for key in split_candidate_keys(state) {
        let diff = ProposedDiff::SplitHighOutDegreeNode { key };
        if split_preview_passes_guard(state, eval, &diff) {
            candidates.push(diff);
        }
    }

    for diff in rewire_candidate_diffs(state) {
        if split_preview_passes_guard(state, eval, &diff) {
            candidates.push(diff);
        }
    }

    candidates
}

fn build_best_two_step_candidate(
    state: &AppState,
    _baseline_eval: &agent_core::domain::DesignScoreVector,
    first_step_candidates: &[ProposedDiff],
) -> Option<ProposedDiff> {
    const TOP_K: usize = 3;

    let mut first_scored = first_step_candidates
        .iter()
        .filter_map(|diff| state.evaluate_diff(diff).map(|r| (diff.clone(), r.score)))
        .collect::<Vec<_>>();
    if first_scored.is_empty() {
        return None;
    }
    first_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let best_single = first_scored[0].1;

    let mut best_two_step: Option<(ProposedDiff, f64)> = None;
    for (first_diff, _) in first_scored.into_iter().take(TOP_K) {
        let mut first_state = state.clone();
        first_state.begin_tx().ok()?;
        if first_state.apply_diff(first_diff.clone()).is_err() {
            continue;
        }
        first_state.commit_tx().ok()?;
        let second_candidates = build_single_step_candidates(&first_state, &first_state.evaluation);

        for second_diff in second_candidates {
            let mut two_step_state = first_state.clone();
            two_step_state.begin_tx().ok()?;
            if two_step_state.apply_diff(second_diff.clone()).is_err() {
                continue;
            }
            two_step_state.commit_tx().ok()?;

            let candidate = ProposedDiff::TwoStep {
                first: Box::new(first_diff.clone()),
                second: Box::new(second_diff),
            };
            let score = state.evaluate_diff(&candidate).map(|r| r.score).unwrap_or(0.0);
            if score <= 0.0 {
                continue;
            }

            match &best_two_step {
                Some((_, best)) if *best >= score => {}
                _ => best_two_step = Some((candidate, score)),
            }
        }
    }

    match best_two_step {
        Some((candidate, score)) if score > best_single => Some(candidate),
        _ => None,
    }
}

fn split_preview_passes_guard(
    state: &AppState,
    _baseline_eval: &agent_core::domain::DesignScoreVector,
    diff: &ProposedDiff,
) -> bool {
    state.evaluate_diff(diff).map(|r| r.accepted).unwrap_or(false)
}

fn diff_contains_rewire(diff: &ProposedDiff) -> bool {
    match diff {
        ProposedDiff::SetDependencies { .. } | ProposedDiff::RewireHighImpactEdge { .. } => true,
        ProposedDiff::TwoStep { first, second } => {
            diff_contains_rewire(first) || diff_contains_rewire(second)
        }
        _ => false,
    }
}

fn diff_kind(diff: &ProposedDiff) -> &'static str {
    match diff {
        ProposedDiff::SplitHighOutDegreeNode { .. } => "Split",
        ProposedDiff::RemoveNode { .. } => "RemoveNode",
        ProposedDiff::SetDependencies { .. } | ProposedDiff::RewireHighImpactEdge { .. } => "Rewire",
        ProposedDiff::TwoStep { .. } => "Other",
        _ => "Other",
    }
}

fn split_candidate_keys(state: &AppState) -> Vec<String> {
    const SPLIT_OUT_DEGREE_MIN: usize = 3;
    const IMPACT_TOP_PERCENTILE: f64 = 0.30;
    const LAMBDA: f64 = 0.60;

    let keys = state.uds.nodes.keys().cloned().collect::<Vec<_>>();
    if keys.is_empty() {
        return Vec::new();
    }

    let index = keys
        .iter()
        .enumerate()
        .map(|(idx, key)| (key.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut adjacency = vec![Vec::<usize>::new(); keys.len()];
    for (owner, deps) in &state.uds.dependencies {
        let Some(&from) = index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = index.get(dep) else {
                continue;
            };
            if from != to {
                adjacency[from].push(to);
            }
        }
    }
    for edges in &mut adjacency {
        edges.sort_unstable();
        edges.dedup();
    }

    let mut scored = Vec::new();
    for key in &keys {
        let out_degree = state
            .uds
            .dependencies
            .get(key)
            .map(|deps| {
                let mut d = deps.clone();
                d.sort();
                d.dedup();
                d.len()
            })
            .unwrap_or(0);
        if out_degree < SPLIT_OUT_DEGREE_MIN {
            continue;
        }

        let Some(&idx) = index.get(key) else {
            continue;
        };
        let impact = propagation_sum_from(idx, &adjacency, LAMBDA);
        scored.push((key.clone(), impact));
    }

    if scored.is_empty() {
        return Vec::new();
    }

    let mut impact_values = scored.iter().map(|(_, impact)| *impact).collect::<Vec<_>>();
    impact_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((impact_values.len() as f64) * (1.0 - IMPACT_TOP_PERCENTILE)).floor() as usize;
    let threshold = impact_values[rank.min(impact_values.len().saturating_sub(1))];

    scored
        .into_iter()
        .filter(|(_, impact)| *impact >= threshold)
        .map(|(key, _)| key)
        .collect::<Vec<_>>()
}

fn rewire_candidate_diffs(state: &AppState) -> Vec<ProposedDiff> {
    const EDGE_TOP_PERCENTILE: f64 = 0.30;
    const LAMBDA: f64 = 0.60;

    let keys = state.uds.nodes.keys().cloned().collect::<Vec<_>>();
    if keys.len() < 3 {
        return Vec::new();
    }

    let index = keys
        .iter()
        .enumerate()
        .map(|(idx, key)| (key.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut adjacency = vec![Vec::<usize>::new(); keys.len()];
    let mut indegree = vec![0_usize; keys.len()];
    for (owner, deps) in &state.uds.dependencies {
        let Some(&from) = index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = index.get(dep) else {
                continue;
            };
            if from == to {
                continue;
            }
            adjacency[from].push(to);
        }
    }
    for edges in &mut adjacency {
        edges.sort_unstable();
        edges.dedup();
        for &to in edges.iter() {
            indegree[to] = indegree[to].saturating_add(1);
        }
    }

    let mut node_impact = vec![0.0_f64; keys.len()];
    for (i, impact) in node_impact.iter_mut().enumerate() {
        *impact = propagation_sum_from(i, &adjacency, LAMBDA);
    }

    let mut edge_scores = Vec::<(usize, usize, f64)>::new();
    for (from, tos) in adjacency.iter().enumerate() {
        for &to in tos {
            let score = node_impact[from] * (1.0 + indegree[to] as f64);
            edge_scores.push((from, to, score));
        }
    }
    if edge_scores.is_empty() {
        return Vec::new();
    }

    let mut score_values = edge_scores.iter().map(|(_, _, s)| *s).collect::<Vec<_>>();
    score_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((score_values.len() as f64) * (1.0 - EDGE_TOP_PERCENTILE)).floor() as usize;
    let threshold = score_values[rank.min(score_values.len().saturating_sub(1))];

    let mut candidates = Vec::new();
    for (from, to, score) in edge_scores {
        if score < threshold {
            continue;
        }
        let owner = &keys[from];
        let old_dep = &keys[to];

        let mut best: Option<(String, f64)> = None;
        for w_idx in 0..keys.len() {
            if w_idx == from || w_idx == to {
                continue;
            }
            let w = &keys[w_idx];
            if adjacency[from].contains(&w_idx) {
                continue;
            }
            let dist = shortest_distance(from, w_idx, &adjacency).unwrap_or(keys.len() + 1);
            let indegree_gain = indegree[to] as f64 - indegree[w_idx] as f64;
            let rewiring_score = (dist as f64) + indegree_gain;
            match &best {
                Some((_, best_score)) if *best_score >= rewiring_score => {}
                _ => best = Some((w.clone(), rewiring_score)),
            }
        }

        if let Some((new_dep, _)) = best {
            candidates.push(ProposedDiff::RewireHighImpactEdge {
                key: owner.clone(),
                from: old_dep.clone(),
                to: new_dep,
            });
        }
    }

    candidates
}

#[derive(Clone)]
struct BenchmarkCase {
    state: AppState,
    pattern: Pattern,
    node_count: usize,
    density: f64,
}

fn generate_diverse_cases(seed: u64, count: usize) -> Vec<BenchmarkCase> {
    let mut rng = LcgRng::new(seed);
    let patterns = [
        Pattern::SparseDag,
        Pattern::DenseGraph,
        Pattern::Chain,
        Pattern::Star,
        Pattern::Random,
    ];

    let mut cases = Vec::with_capacity(count);
    for i in 0..count {
        let pattern = patterns[i % patterns.len()];
        let node_count = rng.gen_usize(3, 20);
        let density = rng.gen_f64(0.1, 0.8);
        let empty_ratio = rng.gen_f64(0.0, 0.5);

        let uds = build_random_uds(pattern, node_count, density, empty_ratio, &mut rng);
        cases.push(BenchmarkCase {
            state: AppState::new(uds),
            pattern,
            node_count,
            density,
        });
    }

    cases
}

#[derive(Clone, Copy)]
enum Pattern {
    SparseDag,
    DenseGraph,
    Chain,
    Star,
    Random,
}

fn compute_propagation_cost(uds: &UnifiedDesignState) -> f64 {
    compute_propagation_cost_new(uds, 0.60, 0.8)
}

fn compute_propagation_cost_new(uds: &UnifiedDesignState, lambda: f64, kappa: f64) -> f64 {
    let n = uds.nodes.len();
    if n <= 1 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let key_index = keys
        .iter()
        .enumerate()
        .map(|(idx, k)| (k.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut adj = vec![Vec::<usize>::new(); n];
    for (owner, deps) in &uds.dependencies {
        let Some(&from) = key_index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = key_index.get(dep) else {
                continue;
            };
            if from == to {
                continue;
            }
            adj[from].push(to);
        }
    }

    for edges in &mut adj {
        edges.sort_unstable();
        edges.dedup();
    }

    let mut total_impact = 0.0_f64;
    for start in 0..n {
        let s = propagation_sum_from(start, &adj, lambda);
        total_impact += (kappa * s).ln_1p();
    }

    let normalizer = ((n - 1) as f64 * (1.0 + kappa).ln()).max(1e-12);
    (total_impact / (n as f64 * normalizer)).clamp(0.0, 1.0)
}

fn compute_propagation_cost_old(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n <= 1 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let key_index = keys
        .iter()
        .enumerate()
        .map(|(idx, k)| (k.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut adj = vec![Vec::<usize>::new(); n];
    for (owner, deps) in &uds.dependencies {
        let Some(&from) = key_index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = key_index.get(dep) else {
                continue;
            };
            if from == to {
                continue;
            }
            adj[from].push(to);
        }
    }

    for edges in &mut adj {
        edges.sort_unstable();
        edges.dedup();
    }
    let edge_count = adj.iter().map(|edges| edges.len()).sum::<usize>();
    let max_edges = n * (n - 1);
    let density = if max_edges == 0 {
        0.0
    } else {
        edge_count as f64 / max_edges as f64
    };

    let mut reachable_pairs = 0_usize;
    for start in 0..n {
        let mut stack = vec![start];
        let mut visited = vec![false; n];
        visited[start] = true;
        while let Some(cur) = stack.pop() {
            for &next in &adj[cur] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
        reachable_pairs += visited
            .iter()
            .enumerate()
            .filter(|(idx, v)| *idx != start && **v)
            .count();
    }
    let reachability = reachable_pairs as f64 / max_edges as f64;

    (0.5 * density + 0.5 * reachability).clamp(0.0, 1.0)
}

fn propagation_sum_from(start: usize, adj: &[Vec<usize>], lambda: f64) -> f64 {
    let n = adj.len();
    if n <= 1 {
        return 0.0;
    }

    let mut dist = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    dist[start] = 0;
    queue.push_back(start);

    while let Some(cur) = queue.pop_front() {
        let next_dist = dist[cur].saturating_add(1);
        for &next in &adj[cur] {
            if dist[next] == usize::MAX {
                dist[next] = next_dist;
                queue.push_back(next);
            }
        }
    }

    let mut sum = 0.0_f64;
    for (idx, d) in dist.iter().enumerate() {
        if idx == start || *d == usize::MAX {
            continue;
        }
        sum += lambda.powi(*d as i32);
    }
    sum
}

fn shortest_distance(start: usize, goal: usize, adj: &[Vec<usize>]) -> Option<usize> {
    if start == goal {
        return Some(0);
    }
    let mut dist = vec![usize::MAX; adj.len()];
    let mut queue = std::collections::VecDeque::new();
    dist[start] = 0;
    queue.push_back(start);
    while let Some(cur) = queue.pop_front() {
        let next_dist = dist[cur].saturating_add(1);
        for &next in &adj[cur] {
            if dist[next] == usize::MAX {
                dist[next] = next_dist;
                if next == goal {
                    return Some(next_dist);
                }
                queue.push_back(next);
            }
        }
    }
    None
}

fn compute_graph_density(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n <= 1 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let key_index = keys
        .iter()
        .enumerate()
        .map(|(idx, k)| (k.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut adj = vec![Vec::<usize>::new(); n];
    for (owner, deps) in &uds.dependencies {
        let Some(&from) = key_index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = key_index.get(dep) else {
                continue;
            };
            if from != to {
                adj[from].push(to);
            }
        }
    }
    for edges in &mut adj {
        edges.sort_unstable();
        edges.dedup();
    }

    let edge_count = adj.iter().map(|edges| edges.len()).sum::<usize>();
    let max_edges = n * (n - 1);
    if max_edges == 0 {
        0.0
    } else {
        edge_count as f64 / max_edges as f64
    }
}

fn compute_spectral_gap(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n <= 1 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let key_index = keys
        .iter()
        .enumerate()
        .map(|(idx, k)| (k.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut a = vec![vec![0.0_f64; n]; n];
    for (owner, deps) in &uds.dependencies {
        let Some(&i) = key_index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&j) = key_index.get(dep) else {
                continue;
            };
            if i != j {
                a[i][j] = 1.0;
            }
        }
    }

    let mut a_sym = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in 0..n {
            a_sym[i][j] = 0.5 * (a[i][j] + a[j][i]);
        }
    }

    let mut d = vec![0.0_f64; n];
    for i in 0..n {
        d[i] = a_sym[i].iter().sum::<f64>();
    }

    let mut l = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i == j {
                l[i][j] = if d[i] > 0.0 { 1.0 } else { 0.0 };
            } else if d[i] > 0.0 && d[j] > 0.0 {
                let v = a_sym[i][j] / (d[i].sqrt() * d[j].sqrt());
                l[i][j] = -v;
            }
        }
    }

    let mut eigenvalues = jacobi_eigenvalues(l);
    eigenvalues.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if eigenvalues.len() < 2 {
        return 0.0;
    }
    let lambda2 = eigenvalues[1].max(0.0);
    let density = compute_graph_density(uds);
    (lambda2 / (1.0 + 3.0 * density)).clamp(0.0, 2.0)
}

fn compute_diffusion_index_spectral_gap(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n <= 1 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let key_index = keys
        .iter()
        .enumerate()
        .map(|(idx, k)| (k.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut a = vec![vec![0.0_f64; n]; n];
    for (owner, deps) in &uds.dependencies {
        let Some(&i) = key_index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&j) = key_index.get(dep) else {
                continue;
            };
            if i != j {
                a[i][j] += 1.0;
            }
        }
    }

    let mut a_sym = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in 0..n {
            a_sym[i][j] = 0.5 * (a[i][j] + a[j][i]);
        }
    }

    let mut d = vec![0.0_f64; n];
    for i in 0..n {
        d[i] = a_sym[i].iter().sum::<f64>();
    }

    let mut l = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i == j {
                l[i][j] = if d[i] > 0.0 { 1.0 } else { 0.0 };
            } else if d[i] > 0.0 && d[j] > 0.0 {
                l[i][j] = -a_sym[i][j] / (d[i].sqrt() * d[j].sqrt());
            }
        }
    }

    let mut eigenvalues = jacobi_eigenvalues(l);
    if eigenvalues.is_empty() {
        return 0.0;
    }
    eigenvalues.retain(|v| v.is_finite());
    if eigenvalues.len() < 2 {
        return 0.0;
    }
    eigenvalues.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lambda2 = eigenvalues[1];
    if !lambda2.is_finite() || lambda2 < 0.0 {
        0.0
    } else {
        lambda2
    }
}

fn jacobi_eigenvalues(mut a: Vec<Vec<f64>>) -> Vec<f64> {
    let n = a.len();
    if n == 0 {
        return Vec::new();
    }

    let eps = 1e-10_f64;
    let max_iter = n * n * 25;

    for _ in 0..max_iter {
        let mut p = 0usize;
        let mut q = 1usize.min(n - 1);
        let mut max_val = 0.0_f64;

        for (i, row) in a.iter().enumerate() {
            for (j, value) in row.iter().enumerate().skip(i + 1) {
                let v = value.abs();
                if v > max_val {
                    max_val = v;
                    p = i;
                    q = j;
                }
            }
        }

        if max_val < eps {
            break;
        }

        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        if apq.abs() < eps {
            continue;
        }

        let tau = (aqq - app) / (2.0 * apq);
        let t = if tau >= 0.0 {
            1.0 / (tau + (1.0 + tau * tau).sqrt())
        } else {
            -1.0 / (-tau + (1.0 + tau * tau).sqrt())
        };
        let c = 1.0 / (1.0 + t * t).sqrt();
        let s = t * c;

        let mut k = 0usize;
        while k < n {
            if k == p || k == q {
                k += 1;
                continue;
            }
            let aik = a[p][k];
            let akq = a[q][k];
            a[p][k] = c * aik - s * akq;
            a[k][p] = a[p][k];
            a[q][k] = s * aik + c * akq;
            a[k][q] = a[q][k];
            k += 1;
        }

        a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;
    }

    let mut vals = vec![0.0_f64; n];
    for i in 0..n {
        vals[i] = a[i][i];
    }
    vals
}

fn compute_cyclic_penalty(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n == 0 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let key_index = keys
        .iter()
        .enumerate()
        .map(|(idx, k)| (k.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut adj = vec![Vec::<usize>::new(); n];
    let mut self_loops = vec![false; n];
    for (owner, deps) in &uds.dependencies {
        let Some(&from) = key_index.get(owner) else {
            continue;
        };
        for dep in deps {
            if dep == owner {
                self_loops[from] = true;
            }
            let Some(&to) = key_index.get(dep) else {
                continue;
            };
            if from != to {
                adj[from].push(to);
            }
        }
    }
    for edges in &mut adj {
        edges.sort_unstable();
        edges.dedup();
    }
    let edge_count = adj.iter().map(|edges| edges.len()).sum::<usize>();
    let max_edges = n * (n - 1);
    let graph_density = if max_edges == 0 {
        0.0
    } else {
        edge_count as f64 / max_edges as f64
    };

    let sccs = tarjan_scc(&adj);
    let mut penalty = 0.0_f64;
    for component in sccs {
        if component.len() > 1 {
            penalty += (1.0 / component.len() as f64) * internal_cycle_intensity(&component, &adj);
        } else if self_loops[component[0]] {
            penalty += 1.0;
        }
    }

    let normalized_cycle = (penalty / n as f64).clamp(0.0, 1.0);
    (0.7 * normalized_cycle + 0.3 * graph_density).clamp(0.0, 1.0)
}

fn modularity_score(uds: &UnifiedDesignState) -> f64 {
    let n = uds.nodes.len();
    if n == 0 {
        return 0.0;
    }

    let keys = uds.nodes.keys().cloned().collect::<Vec<_>>();
    let key_index = keys
        .iter()
        .enumerate()
        .map(|(idx, k)| (k.clone(), idx))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut adj = vec![Vec::<usize>::new(); n];
    for (owner, deps) in &uds.dependencies {
        let Some(&from) = key_index.get(owner) else {
            continue;
        };
        for dep in deps {
            let Some(&to) = key_index.get(dep) else {
                continue;
            };
            if from != to {
                adj[from].push(to);
            }
        }
    }
    for edges in &mut adj {
        edges.sort_unstable();
        edges.dedup();
    }

    let sccs = tarjan_scc(&adj);
    let mut cluster_of = vec![usize::MAX; n];
    for (cid, comp) in sccs.iter().enumerate() {
        for &v in comp {
            cluster_of[v] = cid;
        }
    }

    let mut cohesion_sum = 0.0_f64;
    for from in 0..n {
        let mut cross = 0_usize;
        for &to in &adj[from] {
            if cluster_of[from] != cluster_of[to] {
                cross += 1;
            }
        }
        cohesion_sum += 1.0 / (1.0 + cross as f64);
    }

    (cohesion_sum / n as f64).clamp(0.0, 1.0)
}

fn internal_cycle_intensity(component: &[usize], adj: &[Vec<usize>]) -> f64 {
    if component.len() <= 1 {
        return 0.0;
    }

    let mut in_component = vec![false; adj.len()];
    for &idx in component {
        in_component[idx] = true;
    }

    let mut edge_count = 0_usize;
    for &from in component {
        for &to in &adj[from] {
            if in_component[to] {
                edge_count += 1;
            }
        }
    }

    let size = component.len();
    if size == 0 {
        0.0
    } else {
        edge_count as f64 / size as f64
    }
}

fn tarjan_scc(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Tarjan<'a> {
        adj: &'a [Vec<usize>],
        index: usize,
        indices: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
    }

    impl<'a> Tarjan<'a> {
        fn new(adj: &'a [Vec<usize>]) -> Self {
            let n = adj.len();
            Self {
                adj,
                index: 0,
                indices: vec![None; n],
                lowlink: vec![0; n],
                stack: Vec::new(),
                on_stack: vec![false; n],
                components: Vec::new(),
            }
        }

        fn run(mut self) -> Vec<Vec<usize>> {
            for v in 0..self.adj.len() {
                if self.indices[v].is_none() {
                    self.strong_connect(v);
                }
            }
            self.components
        }

        fn strong_connect(&mut self, v: usize) {
            let v_index = self.index;
            self.indices[v] = Some(v_index);
            self.lowlink[v] = v_index;
            self.index += 1;
            self.stack.push(v);
            self.on_stack[v] = true;

            for &w in &self.adj[v] {
                if self.indices[w].is_none() {
                    self.strong_connect(w);
                    self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
                } else if self.on_stack[w] {
                    let w_index = self.indices[w].unwrap_or(v_index);
                    self.lowlink[v] = self.lowlink[v].min(w_index);
                }
            }

            if self.lowlink[v] == v_index {
                let mut component = Vec::new();
                loop {
                    let w = self.stack.pop().expect("tarjan stack should not be empty");
                    self.on_stack[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }

    Tarjan::new(adj).run()
}

fn build_random_uds(
    pattern: Pattern,
    node_count: usize,
    density: f64,
    empty_ratio: f64,
    rng: &mut LcgRng,
) -> UnifiedDesignState {
    let mut uds = UnifiedDesignState::default();
    let keys = (0..node_count)
        .map(|i| format!("N{i}"))
        .collect::<Vec<_>>();

    for key in &keys {
        let value = if rng.gen_bool(empty_ratio.clamp(0.0, 1.0)) {
            if rng.gen_bool(0.5) {
                String::new()
            } else {
                " ".to_string()
            }
        } else {
            format!("value_{key}")
        };
        uds.nodes.insert(key.clone(), value);
    }

    match pattern {
        Pattern::SparseDag => {
            for i in 0..node_count {
                for j in (i + 1)..node_count {
                    if rng.gen_bool((density * 0.5).clamp(0.0, 1.0)) {
                        uds.dependencies
                            .entry(keys[i].clone())
                            .or_default()
                            .push(keys[j].clone());
                    }
                }
            }
        }
        Pattern::DenseGraph => {
            for i in 0..node_count {
                for j in 0..node_count {
                    if i == j {
                        continue;
                    }
                    if rng.gen_bool((density + 0.2).clamp(0.0, 1.0)) {
                        uds.dependencies
                            .entry(keys[i].clone())
                            .or_default()
                            .push(keys[j].clone());
                    }
                }
            }
        }
        Pattern::Chain => {
            for i in 0..(node_count.saturating_sub(1)) {
                uds.dependencies
                    .entry(keys[i].clone())
                    .or_default()
                    .push(keys[i + 1].clone());
                if rng.gen_bool((density * 0.3).clamp(0.0, 1.0)) {
                    uds.dependencies
                        .entry(keys[i + 1].clone())
                        .or_default()
                        .push(keys[i].clone());
                }
            }
        }
        Pattern::Star => {
            let hub = 0;
            for i in 1..node_count {
                if rng.gen_bool((density + 0.1).clamp(0.0, 1.0)) {
                    uds.dependencies
                        .entry(keys[hub].clone())
                        .or_default()
                        .push(keys[i].clone());
                }
                if rng.gen_bool((density * 0.4).clamp(0.0, 1.0)) {
                    uds.dependencies
                        .entry(keys[i].clone())
                        .or_default()
                        .push(keys[hub].clone());
                }
            }
        }
        Pattern::Random => {
            for i in 0..node_count {
                for j in 0..node_count {
                    if i == j {
                        continue;
                    }
                    if rng.gen_bool(density.clamp(0.0, 1.0)) {
                        uds.dependencies
                            .entry(keys[i].clone())
                            .or_default()
                            .push(keys[j].clone());
                    }
                }
            }
        }
    }

    // Inject invalid dependencies with density-coupled probability.
    let missing_dep_prob = (0.03 + 0.20 * density).clamp(0.0, 1.0);
    let self_loop_prob = (0.05 + 0.80 * density).clamp(0.0, 1.0);
    for key in &keys {
        if rng.gen_bool(missing_dep_prob) {
            uds.dependencies
                .entry(key.clone())
                .or_default()
                .push(format!("MISSING_{}", rng.gen_usize(0, 999)));
        }
        if rng.gen_bool(self_loop_prob) {
            uds.dependencies
                .entry(key.clone())
                .or_default()
                .push(key.clone());
        }
    }

    // Inject density-coupled two-node cycles to make cyclic penalty distribution realistic.
    if node_count >= 2 {
        let pair_cycle_prob = (0.02 + 0.55 * density).clamp(0.0, 1.0);
        for i in 0..node_count {
            for j in (i + 1)..node_count {
                if rng.gen_bool(pair_cycle_prob) {
                    uds.dependencies
                        .entry(keys[i].clone())
                        .or_default()
                        .push(keys[j].clone());
                    uds.dependencies
                        .entry(keys[j].clone())
                        .or_default()
                        .push(keys[i].clone());
                }
            }
        }
    }

    // Ensure density-linked baseline cyclicity so cyclic_penalty correlates with density.
    let forced_self_loops = ((density.clamp(0.0, 1.0) * node_count as f64).round() as usize)
        .min(node_count);
    for key in keys.iter().take(forced_self_loops) {
        uds.dependencies
            .entry(key.clone())
            .or_default()
            .push(key.clone());
    }

    // Inject invalid owner dependency keys with density-coupled probability.
    if rng.gen_bool((0.05 + 0.25 * density).clamp(0.0, 1.0)) {
        uds.dependencies
            .insert(
                format!("MISSING_OWNER_{}", rng.gen_usize(0, 999)),
                vec![keys[0].clone()],
            );
    }

    uds
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().sum::<f64>() / v.len() as f64
}

fn delta_vector_norm(v: &DeltaVector) -> f64 {
    (v.d_consistency.powi(2)
        + v.d_prop_quality.powi(2)
        + v.d_cycle_quality.powi(2)
        + v.d_modularity.powi(2))
    .sqrt()
}

fn min(v: &[f64]) -> f64 {
    v.iter().cloned().reduce(f64::min).unwrap_or(0.0)
}

fn max(v: &[f64]) -> f64 {
    v.iter().cloned().reduce(f64::max).unwrap_or(0.0)
}

fn stddev(v: &[f64]) -> f64 {
    if v.len() <= 1 {
        return 0.0;
    }
    variance(v).sqrt()
}

fn variance(v: &[f64]) -> f64 {
    if v.len() <= 1 {
        return 0.0;
    }
    let m = mean(v);
    v.iter()
        .map(|x| {
            let d = *x - m;
            d * d
        })
        .sum::<f64>()
        / v.len() as f64
}

fn median(v: &[f64]) -> f64 {
    percentile(v, 0.5)
}

fn percentile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut sorted = v.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pp = p.clamp(0.0, 1.0);
    let idx = ((sorted.len().saturating_sub(1)) as f64 * pp).round() as usize;
    sorted[idx.min(sorted.len().saturating_sub(1))]
}

fn ratio_percent(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        (num as f64 / den as f64) * 100.0
    }
}

fn correlation(xs: &[f64], ys: &[f64]) -> f64 {
    if xs.len() != ys.len() || xs.len() < 2 {
        return 0.0;
    }
    let mx = mean(xs);
    let my = mean(ys);
    let mut cov = 0.0;
    let mut vx = 0.0;
    let mut vy = 0.0;

    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = *x - mx;
        let dy = *y - my;
        cov += dx * dy;
        vx += dx * dx;
        vy += dy * dy;
    }

    let denom = (vx * vy).sqrt();
    if denom == 0.0 {
        0.0
    } else {
        cov / denom
    }
}

fn partial_correlation_single_control(x: &[f64], y: &[f64], z: &[f64]) -> f64 {
    if x.len() != y.len() || y.len() != z.len() || x.len() < 2 {
        return 0.0;
    }
    let x_res = residualize_linear(x, z);
    let y_res = residualize_linear(y, z);
    correlation(&x_res, &y_res)
}

fn residualize_linear(target: &[f64], control: &[f64]) -> Vec<f64> {
    if target.len() != control.len() || target.is_empty() {
        return Vec::new();
    }

    let mx = mean(control);
    let my = mean(target);
    let mut var_x = 0.0_f64;
    let mut cov_xy = 0.0_f64;
    for (x, y) in control.iter().zip(target.iter()) {
        let dx = *x - mx;
        var_x += dx * dx;
        cov_xy += dx * (*y - my);
    }

    let slope = if var_x == 0.0 { 0.0 } else { cov_xy / var_x };
    let intercept = my - slope * mx;

    target
        .iter()
        .zip(control.iter())
        .map(|(y, x)| y - (intercept + slope * x))
        .collect::<Vec<_>>()
}

struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        self.state
    }

    fn gen_f64(&mut self, min: f64, max: f64) -> f64 {
        let unit = (self.next_u64() as f64) / (u64::MAX as f64);
        min + unit * (max - min)
    }

    fn gen_usize(&mut self, min: usize, max: usize) -> usize {
        if max <= min {
            return min;
        }
        let span = max - min + 1;
        min + (self.next_u64() as usize % span)
    }

    fn gen_bool(&mut self, p: f64) -> bool {
        self.gen_f64(0.0, 1.0) < p
    }
}
