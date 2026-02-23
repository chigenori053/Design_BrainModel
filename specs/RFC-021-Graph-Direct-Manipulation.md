# RFC-021: Graph Direct Manipulation
Version: 1.0
Status: approved

---

## Purpose
Allow users to directly manipulate the GUI causal graph for rapid exploration and what-if editing.

---

## Interfaces
- GUI node drag interaction to reposition graph nodes.
- GUI action to add an edge between selected nodes.
- GUI action to remove selected node and connected edges.

---

## Data Structures
- `AppState.graph_positions: map<node_id, (x, y)>`
- `AppState.edge_builder_from: Option<String>`

Invariants:
- Removing a node removes all incident edges.
- Edge creation must avoid duplicates and self-loops.
- Graph edits affect UI graph state only (not persisted into semantic model) in Phase7.

---

## State Transitions (if applicable)
- `Idle -> Editing` on direct graph manipulation.
- Graph edits do not trigger analysis automatically.

---

## Constraints
- Must preserve current analyze/explain flow.
- Interaction must remain responsive on typical graph sizes.

---

## Acceptance Criteria
- Node drag updates visual placement immediately.
- User can add edge from one selected node to another.
- User can delete selected node and see graph/edge updates.
