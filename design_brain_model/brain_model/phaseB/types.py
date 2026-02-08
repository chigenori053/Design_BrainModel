from __future__ import annotations

from enum import Enum
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field, ConfigDict, field_validator, model_validator
import uuid


class EvaluationAxis(str, Enum):
    STRUCTURAL_CONSISTENCY = "Structural Consistency"
    REUSABILITY = "Reusability"
    COMPLEXITY = "Complexity"
    CONSTRAINT_SATISFACTION = "Constraint Satisfaction"
    CLARITY = "Clarity"


EVALUATION_AXES: List[EvaluationAxis] = [
    EvaluationAxis.STRUCTURAL_CONSISTENCY,
    EvaluationAxis.REUSABILITY,
    EvaluationAxis.COMPLEXITY,
    EvaluationAxis.CONSTRAINT_SATISFACTION,
    EvaluationAxis.CLARITY,
]


class EvaluationTargetType(str, Enum):
    DESIGN = "DESIGN"
    TEXT = "TEXT"
    UI = "UI"


class EvaluationTarget(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    type: EvaluationTargetType
    structure: Dict[str, Any]
    notes: Optional[str] = None

    model_config = ConfigDict(extra="forbid")

    @field_validator("structure")
    @classmethod
    def _validate_structure(cls, v: Dict[str, Any]) -> Dict[str, Any]:
        if not v:
            raise ValueError("structure は空にできません。")
        return v


class GeometryPoint(BaseModel):
    vector: List[float]
    source_id: str

    model_config = ConfigDict(extra="forbid")

    @model_validator(mode="after")
    def _validate_vector(self) -> "GeometryPoint":
        if len(self.vector) != len(EVALUATION_AXES):
            raise ValueError("GeometryPoint の次元数が評価軸と一致しません。")
        return self


class EvaluationReport(BaseModel):
    targets: List[str]
    geometry_points: List[GeometryPoint]
    distances: List[List[float]]
    qualitative_notes: Optional[str] = None

    model_config = ConfigDict(extra="forbid")

    @model_validator(mode="after")
    def _validate_matrix(self) -> "EvaluationReport":
        n = len(self.targets)
        if len(self.distances) != n:
            raise ValueError("distances の行数が targets と一致しません。")
        for row in self.distances:
            if len(row) != n:
                raise ValueError("distances の列数が targets と一致しません。")
        return self

