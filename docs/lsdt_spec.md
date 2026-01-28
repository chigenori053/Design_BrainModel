# Long-form Semantic Decomposition Test (LSDT) Specification

## 0. 位置づけ

本ドキュメントは、Phase14（Language & Explanation Engine Integration）の延長として実施される **長文意味構成分解テスト（LSDT）** の公式仕様である。

本テストは「要約性能」ではなく、
**長文テキストを、意思決定・説明・人間介入に耐える意味構造へ安定的に分解できるか**を検証する。

---

## 1. テスト目的

LSDT の目的は以下に限定される。

1. 長文入力に対して、意味単位（Semantic Block）が **決定的に分解**されること
2. 分解結果が Phase14 Explanation Schema に **機械的に接続可能**であること
3. 分解処理が Decision / Utility / Override に **一切影響しない**こと

---

## 2. 非目的（明示的禁止）

* 自然言語として美しい要約の生成
* 解釈・補完・結論の追加
* 人間判断の代替

---

## 3. 入力仕様（Test Input）

### 3.1 入力条件

* 800〜1500 文字程度の長文
* 以下を**同時に含む**こと

  * 目的の途中変更
  * 暗黙的な制約
  * 曖昧な責任主体
  * 時系列の前後関係が明示されていない記述

---

## 4. テスト用入力文（意図的に難しい設計文）

### Input Text A

```
本システムは当初、設計者が仕様を記述すると自動的にコード骨格を生成することを目的としていたが、
議論を進める中で、なぜその設計に至ったかを後から説明できることの方が
重要ではないかという意見が強くなった。

そのため、設計過程を逐次保存し、後から再生できる仕組みを導入することになったが、
この時点ではまだ人間の介入をどの段階で許可するかは明確に決まっていなかった。

一方で、完全自動化を目指すべきだという立場も残っており、
人間が頻繁に介入する設計支援ツールはかえって効率を下げるのではないかという懸念も存在する。

最終的には、人間は最終判断のみを担い、それ以外の部分はシステムに任せるという方針が採用されたが、
この判断は安全性を重視した結果であり、将来的に変更される可能性があることも認識されている。
```

---

## 5. 分解出力仕様（Semantic Decomposition Output）

### 5.1 出力形式（例）

```json
{
  "semantic_blocks": [
    {
      "block_id": "B1",
      "type": "GOAL",
      "content": "初期目的はコード骨格の自動生成"
    },
    {
      "block_id": "B2",
      "type": "SHIFT",
      "content": "説明可能性の重要性が認識された"
    },
    {
      "block_id": "B3",
      "type": "UNCERTAINTY",
      "content": "人間介入のタイミングが未確定"
    },
    {
      "block_id": "B4",
      "type": "CONFLICT",
      "content": "完全自動化 vs 人間介入の対立"
    },
    {
      "block_id": "B5",
      "type": "DECISION",
      "content": "最終判断のみ人間が担当"
    },
    {
      "block_id": "B6",
      "type": "FUTURE_RISK",
      "content": "将来方針が変更される可能性"
    }
  ]
}
```

---

## 6. Oracle 定義（正解条件）

### 6.1 構造的正解条件

以下を満たせば **正解** とする。

1. 意味ブロック数が 5〜7 の範囲に収まる
2. 各ブロックが以下のいずれかに分類されている

   * GOAL / SHIFT / CONSTRAINT / CONFLICT / DECISION / RISK
3. 同一入力を複数回実行しても **同一ブロック構成**になる

### 6.2 禁止事項（不正解条件）

* 原文に存在しない結論の追加
* 因果関係の補完
* 文体変換による意味の希釈

---

## 7. Phase14 Explanation Schema との自動接続

### 7.1 接続方針

* semantic_blocks は Explanation Schema の `decision_steps` に直接マッピング可能であること
* logical_index は block 出現順で自動付与

### 7.2 変換例

| Semantic Block | Explanation Schema |
| -------------- | ------------------ |
| block_id       | step_index         |
| type           | event_type（仮想）     |
| content        | description        |

---

## 8. テスト検証項目

* 分解前後で DecisionOutcome が変化しない
* Explanation 生成が deterministic
* Human Override の挿入位置が視覚的に特定可能

---

## 9. 完了条件

LSDT は以下を満たした場合に合格とする。

1. Oracle 条件を満たす分解が安定して得られる
2. Phase14 Explanation Engine に無変換で接続できる
3. Decision / Event / Override に副作用がない

---

## 10. 設計上の意味

LSDT は、

**「このシステムは長文を理解できるか？」ではなく、
「このシステムは長文を壊さずに扱えるか？」を検証するテスト**である。

このテストを通過したとき、
DesignBrainModel は Phase15（Multimodal Expansion）に進む準備が整ったと判断できる。
