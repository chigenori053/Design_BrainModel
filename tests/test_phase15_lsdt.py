import pytest
from typing import Dict, List, Any

from design_brain_model.brain_model.language_engine import decompose_text


# --- Test Data from docs/lsdt_spec.md ---

INPUT_TEXT_A = """本システムは当初、設計者が仕様を記述すると自動的にコード骨格を生成することを目的としていたが、
議論を進める中で、なぜその設計に至ったかを後から説明できることの方が
重要ではないかという意見が強くなった。

そのため、設計過程を逐次保存し、後から再生できる仕組みを導入することになったが、
この時点ではまだ人間の介入をどの段階で許可するかは明確に決まっていなかった。

一方で、完全自動化を目指すべきだという立場も残っており、
人間が頻繁に介入する設計支援ツールはかえって効率を下げるのではないかという懸念も存在する。

最終的には、人間は最終判断のみを担い、それ以外の部分はシステムに任せるという方針が採用されたが、
この判断は安全性を重視した結果であり、将来的に変更される可能性があることも認識されている。
"""

# As per Section 6.1 of lsdt_spec.md, the valid types for semantic blocks.
# The example output includes 'UNCERTAINTY', while the Oracle definition specifies 'CONSTRAINT'.
# To be robust, this test suite accepts types from both the definition and the example.
VALID_BLOCK_TYPES = (
    "GOAL",
    "SHIFT",
    "CONSTRAINT", # From Oracle Definition
    "CONFLICT",
    "DECISION",
    "RISK",
    "UNCERTAINTY", # From Example Output
)


def test_lsdt_output_structure_is_valid():
    """
    Verifies that the output structure conforms to the LSDT specification (Section 5 & 6).
    - Checks for the presence and type of the 'semantic_blocks' key.
    - Validates that the number of blocks is within the accepted range [5, 7].
    - Ensures each block contains the required keys ('block_id', 'type', 'content').
    - Confirms that the 'type' of each block is among the officially defined valid types.
    """
    # Execute the function under test
    result = decompose_text(INPUT_TEXT_A)

    # 1. Validate top-level structure
    assert isinstance(result, dict), "The root of the output must be a dictionary."
    assert "semantic_blocks" in result, "The output must contain a 'semantic_blocks' key."
    
    blocks = result["semantic_blocks"]
    assert isinstance(blocks, list), "The 'semantic_blocks' value must be a list."

    # 2. Validate block count against Oracle Definition (6.1.1)
    assert 5 <= len(blocks) <= 7, f"Expected 5 to 7 blocks, but found {len(blocks)}."

    # 3. Validate individual block structure and content
    required_keys = {"block_id", "type", "content"}
    all_block_ids = set()

    for i, block in enumerate(blocks):
        assert isinstance(block, dict), f"Block at index {i} must be a dictionary."
        
        # Check for required keys
        assert required_keys.issubset(block.keys()), f"Block at index {i} is missing required keys. Found: {list(block.keys())}"
        
        # Check block_id uniqueness and format
        block_id = block["block_id"]
        assert isinstance(block_id, str) and block_id, f"block_id at index {i} must be a non-empty string."
        assert block_id not in all_block_ids, f"Found duplicate block_id: {block_id}"
        all_block_ids.add(block_id)

        # Check type validity against Oracle Definition (6.1.2)
        block_type = block["type"]
        assert block_type in VALID_BLOCK_TYPES, f"Block '{block_id}' has an invalid type '{block_type}'."
        
        # Check content
        block_content = block["content"]
        assert isinstance(block_content, str) and block_content, f"Content for block '{block_id}' must be a non-empty string."


def test_lsdt_is_deterministic():
    """
    Verifies that the decomposition is deterministic, as required by the spec (Section 6.1.3).
    Executing the function multiple times on the same input must produce identical results.
    """
    # Execute the function multiple times
    result1 = decompose_text(INPUT_TEXT_A)
    result2 = decompose_text(INPUT_TEXT_A)

    # Compare the results
    assert result1 == result2, "Decomposition output is not deterministic. Subsequent calls returned different results."
