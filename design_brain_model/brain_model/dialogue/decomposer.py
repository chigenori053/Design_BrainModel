from typing import Optional
from .types import DecomposedElements

class Decomposer:
    """
    自由記述入力を設計要素に分解する (Spec Vol.2 Sec 4)
    """
    def decompose(self, raw_input: str) -> DecomposedElements:
        # 実際にはNLPモデル等を使用するが、ここでは簡易的な文字列抽出を行う
        # 推測補完は禁止 (Sec 4.4)
        elements = DecomposedElements()
        
        lines = raw_input.split('\n')
        for line in lines:
            if "目的:" in line:
                elements.objective = line.replace("目的:", "").strip()
            elif "範囲:" in line:
                elements.scope_in = [item.strip() for item in line.replace("範囲:", "").split(',')]
            # ... 他のフィールドも同様に簡易抽出可能にする
            
        return elements