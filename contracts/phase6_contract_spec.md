# Phase6 Contract Spec

## Objective

Phase6.5 Step1 establishes a single source of truth for runtime reasoning contracts.
All Phase6 runtime boundaries must use the canonical types from `crates/contracts`.

## Canonical Types

- `ReasoningInput`
- `SemanticRepresentation`
- `MemoryCandidate`
- `Hypothesis`
- `Relation`
- `EvaluationScore`
- `Decision`
- `ValidationResult`
- `ReasoningTrace`
- `TraceStep`
- `TraceStats`

## Canonical Rules

- Contract types are defined only in `crates/contracts/src/lib.rs`.
- Other Phase6 boundary crates must import or re-export those types. They must not redefine them.
- `Context` is never `None`. Empty context uses an empty struct value.
- `SemanticRepresentation.intents` and `SemanticRepresentation.relations` are stable-sorted.
- `SemanticRepresentation.hash` is deterministic for the same semantic content.
- `MemoryCandidate.score` is normalized into `[0,1]`.
- Candidate ordering is stable: `score desc`, then `id asc`.
- `Hypothesis.parent` forms a DAG. Cycles are forbidden.
- `Hypothesis.state_hash` is the duplicate-pruning key.
- `Hypothesis.semantic_hash` is the semantic duplicate-pruning key.
- `EvaluationScore.total` is computed from normalized weights where the sum of weights is `1.0`.
- `ScoreParts.goal_distance` stores the transformed "higher is better" score.
- `Decision` depends only on `EvaluationScore`.
- `ValidationResult` never mutates score.
- `ReasoningTrace.steps` are sorted by `depth`.
- `ReasoningTrace.stats` must be recomputable from `steps`.

## Layer Boundaries

| Layer | Input | Output | Must Not |
| --- | --- | --- | --- |
| Semantic | text | `SemanticRepresentation` | score |
| Recall | semantic | `Vec<MemoryCandidate>` | hypothesis generation |
| Search | recall candidates | `Vec<Hypothesis>` | validation |
| Evaluation | hypothesis | `EvaluationScore` | decision |
| Decision | score | `Decision` | validation |
| Validation | hypothesis | `ValidationResult` | score mutation |
| Trace | pipeline metrics | `ReasoningTrace` | state mutation |

## Determinism

- Stable sort all `Vec`s before emitting them.
- Tie-breakers are always deterministic.
- Cache eviction must be deterministic.
- LLM fallback must use strict JSON validation and exact input caching.

## Verification

- `tests/contract` contains contract audit tests and lint checks.
- CI must run the contract audit suite.
