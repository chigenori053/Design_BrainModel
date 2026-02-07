from .controller import start_dialogue, submit_answer
from .renderer import render
from .models import Stability

def test_natural_dialogue_flow():
    print("\n--- Dialogue MVP Flow Start ---")
    state = start_dialogue()

    # 1. Objective
    state = submit_answer(state, "objective", "3D HolographicMemory の検証")
    print("\n[System]")
    print(render(state))

    # 2. Scope In
    state = submit_answer(
        state, "scope_in",
        ["素材情報の統合", "3D座標データの操作"]
    )
    print("\n[System]")
    print(render(state))

    # 3. Scope Out
    state = submit_answer(
        state, "scope_out",
        ["レンダリング", "UI構築"]
    )
    print("\n[System]")
    print(render(state))

    # 4. Success Criteria
    state = submit_answer(
        state, "success_criteria",
        "構造と操作イメージが掴めること"
    )
    print("\n[System]")
    print(render(state))

    assert state["readiness"].stability == Stability.STABLE
    print("\n--- Dialogue MVP Flow End (STABLE Reached) ---")

if __name__ == "__main__":
    test_natural_dialogue_flow()