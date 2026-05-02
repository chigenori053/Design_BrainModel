use super::checksum::{Checksum, ChecksumBuilder};

/// Combined checksum covering all four execution dimensions:
/// plan, output, effect, and state.
///
/// Spec §4.2  ExecutionTraceHash
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionTraceHash {
    pub plan_checksum: Checksum,
    pub output_checksum: Checksum,
    pub effect_checksum: Checksum,
    pub state_checksum: Checksum,
}

impl ExecutionTraceHash {
    /// Compute a single ordered hash across all four fields.
    /// Ordering is: plan → output → effect → state (stable).
    pub fn total(&self) -> Checksum {
        ChecksumBuilder::new()
            .update(self.plan_checksum.as_bytes())
            .update(self.output_checksum.as_bytes())
            .update(self.effect_checksum.as_bytes())
            .update(self.state_checksum.as_bytes())
            .finish()
    }

    /// Returns `true` only when all four field checksums are identical.
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checksum::checksum::Checksum;

    fn dummy(seed: u8) -> Checksum {
        Checksum::of(&[seed; 32])
    }

    #[test]
    fn total_is_deterministic() {
        let h = ExecutionTraceHash {
            plan_checksum: dummy(1),
            output_checksum: dummy(2),
            effect_checksum: dummy(3),
            state_checksum: dummy(4),
        };
        assert_eq!(h.total(), h.total());
    }

    #[test]
    fn field_order_affects_total() {
        let h1 = ExecutionTraceHash {
            plan_checksum: dummy(1),
            output_checksum: dummy(2),
            effect_checksum: dummy(3),
            state_checksum: dummy(4),
        };
        let h2 = ExecutionTraceHash {
            plan_checksum: dummy(4),
            output_checksum: dummy(3),
            effect_checksum: dummy(2),
            state_checksum: dummy(1),
        };
        assert_ne!(h1.total(), h2.total());
    }

    #[test]
    fn matches_reflexive() {
        let h = ExecutionTraceHash {
            plan_checksum: dummy(7),
            output_checksum: dummy(8),
            effect_checksum: dummy(9),
            state_checksum: dummy(10),
        };
        assert!(h.matches(&h.clone()));
    }
}
