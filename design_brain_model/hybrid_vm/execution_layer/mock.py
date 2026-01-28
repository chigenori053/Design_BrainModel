import hashlib
import json
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
        # Mock Logic: Deterministic outcome based on structure hash
        payload = json.dumps(structure, sort_keys=True, separators=(",", ":"))
        digest = hashlib.sha256(payload.encode("utf-8")).hexdigest()
        outcome = int(digest[:8], 16) / 0xFFFFFFFF
        
        if outcome > 0.7:
            return True, "System running successfully. Throughput: 1000 req/s", None
        elif outcome > 0.4:
            # Implementation Error (Auto-fixable in theory)
            return False, "Error: ConnectionRefused on port 5432", "implementation"
        else:
            # Design Error (Needs user feedback)
            return False, "Error: Architecture mismatch. Component 'DB' requires 'Storage' interface.", "design"
