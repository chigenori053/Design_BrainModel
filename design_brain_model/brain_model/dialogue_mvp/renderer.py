from .models import DialoguePhase
from .questions import QUESTIONS

def render(state: dict) -> str:
    unit = state["unit"]
    readiness = state["readiness"]
    phase = state["phase"]
    last = state.get("last_confirmed")

    output = []

    if last:
        if last == "objective":
            output.append(f"「{unit.objective}」について整理していきます。")
        elif last == "scope_in":
            items = ", ".join(unit.scope_in)
            output.append(f"ここまでで、{items} を行うことが整理できています。")
        elif last == "scope_out":
            items = ", ".join(unit.scope_out)
            output.append(f"今回は {items} は対象外としています。")
        elif last == "success_criteria":
            output.append(f"成功条件として「{unit.success_criteria}」を設定しました。")

    if phase == DialoguePhase.CLARIFYING:
        next_field = readiness.missing[0]
        output.append(QUESTIONS[next_field])

    if phase == DialoguePhase.STABLE:
        output.append("目的・やること・やらないこと・成功条件が整理できました。")

    return "\n".join(output)