from .models import SemanticUnitL2, ReadinessReport, Stability

def inspect(unit: SemanticUnitL2) -> ReadinessReport:
    missing = []

    if "objective" not in unit.confirmed:
        missing.append("objective")
    if "scope_in" not in unit.confirmed:
        missing.append("scope_in")
    if "scope_out" not in unit.confirmed:
        missing.append("scope_out")
    if "success_criteria" not in unit.confirmed:
        missing.append("success_criteria")

    if not unit.confirmed:
        stability = Stability.UNSTABLE
    elif missing:
        stability = Stability.PARTIAL
    else:
        stability = Stability.STABLE

    return ReadinessReport(stability=stability, missing=missing)
