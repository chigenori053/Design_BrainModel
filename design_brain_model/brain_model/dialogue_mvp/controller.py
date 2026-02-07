from .readiness import inspect
from .models import SemanticUnitL2, DialoguePhase, Stability

def start_dialogue():
    unit = SemanticUnitL2()
    readiness = inspect(unit)
    return {
        "unit": unit, 
        "readiness": readiness, 
        "phase": DialoguePhase.INTAKE
    }

def submit_answer(state: dict, field: str, value):
    unit = state["unit"]

    if field == "objective":
        unit.objective = value
    elif field == "scope_in":
        unit.scope_in = value
    elif field == "scope_out":
        unit.scope_out = value
    elif field == "success_criteria":
        unit.success_criteria = value

    unit.confirmed.add(field)
    readiness = inspect(unit)

    phase = (
        DialoguePhase.STABLE
        if readiness.stability == Stability.STABLE
        else DialoguePhase.CLARIFYING
    )

    return {
        "unit": unit,
        "readiness": readiness,
        "phase": phase,
        "last_confirmed": field,
    }
