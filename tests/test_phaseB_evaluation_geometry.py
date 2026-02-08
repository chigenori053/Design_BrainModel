from design_brain_model.brain_model.phaseB import (
    EvaluationEngine,
    EvaluationTarget,
    EvaluationTargetType,
    EVALUATION_AXES,
)


def test_evaluation_engine_builds_geometry_and_distances() -> None:
    engine = EvaluationEngine()
    targets = [
        EvaluationTarget(
            type=EvaluationTargetType.DESIGN,
            structure={"nodes": ["A", "B"], "edges": [["A", "B"]]},
        ),
        EvaluationTarget(
            type=EvaluationTargetType.TEXT,
            structure={"text": "short explanation"},
        ),
        EvaluationTarget(
            type=EvaluationTargetType.UI,
            structure={"layout": {"rows": 2, "cols": 3}, "constraints": ["align"]},
        ),
    ]

    report = engine.evaluate(targets)

    assert len(report.targets) == 3
    assert len(report.geometry_points) == 3
    assert len(report.distances) == 3
    assert all(len(row) == 3 for row in report.distances)

    for point in report.geometry_points:
        assert len(point.vector) == len(EVALUATION_AXES)


def test_distance_diagonal_is_zero() -> None:
    engine = EvaluationEngine()
    targets = [
        EvaluationTarget(
            type=EvaluationTargetType.DESIGN,
            structure={"nodes": ["A"], "edges": []},
        ),
        EvaluationTarget(
            type=EvaluationTargetType.TEXT,
            structure={"text": "another"},
        ),
    ]
    report = engine.evaluate(targets)

    assert report.distances[0][0] == 0.0
    assert report.distances[1][1] == 0.0

