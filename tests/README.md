DesignBrainModel test suite categories:

- `invariants`: structural and mathematical assumptions that must never regress.
- `engine`: correctness checks for search, evaluation, and memory components.
- `knowledge_engine`: retrieval, parsing, validation, reasoning integration, and search-impact checks for external knowledge.
- `determinism`: same input must yield the same output.
- `integration`: minimal end-to-end pipeline coverage.
- `experiments`: research tests kept out of default CI with `#[ignore]`.

Default CI executes `invariants`, `engine`, `knowledge_engine`, `determinism`, and `integration`.
Experimental coverage runs only when requested.
