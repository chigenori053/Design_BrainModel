# System Rules
Version: 1.0

---

## 1. Structural Integrity Rules

- No circular dependency introduction.
- No cross-layer direct access.
- Memory layer interfaces are immutable without version bump.
- State machine changes require version increment.

---

## 2. Specification Discipline

- No code modification without spec reference.
- Spec file must include:
  - Purpose
  - Interfaces
  - Data Structures
  - State Transitions (if applicable)

---

## 3. AI Collaboration Rules

- ResearchAgent may propose, never implement.
- CodingAgent may implement, never redesign.
- ValidationAgent cannot modify code.
- Agents may not edit TASK_STATE.yaml unless specified owner.

---

## 4. State Governance Rules

Allowed task status:
- proposed
- approved
- in_progress
- blocked
- review
- completed

Status transitions:
proposed -> approved
approved -> in_progress
in_progress -> review
review -> completed

Rollback requires Architect approval.
