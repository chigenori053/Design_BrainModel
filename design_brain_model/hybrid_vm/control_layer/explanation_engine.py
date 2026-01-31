from __future__ import annotations

from typing import Any, Dict, List, Optional


class ExplanationEngine:
    """
    Phase14: Read-only explanation engine.
    Consumes snapshot data and returns a deterministic, structured explanation.
    This is the implementation of the Language DHM v1 specification.
    """

    def generate(self, snapshot: Dict[str, Any]) -> Dict[str, Any]:
        state = snapshot.get("state", {})
        events = list(snapshot.get("events", []))
        semantic_blocks = snapshot.get("semantic_blocks", [])

        decision_state = state.get("decision_state", {})
        outcomes = decision_state.get("outcomes", [])
        if not outcomes:
            return {
                "decision_id": None,
                "final_decision": None,
                "decision_source": None,
                "logical_index": None,
                "summary": "No decision available.",
                "decision_steps": self._build_steps(events, semantic_blocks),
                "override": {"exists": False, "action": None, "reason": None},
            }

        outcome = outcomes[-1]
        final_decision = outcome.get("consensus_status")
        override_exists = bool(outcome.get("override_event_id") or outcome.get("overridden_decision_id"))
        decision_source = self._determine_source(override_exists, outcome)
        logical_index = self._decision_logical_index(events)

        override_action = None
        if override_exists:
            override_action = final_decision

        summary = self._build_summary(final_decision, decision_source, override_exists)

        return {
            "decision_id": outcome.get("outcome_id"),
            "final_decision": final_decision,
            "decision_source": decision_source,
            "logical_index": logical_index,
            "summary": summary,
            "decision_steps": self._build_steps(events, semantic_blocks),
            "override": {
                "exists": override_exists,
                "action": override_action,
                "reason": outcome.get("human_reason"),
            },
        }

    def _determine_source(self, override_exists: bool, outcome: Dict[str, Any]) -> str:
        if override_exists:
            return "HUMAN_OVERRIDE"
        evaluations = outcome.get("evaluations", [])
        if evaluations:
            return "CONSENSUS"
        return "UTILITY"

    def _decision_logical_index(self, events: List[Dict[str, Any]]) -> Optional[int]:
        decision_events = [e for e in events if e.get("type") == "decision_made"]
        if not decision_events:
            return None
        decision_events.sort(key=lambda e: e.get("logical_index") or 0)
        return decision_events[-1].get("logical_index")

    def _build_steps(
        self, events: List[Dict[str, Any]], semantic_blocks: List[Dict[str, Any]]
    ) -> List[Dict[str, Any]]:
        
        items = []
        for event in events:
            items.append({
                "sort_key": event.get("logical_index") or 0,
                "source": "event",
                "data": event,
            })
        
        # For now, append semantic blocks after events.
        # DHM spec requires sorting by logical_index, but blocks don't have one.
        # We'll use a simple heuristic to place them after all events for now.
        max_event_index = max((e.get("logical_index") or 0 for e in events), default=0)
        for i, block in enumerate(semantic_blocks, 1):
            items.append({
                "sort_key": max_event_index + i,
                "source": "semantic_block",
                "data": block,
            })

        ordered_items = sorted(items, key=lambda i: i["sort_key"])
        
        steps: List[Dict[str, Any]] = []
        for idx, item in enumerate(ordered_items, start=1):
            if item["source"] == "event":
                event_type = item["data"].get("type")
                description = self._describe_item(event_type, item["data"])
            else:  # semantic_block
                event_type = "SEMANTIC_BLOCK"
                description = self._describe_item(event_type, item["data"])

            steps.append(
                {
                    "step_index": idx,
                    "event_type": event_type,
                    "description": description,
                }
            )
        return steps

    def _describe_item(self, item_type: Optional[str], data: Dict[str, Any]) -> str:
        if item_type == "SEMANTIC_BLOCK":
            return f"Semantic Block ({data.get('type')}): {data.get('content')}"
        if item_type == "user_input":
            return "User input received."
        if item_type == "execution_request":
            return "Execution requested."
        if item_type == "execution_result":
            return "Execution result recorded."
        if item_type == "decision_made":
            return "Decision finalized."
        if item_type == "human_override":
            return "Human override applied."
        if item_type == "request_reevaluation":
            return "Reevaluation requested."
        if item_type == "vm_terminate":
            return "VM terminated."
        return "Unhandled event."

    def _build_summary(self, final_decision: Optional[str], decision_source: Optional[str], override_exists: bool) -> str:
        if final_decision is None:
            return "No decision available."
        override_str = "Override: YES" if override_exists else "Override: NO"
        return f"Final: {final_decision} | Source: {decision_source} | {override_str}"
