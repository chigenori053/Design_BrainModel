# Design Brain Model (Python Project)

**Single Source of Truth** for the project.

## Directory Structure
- `hybrid_vm/`: Core logic, State Management, Interface Layer.
- `brain_model/`: AI logic, Decision Making (formerly `design_brain`).
- `tests/`: Verification scripts and tests.

## Key Responsibilities
- **State Management**: Holding the canonical state of the system in `VMState`.
- **Decision Making**: Executing logic to determine the next state.
- **Snapshot**: Providing reproducible snapshots of the system.
- **Interface**: Exposing APIs for external clients (like Rust UI) via `hybrid_vm/interface_layer`.

## Verification
Run verification scripts from this directory:
```bash
python3 verify_phase1.py
```
