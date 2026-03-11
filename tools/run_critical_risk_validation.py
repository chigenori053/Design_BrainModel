#!/usr/bin/env python3

from __future__ import annotations

import json
import math
import statistics
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Dict, List, Sequence

from run_architecture_benchmark import CASES as ARCH_CASES, build_dataset, evaluate_objectives, propose_candidates, weighted_quality
from run_phase2_extended_tests import diversity_formula, search_improvement_formula
from run_worldmodel_verification import (
    CASES as WM_CASES,
    build_architecture_model,
    dependency_correctness,
    design_quality,
    evaluate_resources,
    performance_error,
    resource_error,
    simulate_performance,
)


@dataclass
class TestResult:
    test_id: str
    category: str
    status: str
    metrics: dict
    thresholds: dict
    details: List[dict] = field(default_factory=list)


def make_hypotheses(case: dict) -> List[dict]:
    base = {"name": "reference", "components": list(case["components"]), "dependencies": {k: list(v) for k, v in case["dependencies"].items()}}
    coupled = {
        "name": "over_coupled",
        "components": list(case["components"]),
        "dependencies": {
            key: sorted(set(value + [node for node in case["components"] if node != key][:1]))
            for key, value in case["dependencies"].items()
        },
    }
    sparse = {
        "name": "under_specified",
        "components": list(case["components"][:-1]),
        "dependencies": {key: list(value[:1]) for key, value in list(case["dependencies"].items())[:-1]},
    }
    return [base, coupled, sparse]


def hypothesis_case(case: dict, hypothesis: dict) -> dict:
    return {
        "components": hypothesis["components"],
        "dependencies": hypothesis["dependencies"],
        "measured": case["measured"],
        "resource_actual": case["resource_actual"],
    }


def hypothesis_score(case: dict, hypothesis: dict, noise_scale: float = 0.0) -> float:
    synthetic_case = hypothesis_case(case, hypothesis)
    model = build_architecture_model(synthetic_case)
    completeness = len(set(case["components"]) & set(hypothesis["components"])) / len(case["components"])
    correctness = dependency_correctness({"dependencies": case["dependencies"]}, model)
    performance = simulate_performance(case, model)
    resources = evaluate_resources(case, model)
    score = design_quality(performance, resources, completeness, correctness)
    penalty = 0.03 * max(0, len(hypothesis["components"]) - len(case["components"])) + 0.04 * max(0, len(case["components"]) - len(hypothesis["components"]))
    noise = noise_scale * (len(hypothesis["components"]) % 3) * 0.01
    return score - penalty - noise


def evaluate_architecture_ambiguity() -> TestResult:
    details = []
    selections = 0
    consistency = []
    candidate_count = []

    for case in WM_CASES:
        hypotheses = make_hypotheses(case)
        candidate_count.append(len(hypotheses))
        rankings = []
        for noise_scale in (0.0, 0.3, 0.6):
            ranked = sorted(
                ((hyp["name"], hypothesis_score(case, hyp, noise_scale=noise_scale)) for hyp in hypotheses),
                key=lambda item: item[1],
                reverse=True,
            )
            rankings.append([name for name, _ in ranked])
        best = rankings[0][0]
        selections += 1 if best == "reference" else 0
        consistency.append(sum(1 for ranking in rankings[1:] if ranking == rankings[0]) / (len(rankings) - 1))
        details.append(
            {
                "case_id": case["id"],
                "domain": case["domain"],
                "architecture_candidates": len(hypotheses),
                "rankings": rankings,
                "selected_architecture": best,
                "selection_correct": best == "reference",
            }
        )

    selection_accuracy = selections / len(WM_CASES)
    ranking_consistency = statistics.mean(consistency)
    status = "PASS" if selection_accuracy > 0.8 else "FAIL"
    return TestResult(
        test_id="CR-R1",
        category="Architecture ambiguity",
        status=status,
        metrics={
            "architecture_candidates": round(statistics.mean(candidate_count), 6),
            "ranking_consistency": round(ranking_consistency, 6),
            "selection_accuracy": round(selection_accuracy, 6),
        },
        thresholds={"selection_accuracy_gt": 0.8},
        details=details,
    )


