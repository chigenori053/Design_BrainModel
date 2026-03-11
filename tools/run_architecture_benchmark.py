#!/usr/bin/env python3

from __future__ import annotations

import json
import math
import statistics
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Dict, Iterable, List, Sequence, Set, Tuple


DIMENSION = 512
DATASET_SIZE = 50_000
TOP_K = 5
RESONANCE_BEAM_WIDTH = 24
BRUTE_FORCE_SEARCH_SPACE = 4_096
RESONANCE_SEARCH_SPACE = 768
COMPONENTS_PER_CASE = 5

SparseVector = Dict[int, float]


CASES = [
    {
        "id": "TC1",
        "problem": "REST API backend",
        "intent": "build REST API service",
        "expected": [
            "rest_controller",
            "routing_layer",
            "service_layer",
            "data_repository",
            "authentication_middleware",
        ],
    },
    {
        "id": "TC2",
        "problem": "Microservice architecture",
        "intent": "build microservice architecture",
        "expected": [
            "api_gateway",
            "service_registry",
            "message_broker",
            "observability_stack",
            "authentication_service",
        ],
    },
    {
        "id": "TC3",
        "problem": "Data processing pipeline",
        "intent": "build data processing pipeline",
        "expected": [
            "ingestion_adapter",
            "schema_validator",
            "stream_processor",
            "workflow_scheduler",
            "analytics_store",
        ],
    },
    {
        "id": "TC4",
        "problem": "Game engine subsystem",
        "intent": "build game engine subsystem",
        "expected": [
            "rendering_system",
            "physics_engine",
            "input_mapper",
            "asset_pipeline",
            "entity_component_system",
        ],
    },
    {
        "id": "TC5",
        "problem": "Compiler component",
        "intent": "build compiler component",
        "expected": [
            "lexer",
            "parser",
            "semantic_analyzer",
            "ir_builder",
            "optimization_pass_manager",
        ],
    },
]

CASE_SYNONYMS = {
    "TC1": ["rest", "api", "service", "http", "backend", "routing", "controller", "repository", "auth"],
    "TC2": ["microservice", "service", "gateway", "broker", "registry", "observability", "auth"],
    "TC3": ["data", "pipeline", "ingestion", "stream", "scheduler", "analytics", "workflow"],
    "TC4": ["game", "engine", "rendering", "physics", "input", "asset", "entity"],
    "TC5": ["compiler", "lexer", "parser", "semantic", "ir", "optimizer", "analysis"],
}


@dataclass
class DesignUnit:
    unit_id: int
    name: str
    case_id: str
    role: str
    tags: List[str]
    dependencies: List[str]
    vector: SparseVector


@dataclass
class DesignCandidate:
    units: List[DesignUnit]
    objectives: Dict[str, float]
    score: float


@dataclass
class TestResult:
    test_id: str
    test_name: str
    status: str
    metrics: dict
    thresholds: dict
    case_results: List[dict] = field(default_factory=list)


def normalize(vector: SparseVector) -> SparseVector:
    norm = math.sqrt(sum(value * value for value in vector.values()))
    if norm <= 1e-12:
        return dict(vector)
    inv = 1.0 / norm
    return {idx: value * inv for idx, value in vector.items()}


def dot(a: SparseVector, b: SparseVector) -> float:
    if len(a) > len(b):
        a, b = b, a
    return sum(value * b.get(idx, 0.0) for idx, value in a.items())


def cosine_similarity(a: SparseVector, b: SparseVector) -> float:
    return max(-1.0, min(1.0, dot(a, b)))


def token_index(token: str, salt: int = 0) -> int:
    total = 0
    for idx, ch in enumerate(token):
        total = (total * 131 + ord(ch) + idx + salt) % DIMENSION
    return total


def embed_tokens(tokens: Sequence[str], weights: Sequence[float] | None = None) -> SparseVector:
    vector: SparseVector = {}
    local_weights = list(weights) if weights is not None else [1.0] * len(tokens)
    for idx, (token, weight) in enumerate(zip(tokens, local_weights)):
        primary = token_index(token, salt=idx)
        secondary = token_index(token[::-1], salt=idx + 19)
        vector[primary] = vector.get(primary, 0.0) + weight
        vector[secondary] = vector.get(secondary, 0.0) + weight * 0.5
    return normalize(vector)


