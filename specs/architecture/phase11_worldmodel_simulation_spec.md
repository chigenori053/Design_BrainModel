# Phase11 WorldModel & Simulation Specification

Primary spec for the Phase11 implementation.

## Goal

Phase11 upgrades the pipeline from:

Architecture -> WorldState -> Search -> Evaluation

to:

Architecture -> WorldState -> Simulation -> Evaluation

This turns the system into search plus simulation.

## Layer

Phase11 adds:

- `world_model`

Resulting stack:

- `apps`
- `runtime`
- `core`
- `engine`
- `domain`
- `world_model`

## World model structure

The world model layer must contain:

- `system_model`
- `math_model`
- `geometry_model`
- `execution_model`
- `simulation_engine`

## System model

Required capabilities:

- dependency graph analysis
- call graph analysis
- module graph analysis
- runtime flow estimation

Checks:

- dependency cycle
- module coupling
- architecture layering

## Math model

Required capabilities:

- algebra engine
- logic engine
- constraint solver

Checks:

- algorithm verification
- constraint solving
- formal reasoning

## Geometry model

Required capabilities:

- geometry engine
- layout engine
- spatial constraint

Checks:

- UI layout
- graph layout
- architecture visualization

## Execution model

Required capabilities:

- execution graph
- resource model
- latency model
- memory model

Checks:

- runtime complexity
- memory usage
- dependency cost

## Simulation engine

Simulation must produce:

- `performance_score`
- `correctness_score`
- `constraint_score`

and integrate with search as:

Search -> Simulation -> Score

## Memory integration

Simulation should use MemorySpace recall as a heuristic:

Recall -> Simulation -> Search
