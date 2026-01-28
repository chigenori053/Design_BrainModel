from typing import List, Optional
from design_brain_model.hybrid_vm.control_layer.state import DecisionOutcome, ConsensusStatus, EvaluationResult, Role

class ExplanationGenerator:
    """
    Phase 4: Explanation Generator.
    Responsible for converting DecisionOutcome internal state into
    human-readable, deterministic structured text (Japanese).
    """

    def generate(self, outcome: DecisionOutcome) -> str:
        """
        Generates a structured explanation for the given DecisionOutcome.
        """
        parts = []

        # 1. Decision Summary
        status_str = outcome.consensus_status.value if outcome.consensus_status else "UNKNOWN"
        
        # Identify winner if any
        winner_id = "None"
        winner_content = ""
        if outcome.ranked_candidates:
            # Assumes index 0 is the winner/top-ranked
            winner = outcome.ranked_candidates[0]
            winner_id = winner.candidate_id
            winner_content = winner.content

        parts.append(f"【決定概要】\nステータス: {status_str}\n選択された候補: {winner_content} (ID: {winner_id})")

        # 2. Reasoning Basis
        eval_count = len(outcome.evaluations)
        
        # Calculate aggregate metrics for display (re-calculation to be safe or rely on what's in outcome if available? 
        # Outcome doesn't store aggregate stats directly, only individual evaluations. 
        # We can re-derive them or just describe them.)
        
        avg_conf = 0.0
        avg_ent = 0.0
        if eval_count > 0:
            avg_conf = sum(e.confidence for e in outcome.evaluations) / eval_count
            avg_ent = sum(e.entropy for e in outcome.evaluations) / eval_count

        parts.append(f"【判断根拠】\n評価数: {eval_count}")
        parts.append(f"集合的確信度: {avg_conf:.2f}, 集合的エントロピー: {avg_ent:.2f}")

        # 3. Uncertainty Explanation
        if outcome.consensus_status == ConsensusStatus.REVIEW:
            parts.append("【不確実性説明】\n警告: エントロピーが高いため、評価者間で意見が割れている、または判断に迷いがあります。人間のレビューを推奨します。")
        elif outcome.consensus_status == ConsensusStatus.ESCALATE:
            parts.append("【不確実性説明】\n警告: 確信度が低いため、システムはこの判断に十分な自信を持てません。上位判断者へのエスカレーションが必要です。")
        elif outcome.consensus_status == ConsensusStatus.REJECT:
            parts.append("【不確実性説明】\n却下: 有効な候補が存在しない、または全ての候補が基準を満たしませんでした。")
        
        # 4. Human Involvement
        if outcome.human_reason:
            parts.append(f"【人間介入】\nあり\n理由: {outcome.human_reason}")
        else:
            # Check if any evaluation was by USER (implicit human involvement check)
            # Though human_reason field is the primary flag for Override/HITL.
            pass

        # 5. Decision History (Lineage)
        if outcome.lineage:
            parts.append(f"【履歴】\n再評価 (元: {outcome.lineage})")
            # Ideally we would fetch the previous outcome to compare, but ExplanationGenerator 
            # might be stateless. For now, just stating it's a re-evaluation is sufficient per MVP.
        else:
            parts.append("【履歴】\n新規判断")

        return "\n\n".join(parts)
