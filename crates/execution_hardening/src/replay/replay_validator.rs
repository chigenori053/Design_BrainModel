use crate::checksum::ExecutionTraceHash;
use crate::error::HardeningError;

/// Validates that two executions of the same plan produce identical results
/// by comparing their `ExecutionTraceHash` values.
///
/// Spec §9: Replay保証
/// - Same IR + same ExecutionOptions + same initial State
///   → identical output / effect / state checksums
/// - Verification: trace hash 一致
pub struct ReplayValidator;

impl ReplayValidator {
    pub fn new() -> Self {
        Self
    }

    /// Compare the trace hash from the original run with that of a replay run.
    ///
    /// Returns `Ok(())` when all four checksums match exactly.
    /// Returns `Err(TraceMismatch)` on any divergence (fail-fast, spec §12).
    pub fn validate(
        &self,
        original: &ExecutionTraceHash,
        replayed: &ExecutionTraceHash,
    ) -> Result<(), HardeningError> {
        if original.matches(replayed) {
            Ok(())
        } else {
            Err(HardeningError::TraceMismatch {
                expected_hash: original.total().to_hex(),
                actual_hash: replayed.total().to_hex(),
            })
        }
    }

    /// Validate field-by-field and report the first diverging dimension.
    pub fn validate_detailed(
        &self,
        original: &ExecutionTraceHash,
        replayed: &ExecutionTraceHash,
    ) -> Result<(), HardeningError> {
        if original.plan_checksum != replayed.plan_checksum {
            return Err(HardeningError::TraceMismatch {
                expected_hash: original.plan_checksum.to_hex(),
                actual_hash: replayed.plan_checksum.to_hex(),
            });
        }
        if original.output_checksum != replayed.output_checksum {
            return Err(HardeningError::TraceMismatch {
                expected_hash: original.output_checksum.to_hex(),
                actual_hash: replayed.output_checksum.to_hex(),
            });
        }
        if original.effect_checksum != replayed.effect_checksum {
            return Err(HardeningError::TraceMismatch {
                expected_hash: original.effect_checksum.to_hex(),
                actual_hash: replayed.effect_checksum.to_hex(),
            });
        }
        if original.state_checksum != replayed.state_checksum {
            return Err(HardeningError::TraceMismatch {
                expected_hash: original.state_checksum.to_hex(),
                actual_hash: replayed.state_checksum.to_hex(),
            });
        }
        Ok(())
    }
}

impl Default for ReplayValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checksum::Checksum;

    fn make_hash(seed: u8) -> ExecutionTraceHash {
        ExecutionTraceHash {
            plan_checksum: Checksum::of(&[seed]),
            output_checksum: Checksum::of(&[seed + 1]),
            effect_checksum: Checksum::of(&[seed + 2]),
            state_checksum: Checksum::of(&[seed + 3]),
        }
    }

    #[test]
    fn identical_hashes_pass() {
        let v = ReplayValidator::new();
        let h = make_hash(10);
        assert!(v.validate(&h, &h.clone()).is_ok());
    }

    #[test]
    fn diverged_hashes_fail() {
        let v = ReplayValidator::new();
        let original = make_hash(10);
        let diverged = make_hash(20);
        assert!(matches!(
            v.validate(&original, &diverged),
            Err(HardeningError::TraceMismatch { .. })
        ));
    }

    #[test]
    fn detailed_reports_first_diverging_field() {
        let v = ReplayValidator::new();
        let original = make_hash(10);
        let mut diverged = original.clone();
        diverged.output_checksum = Checksum::of(b"different");
        let err = v.validate_detailed(&original, &diverged).unwrap_err();
        // Should report divergence in output checksum, not total hash
        assert!(matches!(err, HardeningError::TraceMismatch { .. }));
    }
}