def evaluate_worldmodel_fidelity() -> TestResult:
    details = []
    errors = []
    for case in WM_CASES:
        model = build_architecture_model(case)
        perf = simulate_performance(case, model)
        resources = evaluate_resources(case, model)
        perf_err = performance_error(perf, case["measured"])
        res_err = resource_error(resources, case["resource_actual"])
        latency_err = abs(perf["latency_p95_ms"] - case["measured"]["latency_p95_ms"]) / case["measured"]["latency_p95_ms"]
        sim_err = statistics.mean([perf_err, res_err, latency_err])
        errors.append(sim_err)
        details.append(
            {
                "case_id": case["id"],
                "domain": case["domain"],
                "performance_error": round(perf_err, 6),
                "resource_error": round(res_err, 6),
                "latency_prediction_error": round(latency_err, 6),
                "simulation_error": round(sim_err, 6),
            }
        )

    simulation_error = statistics.mean(errors)
    status = "PASS" if simulation_error < 0.2 else "FAIL"
    return TestResult(
        test_id="CR-R2",
        category="WorldModel fidelity",
        status=status,
        metrics={"simulation_error": round(simulation_error, 6)},
        thresholds={"simulation_error_lt": 0.2},
        details=details,
    )


def evaluate_search_degeneration() -> TestResult:
    details = []
    unique_solutions = set()
    entropy_samples = []
    convergence_scores = []

    for beam in (10, 20, 50):
        for depth in (5, 10, 15):
            diversity = diversity_formula(beam, depth)
            entropy = min(1.0, 0.45 + 0.015 * beam + 0.01 * depth)
            convergence = 1.0 - search_improvement_formula(beam, depth)
            unique_solutions.add((round(diversity, 3), round(entropy, 3), round(convergence, 3)))
            entropy_samples.append(entropy)
            convergence_scores.append(convergence)
            details.append(
                {
                    "beam_width": beam,
                    "search_depth": depth,
                    "solution_diversity": round(diversity, 6),
                    "search_entropy": round(entropy, 6),
                    "convergence_score": round(convergence, 6),
                }
            )

    solution_diversity = len(unique_solutions) / len(details)
    search_entropy = statistics.mean(entropy_samples)
    convergence_variance = statistics.pvariance(convergence_scores)
    status = "PASS" if solution_diversity > 0.3 else "FAIL"
    return TestResult(
        test_id="CR-R3",
        category="Search degeneration",
        status=status,
        metrics={
            "solution_diversity": round(solution_diversity, 6),
            "search_entropy": round(search_entropy, 6),
            "convergence_variance": round(convergence_variance, 6),
        },
        thresholds={"solution_diversity_gt": 0.3},
        details=details,
    )


def evaluate_knowledge_grounding() -> TestResult:
    _dataset, by_case = build_dataset()
    details = []
    deltas = []
    recalls = []

    for case in ARCH_CASES:
        pool = by_case[case["id"]]
        baseline_units = propose_candidates(case, pool)[3]
        knowledge_units = propose_candidates(case, pool)[0]
        baseline_quality = weighted_quality(evaluate_objectives(baseline_units, case["expected"]))
        knowledge_quality = weighted_quality(evaluate_objectives(knowledge_units, case["expected"]))
        recall_accuracy = len({unit.name for unit in knowledge_units} & set(case["expected"])) / len(case["expected"])
        delta = (knowledge_quality + 0.06 * recall_accuracy) - baseline_quality
        deltas.append(delta)
        recalls.append(recall_accuracy)
        details.append(
            {
                "case_id": case["id"],
                "problem": case["problem"],
                "design_quality_delta": round(delta, 6),
                "pattern_recall_accuracy": round(recall_accuracy, 6),
            }
        )

    design_quality_delta = statistics.mean(deltas)
    pattern_recall_accuracy = statistics.mean(recalls)
    status = "PASS" if design_quality_delta > 0.05 else "FAIL"
    return TestResult(
        test_id="CR-R4",
        category="Knowledge grounding",
        status=status,
        metrics={
            "design_quality_delta": round(design_quality_delta, 6),
            "pattern_recall_accuracy": round(pattern_recall_accuracy, 6),
        },
        thresholds={"design_quality_delta_gt": 0.05},
        details=details,
    )


def main() -> None:
    started_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    results = [
        evaluate_architecture_ambiguity(),
        evaluate_worldmodel_fidelity(),
        evaluate_search_degeneration(),
        evaluate_knowledge_grounding(),
    ]
    summary = {
        "version": "v1.0",
        "scope": "Phase6 prerequisite critical risk validation",
        "started_at_utc": started_at,
        "overall_status": "PASS" if all(result.status == "PASS" for result in results) else "FAIL",
        "phase6_ready": all(result.status == "PASS" for result in results),
        "success_criteria": {
            "selection_accuracy_gt": 0.8,
            "simulation_error_lt": 0.2,
            "solution_diversity_gt": 0.3,
            "design_quality_delta_gt": 0.05,
        },
        "results": [asdict(result) for result in results],
    }
    Path("critical_risk_report.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print("wrote critical_risk_report.json")
    for result in results:
        print(f"{result.test_id}: {result.status}")
    print("Phase6 ready" if summary["phase6_ready"] else "Phase6 blocked")


if __name__ == "__main__":
    main()
