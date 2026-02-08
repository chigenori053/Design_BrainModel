from __future__ import annotations

from enum import Enum
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field, ConfigDict, field_validator, model_validator
import uuid
import time


class KnowledgeType(str, Enum):
    STRUCTURAL = "STRUCTURAL"
    ALGORITHMIC = "ALGORITHMIC"
    CONSTRAINT = "CONSTRAINT"
    EXPERIENCE = "EXPERIENCE"


class OriginSourceType(str, Enum):
    HUMAN = "HUMAN"
    WEB = "WEB"
    DOC = "DOC"


class Constraint(BaseModel):
    name: str
    description: str


class Scope(BaseModel):
    domain: str
    conditions: Dict[str, Any] = Field(default_factory=dict)


class KnowledgeOrigin(BaseModel):
    source_type: OriginSourceType
    evidence_id: Optional[str] = None

    @model_validator(mode="after")
    def _validate_evidence_id(self) -> "KnowledgeOrigin":
        if self.source_type in {OriginSourceType.WEB, OriginSourceType.DOC} and not self.evidence_id:
            raise ValueError("origin.evidence_id は WEB/DOC 起源の場合に必須です。")
        return self


class KnowledgeUnit(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    type: KnowledgeType

    abstract_structure: Dict[str, Any]
    constraints: List[Constraint] = Field(default_factory=list)
    applicability_scope: Scope

    origin: KnowledgeOrigin

    confidence: Optional[float] = None
    created_at: float = Field(default_factory=lambda: float(time.time()))

    model_config = ConfigDict(extra="forbid")

    @field_validator("abstract_structure")
    @classmethod
    def _validate_abstract_structure(cls, v: Dict[str, Any]) -> Dict[str, Any]:
        if not v:
            raise ValueError("abstract_structure は空にできません。")
        return v

    @model_validator(mode="after")
    def _validate_confidence(self) -> "KnowledgeUnit":
        if self.confidence is not None and not (0.0 <= self.confidence <= 1.0):
            raise ValueError("confidence は 0.0〜1.0 の範囲である必要があります。")
        return self


class KnowledgeStoreInput(BaseModel):
    """
    保存条件の検証用入力。
    Human Override が true のときのみ Knowledge を保存できる。
    """
    knowledge: KnowledgeUnit
    human_override: bool
    reusability_confirmed: bool

