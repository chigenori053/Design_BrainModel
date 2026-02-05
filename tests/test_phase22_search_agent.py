import pytest
from design_brain_model.brain_model.phase22.agent import SearchAgent
from design_brain_model.brain_model.phase22.types import SearchRequest, SearchArtifact, SearchState

class TestPhase22SearchAgent:

    def test_shallow_search_flow(self):
        agent = SearchAgent()
        request = SearchRequest(
            query="Design Patterns 2026",
            mode="shallow",
            parameters={"max_sources": 3}
        )
        
        artifact = agent.execute_search(request)
        
        assert isinstance(artifact, SearchArtifact)
        assert len(artifact.sources) == 3
        assert agent.state == SearchState.COMPLETED
        assert "shallow" in artifact.mode

    def test_deep_search_fixed_parameters(self):
        """
        Verify DeepSearch is just more volume, not autonomous reasoning.
        """
        agent = SearchAgent()
        request = SearchRequest(
            query="Quantum Architecture",
            mode="deep",
            parameters={"max_sources": 10}
        )
        
        artifact = agent.execute_search(request)
        assert len(artifact.sources) >= 10
        assert artifact.query == "Quantum Architecture" # Query remains fixed

    def test_isolation_from_human_constraint(self):
        """
        SearchAgent must reject non-human requests.
        """
        agent = SearchAgent()
        request = SearchRequest(
            query="Auto search",
            requested_by="agent" # Prohibited
        )
        
        with pytest.raises(ValueError, match="only accepts requests explicitly from 'human'"):
            agent.execute_search(request)

    def test_artifact_content_neutrality(self):
        """
        Verify that artifact contains raw info, not evaluations.
        """
        agent = SearchAgent()
        request = SearchRequest(query="Safety Specs")
        artifact = agent.execute_search(request)
        
        # Check that we don't have evaluative fields (inferred from lack of them in type)
        assert not hasattr(artifact, 'summary')
        assert not hasattr(artifact, 'recommendation')
        assert not hasattr(artifact, 'score')

    def test_no_side_effects_on_l2(self):
        """
        Verify SearchAgent has no access to Core/L2 components.
        (Structural check: it doesn't import them).
        """
        # This is a static analysis check. SearchAgent should not have any 
        # imports from brain_model.phase20 or similar.
        import sys
        # Clear modules to be sure
        modules = [m for m in sys.modules if 'design_brain_model.brain_model.phase20' in m]
        # In a real test we'd ensure SearchAgent.py doesn't contain these strings
        with open("design_brain_model/brain_model/phase22/agent.py", "r") as f:
            content = f.read()
            assert "phase20" not in content
            assert "phase21" not in content
            assert "L2" not in content
            assert "Proposal" not in content
            assert "DesignIssue" not in content
