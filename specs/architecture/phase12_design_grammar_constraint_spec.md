# Phase12 Design Grammar & Constraint Specification

Primary spec for the Phase12 implementation.

## Goal

Phase12 constrains search to valid architecture candidates only.

Pipeline:

Grammar -> Valid architecture only -> Search -> Simulation

## Domain update

The domain layer now includes:

- `design_domain`
- `semantic_domain`
- `design_grammar`

## Design grammar

The grammar layer must contain:

- `grammar_engine.rs`
- `architecture_rules.rs`
- `dependency_rules.rs`
- `constraint_rules.rs`
- `validation.rs`

The grammar decides whether an architecture is valid.

## Grammar model

Architecture generation rules:

- `Architecture -> ClassUnit+`
- `ClassUnit -> StructureUnit+`
- `StructureUnit -> DesignUnit+`

## Dependency grammar

Layered dependency rule:

- `UI -> Service -> Repository -> Database`

Forbidden example:

- `Database -> UI`

## Constraint engine

Constraints include:

- layer constraint
- dependency constraint
- complexity constraint
- naming constraint

## Search integration

Phase12 pipeline:

Search -> Grammar validation -> Simulation -> Score

Simulation must only run for grammar-valid candidates.

## Design unit rules

Design units must support:

- inputs
- outputs
- dependencies
- semantics

Validation must check:

- outputs feed compatible inputs
- no circular data flow

## Intent constraint

Semantic intent constrains grammar choices.

Example:

- `Web API` requires controller, service, and repository structure
