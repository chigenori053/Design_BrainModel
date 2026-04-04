# PhaseG1d-1.5b
## ExplicitTarget Candidate Prune Guard

Target:
apps/cli/src/coding.rs

Rule:
After candidate generation,
retain only candidates whose path exactly matches explicit target.

Hard Guard:
candidate.path MUST equal explicit target
