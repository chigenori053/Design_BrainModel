import sys
import os
import unittest

# Add project root to path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..', 'design_brain_model')))

from brain_model.language_engine import (
    LanguageEngine,
    LanguageInput,
    Constraint,
    CausalSummary,
)
from brain_model.memory.types import SemanticUnit
from hybrid_vm.control_layer.state import (
    DecisionOutcome,
    RankedCandidate,
    UtilityVector,
    ConsensusStatus,
)


class TestPhase10LanguageEngine(unittest.TestCase):
    def setUp(self):
        self.engine = LanguageEngine()

    def test_deterministic_output(self):
        unit = SemanticUnit(content="Database", type="concept")
        decision = DecisionOutcome(
            resolves_question_id="q1",
            consensus_status=ConsensusStatus.ACCEPT,
            ranked_candidates=[
                RankedCandidate(
                    candidate_id="cand-1",
                    content="Use PostgreSQL",
                    final_score=0.9,
                    utility_vector_snapshot=UtilityVector(performance=0.9),
                )
            ],
            explanation="",
        )
        input_data = LanguageInput(
            semantic_units=[unit],
            decision=decision,
            constraints=[Constraint(content="No downtime")],
            causal_summary=CausalSummary(summary="Cost and maintainability were prioritized."),
        )

        first = self.engine.generate(input_data)
        second = self.engine.generate(input_data)

        self.assertEqual(first.text, second.text)
        self.assertEqual(first.explanation_level, second.explanation_level)

    def test_empty_input(self):
        input_data = LanguageInput()
        output = self.engine.generate(input_data)
        self.assertEqual(output.text, "No language output available.")


if __name__ == "__main__":
    unittest.main()
