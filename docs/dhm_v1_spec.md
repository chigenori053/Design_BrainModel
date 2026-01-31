# Language DHM v1 Specification (Deterministic Holographic Meaning Projection)

## 0. 位置づけ

Language DHM v1 は、Design_BrainModel における **初期段階の言語化能力**を担う中核コンポーネントである。

本コンポーネントは「生成」や「推論」を行わず、
**既に確定した内部状態を、人間が理解可能な言語構造へ決定的に射影（Projection）する**ことのみを目的とする。

Phase14（Language & Explanation Engine Integration）の実装体であり、
Text UI や将来の UI 群に対する **唯一の説明供給源**となる。

本仕様書では、言語化処理を **数学的写像として定義**し、
実装における決定性・非介入性を数式レベルで保証する。

---

## 1. 設計目的 (Objective)

Language DHM v1 の目的は以下に限定される。

1. 内部状態（Decision / Event / Memory）を **壊さずに言語化**する
2. 言語化結果が **常に再現可能（決定的）**であること
3. Human Override を含む責任構造を **明示的に表現**する

---

## 2. 非目的（明示的禁止事項）

* 新しい結論・理由・判断の生成
* 内部状態の補完・解釈・最適化
* Decision / Utility / Event への影響
* 学習・フィードバック・自己更新

---

## 3. 入力仕様 (Inputs)

Language DHM v1 は以下の **確定済みデータのみ**を入力として受け取る。

### 3.1 必須入力

* DecisionOutcome
* Event Lineage（logical_index 順）
* Semantic Blocks（LSDT 出力）
* Human Override Trace（存在する場合）

### 3.2 禁止入力

* 未確定 State
* wall_timestamp に依存した情報
* 外部テキスト入力

---

## 4. 出力仕様 (Outputs)

### 4.1 出力形式

* Phase14 Explanation Schema に **完全準拠**した構造化テキスト
* JSON / dict 形式（自然文ではない）

### 4.2 出力保証

* 同一入力 → 同一出力
* 出力順序は logical_index に完全準拠

---

## 5. 内部処理フロー（決定的）

Language DHM v1 は、内部状態集合を言語構造集合へ写像する **決定的関数**として定義される。

### 5.1 全体写像

内部状態集合を以下で定義する。

* 決定結果: D
* イベント系列: E = {e_1, e_2, ..., e_n}（logical_index 昇順）
* 意味ブロック集合: S = {s_1, s_2, ..., s_m}
* 人間介入情報: H（存在しない場合は 空集合）

Language DHM v1 は、次の関数として定義される。

L: (D, E, S, H) -> X

ここで X は Phase14 Explanation Schema に準拠した構造化説明である。

---

### 5.2 Selection（説明対象選択）

説明対象集合 T は、次の決定的関数で定義される。

T = f_select(D, E, S, H)

ただし、

* f_select は条件分岐のみを含み、確率項を含まない
* T に含まれる要素数は入力サイズに依存するが、順序は未定義

---

### 5.3 Ordering（順序付け）

T に含まれる要素は、logical_index に基づき全順序付けされる。

T' = sort_logical_index(T)

この操作により、

* T' は常に同一入力に対して同一順序を持つ
* wall_timestamp は一切参照されない

---

### 5.4 Template Mapping（構造→言語写像）

順序付けされた要素 T' に対し、テンプレート写像を適用する。

X = f_template(T')

f_template は有限集合のテンプレート関数から選択される。

---

## 6. テンプレート選択規則

### 6.1 Summary テンプレート

* 最終 Decision
* 判断源（HUMAN_OVERRIDE / CONSENSUS / UTILITY）
* Override の有無

### 6.2 Step テンプレート

* Event / Semantic Block を 1 ステップとして表現
* 因果・評価は **事実列挙のみ**

### 6.3 Override テンプレート

* Override が存在する場合は **必ず最上位表示**

---

## 7. 決定性ルール（数理的定義）

Language DHM v1 の決定性は、次の条件で保証される。

### 7.1 関数決定性

同一入力は常に同一出力を与える。

(D, E, S, H) = (D', E', S', H') のとき、
L(D, E, S, H) = L(D', E', S', H')

### 7.2 禁止操作

以下の操作は **決定性破壊操作**として禁止される。

* 非決定的ソート
* ランダム選択
* 外部状態参照
* wall_timestamp 依存

---

## 8. テスト要件

### 8.1 非介入テスト

* 言語化前後で DecisionOutcome が不変であること

### 8.2 再現性テスト

* 同一 Snapshot → 完全一致出力

### 8.3 Override 明示テスト

* Override が説明内で必ず識別可能

---

## 9. Phase14 との関係

* Language DHM v1 は Phase14 の Explanation Engine の **実装本体**である
* UI / API は本コンポーネントを直接参照してはならない

---

## 10. 将来拡張の境界

Language DHM v1 は以下を **意図的に未実装**とする。

* 自然言語生成（v2 以降）
* 多言語対応
* 文体・感情・説得表現

---

## 11. 設計上の意味

Language DHM v1 は、

**思考状態空間から言語状態空間への決定的射影 L を初めて明示的に定義したフェーズ**である。

この射影が存在することで、

* 思考（Decision / Event）と
* 表現（Language / UI）

は数学的に分離される。

結果として Design_BrainModel は、

* 壊れず
* 再現可能で
* 説明責任を保持したまま

人間と相互作用できる設計支援エンジンとなる。