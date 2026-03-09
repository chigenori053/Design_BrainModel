use world_model_core::Action;

use crate::policy_model::ActionType;

pub fn action_type_for_action(action: &Action) -> ActionType {
    match action {
        Action::AddDesignUnit { layer, .. } => match layer {
            design_domain::Layer::Ui => ActionType::AddUi,
            design_domain::Layer::Service => ActionType::AddService,
            design_domain::Layer::Repository => ActionType::AddRepository,
            design_domain::Layer::Database => ActionType::AddDatabase,
        },
        Action::RemoveDesignUnit => ActionType::RemoveDesignUnit,
        Action::ConnectDependency { .. } => ActionType::ConnectDependency,
        Action::SplitStructure => ActionType::SplitStructure,
        Action::MergeStructure => ActionType::MergeStructure,
    }
}

pub fn policy_weight_for_action(
    action: &Action,
    weights: &std::collections::HashMap<ActionType, f64>,
) -> f64 {
    weights
        .get(&action_type_for_action(action))
        .copied()
        .unwrap_or(0.0)
}
