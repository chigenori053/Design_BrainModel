import sys
from design_brain_model.brain_model.dialogue.controller import DialogueController
from design_brain_model.brain_model.dialogue.types import DialoguePhase, HumanOverrideAction
from design_brain_model.brain_model.memory.types import Stability

def interactive_test():
    controller = DialogueController()
    print("=== Design_BrainModel Interactive Dialogue Test ===")
    print("Input 'exit' to quit.\n")

    # Initial Input
    user_input = input("【Input Request】> ")
    if user_input.lower() in ['exit', 'quit']: return

    state = controller.start_dialogue(user_input)

    while True:
        print(f"\n--- State: {state.phase} (Stability: {state.semantic_unit.stability}) ---")
        
        u = state.semantic_unit
        print("[Current Understanding]")
        print(f"  Objective: {u.objective if u.objective else '(None)'}")
        print(f"  Scope In: {u.scope_in}")
        print(f"  Confirmed: {u.confirmed_fields}")

        if state.readiness.blocking_issues:
            print("\n[!! BLOCKING ISSUES !!]")
            for issue in state.readiness.blocking_issues:
                print(f"  - {issue}")

        if state.phase == DialoguePhase.CLARIFYING:
            print("\n[Questions]")
            if not state.open_questions:
                print("  (Processing...)")
            else:
                for i, q in enumerate(state.open_questions):
                    print(f"  {i+1}. [{q.target_field}] {q.prompt}")
            
            print("\nFormat: 'field: value' (e.g., objective: 3D App)")
            ans = input("【Your Answer】> ")
            
            if ans.lower() in ['exit', 'quit']: break
            
            if ":" in ans:
                field, val = ans.split(":", 1)
                field = field.strip()
                val = val.strip()
                if field in ['scope_in', 'scope_out', 'success_criteria']:
                    val = [item.strip() for item in val.split(",")]
                state = controller.submit_answer(field, val)
            else:
                state = controller.submit_divergence(ans)

        elif state.phase == DialoguePhase.CANDIDATES_READY:
            print("\n[Design Candidates]")
            for i, cand in enumerate(state.candidates):
                print(f"\n[{i+1}] {cand.label}")
                print(f"  Intent: {cand.design_intent}")
                print(f"  Decisions: {', '.join(cand.key_decisions)}")

            print("\nActions: 'accept 1', 'diverge (text)', 'exit'")
            ans = input("【Action】> ")
            
            if ans.lower().startswith("accept"):
                try:
                    idx = int(ans.split()[1]) - 1
                    state = controller.human_override(HumanOverrideAction.ACCEPT, candidate_id=state.candidates[idx].id)
                    print("\n--- Design Confirmed (Human Override) ---")
                    break
                except:
                    print("Invalid candidate number.")
            elif ans.lower() in ['exit', 'quit']:
                break
            else:
                state = controller.submit_divergence(ans)

        elif state.phase == DialoguePhase.READONLY:
            print("\nProcess finished.")
            break

if __name__ == "__main__":
    interactive_test()