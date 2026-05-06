use crate::core::CoreRequestKind;

pub fn route_core(input: &str, has_context: bool) -> CoreRequestKind {
    crate::router::route(input, has_context).0
}
