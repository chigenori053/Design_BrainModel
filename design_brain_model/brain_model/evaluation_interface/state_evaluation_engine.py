import json
import statistics
from typing import List, Dict, Any, Optional
from dataclasses import dataclass, asdict
from collections import defaultdict
from enum import Enum

class EvaluationStatus(str, Enum):
    OK = "OK"
    WARNING = "WARNING"
    CRITICAL = "CRITICAL"

@dataclass
class StatSummary:
    mean: float
    median: float
    std: float
    min: float
    max: float
    count: int

class StateEvaluationEngine:
    """
    Phase 19-2: StateEvaluationEngine
    
    Role:
    Statistically evaluates logs across multiple executions to determine
    system health and threshold validity.
    Does NOT control, optimize, or propose changes.
    """

    def __init__(self):
        self.decision_logs: List[Dict[str, Any]] = []
        self.memory_logs: List[Dict[str, Any]] = []
        self.recall_logs: List[Dict[str, Any]] = []
        self.design_eval_logs: List[Dict[str, Any]] = []
        
    def load_logs(self, logs_data: Dict[str, List[Dict[str, Any]]]):
        """
        Loads logs into the engine.
        In a real scenario, this might read from a directory of JSONL files.
        Here we accept a dictionary of lists for flexibility.
        """
        if "decisions" in logs_data:
            self.decision_logs.extend(logs_data["decisions"])
        if "memory" in logs_data:
            self.memory_logs.extend(logs_data["memory"])
        if "recall" in logs_data:
            self.recall_logs.extend(logs_data["recall"])
        if "design_eval" in logs_data:
            self.design_eval_logs.extend(logs_data["design_eval"])

    def evaluate(self) -> Dict[str, Any]:
        """
        Performs the statistical evaluation.
        """
        report = {
            "decision_stats": self._evaluate_decisions(),
            "memory_stats": self._evaluate_memory(),
            "recall_stats": self._evaluate_recall(),
            "design_eval_stats": self._evaluate_design_eval(),
            "threshold_verification": self._verify_thresholds(),
            "overall_status": EvaluationStatus.OK.value # Default, updated by logic
        }
        
        # Simple rollup status logic
        statuses = [
            report["decision_stats"]["status"],
            report["memory_stats"]["status"],
            report["recall_stats"]["status"],
            report["design_eval_stats"]["status"]
        ]
        
        if EvaluationStatus.CRITICAL.value in statuses:
            report["overall_status"] = EvaluationStatus.CRITICAL.value
        elif EvaluationStatus.WARNING.value in statuses:
            report["overall_status"] = EvaluationStatus.WARNING.value
            
        return report

    def export(self, report: Dict[str, Any], format: str = "json") -> str:
        if format == "json":
            return json.dumps(report, indent=2)
        # CSV support can be added if needed, but JSON is primary for MVP
        raise NotImplementedError(f"Format {format} not supported")

    def _calc_stats(self, values: List[float]) -> Optional[StatSummary]:
        if not values:
            return None
        return StatSummary(
            mean=statistics.mean(values),
            median=statistics.median(values),
            std=statistics.stdev(values) if len(values) > 1 else 0.0,
            min=min(values),
            max=max(values),
            count=len(values)
        )

    def _evaluate_decisions(self) -> Dict[str, Any]:
        if not self.decision_logs:
            return {"status": EvaluationStatus.OK.value, "note": "No data"}

        confidences = [d.get("confidence", 0.0) for d in self.decision_logs]
        entropies = [d.get("entropy", 0.0) for d in self.decision_logs]
        utilities = [d.get("utility", 0.0) for d in self.decision_logs]
        
        # Label ratios
        label_counts = defaultdict(int)
        for d in self.decision_logs:
            label_counts[d.get("label", "UNKNOWN")] += 1
        
        total = len(self.decision_logs)
        label_ratios = {k: v / total for k, v in label_counts.items()}
        
        # Status determination
        status = EvaluationStatus.OK
        note = "Stable"
        
        # Check for skewed distributions or high review rates
        if label_ratios.get("REVIEW", 0) > 0.3: # Arbitrary threshold for MVP warning
            status = EvaluationStatus.WARNING
            note = "High REVIEW rate detected"
            
        stats_summary = {
            "confidence": asdict(self._calc_stats(confidences)),
            "entropy": asdict(self._calc_stats(entropies)),
            "utility": asdict(self._calc_stats(utilities)),
            "label_ratios": label_ratios,
            "status": status.value,
            "note": note
        }
        return stats_summary

    def _evaluate_memory(self) -> Dict[str, Any]:
        if not self.memory_logs:
            return {"status": EvaluationStatus.OK.value, "note": "No data"}
            
        store_counts = defaultdict(int)
        for m in self.memory_logs:
            store_counts[m.get("store_type", "UNKNOWN")] += 1
            
        total = len(self.memory_logs)
        store_ratios = {k: v / total for k, v in store_counts.items()}
        
        # Quarantine dwell time (if available, simplified here)
        # Assuming logs might have 'dwell_time'
        quarantine_logs = [m for m in self.memory_logs if m.get("store_type") == "QuarantineStore"]
        dwell_times = [m.get("dwell_time", 0) for m in quarantine_logs if "dwell_time" in m]
        
        status = EvaluationStatus.OK
        note = "Stable"
        
        if store_ratios.get("QuarantineStore", 0) > 0.5:
            status = EvaluationStatus.WARNING
            note = "Quarantine accumulating"

        return {
            "store_ratios": store_ratios,
            "quarantine_dwell_stats": asdict(self._calc_stats(dwell_times)) if dwell_times else None,
            "status": status.value,
            "note": note
        }

    def _evaluate_recall(self) -> Dict[str, Any]:
        if not self.recall_logs:
            return {"status": EvaluationStatus.OK.value, "note": "No data"}
            
        eu_deltas = [r.get("eu_delta", 0.0) for r in self.recall_logs]
        success_count = sum(1 for eu in eu_deltas if eu > 0)
        success_rate = success_count / len(eu_deltas) if eu_deltas else 0.0
        
        status = EvaluationStatus.OK
        note = "Stable"
        
        if success_rate < 0.1: # Very low utility from recall
            status = EvaluationStatus.WARNING
            note = "Low recall utility"

        return {
            "eu_delta_stats": asdict(self._calc_stats(eu_deltas)),
            "success_rate": success_rate,
            "status": status.value,
            "note": note
        }

    def _evaluate_design_eval(self) -> Dict[str, Any]:
        if not self.design_eval_logs:
            return {"status": EvaluationStatus.OK.value, "note": "No data"}
            
        completeness = [d.get("completeness_score", 0.0) for d in self.design_eval_logs]
        consistency = [d.get("consistency_score", 0.0) for d in self.design_eval_logs]
        
        status = EvaluationStatus.OK
        note = "Stable"
        
        # Check for consistently poor language scores
        avg_comp = statistics.mean(completeness)
        if avg_comp < 0.5:
            status = EvaluationStatus.WARNING
            note = "Low completeness scores"

        return {
            "completeness_stats": asdict(self._calc_stats(completeness)),
            "consistency_stats": asdict(self._calc_stats(consistency)),
            "status": status.value,
            "note": note
        }

    def _verify_thresholds(self) -> List[Dict[str, Any]]:
        """
        Empirically validates thresholds.
        Example: confidence >= 0.40
        """
        results = []
        
        if self.decision_logs:
            # Check confidence >= 0.40
            threshold = 0.40
            below = [d for d in self.decision_logs if d.get("confidence", 0) < threshold]
            above = [d for d in self.decision_logs if d.get("confidence", 0) >= threshold]
            
            # Compare utility or other metrics between groups
            below_utility = [d.get("utility", 0) for d in below]
            above_utility = [d.get("utility", 0) for d in above]
            
            results.append({
                "threshold": "confidence >= 0.40",
                "below_threshold": {
                    "count": len(below),
                    "mean_utility": statistics.mean(below_utility) if below_utility else 0.0
                },
                "above_threshold": {
                    "count": len(above),
                    "mean_utility": statistics.mean(above_utility) if above_utility else 0.0
                },
                "status": "OK" if (not below or not above or statistics.mean(above_utility) > statistics.mean(below_utility)) else "INVERSION_DETECTED",
                "note": "Higher confidence correlates with higher utility" if (not below or not above or statistics.mean(above_utility) > statistics.mean(below_utility)) else "Warning: Lower confidence has higher utility"
            })
            
        return results
