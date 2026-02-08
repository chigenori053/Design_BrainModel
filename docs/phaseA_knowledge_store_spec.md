# Knowledge Store 正式仕様書（Design_BrainModel / PhaseA）

構成

第1部：Knowledge Store 中核仕様（本書）
第2部：Evidence Store 統合仕様（次書）

※ 両者は 1つの仕様書セットとして扱う

---

## 第1部 Knowledge Store 中核仕様（PhaseA 固定）

### 1. Knowledge Store の目的（最重要）

Knowledge Store の目的は次の一点に限定される。

Design_BrainModel が再利用可能な設計能力を  
安定的・説明可能な形で蓄積すること

Knowledge Store は：

- 学習エンジンではない
- 自律進化を行わない
- 曖昧な知識を許容しない

### 2. Knowledge の正式定義

PhaseA における Knowledge とは：

人間の承認を経て保存された、  
再利用可能な抽象設計構造・アルゴリズム構造・制約構造

である。

含まれるもの

- 設計構造（Graph / AST）
- アルゴリズムの構成
- 再利用条件・制約
- 適用スコープ

含まれないもの

- 生テキスト
- Web 情報の引用
- 思考ログ
- 評価途中の数値

### 3. Knowledge Store の責務境界

#### 3.1 Knowledge Store が行うこと

- Knowledge の保存
- Knowledge の検索（Recall）
- Knowledge の参照
- Knowledge の由来追跡

#### 3.2 Knowledge Store が行ってはいけないこと

- Knowledge の自動生成
- Knowledge の自動更新
- Knowledge の自動削除
- 外部情報の直接保存

### 4. Knowledge の分類（PhaseA固定）

```
KnowledgeType =
  STRUCTURAL      # 設計構造・構成パターン
  ALGORITHMIC     # アルゴリズム構造
  CONSTRAINT      # 制約・前提条件
  EXPERIENCE      # 結果としての成功/失敗
```

### 5. KnowledgeUnit データモデル（正式）

```
KnowledgeUnit:
  id: UUID
  type: KnowledgeType

  abstract_structure: Graph | AST
  constraints: List[Constraint]
  applicability_scope: Scope

  origin:
    source_type: Enum { HUMAN, WEB, DOC }
    evidence_id: Optional[UUID]

  confidence: Optional[Float]
  created_at: Timestamp
```

フィールド制約

abstract_structure

- 再生成可能であること
- 生コード・全文テキスト禁止

confidence

- Human Override 後のみ付与可能

origin.evidence_id

- Web / Doc 起源の場合は必須

### 6. Knowledge 保存条件（厳格）

KnowledgeUnit は 以下すべてを満たす場合のみ保存可能。

- Human Override = true
- 抽象構造が明示されている
- 適用スコープが定義されている
- 再利用可能性が確認されている

### 7. Knowledge のライフサイクル

```
[CREATED]
     ↓
STORED
     ↓
RECALLED
     ↓
USED_IN_DESIGN
```

※ PhaseA では 削除・昇格・劣化処理は行わない

### 8. Recall（検索）仕様

#### 8.1 Recall 対象

Recall 対象 = KnowledgeUnit のみ  
Evidence は Recall 対象外

#### 8.2 Recall 原則

- Recall-First を厳守
- 完全一致ではなく構造類似を優先
- 類似度スコアは 判断材料のみ

### 9. Knowledge Store API（最小）

保存  
`store_knowledge(knowledge_unit) -> KnowledgeID`

取得  
`get_knowledge(knowledge_id) -> KnowledgeUnit`

検索  
`recall_knowledge(query_structure) -> List[KnowledgeUnit]`

### 10. Knowledge Store の禁止事項（明示）

以下は 仕様レベルで禁止。

- WebSearchAgent からの直接書き込み
- KnowledgeUnit の自動編集
- Evidence を Knowledge として保存
- Confidence の自動更新

### 11. PhaseA 完了条件（Knowledge Store）

- KnowledgeUnit の保存・取得が可能
- Recall が Knowledge のみを対象にする
- Origin（由来）が必ず追跡できる
- Evidence Store と完全分離されている

