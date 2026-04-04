# PhaseG1d-1.5
## ExplicitTarget Candidate Suppression

Target:
apps/cli/src/refactor/planner.rs

Rule:
When PatchScope::ExplicitTargetOnly is active,
candidate source MUST be restricted to explicit target file AST only.

Forbidden:
- runtime_vm fallback
- cross-crate symbol expansion
- adapter_app_interface synthesis
