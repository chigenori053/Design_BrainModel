# RFC-020: Pareto Optimization Engine
Version: 1.0
Status: approved

---

## Purpose
Automate trade-off resolution for design drafts by prioritizing non-dominated candidates on a Pareto frontier.

---

## Interfaces
- `HybridVM::pareto_optimize_drafts(drafts: Vec<DesignDraft>) -> Vec<DesignDraft>`
- CLI `explain` must return drafts ordered by Pareto rank first, then impact score.

---

## Data Structures
- `ParetoPoint`
  - `draft_id: String`
  - `stability_gain: f64` (maximize)
  - `ambiguity_cost: f64` (minimize)
  - `complexity_cost: f64` (minimize)
- `ParetoRank`
  - `draft_id: String`
  - `rank: usize`

Invariants:
- Same input draft set yields deterministic ordering.
- No draft is dropped unless duplicated by ID.

---

## State Transitions (if applicable)
Not applicable.

---

## Constraints
- Keep API compatibility for existing draft generation calls.
- Sorting complexity must remain acceptable for small candidate sets (<= 32).

---

## Acceptance Criteria
- `explain` output order reflects Pareto priority.
- Dominated drafts are ranked lower than non-dominated drafts.
- Existing CLI tests continue to pass.
