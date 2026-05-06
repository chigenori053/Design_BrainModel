use super::git_router::route_git_command;
use super::routing_result::RoutingResult;

pub fn route_command(input: &str, has_context: bool) -> RoutingResult {
    if let Some(git) = route_git_command(input) {
        return match git {
            Ok(command) => RoutingResult::Git(command),
            Err(err) => RoutingResult::Rejected(err),
        };
    }

    let kind = crate::routing::core_router::route_core(input, has_context);
    if matches!(kind, crate::core::CoreRequestKind::NaturalLanguage) {
        RoutingResult::NaturalLanguage
    } else {
        RoutingResult::Core(kind)
    }
}
