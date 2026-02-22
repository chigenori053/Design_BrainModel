use std::sync::Mutex;

use core_types::ObjectiveVector;
use field_engine::FieldEngine;
use hybrid_vm::{Chm, Evaluator, HybridVM, StructuralEvaluator};
use memory_space::{DesignState, MemoryInterferenceTelemetry};

use crate::domain::{Hypothesis, Score};
use crate::SystemEvaluator;

pub trait EvaluationCapability: Send + Sync {
    fn evaluate(&self, hypothesis: &Hypothesis) -> Score;
}

impl<'a> SystemEvaluator<'a> {
    pub fn with_base(chm: &'a Chm, _field: &'a FieldEngine, base: StructuralEvaluator) -> Self {
        Self {
            vm: Mutex::new(HybridVM::with_default_memory(base)),
            _chm: chm,
        }
    }

    pub fn take_memory_telemetry(&self) -> MemoryInterferenceTelemetry {
        self.vm
            .lock()
            .expect("hybrid vm mutex poisoned")
            .take_memory_telemetry()
    }
}

impl Evaluator for SystemEvaluator<'_> {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector {
        self.vm
            .lock()
            .expect("hybrid vm mutex poisoned")
            .evaluate(state)
            .clamped()
    }
}
