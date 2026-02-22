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
    pub fn with_base(
        chm: &'a Chm,
        _field: &'a FieldEngine,
        base: StructuralEvaluator,
    ) -> Result<Self, hybrid_vm::SemanticError> {
        let vm = HybridVM::with_default_memory(base)?;
        Ok(Self {
            vm: Mutex::new(vm),
            _chm: chm,
        })
    }

    pub fn take_memory_telemetry(&self) -> MemoryInterferenceTelemetry {
        match self.vm.lock() {
            Ok(mut vm) => vm.take_memory_telemetry(),
            Err(_) => MemoryInterferenceTelemetry::default(),
        }
    }
}

impl Evaluator for SystemEvaluator<'_> {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector {
        match self.vm.lock() {
            Ok(mut vm) => vm.evaluate(state).clamped(),
            Err(_) => ObjectiveVector {
                f_struct: 0.0,
                f_field: 0.0,
                f_risk: 0.0,
                f_shape: 0.0,
            },
        }
    }
}
