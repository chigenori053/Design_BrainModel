from dataclasses import dataclass, field
from typing import List, Dict, Optional
from design_brain_model.hybrid_vm.control_layer.state import DecisionNode, DecisionNodeSnapshot, DecisionNodeStatus

@dataclass
class LanguageContext:
    decision_node: DecisionNode
    snapshot_before: Optional[DecisionNodeSnapshot]
    snapshot_after: Optional[DecisionNodeSnapshot]
    l2_semantic_unit: Optional[Dict] = None # Simplified representation for now

@dataclass
class LanguageReport:
    summary: str
    decision_state: str
    l2_description: str
    override_rationale: str
    differences: Dict[str, List[str]]
    non_actions: List[str]
    scope_limits: List[str]

class LanguageArticulationValidator:
    """
    Phase 17.9: Language Articulation Validation (LAV) Engine.
    Ensures that the DecisionNode state can be articulated purely based on structure,
    without inference, external knowledge, or subjective evaluation.
    """

    PROHIBITED_WORDS = [
        "推測", "考えられる", "可能性",
        "最適", "望ましい", "改善",
        "おそらく", "一般に", "maybe", "likely", "optimal"
    ]

    def validate(self, context: LanguageContext) -> LanguageReport:
        if context.decision_node.status != DecisionNodeStatus.OVERRIDDEN_L2:
             raise ValueError("LAV can only be executed on OVERRIDDEN_L2 nodes.")

        report = LanguageReport(
            summary=self._generate_summary(context),
            decision_state=self._describe_decision_state(context),
            l2_description=self._describe_l2(context),
            override_rationale=self._describe_rationale(context),
            differences=self._analyze_differences(context),
            non_actions=self._list_non_actions(context),
            scope_limits=self._list_scope_limits(context)
        )
        
        self._check_constraints(report)
        return report

    def _generate_summary(self, context: LanguageContext) -> str:
        return f"DecisionNode {context.decision_node.id} is in OVERRIDDEN_L2 state."

    def _describe_decision_state(self, context: LanguageContext) -> str:
        return f"Current status is {context.decision_node.status.value}. The decision is fixed by human intervention."

    def _describe_l2(self, context: LanguageContext) -> str:
        target_id = context.decision_node.override_target_l2
        return f"The selected L2 candidate ID is {target_id}. This value is structurally assigned."

    def _describe_rationale(self, context: LanguageContext) -> str:
        return "This result was determined via Human Override. Automatic evaluation is not the final judgment."

    def _analyze_differences(self, context: LanguageContext) -> Dict[str, List[str]]:
        changed = []
        unchanged = []
        
        before = context.snapshot_before
        after = context.snapshot_after
        
        if before and after:
            if before.selected_candidate != after.selected_candidate:
                changed.append("selected_candidate")
            else:
                unchanged.append("selected_candidate")
                
            if before.confidence != after.confidence:
                changed.append("confidence")
            else:
                unchanged.append("confidence")
                
            if before.entropy != after.entropy:
                changed.append("entropy")
            else:
                unchanged.append("entropy")
        else:
             unchanged.append("comparison_unavailable")

        return {"changed": changed, "unchanged": unchanged}

    def _list_non_actions(self, context: LanguageContext) -> List[str]:
        return [
            "Inference was not executed.",
            "Re-evaluation was not executed.",
            "Learning update was not executed."
        ]
    
    def _list_scope_limits(self, context: LanguageContext) -> List[str]:
        return [
            "No external knowledge accessed.",
            "No subjective evaluation performed.",
            "Description limited to structural facts."
        ]

    def _check_constraints(self, report: LanguageReport):
        content = (
            report.summary + 
            report.decision_state + 
            report.l2_description + 
            report.override_rationale +
            " ".join(report.non_actions) + 
            " ".join(report.scope_limits)
        )
        for word in self.PROHIBITED_WORDS:
            if word in content:
                raise ValueError(f"Prohibited vocabulary detected: {word}")
