from enum import Enum, auto
from dataclasses import dataclass, field
from typing import List, Optional, Dict
import uuid
from datetime import datetime

class SearchState(str, Enum):
    IDLE = "IDLE"
    SEARCHING = "SEARCHING"
    COMPLETED = "COMPLETED"
    FAILED = "FAILED"

@dataclass
class SearchRequest:
    """
    Input contract for SearchAgent. Must be requested by human.
    """
    request_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    query: str = ""
    mode: str = "shallow" # shallow | deep
    parameters: Dict = field(default_factory=lambda: {
        "max_sources": 5,
        "max_depth": 1,
        "domains": [],
        "language": "en"
    })
    requested_by: str = "human"

@dataclass
class SearchSource:
    source_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    url: str = ""
    retrieved_at: str = field(default_factory=lambda: datetime.now().isoformat())
    raw_content: str = ""
    metadata: Dict = field(default_factory=dict) # title, author, etc.

@dataclass
class SearchArtifact:
    """
    The only output from SearchAgent. Contains raw, unevaluated information.
    NO summary, NO judgment, NO recommendations.
    """
    artifact_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    request_id: str = ""
    query: str = ""
    mode: str = "shallow"
    sources: List[SearchSource] = field(default_factory=list)
    extraction_notes: str = "" # Factual notes about the search process
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())
