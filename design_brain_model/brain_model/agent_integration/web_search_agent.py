from __future__ import annotations

from typing import List, Optional

import time
from .types import DesignKnowledgeUnit, SourceRef, new_id
from ..phase22.agent import SearchAgent
from ..phase22.types import SearchRequest, SearchSource


class WebSearchAgent:
    """
    Collects external knowledge without assimilating it.
    Returns raw references as DesignKnowledgeUnit.
    """

    def __init__(self, max_searches_per_session: int = 3, cooldown_seconds: int = 15):
        self._search_agent = SearchAgent()
        self._max_searches = max_searches_per_session
        self._cooldown_seconds = cooldown_seconds
        self._search_count = 0
        self._last_search_at: Optional[float] = None

    def search(self, query: str, requested_by: str = "human", mode: str = "shallow", max_sources: int = 5) -> DesignKnowledgeUnit:
        if not self._can_search():
            return DesignKnowledgeUnit(
                id=new_id("knowledge"),
                query=query,
                content_summary="Search skipped due to cooldown or session limit.",
                relevance=0.0,
                confidence=0.0,
                source_refs=[],
                notes="Search suppressed by safety constraints.",
            )

        # Phase22 SearchAgent requires 'human'. We preserve origin in notes.
        effective_requestor = "human" if requested_by != "human" else requested_by
        request = SearchRequest(
            query=query,
            requested_by=effective_requestor,
            mode=mode,
            parameters={"max_sources": max_sources, "max_depth": 1, "domains": [], "language": "en"},
        )
        artifact = self._search_agent.execute_search(request)

        sources = [self._map_source(src) for src in artifact.sources]
        self._record_search()
        origin_note = "" if requested_by == "human" else f"Triggered by model judgement: {requested_by}. "
        summary = " ".join([s.excerpt for s in sources])[:400]
        confidence = min(1.0, 0.2 + 0.15 * len(sources))
        relevance = min(1.0, 0.1 + 0.18 * len(sources))
        return DesignKnowledgeUnit(
            id=new_id("knowledge"),
            query=query,
            content_summary=summary or "No content extracted.",
            relevance=relevance,
            confidence=confidence,
            source_refs=sources,
            notes=f"{origin_note}{artifact.extraction_notes}",
        )

    @staticmethod
    def _map_source(source: SearchSource) -> SourceRef:
        title = source.metadata.get("title") if isinstance(source.metadata, dict) else ""
        excerpt = source.raw_content[:200]
        return SourceRef(
            url=source.url,
            title=title or "",
            excerpt=excerpt,
            retrieved_at=source.retrieved_at,
        )

    def _can_search(self) -> bool:
        if self._search_count >= self._max_searches:
            return False
        if self._last_search_at is None:
            return True
        return (time.time() - self._last_search_at) >= self._cooldown_seconds

    def _record_search(self) -> None:
        self._search_count += 1
        self._last_search_at = time.time()
