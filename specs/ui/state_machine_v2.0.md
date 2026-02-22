# UI State Machine Redesign
Version: 2.0
Status: approved

---

## Purpose
Define a deterministic UI state transition model for DesignDraft UI to eliminate ambiguous state behavior.

---

## Interfaces
- `UiStateMachine::current_state() -> UiState`
- `UiStateMachine::dispatch(event: UiEvent) -> TransitionResult`
- `TransitionResult`
  - `next_state: UiState`
  - `side_effects: Vec<UiEffect>`

---

## Data Structures
- `UiState`
  - `Idle`
  - `Editing`
  - `Analyzing`
  - `Reviewing`
  - `Error`
- `UiEvent`
  - `StartEdit`
  - `Submit`
  - `AnalysisSucceeded`
  - `AnalysisFailed`
  - `Revise`
  - `Reset`
- Invariants
  - Exactly one active `UiState` at a time.
  - Every accepted event must map to exactly one next state.
  - Undefined transitions are rejected with `TransitionResult` indicating no state change.

---

## State Transitions (if applicable)
Deterministic transition table:

| Current | Event | Next | Notes |
|---|---|---|---|
| Idle | StartEdit | Editing | Begin user input |
| Editing | Submit | Analyzing | Trigger analysis request |
| Analyzing | AnalysisSucceeded | Reviewing | Show analysis result |
| Analyzing | AnalysisFailed | Error | Surface error UI |
| Reviewing | Revise | Editing | Return to edit mode |
| Error | Revise | Editing | Retry from editing |
| * | Reset | Idle | Global reset |

All other event/state combinations are invalid and must not mutate state.

---

## Constraints
- Transition evaluation must be O(1) by table lookup.
- No ambiguous state transitions are permitted.
- Backward compatibility: existing UI screens keep current labels and interaction entry points.

---

## Acceptance Criteria
- Transition table is complete and deterministic for all supported events.
- Undefined transitions are rejected and logged.
- Unit tests cover every valid transition and representative invalid transitions.
- No ambiguous states are observed in integration flow tests.
