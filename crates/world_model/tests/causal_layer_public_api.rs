use world_model::{
    CausalPropagationEdge, CausalPropagationGraph, CausalRuntimeState, SemanticCausalEngine,
};

#[test]
fn causal_layer_public_api_is_available() {
    let _ = std::any::type_name::<CausalRuntimeState>();
    let _ = std::any::type_name::<CausalPropagationGraph>();
    let _ = std::any::type_name::<CausalPropagationEdge>();
    let _ = std::any::type_name::<SemanticCausalEngine>();
}
