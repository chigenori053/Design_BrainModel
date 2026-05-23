use super::hardened_trace::HardenedStepTrace;
use crate::checksum::Checksum;

/// Writes execution traces in JSONL format (one JSON object per line).
///
/// Spec §8.2: 保存形式 JSONL (deterministic order)
///
/// Each line is a complete, self-contained JSON record that can be parsed
/// independently for debugging, audit, or replay.
pub struct TraceWriter {
    lines: Vec<String>,
}

impl TraceWriter {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Append one step trace as a JSONL line.
    pub fn write_step(&mut self, trace: &HardenedStepTrace) -> Result<(), String> {
        let line = serde_json::to_string(trace).map_err(|e| e.to_string())?;
        self.lines.push(line);
        Ok(())
    }

    /// Return the complete JSONL output (one record per line, LF-terminated).
    pub fn to_jsonl(&self) -> String {
        let mut out = self.lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    /// Compute a deterministic checksum of the full JSONL output.
    ///
    /// This checksum can be compared across two runs to verify replay fidelity.
    pub fn checksum(&self) -> Checksum {
        Checksum::of(self.to_jsonl().as_bytes())
    }

    /// Number of steps recorded.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

impl Default for TraceWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::hardened_trace::{HardenedStepTrace, HardenedTraceInput};

    fn sample_trace(index: usize) -> HardenedStepTrace {
        HardenedStepTrace::new(HardenedTraceInput {
            step_index: index,
            phase: "build".to_string(),
            command: vec!["cargo".into(), "build".into()],
            stdout: "Compiling...".into(),
            stderr: String::new(),
            exit_code: Some(0),
            success: true,
            timestamp_ms: 1_000_000,
            end_timestamp_ms: 1_000_050,
            staged_effect_keys: vec![],
            committed_effect_keys: vec![],
        })
    }

    #[test]
    fn jsonl_has_one_line_per_step() {
        let mut w = TraceWriter::new();
        w.write_step(&sample_trace(0)).unwrap();
        w.write_step(&sample_trace(1)).unwrap();
        let out = w.to_jsonl();
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn checksum_is_stable_across_runs() {
        let mut w1 = TraceWriter::new();
        w1.write_step(&sample_trace(0)).unwrap();

        let mut w2 = TraceWriter::new();
        w2.write_step(&sample_trace(0)).unwrap();

        assert_eq!(w1.checksum(), w2.checksum());
    }

    #[test]
    fn checksum_differs_when_content_differs() {
        let mut w1 = TraceWriter::new();
        w1.write_step(&sample_trace(0)).unwrap();

        let mut w2 = TraceWriter::new();
        w2.write_step(&sample_trace(1)).unwrap(); // different step_index

        assert_ne!(w1.checksum(), w2.checksum());
    }
}
