import json
from design_brain_model.brain_model.dialogue.controller import DialogueController

def run_app_design_start():
    controller = DialogueController()
    
    # ユーザーの要望
    user_request = "HolographicMemoryを3D構造物とした3Dモデリングアプリを開発したい"
    print("--- ユーザー入力 ---")
    print(user_request)
    print("")
    
    # 対話開始（要素分解）
    state = controller.start_dialogue(user_request)
    
    print("--- 判定結果 (Readiness Report) ---")
    print(f"状態: {state.readiness.stability}")
    print(f"充足済み: {state.readiness.satisfied_requirements}")
    print(f"不足要素: {state.readiness.missing_requirements}")
    
    print("\n--- 抽出された SemanticUnitL2 (現在の理解) ---")
    unit = state.semantic_unit
    # Pydanticモデルから辞書を取得
    print(f"主目的: {unit.objective}")
    print(f"確定済みフィールド: {unit.confirmed_fields}")
    
    print("\n--- 次の質問 (システムからの問いかけ) ---")
    for q in state.open_questions:
        print(f"[{q.target_field}] {q.prompt}")

if __name__ == "__main__":
    run_app_design_start()