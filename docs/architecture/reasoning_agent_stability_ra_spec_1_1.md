# ReasoningAgent 安定化仕様

RA-SPEC-1.1

- 対象
  - crates/reasoning_agent
  - crates/memory_space

## 目的

- Recall-First 推論の安定性保証
- MemorySpace の誤共鳴検出
- 仮説爆発防止
- 決定論推論保証

## 1 Reasoning Pipeline 仕様

```text
reason(input)
  ↓
recall
  ↓
(resonance ≥ threshold)
  ├ yes → reuse memory solution
  └ no
       ↓
   hypothesis generation
       ↓
   simulation
       ↓
   evaluation
       ↓
   argmax selection
```

## 2 ReasoningAgent パラメータ

- recall_threshold : f64
- top_k : usize
- max_hypotheses : usize
- max_depth : usize
- entropy_threshold : f64

## 3 Resonance Entropy

`p_i = r_i / Σr_i`

`H = -Σ p_i log(p_i)`

判定:

```text
if H > entropy_threshold
   → recall unreliable
```

## 4 Recall 判定仕様

Recall 成功条件:

```text
best_resonance ≥ recall_threshold
AND
entropy ≤ entropy_threshold
```

## 5 Hypothesis Generation

- generated_hypotheses ≤ max_hypotheses
- deterministic ordering

## 6 Simulation

- next_state = bind(state, action)
- simulation depth ≤ max_depth
- state_norm ≤ state_bound

## 7 Evaluation

- MemoryEngine query
- top-k resonance
- score = mean(resonance)

## 8 Argmax Selection

- best = argmax(score)
- tie-break: lowest hypothesis index

## 9 Determinism Requirement

same input → same output を保証する。

## 10 MemorySpace Interaction

Reasoning 中の memory mutation を禁止し、query only で利用する。

## 11 リスク検証テスト

- recall_override
- resonance_entropy
- false_resonance
- hypothesis_growth
- simulation_stability
- evaluation_determinism
- argmax
- latency
