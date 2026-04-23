/// Integration tests for the determinism audit pipeline (spec §13).
///
/// Note on controller isolation (spec §11): BeamSearchController mutates its
/// memory during search. To satisfy "同一 memory snapshot", replay must use a
/// fresh controller (same bootstrap state as the original). Two independent
/// `BeamSearchController::default()` instances start with identical bootstrap
/// patterns, guaranteeing the same pre-search memory state.
#[cfg(test)]
mod determinism_tests {
    use design_search_engine::{BeamSearchController, SearchConfig};
    use search_verification::{rest_api_state, verification_config};

    use crate::{capture, diff, replay, FailureClass, MatchStatus};

    fn controller() -> BeamSearchController {
        BeamSearchController::default()
    }

    fn config() -> SearchConfig {
        SearchConfig {
            beam_width: 4,
            max_depth: 3,
            ..verification_config(0.15)
        }
    }

    /// §13.1 — trace == replay_trace when both runs start from the same state.
    /// Uses separate fresh controllers so memory is identical at start (spec §11).
    #[test]
    fn determinism_trace_equals_replay() {
        let ctrl1 = controller();
        let ctrl2 = controller(); // fresh, same bootstrap patterns
        let state = rest_api_state();
        let cfg = config();

        let original = capture(state.clone(), cfg, &[], &ctrl1);
        let replayed = replay(&original, &ctrl2);
        let report = diff(&original, &replayed);

        assert!(
            report.deterministic,
            "pipeline must be deterministic: {}\n\nLayer details:\n{}",
            report.summary,
            report
                .layer_diffs
                .iter()
                .map(|d| format!(
                    "  {}: {:?} — {:?}",
                    d.layer, d.match_status, d.details
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// §13.2 — every layer is individually compared; all must be present.
    #[test]
    fn all_layers_present_in_report() {
        let ctrl1 = controller();
        let ctrl2 = controller();
        let state = rest_api_state();
        let cfg = config();

        let original = capture(state.clone(), cfg, &[], &ctrl1);
        let replayed = replay(&original, &ctrl2);
        let report = diff(&original, &replayed);

        let layer_names: Vec<&str> = report
            .layer_diffs
            .iter()
            .map(|d| d.layer.as_str())
            .collect();

        for expected in &["input", "knowledge", "ir", "memory", "search", "code", "patch"] {
            assert!(
                layer_names.contains(expected),
                "layer '{}' missing from report",
                expected
            );
        }
    }

    /// §13.3 — when a knowledge mismatch is injected, ExternalNondeterminism is reported.
    #[test]
    fn failure_detection_knowledge_mismatch() {
        let ctrl1 = controller();
        let ctrl2 = controller();
        let state = rest_api_state();
        let cfg = config();

        let original = capture(state.clone(), cfg, &[], &ctrl1);
        let mut replayed = replay(&original, &ctrl2);

        // Simulate non-deterministic WebSearch result.
        replayed.knowledge.content_hash = "ffffffffffffffff".into();
        replayed.knowledge.documents.clear();

        let report = diff(&original, &replayed);

        assert!(!report.deterministic, "injected mismatch must be detected");
        assert!(
            matches!(report.failure_class, Some(FailureClass::ExternalNondeterminism)),
            "knowledge mismatch must classify as ExternalNondeterminism, got {:?}",
            report.failure_class
        );
    }

    /// §13.3 — when search ordering changes (after confirming memory matches),
    /// SearchOrderingBug is reported.
    #[test]
    fn failure_detection_search_mismatch() {
        let ctrl1 = controller();
        let ctrl2 = controller();
        let state = rest_api_state();
        let cfg = config();

        let original = capture(state.clone(), cfg, &[], &ctrl1);
        let mut replayed = replay(&original, &ctrl2);

        // Ensure the memory layer matches first so the classifier reaches search.
        assert_eq!(
            original.memory.len(),
            replayed.memory.len(),
            "memory must match before injecting search mismatch"
        );

        // Corrupt the top search state hash to simulate beam ordering divergence.
        if let Some(first) = replayed.search.first_mut() {
            first.state_hash = "deadbeefdeadbeef".into();
        }

        let report = diff(&original, &replayed);

        assert!(!report.deterministic, "injected mismatch must be detected");
        assert!(
            matches!(report.failure_class, Some(FailureClass::SearchOrderingBug)),
            "search mismatch must classify as SearchOrderingBug, got {:?}",
            report.failure_class
        );
    }

    /// §13.4 — a trace serialised to JSON and deserialized replays identically.
    #[test]
    fn replay_from_serialized_trace() {
        let ctrl1 = controller();
        let ctrl2 = controller();
        let state = rest_api_state();
        let cfg = config();

        let original = capture(state, cfg, &[], &ctrl1);

        // Round-trip through JSON (simulates writing trace.json and loading it back).
        let json = serde_json::to_string(&original).expect("serialization");
        let reloaded: crate::FullTrace = serde_json::from_str(&json).expect("deserialization");

        let replayed = replay(&reloaded, &ctrl2);
        let report = diff(&reloaded, &replayed);

        assert!(
            report.deterministic,
            "replay from serialized trace must be deterministic: {}",
            report.summary
        );
    }

    /// §13.2 — all layers show Match on a clean replay with isolated controllers.
    #[test]
    fn all_layers_match_on_clean_run() {
        let ctrl1 = controller();
        let ctrl2 = controller();
        let state = rest_api_state();
        let cfg = config();

        let original = capture(state, cfg, &[], &ctrl1);
        let replayed = replay(&original, &ctrl2);
        let report = diff(&original, &replayed);

        for layer in &report.layer_diffs {
            assert_eq!(
                layer.match_status,
                MatchStatus::Match,
                "layer '{}' must match on a clean replay. details: {:?}",
                layer.layer,
                layer.details
            );
        }
    }
}
