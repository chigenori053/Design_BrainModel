import random
from typing import Dict, Any, Tuple

class MockExecutionEngine:
    """
    Simulates external system execution.
    """
    
    def run_system(self, structure: Dict[str, Any]) -> Tuple[bool, str, str | None]:
        """
        Runs the system defined by 'structure'.
        Returns: (success, result_message, error_type)
        """
        # Mock Logic: Randomly fail to test error feedback loop
        outcome = random.random()
        
        if outcome > 0.7:
            return True, "System running successfully. Throughput: 1000 req/s", None
        elif outcome > 0.4:
            # Implementation Error (Auto-fixable in theory)
            return False, "Error: ConnectionRefused on port 5432", "implementation"
        else:
            # Design Error (Needs user feedback)
            return False, "Error: Architecture mismatch. Component 'DB' requires 'Storage' interface.", "design"
