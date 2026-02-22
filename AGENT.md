# Agent Operational Charter
Version: 1.0
Scope: Entire Repository

---

## 1. Authority Hierarchy

Priority Order:
1. specs/
2. Rule.md
3. TASK_STATE.yaml
4. Implementation Code

Implementation must never override specification.

---

## 2. Role Definitions

### Architect (Human)
- Approves specifications
- Resolves architectural conflicts
- Final decision authority

### ResearchAgent (Gemini CLI)
- Generates specification proposals
- Produces alternative architectural designs
- Does NOT write production code
- Does NOT modify implementation files

### CodingAgent (Codex CLI)
- Implements approved specifications
- Refactors for structural integrity
- Generates tests
- Does NOT redefine architecture

### ValidationAgent
- Verifies spec-implementation alignment
- Runs performance and regression checks
- Reports violations

---

## 3. Change Protocol

Step 1: Proposal created in specs/
Step 2: Architect approval
Step 3: TASK_STATE.yaml updated to approved
Step 4: CodingAgent implementation
Step 5: ValidationAgent review
Step 6: Mark completed

No direct implementation without spec reference.

---

## 4. Specification Rules

- Every task must reference exactly one primary spec file.
- Spec version must be incremented when structural changes occur.
- Deprecated specs moved to specs/deprecated/

---

## 5. Conflict Resolution

If ambiguity occurs:
1. Refer to Rule.md
2. If unresolved -> Architect decision required

Agents must not self-resolve structural contradictions.

---

## Governance Enforcement

- Tasks may not skip status transitions.
- Completion requires review state.
- Spec file must exist before implementation.
- Technical debt must be recorded immediately.

---

## Specification Quality Enforcement

- Specs must follow the standard template.
- Tasks cannot enter implementation without approved spec.
- Technical debt must include priority and status.
- Architect approval is mandatory before implementation.
