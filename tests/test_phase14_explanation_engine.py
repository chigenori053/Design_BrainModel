from design_brain_model.hybrid_vm.core import HybridVM
from design_brain_model.hybrid_vm.control_layer.state import DecisionCandidate, Policy, Role
from design_brain_model.hybrid_vm.control_layer.explanation_engine import ExplanationEngine
from design_brain_model.brain_model.language_engine import decompose_text

# Input text from lsdt_spec.md, used to generate semantic blocks
INPUT_TEXT_A = """本システムは当初、設計者が仕様を記述すると自動的にコード骨格を生成することを目的としていたが、
議論を進める中で、なぜその設計に至ったかを後から説明できることの方が
重要ではないかという意見が強くなった。

そのため、設計過程を逐次保存し、後から再生できる仕組みを導入することになったが、
この時点ではまだ人間の介入をどの段階で許可するかは明確に決まっていなかった。

一方で、完全自動化を目指すべきだという立場も残っており、
人間が頻繁に介入する設計支援ツールはかえって効率を下げるのではないかという懸念も存在する。

最終的には、人間は最終判断のみを担い、それ以外の部分はシステムに任せるという方針が採用されたが、
この判断は安全性を重視した結果であり、将来的に変更される可能性があることも認識されている。
"""

def _build_candidates(question_id: str):
    return [
        DecisionCandidate(
            resolves_question_id=question_id,
            content="Use Hash Sharding (High Perf, Complex)",
            proposed_by=Role.BRAIN,
        ),
        DecisionCandidate(
            resolves_question_id=question_id,
            content="Use Range Sharding (Easy, Scale risk)",
            proposed_by=Role.USER,
        ),
    ]


def test_explanation_read_only():
    vm = HybridVM.create()
    policy = Policy(name="Perf", weights={"performance": 1.0})
    vm.evaluate_decision("q1", _build_candidates("q1"), policy)

    snapshot_before = vm.get_state_snapshot()
    expl_snapshot = vm.get_explanation_snapshot()

    engine = ExplanationEngine()
    _ = engine.generate(expl_snapshot)

    snapshot_after = vm.get_state_snapshot()
    assert snapshot_before == snapshot_after


def test_explanation_determinism():
    vm = HybridVM.create()
    policy = Policy(name="Perf", weights={"performance": 1.0})
    vm.evaluate_decision("q1", _build_candidates("q1"), policy)
    expl_snapshot = vm.get_explanation_snapshot()

    engine = ExplanationEngine()
    out1 = engine.generate(expl_snapshot)
    out2 = engine.generate(expl_snapshot)
    assert out1 == out2


def test_explanation_integrates_semantic_blocks():
    """
    Tests if the ExplanationEngine correctly integrates semantic_blocks from LSDT,
    as required by the DHM v1 Specification.
    """
    # 1. Setup a basic VM snapshot
    vm = HybridVM.create()
    expl_snapshot = vm.get_explanation_snapshot()

    # 2. Generate semantic blocks to simulate LSDT input
    lsdt_output = decompose_text(INPUT_TEXT_A)
    semantic_blocks = lsdt_output.get("semantic_blocks", [])
    assert semantic_blocks, "LSDT must produce semantic blocks for this test to be valid."

    # 3. Add semantic_blocks to the snapshot, simulating the complete input for DHM
    expl_snapshot["semantic_blocks"] = semantic_blocks

    # 4. Generate the explanation
    engine = ExplanationEngine()
    explanation = engine.generate(expl_snapshot)

    # 5. Verify that the content from semantic blocks is present in the decision steps
    all_step_descriptions = " ".join(step["description"] for step in explanation["decision_steps"])

    # We expect the content of the semantic blocks to be integrated into the steps.
    # Let's check for the content of the first block.
    first_block_content = semantic_blocks[0]["content"]

    # This test will fail because the current engine does not process "semantic_blocks"
    assert first_block_content in all_step_descriptions, \
        "Content from the first semantic block should be in the explanation."

    # Also, verify that a new step type is introduced
    step_types = {step["event_type"] for step in explanation["decision_steps"]}
    assert "SEMANTIC_BLOCK" in step_types, "A step type for semantic blocks should exist."