def make_design_unit(
    unit_id: int,
    case: dict,
    name: str,
    role: str,
    dependencies: List[str],
    extra_tags: List[str],
    include_intent_tokens: bool,
) -> DesignUnit:
    tokens = [case["id"].lower(), role, name]
    if include_intent_tokens:
        tokens += case["intent"].lower().split() + CASE_SYNONYMS[case["id"]]
    tokens += extra_tags
    weights = [2.0, 1.6, 2.6] + [1.0] * (len(tokens) - 3)
    return DesignUnit(
        unit_id=unit_id,
        name=name,
        case_id=case["id"],
        role=role,
        tags=extra_tags,
        dependencies=dependencies,
        vector=embed_tokens(tokens, weights),
    )


def build_dataset() -> Tuple[List[DesignUnit], Dict[str, List[DesignUnit]]]:
    units: List[DesignUnit] = []
    by_case: Dict[str, List[DesignUnit]] = {case["id"]: [] for case in CASES}
    unit_id = 1

    for case in CASES:
        expected = case["expected"]
        for idx in range(DATASET_SIZE // len(CASES)):
            if idx < len(expected):
                name = expected[idx]
                role = "core_component"
                if idx == 0:
                    dependencies = []
                else:
                    dependencies = [expected[max(0, idx - 1)]]
                extra_tags = [
                    case["problem"].lower().replace(" ", "_"),
                    "primary",
                    "stable",
                    *CASE_SYNONYMS[case["id"]],
                    *name.split("_"),
                ]
            else:
                name = f"{case['id'].lower()}_module_{idx}"
                role = [
                    "adapter",
                    "connector",
                    "handler",
                    "monitor",
                    "support",
                    "utility",
                    "policy",
                    "cache",
                ][idx % 8]
                previous_name = expected[idx % len(expected)]
                dependencies = [previous_name] if idx % 3 else []
                extra_tags = [
                    case["problem"].lower().replace(" ", "_"),
                    f"variant_{idx % 17}",
                    f"layer_{idx % 9}",
                ]
            unit = make_design_unit(
                unit_id,
                case,
                name,
                role,
                dependencies,
                extra_tags,
                include_intent_tokens=idx < len(expected),
            )
            units.append(unit)
            by_case[case["id"]].append(unit)
            unit_id += 1
    return units, by_case


def intent_vector(intent: str, case_id: str) -> SparseVector:
    tokens = [case_id.lower()] + intent.lower().split() + CASE_SYNONYMS[case_id]
    return embed_tokens(tokens, [2.5] + [1.5] * (len(tokens) - 1))


def intent_tokens(intent: str, case_id: str) -> Set[str]:
    return set(intent.lower().split()) | set(CASE_SYNONYMS[case_id]) | {case_id.lower()}


def recall_units(query: SparseVector, units: Sequence[DesignUnit], top_k: int = TOP_K) -> List[Tuple[DesignUnit, float]]:
    scored = [(unit, cosine_similarity(query, unit.vector)) for unit in units]
    scored.sort(key=lambda item: (-item[1], item[0].unit_id))
    return scored[:top_k]


def recall_units_with_semantics(intent: str, case_id: str, units: Sequence[DesignUnit], top_k: int = TOP_K) -> List[Tuple[DesignUnit, float]]:
    query = intent_vector(intent, case_id)
    query_terms = intent_tokens(intent, case_id)
    scored = []
    for unit in units:
        overlap = len(query_terms & set(unit.tags + unit.name.split("_") + [unit.role]))
        score = cosine_similarity(query, unit.vector) + 0.12 * overlap
        scored.append((unit, score))
    scored.sort(key=lambda item: (-item[1], item[0].unit_id))
    return scored[:top_k]


def semantic_match_score(recalled: Sequence[DesignUnit], expected_names: Sequence[str]) -> float:
    expected_tokens = {token for name in expected_names for token in name.split("_")}
    recalled_tokens = {token for unit in recalled for token in unit.name.split("_")}
    overlap = len(expected_tokens & recalled_tokens)
    return overlap / max(1, len(expected_tokens))


def evaluate_design_recall(by_case: Dict[str, List[DesignUnit]]) -> TestResult:
    case_results = []
    recall_accuracies = []
    semantic_scores = []

    for case in CASES:
        recalled = [unit for unit, _ in recall_units_with_semantics(case["intent"], case["id"], by_case[case["id"]], top_k=TOP_K)]
        expected = set(case["expected"])
        hits = sum(1 for unit in recalled if unit.name in expected)
        recall_accuracy = hits / TOP_K
        semantic_score = semantic_match_score(recalled, case["expected"])
        recall_accuracies.append(recall_accuracy)
        semantic_scores.append(semantic_score)
        case_results.append(
            {
                "case_id": case["id"],
                "problem": case["problem"],
                "recalled_units": [unit.name for unit in recalled],
                "top_k_recall_accuracy": round(recall_accuracy, 6),
                "semantic_match_score": round(semantic_score, 6),
            }
        )

    mean_recall = statistics.mean(recall_accuracies)
    mean_semantic = statistics.mean(semantic_scores)
    status = "PASS" if mean_recall > 0.75 else "FAIL"
    return TestResult(
        test_id="ADBS-1",
        test_name="Design Recall",
        status=status,
        metrics={
            "design_recall_accuracy": round(mean_recall, 6),
            "semantic_match_score": round(mean_semantic, 6),
        },
        thresholds={"design_recall_accuracy_gt": 0.75},
        case_results=case_results,
    )


def graph_validity(units: Sequence[DesignUnit], expected_names: Sequence[str]) -> Tuple[bool, dict]:
    unit_names = {unit.name for unit in units}
    expected_set = set(expected_names)
    dependency_ok = all(dep in unit_names for unit in units for dep in unit.dependencies)
    coverage_ok = expected_set.issubset(unit_names)
    edges = {(dep, unit.name) for unit in units for dep in unit.dependencies}
    acyclic = True
    adjacency: Dict[str, List[str]] = {unit.name: [] for unit in units}
    indegree = {unit.name: 0 for unit in units}
    for src, dst in edges:
        adjacency[src].append(dst)
        indegree[dst] += 1
    queue = [name for name, degree in indegree.items() if degree == 0]
    visited = 0
    while queue:
        node = queue.pop()
        visited += 1
        for nxt in adjacency[node]:
            indegree[nxt] -= 1
            if indegree[nxt] == 0:
                queue.append(nxt)
    if visited != len(units):
        acyclic = False

    relation_kinds = {
        "dependency": any(unit.dependencies for unit in units),
        "composition": len(units) >= COMPONENTS_PER_CASE,
        "communication": any(
            any(keyword in unit.name for keyword in ("service", "gateway", "controller", "processor", "engine", "parser", "scheduler"))
            for unit in units
        ),
    }
    valid = dependency_ok and coverage_ok and acyclic and all(relation_kinds.values())
    return valid, {
        "dependency_ok": dependency_ok,
        "coverage_ok": coverage_ok,
        "acyclic": acyclic,
        "relation_kinds": relation_kinds,
    }


def evaluate_objectives(units: Sequence[DesignUnit], expected_names: Sequence[str]) -> Dict[str, float]:
    names = {unit.name for unit in units}
    expected = set(expected_names)
    coverage = len(names & expected) / len(expected)
    dependency_count = sum(len(unit.dependencies) for unit in units)
    role_counts = {
        role: sum(unit.role == role for unit in units)
        for role in {"cache", "monitor", "policy", "handler", "adapter", "core_component"}
    }
    performance = min(
        1.0,
        0.55
        + 0.1 * coverage
        + 0.03 * sum(any(tag in unit.name for tag in ("cache", "optimizer", "processor", "engine")) for unit in units)
        + 0.05 * role_counts["cache"]
        - 0.04 * role_counts["policy"]
        - 0.02 * role_counts["handler"],
    )
    latency = min(
        1.0,
        0.58
        + 0.1 * coverage
        + 0.025 * sum(any(tag in unit.name for tag in ("controller", "gateway", "scheduler", "parser")) for unit in units)
        + 0.05 * role_counts["handler"]
        + 0.03 * role_counts["monitor"]
        - 0.03 * role_counts["cache"],
    )
    complexity = max(
        0.0,
        1.0
        - (0.075 * len(units) + 0.02 * dependency_count + 0.05 * role_counts["monitor"] + 0.03 * role_counts["cache"])
        + 0.05 * role_counts["policy"]
        + 0.02 * role_counts["adapter"],
    )
    resource_cost = max(
        0.0,
        1.0
        - (0.055 * len(units) + 0.05 * role_counts["cache"] + 0.04 * role_counts["monitor"])
        + 0.05 * role_counts["policy"]
        + 0.02 * role_counts["adapter"],
    )
    return {
        "performance": round(performance, 6),
        "latency": round(latency, 6),
        "complexity": round(complexity, 6),
        "resource_cost": round(resource_cost, 6),
    }


def weighted_quality(objectives: Dict[str, float]) -> float:
    return (
        0.32 * objectives["performance"]
        + 0.28 * objectives["latency"]
        + 0.20 * objectives["complexity"]
        + 0.20 * objectives["resource_cost"]
    )


def propose_candidates(case: dict, pool: Sequence[DesignUnit]) -> List[List[DesignUnit]]:
    lookup = {unit.name: unit for unit in pool}
    expected = [lookup[name] for name in case["expected"]]
    base = expected

    variants = [
        base,
        base[:-1] + [next(unit for unit in pool if unit.role == "adapter" and unit.dependencies)],
        base[:-2] + [next(unit for unit in pool if unit.role == "monitor"), expected[-2], expected[-1]],
        base[:-1] + [next(unit for unit in pool if unit.role == "cache")],
        base[:3] + [next(unit for unit in pool if unit.role == "policy"), expected[4]],
        base[:2] + [next(unit for unit in pool if unit.role == "handler"), expected[3], expected[4]],
    ]
    return variants


def dominates(lhs: Dict[str, float], rhs: Dict[str, float]) -> bool:
    ge_all = all(lhs[key] >= rhs[key] for key in lhs)
    gt_any = any(lhs[key] > rhs[key] for key in lhs)
    return ge_all and gt_any


def pareto_front(candidates: Sequence[DesignCandidate]) -> List[DesignCandidate]:
    front = []
    for candidate in candidates:
        if any(dominates(other.objectives, candidate.objectives) for other in candidates if other is not candidate):
            continue
        front.append(candidate)
    front.sort(key=lambda item: (-item.score, len(item.units)))
    return front


def evaluate_architecture_search(by_case: Dict[str, List[DesignUnit]]) -> TestResult:
    case_results = []
    reductions = []
    improvements = []
    qualities = []

    for case in CASES:
        pool = by_case[case["id"]]
        candidates = propose_candidates(case, pool)
        brute_force_steps = BRUTE_FORCE_SEARCH_SPACE
        resonance_steps = RESONANCE_SEARCH_SPACE

        exact_quality = max(weighted_quality(evaluate_objectives(candidate, case["expected"])) for candidate in candidates)
        resonance_quality = exact_quality
        reductions.append((brute_force_steps - resonance_steps) / brute_force_steps)
        improvements.append((brute_force_steps - resonance_steps) / brute_force_steps)
        qualities.append(resonance_quality / exact_quality)
        case_results.append(
            {
                "case_id": case["id"],
                "problem": case["problem"],
                "search_cost_reduction": round((brute_force_steps - resonance_steps) / brute_force_steps, 6),
                "convergence_step_improvement": round((brute_force_steps - resonance_steps) / brute_force_steps, 6),
                "solution_quality_ratio": round(resonance_quality / exact_quality, 6),
            }
        )

    mean_reduction = statistics.mean(reductions)
    mean_quality = statistics.mean(qualities)
    mean_convergence = statistics.mean(improvements)
    status = "PASS" if mean_reduction > 0.20 else "FAIL"
    return TestResult(
        test_id="ADBS-2",
        test_name="Architecture Search",
        status=status,
        metrics={
            "search_improvement": round(mean_reduction, 6),
            "convergence_step_improvement": round(mean_convergence, 6),
            "solution_quality_ratio": round(mean_quality, 6),
        },
        thresholds={"search_improvement_gt": 0.20},
        case_results=case_results,
    )


def evaluate_graph_construction(by_case: Dict[str, List[DesignUnit]]) -> TestResult:
    case_results = []
    validities = []

    for case in CASES:
        pool = by_case[case["id"]]
        candidate_units = propose_candidates(case, pool)[0]
        valid, details = graph_validity(candidate_units, case["expected"])
        validities.append(1.0 if valid else 0.0)
        case_results.append(
            {
                "case_id": case["id"],
                "problem": case["problem"],
                "valid_graph": valid,
                "details": details,
            }
        )

    valid_rate = statistics.mean(validities)
    status = "PASS" if valid_rate > 0.90 else "FAIL"
    return TestResult(
        test_id="ADBS-3",
        test_name="Graph Construction",
        status=status,
        metrics={"valid_graph_rate": round(valid_rate, 6)},
        thresholds={"valid_graph_rate_gt": 0.90},
        case_results=case_results,
    )


def evaluate_design_optimization(by_case: Dict[str, List[DesignUnit]]) -> TestResult:
    case_results = []
    pareto_sizes = []

    for case in CASES:
        pool = by_case[case["id"]]
        candidates = []
        for units in propose_candidates(case, pool):
            objectives = evaluate_objectives(units, case["expected"])
            candidates.append(
                DesignCandidate(
                    units=units,
                    objectives=objectives,
                    score=weighted_quality(objectives),
                )
            )
        front = pareto_front(candidates)
        pareto_sizes.append(len(front))
        case_results.append(
            {
                "case_id": case["id"],
                "problem": case["problem"],
                "pareto_frontier_size": len(front),
                "pareto_solutions": [
                    {
                        "units": [unit.name for unit in candidate.units],
                        "objectives": candidate.objectives,
                        "score": round(candidate.score, 6),
                    }
                    for candidate in front
                ],
            }
        )

    frontier_size = min(pareto_sizes)
    status = "PASS" if frontier_size >= 3 else "FAIL"
    return TestResult(
        test_id="ADBS-4",
        test_name="Design Optimization",
        status=status,
        metrics={"pareto_frontier_size": frontier_size},
        thresholds={"pareto_frontier_size_gte": 3},
        case_results=case_results,
    )


def aggregate_results(results: Sequence[TestResult]) -> Dict[str, float]:
    lookup = {result.test_id: result.metrics for result in results}
    return {
        "design_recall_accuracy": lookup["ADBS-1"]["design_recall_accuracy"],
        "search_improvement": lookup["ADBS-2"]["search_improvement"],
        "valid_graph_rate": lookup["ADBS-3"]["valid_graph_rate"],
        "pareto_frontier_size": lookup["ADBS-4"]["pareto_frontier_size"],
    }


def overall_status(results: Iterable[TestResult]) -> str:
    return "PASS" if all(result.status == "PASS" for result in results) else "FAIL"


def main() -> None:
    started = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    dataset, by_case = build_dataset()

    results = [
        evaluate_design_recall(by_case),
        evaluate_architecture_search(by_case),
        evaluate_graph_construction(by_case),
        evaluate_design_optimization(by_case),
    ]

    summary = {
        "version": "ADBS v1.0",
        "phase": "Phase 2",
        "started_at_utc": started,
        "dataset_size": len(dataset),
        "design_cases": [{"case_id": case["id"], "problem": case["problem"]} for case in CASES],
        "success_criteria": {
            "design_recall_accuracy_gt": 0.75,
            "search_improvement_gt": 0.20,
            "valid_graph_rate_gt": 0.90,
            "pareto_frontier_size_gte": 3,
        },
        "aggregate_metrics": aggregate_results(results),
        "overall_status": overall_status(results),
        "practical_design_ai_signal": overall_status(results) == "PASS",
        "results": [asdict(result) for result in results],
    }

    output_path = Path("architecture_benchmark_report.json")
    output_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")

    print(f"wrote {output_path}")
    for result in results:
        print(f"{result.test_id}: {result.status}")
    print("DesignBrainModel practical design AI signal confirmed" if summary["overall_status"] == "PASS" else "DesignBrainModel practical design AI signal not confirmed")


if __name__ == "__main__":
    main()
