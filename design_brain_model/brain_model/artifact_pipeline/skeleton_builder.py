from dataclasses import dataclass, field
from typing import List, Dict, Optional, Any
from design_brain_model.brain_model.language_engine.domain import LanguageReport

@dataclass
class DirectorySkeleton:
    paths: List[str]

@dataclass
class APISkeleton:
    endpoints: List[str]
    methods: List[str]

@dataclass
class TypeSkeleton:
    types: Dict[str, Any]

@dataclass
class SkeletonProvenance:
    l2_id: str
    snapshot_id: str
    language_report_summary: str

@dataclass
class CodeSkeleton:
    directory: DirectorySkeleton
    api: APISkeleton
    types: TypeSkeleton
    provenance: SkeletonProvenance

class CodeSkeletonBuilder:
    """
    Phase 18-1: CodeSkeletonBuilder.
    Visualizes L2 SemanticUnit structure as directory, API, and type skeletons.
    Strictly forbids inference, logic generation, or L2 modification.
    """

    def build(self, l2_semantic_unit: Dict[str, Any], snapshot_id: str, language_report: LanguageReport) -> CodeSkeleton:
        
        # 1. Build Directory Skeleton (Purely from L2 structure)
        # Assuming L2 unit content or properties imply structure. 
        # For now, we extract "components" or similar keys if they exist in the mock L2 dict.
        # If L2 is a DecisionCandidate, "content" might be the starting point.
        
        dir_paths = []
        # Mock logic: extracting paths from content if it resembles a path, else default to module structure
        content = l2_semantic_unit.get("content", "")
        
        # Simple parsing for mock
        path_part = content.split()[0] if content else ""
        
        if "/" in path_part:
            dir_paths.append(path_part)
        else:
             # Basic structure based on L2 ID to ensure uniqueness and traceability
             dir_paths.append(f"src/{l2_semantic_unit.get('candidate_id', 'unknown')}")

        directory = DirectorySkeleton(paths=dir_paths)

        # 2. Build API Skeleton
        # Extract function-like signatures from content
        endpoints = []
        methods = []
        # Very basic extraction for the skeleton
        if "API" in content or "Endpoint" in content:
             endpoints.append("/api/v1/resource") # Placeholder strictly derived if parser existed
             methods.append("GET")

        api = APISkeleton(endpoints=endpoints, methods=methods)

        # 3. Build Type Skeleton
        # Extract potential types
        types = {}
        # Placeholder
        types["L2Entity"] = "object"

        type_skeleton = TypeSkeleton(types=types)

        # 4. Provenance
        provenance = SkeletonProvenance(
            l2_id=l2_semantic_unit.get("candidate_id", ""),
            snapshot_id=snapshot_id,
            language_report_summary=language_report.summary
        )

        return CodeSkeleton(
            directory=directory,
            api=api,
            types=type_skeleton,
            provenance=provenance
        )
