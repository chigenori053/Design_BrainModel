# Phase10 Architecture Specification

Primary spec for the Phase10 implementation.

## Goal

DesignBrainModel must represent design reasoning as:

Architecture -> WorldState -> Search -> Evaluation

and treat design as state-space exploration.

## Required layers

- `apps`: GUI / CLI
- `runtime`: agent orchestration
- `core`: shared core types
- `engine`: search and evaluation algorithms
- `domain`: problem representation

## Domain layer

Two domains are required.

### design_domain

- `architecture.rs`
- `design_unit.rs`
- `structure_unit.rs`
- `class_unit.rs`
- `dependency.rs`
- `constraint.rs`
- `architecture_graph.rs`

Hierarchy:

- `Architecture`
- `ClassUnit`
- `StructureUnit`
- `DesignUnit`

### semantic_domain

- `concept.rs`
- `semantic_unit.rs`
- `intent.rs`
- `meaning_graph.rs`
- `semantic_relation.rs`

Mapping:

- `Intent -> Concept -> SemanticUnit`
- `SemanticUnit -> DesignUnit`

## World state

WorldState is an architecture snapshot with:

- `architecture`
- `constraints`
- `evaluation`
- `score`
- `depth`
- `history`

## Action model

Actions transform one world state into another.

- `AddDesignUnit`
- `RemoveDesignUnit`
- `ConnectDependency`
- `SplitStructure`
- `MergeStructure`

## Search engine

Search must support:

- expansion from `WorldState`
- evaluation of candidate states
- pruning to the next frontier
- deterministic beam-search execution
- recall-first heuristics backed by MemorySpace recall

## Evaluation

Score is a vector:

- `structural_quality`
- `dependency_quality`
- `constraint_satisfaction`
- `complexity`

Selection uses Pareto-aware ranking.
