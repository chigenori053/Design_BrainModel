from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, Any, Optional
import uuid


@dataclass(frozen=True)
class EvidenceRecord:
    id: str
    summary: str
    source: str


class EvidenceStore:
    """
    PhaseA: Evidence Store（最小実装）
    Evidence は Knowledge として保存しない。
    """
    def __init__(self):
        self._records: Dict[str, EvidenceRecord] = {}

    def store_evidence(self, summary: str, source: str) -> str:
        record_id = str(uuid.uuid4())
        self._records[record_id] = EvidenceRecord(
            id=record_id,
            summary=summary,
            source=source,
        )
        return record_id

    def get(self, evidence_id: str) -> Optional[EvidenceRecord]:
        return self._records.get(evidence_id)

    def snapshot(self) -> Dict[str, EvidenceRecord]:
        return dict(self._records)

