import pytest
from design_brain_model.brain_model.evaluation_interface.state_evaluation_engine import StateEvaluationEngine, EvaluationStatus

class TestStateEvaluationEngine:
    
    @pytest.fixture
    def engine(self):
        return StateEvaluationEngine()

    def test_healthy_run(self, engine):
        """
        Tests a scenario where metrics are healthy.
        """
        logs = {
            "decisions": [
                {"label": "ACCEPT", "confidence": 0.9, "entropy": 0.1, "utility": 0.8},
                {"label": "ACCEPT", "confidence": 0.8, "entropy": 0.2, "utility": 0.7},
                {"label": "REJECT", "confidence": 0.95, "entropy": 0.05, "utility": 0.9},
                # One low confidence one to test threshold separation
                {"label": "REVIEW", "confidence": 0.3, "entropy": 0.8, "utility": 0.1} 
            ],
            "memory": [
                {"store_type": "CanonicalStore", "status": "ACTIVE"},
                {"store_type": "CanonicalStore", "status": "ACTIVE"},
                {"store_type": "QuarantineStore", "dwell_time": 10}
            ],
            "recall": [
                {"eu_delta": 0.1},
                {"eu_delta": 0.2},
                {"eu_delta": -0.05} # Some noise is fine
            ],
            "design_eval": [
                {"completeness_score": 1.0, "consistency_score": 1.0},
                {"completeness_score": 0.9, "consistency_score": 0.9}
            ]
        }
        
        engine.load_logs(logs)
        report = engine.evaluate()
        
        assert report["overall_status"] == EvaluationStatus.OK.value
        assert report["decision_stats"]["label_ratios"]["ACCEPT"] == 0.5
        assert report["decision_stats"]["confidence"]["mean"] > 0.7
        
        # Threshold check
        threshold_res = report["threshold_verification"][0]
        assert threshold_res["threshold"] == "confidence >= 0.40"
        assert threshold_res["above_threshold"]["mean_utility"] > threshold_res["below_threshold"]["mean_utility"]

    def test_warning_run(self, engine):
        """
        Tests a scenario that should trigger warnings.
        """
        logs = {
            "decisions": [
                {"label": "REVIEW", "confidence": 0.5, "entropy": 0.9, "utility": 0.2},
                {"label": "REVIEW", "confidence": 0.5, "entropy": 0.9, "utility": 0.2},
                {"label": "REVIEW", "confidence": 0.5, "entropy": 0.9, "utility": 0.2},
                {"label": "ACCEPT", "confidence": 0.5, "entropy": 0.9, "utility": 0.2},
            ], # 75% Review
            "memory": [],
            "recall": [],
            "design_eval": []
        }
        
        engine.load_logs(logs)
        report = engine.evaluate()
        
        assert report["overall_status"] == EvaluationStatus.WARNING.value
        assert "High REVIEW rate" in report["decision_stats"]["note"]

    def test_empty_logs(self, engine):
        """
        Tests resilience to empty logs.
        """
        engine.load_logs({})
        report = engine.evaluate()
        
        assert report["overall_status"] == EvaluationStatus.OK.value
        assert report["decision_stats"]["status"] == EvaluationStatus.OK.value
        assert report["decision_stats"]["note"] == "No data"

    def test_export_json(self, engine):
        logs = {
             "decisions": [{"label": "ACCEPT", "confidence": 0.9, "utility": 0.9}]
        }
        engine.load_logs(logs)
        report = engine.evaluate()
        json_output = engine.export(report, format="json")
        
        assert "decision_stats" in json_output
        assert "confidence" in json_output
