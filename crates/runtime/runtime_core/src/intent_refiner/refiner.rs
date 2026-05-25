use std::collections::HashMap;
use std::sync::Arc;

use memory_engine::MemoryEngine;

use crate::intent_refiner::default::apply_default;
use crate::intent_refiner::inference::InferenceEngine;
use crate::intent_refiner::memory_adapter::apply_memory;
use crate::intent_refiner::merger::merge;
use crate::intent_refiner::normalizer::Normalizer;
use crate::intent_refiner::rule_engine::RuleEngine;
use crate::intent_refiner::tokenizer::Tokenizer;
use crate::stable_v03::CoreResult;

#[derive(Clone, Debug, PartialEq)]
pub struct StructuredIntent {
    pub goal: String,
    pub slots: SlotMap,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlotMap {
    pub core: HashMap<CoreSlot, SlotValue>,
    pub system: HashMap<SystemSlot, SlotValue>,
    pub quality: HashMap<QualitySlot, SlotValue>,
    pub optional: HashMap<OptionalSlot, SlotValue>,
}

impl SlotMap {
    pub fn insert_core(&mut self, key: CoreSlot, value: SlotValue) {
        self.core.insert(key, value);
    }

    pub fn insert_system(&mut self, key: SystemSlot, value: SlotValue) {
        self.system.insert(key, value);
    }

    pub fn insert_quality(&mut self, key: QualitySlot, value: SlotValue) {
        self.quality.insert(key, value);
    }

    pub fn insert_optional(&mut self, key: OptionalSlot, value: SlotValue) {
        self.optional.insert(key, value);
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CoreSlot {
    InterfaceType,
    Language,
    Framework,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum SystemSlot {
    Language,
    Runtime,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum QualitySlot {
    Determinism,
    Performance,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum OptionalSlot {
    ArchitectureStyle,
    Testing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotValue {
    pub value: String,
    pub confidence: f32,
    pub source: SlotSource,
}

impl SlotValue {
    pub fn new(value: String, confidence: f32, source: SlotSource) -> Self {
        Self {
            value,
            confidence,
            source,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotSource {
    Explicit,
    Memory,
    Default,
    Inferred,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatContext {
    pub history: Vec<String>,
    pub last_slots: Option<SlotMap>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clarification {
    pub missing: Vec<CoreSlot>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum IntentExecution {
    Ready(StructuredIntent),
    NeedClarification(Clarification),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentError {
    EmptyInput,
    InvalidInput,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntentTrace {
    pub tokens: Vec<String>,
    pub extracted: SlotMap,
    pub inferred: SlotMap,
    pub final_slots: SlotMap,
}

pub trait IntentRefiner: Send + Sync {
    fn refine(&self, input: &str, context: &ChatContext) -> CoreResult<IntentExecution>;
    fn refine_with_trace(
        &self,
        input: &str,
        context: &ChatContext,
    ) -> CoreResult<(IntentExecution, IntentTrace)>;
}

pub struct DefaultIntentRefiner {
    normalizer: Normalizer,
    tokenizer: Tokenizer,
    rule_engine: RuleEngine,
    inference: InferenceEngine,
    memory: Arc<dyn MemoryEngine>,
}

impl DefaultIntentRefiner {
    pub fn new(memory: Arc<dyn MemoryEngine>) -> Self {
        Self {
            normalizer: Normalizer,
            tokenizer: Tokenizer,
            rule_engine: RuleEngine,
            inference: InferenceEngine,
            memory,
        }
    }

    pub fn try_refine_with_trace(
        &self,
        input: &str,
        context: &ChatContext,
    ) -> Result<(IntentExecution, IntentTrace), IntentError> {
        let normalized = self.normalizer.normalize(input);
        if normalized.is_empty() {
            return Err(IntentError::EmptyInput);
        }

        let tokens = self.tokenizer.tokenize(&normalized);
        if tokens.is_empty() {
            return Err(IntentError::InvalidInput);
        }

        let rule_slots = self.rule_engine.extract(&tokens);
        let inferred = self.inference.infer(&normalized);
        let merged = merge(rule_slots.clone(), inferred.clone());
        let with_memory = apply_memory(merged, &normalized, context, self.memory.as_ref());
        let completed = apply_default(with_memory);
        let intent = StructuredIntent {
            goal: normalized,
            slots: completed.clone(),
        };
        let trace = IntentTrace {
            tokens,
            extracted: rule_slots,
            inferred,
            final_slots: completed,
        };
        Ok((finalize(intent), trace))
    }
}

impl IntentRefiner for DefaultIntentRefiner {
    fn refine(&self, input: &str, context: &ChatContext) -> CoreResult<IntentExecution> {
        self.try_refine_with_trace(input, context)
            .map(|(execution, _)| execution)
            .map_err(|_| crate::stable_v03::CoreError::InvalidInput)
    }

    fn refine_with_trace(
        &self,
        input: &str,
        context: &ChatContext,
    ) -> CoreResult<(IntentExecution, IntentTrace)> {
        self.try_refine_with_trace(input, context)
            .map_err(|_| crate::stable_v03::CoreError::InvalidInput)
    }
}

const REQUIRED_CORE_SLOTS: [CoreSlot; 3] = [
    CoreSlot::InterfaceType,
    CoreSlot::Language,
    CoreSlot::Framework,
];

fn finalize(intent: StructuredIntent) -> IntentExecution {
    let missing = find_missing_core_slots(&intent.slots);
    if missing.is_empty() {
        IntentExecution::Ready(intent)
    } else {
        IntentExecution::NeedClarification(build_clarification(missing))
    }
}

fn find_missing_core_slots(slots: &SlotMap) -> Vec<CoreSlot> {
    REQUIRED_CORE_SLOTS
        .iter()
        .filter(|slot| !slots.core.contains_key(slot))
        .copied()
        .collect()
}

fn build_clarification(missing: Vec<CoreSlot>) -> Clarification {
    let message = match missing.as_slice() {
        [CoreSlot::InterfaceType] => "Which interface type do you want? (api/web/worker)".to_string(),
        [CoreSlot::Language] => "Which language do you want? (rust/typescript/go)".to_string(),
        [CoreSlot::Framework] => "Which framework do you want? (axum/actix/express)".to_string(),
        [CoreSlot::Language, CoreSlot::Framework] => {
            "Which language and framework do you want? (rust+axum/typescript+express/go+gin)".to_string()
        }
        _ => "Which interface type, language, and framework do you want? (api+rust+axum/web+typescript+next)".to_string(),
    };
    Clarification { missing, message }
}
