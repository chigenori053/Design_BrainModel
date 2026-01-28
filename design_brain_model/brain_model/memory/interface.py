from typing import Protocol, List
from dataclasses import dataclass
import numpy as np

@dataclass
class MemoryHit:
    """Represents a single result from a memory query."""
    key: str
    resonance: float # Similarity score (e.g., inner product, cosine similarity)

class OpticalMemoryInterface(Protocol):
    """
    Defines the minimal, purely abstract interface for Optical Memory.
    This memory knows nothing about semantics; it only handles keys and vectors.
    (Phase16 Specification)
    """

    def save(self, key: str, vector: np.ndarray) -> None:
        """
        Saves a vector with its associated key.
        The caller is responsible for ensuring the vector is normalized.
        """
        ...

    def query(self, vector: np.ndarray, top_k: int = 1) -> List[MemoryHit]:
        """
        Queries the memory with a vector and returns the top_k most similar hits.
        """
        ...
