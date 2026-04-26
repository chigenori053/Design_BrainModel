#[path = "determinism_break/support.rs"]
mod support;

#[path = "determinism_break/memory_order.rs"]
mod memory_order;

#[path = "determinism_break/memory_tie.rs"]
mod memory_tie;

#[path = "determinism_break/search_tie.rs"]
mod search_tie;

#[path = "determinism_break/beam_instability.rs"]
mod beam_instability;

#[path = "determinism_break/ir_shuffle.rs"]
mod ir_shuffle;

#[path = "determinism_break/hashmap_order.rs"]
mod hashmap_order;

#[path = "determinism_break/knowledge_order.rs"]
mod knowledge_order;

#[path = "determinism_break/codegen_order.rs"]
mod codegen_order;

#[path = "determinism_break/patch_order.rs"]
mod patch_order;

#[path = "determinism_break/websearch_nondet.rs"]
mod websearch_nondet;

#[path = "determinism_break/memory_drift.rs"]
mod memory_drift;
