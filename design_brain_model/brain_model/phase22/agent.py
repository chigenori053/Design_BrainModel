from typing import List, Optional
import uuid
from datetime import datetime
from .types import SearchRequest, SearchArtifact, SearchSource, SearchState

class SearchAgent:
    """
    SearchAgent (Phase22-A): Observes the world, never decides.
    Isolated from core design states and memory.
    """

    def __init__(self):
        self._state = SearchState.IDLE

    @property
    def state(self) -> SearchState:
        return self._state

    def execute_search(self, request: SearchRequest) -> SearchArtifact:
        """
        Main entry point for search operations.
        Only accepts requests from 'human'.
        """
        if request.requested_by != "human":
            raise ValueError("SearchAgent only accepts requests explicitly from 'human'.")

        self._state = SearchState.SEARCHING
        
        try:
            # 1. Determine search volume based on mode
            # shallow: basic search
            # deep: more sources/depth but fixed parameters
            limit = request.parameters.get("max_sources", 5)
            if request.mode == "deep":
                # Deep mode fixed to higher limit but no autonomous query generation
                limit = max(limit, 10)

            # 2. Perform Observation (Mocked for logic verification)
            # In real usage, this would call google_web_search or similar tools.
            sources = self._observe_web(request.query, limit=limit)

            artifact = SearchArtifact(
                request_id=request.request_id,
                query=request.query,
                mode=request.mode,
                sources=sources,
                extraction_notes=f"Retrieved {len(sources)} sources for query '{request.query}' in {request.mode} mode."
            )

            self._state = SearchState.COMPLETED
            return artifact

        except Exception as e:
            self._state = SearchState.FAILED
            # Return empty artifact or raise? Spec says SearchArtifact is the output.
            # We return an artifact with failed status/notes.
            return SearchArtifact(
                request_id=request.request_id,
                query=request.query,
                mode=request.mode,
                extraction_notes=f"Search FAILED: {str(e)}"
            )

    def _observe_web(self, query: str, limit: int) -> List[SearchSource]:
        """
        Internal observation method. Strictly factual retrieval.
        """
        sources = []
        # Mocking retrieval of sources
        for i in range(limit):
            sources.append(SearchSource(
                url=f"https://example.com/source_{i}",
                raw_content=f"Raw content from source {i} about {query}...",
                metadata={"title": f"External Data Source {i}"}
            ))
        return sources
