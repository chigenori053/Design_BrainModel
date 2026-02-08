from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Tuple, Any
import json

from .types import KnowledgeUnit, KnowledgeStoreInput


@dataclass(frozen=True)
class KnowledgeHit:
    knowledge_id: str
    similarity: float


class KnowledgeStore:
    """
    PhaseA: Knowledge Store (中核仕様)
    - Knowledge の保存/取得/検索のみ
    - 自動生成/自動更新/自動削除は禁止
    """
    def __init__(self, store_dir: Path):
        self.store_dir = store_dir
        self.store_dir.mkdir(parents=True, exist_ok=True)
        self._units_path = self.store_dir / "knowledge_units.jsonl"
        self._cache: Dict[str, KnowledgeUnit] = {}
        self.load()

    def load(self) -> None:
        if not self._units_path.exists():
            return
        with self._units_path.open("r", encoding="utf-8") as f:
            for line in f:
                if not line.strip():
                    continue
                try:
                    data = json.loads(line)
                    unit = KnowledgeUnit(**data)
                    self._cache[unit.id] = unit
                except Exception:
                    # 仕様により不正行は無視
                    continue

    def store_knowledge(self, payload: KnowledgeStoreInput) -> str:
        """
        KnowledgeUnit の保存。保存条件を厳格に検証する。
        """
        unit = payload.knowledge
        if not payload.human_override:
            raise ValueError("Human Override = true が必須です。")
        if not payload.reusability_confirmed:
            raise ValueError("再利用可能性の確認が必須です。")
        if not unit.abstract_structure:
            raise ValueError("抽象構造が明示されていません。")
        if not unit.applicability_scope:
            raise ValueError("適用スコープが定義されていません。")
        if unit.id in self._cache:
            # 自動更新は禁止。既存 ID は拒否。
            raise ValueError("既存 KnowledgeUnit の更新は禁止されています。")

        self._cache[unit.id] = unit
        with self._units_path.open("a", encoding="utf-8") as f:
            f.write(unit.model_dump_json() + "\n")
        return unit.id

    def get_knowledge(self, knowledge_id: str) -> KnowledgeUnit | None:
        return self._cache.get(knowledge_id)

    def recall_knowledge(self, query_structure: Dict[str, Any], top_k: int = 5) -> List[KnowledgeHit]:
        """
        構造類似を優先した Recall。
        類似度は判断材料のみであり、完全一致は要求しない。
        """
        if not query_structure:
            return []
        query_tokens = _tokenize_structure(query_structure)
        if not query_tokens:
            return []

        scored: List[KnowledgeHit] = []
        for unit in self._cache.values():
            unit_tokens = _tokenize_structure(unit.abstract_structure)
            similarity = _jaccard_similarity(query_tokens, unit_tokens)
            if similarity > 0.0:
                scored.append(KnowledgeHit(knowledge_id=unit.id, similarity=similarity))

        scored.sort(key=lambda h: h.similarity, reverse=True)
        return scored[:max(0, top_k)]


def _tokenize_structure(structure: Any) -> List[str]:
    """
    構造の安定的トークン化。
    - dict: キーと値を再帰的にトークン化
    - list/tuple: 要素を順不同トークン化
    - str/int/float/bool: 文字列化
    """
    tokens: List[str] = []
    _walk_structure(structure, tokens)
    return tokens


def _walk_structure(value: Any, tokens: List[str]) -> None:
    if value is None:
        return
    if isinstance(value, dict):
        for k, v in value.items():
            tokens.append(f"key:{k}")
            _walk_structure(v, tokens)
        return
    if isinstance(value, (list, tuple)):
        for item in value:
            _walk_structure(item, tokens)
        return
    tokens.append(f"val:{str(value)}")


def _jaccard_similarity(a: Iterable[str], b: Iterable[str]) -> float:
    set_a = set(a)
    set_b = set(b)
    if not set_a or not set_b:
        return 0.0
    return len(set_a & set_b) / len(set_a | set_b)

