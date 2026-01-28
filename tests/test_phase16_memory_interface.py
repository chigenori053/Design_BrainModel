import pytest
import numpy as np
from typing import Dict, List

from design_brain_model.brain_model.memory.interface import OpticalMemoryInterface, MemoryHit
from design_brain_model.brain_model.memory.types import SemanticRepresentation
from design_brain_model.brain_model.language_engine import LanguageEngine

class MockOpticalMemory(OpticalMemoryInterface):
    """
    A mock implementation of the OpticalMemoryInterface for testing purposes.
    It stores vectors in a simple dictionary and uses dot product for resonance.
    """
    def __init__(self):
        self._storage: Dict[str, np.ndarray] = {}

    def save(self, key: str, vector: np.ndarray) -> None:
        """Saves the vector, ensuring it's a complex type for dot product consistency."""
        print(f"DEBUG: Saving key {key} with vector shape {vector.shape}")
        self._storage[key] = vector.astype(np.complex128)

    def query(self, vector: np.ndarray, top_k: int = 1) -> List[MemoryHit]:
        """Queries using the dot product of normalized complex vectors."""
        query_vector = vector.astype(np.complex128)
        
        hits = []
        for key, stored_vector in self._storage.items():
            # Using np.vdot for the conjugate dot product of complex vectors.
            # The absolute value gives the magnitude of the projection (interference strength).
            resonance = np.abs(np.vdot(query_vector, stored_vector))
            hits.append(MemoryHit(key=key, resonance=resonance))
        
        # Sort by resonance in descending order
        hits.sort(key=lambda x: x.resonance, reverse=True)
        
        return hits[:top_k]

@pytest.fixture
def engine():
    return LanguageEngine()

@pytest.fixture
def memory():
    return MockOpticalMemory()

def test_full_save_recall_interpret_flow(engine, memory):
    """
    Tests the complete workflow: creating representations, saving, recalling, and interpreting.
    """
    # 1. Create two different semantic representations
    sr1 = engine.create_representations_from_text("This is the first sentence about cats.")[0]
    sr2 = engine.create_representations_from_text("This is a second, different sentence about dogs.")[0]

    # 2. Save the first representation to memory
    key1, vector1 = sr1.export_for_memory()
    memory.save(key1, vector1)

    # 3. Use the second representation to recall from memory
    hits = sr2.recall(memory, top_k=1)
    
    # 4. Interpret the result with a reasonable threshold
    threshold = 0.5
    recall_result = sr2.interpret_recall(hits, threshold)

    # Since the sentences are different, we expect a low resonance and no recall
    assert not recall_result.recalled
    assert recall_result.best_hit_id is None
    assert recall_result.resonance < threshold
    
    # Check that confidence was updated based on the (low) resonance
    assert sr2.confidence == pytest.approx(recall_result.resonance)
    assert sr2.entropy == pytest.approx(1.0 - recall_result.resonance)


def test_identical_input_produces_high_resonance(engine, memory):
    """
    Tests that recalling with a semantically identical (but different ID) object
    results in a high resonance and successful recall.
    """
    # 1. Create two identical semantic representations
    text = "A sentence that will be repeated."
    sr1 = engine.create_representations_from_text(text)[0]
    sr_clone = engine.create_representations_from_text(text)[0]
    
    # Ensure they are different objects with different IDs
    assert sr1.id != sr_clone.id

    # 2. Save the first representation
    key1, vector1 = sr1.export_for_memory()
    memory.save(key1, vector1)

    # 3. Recall using the clone
    hits = sr_clone.recall(memory, top_k=1)
    
    # 4. Interpret the result
    threshold = 0.9
    recall_result = sr_clone.interpret_recall(hits, threshold)

    # Expect high resonance and a successful recall
    assert recall_result.recalled
    assert recall_result.best_hit_id == sr1.id
    assert recall_result.resonance == pytest.approx(1.0) # Should be very close to 1.0
    
    # Check confidence update
    assert sr_clone.confidence == pytest.approx(1.0)


def test_recall_ignores_self(engine, memory):
    """
    Tests that an object does not recall itself from memory, even if it's the top hit.
    """
    # 1. Create and save a representation
    sr1 = engine.create_representations_from_text("A lonely sentence.")[0]
    key1, vector1 = sr1.export_for_memory()
    memory.save(key1, vector1)
    
    # 2. Add another, different representation
    sr2 = engine.create_representations_from_text("A different thought.")[0]
    key2, vector2 = sr2.export_for_memory()
    memory.save(key2, vector2)

    # 3. Recall using the first object. The top hit will be itself.
    # The implementation should ignore it and check the next one.
    hits = sr1.recall(memory, top_k=2) # Get top 2 to see past self
    
    # 4. Interpret the result
    threshold = 0.5
    recall_result = sr1.interpret_recall(hits, threshold)

    # The next-best hit (sr2) should have low resonance.
    assert not recall_result.recalled
    assert recall_result.best_hit_id is None

def test_empty_memory_recall(engine, memory):
    """
    Tests that recalling from an empty memory returns a non-recalled result.
    """
    sr1 = engine.create_representations_from_text("Querying an empty void.")[0]
    
    hits = sr1.recall(memory, top_k=1)
    recall_result = sr1.interpret_recall(hits, 0.5)
    
    assert not recall_result.recalled
    assert recall_result.best_hit_id is None
    assert recall_result.resonance == 0.0
    assert sr1.confidence == 0.0
    assert sr1.entropy == 1.0
