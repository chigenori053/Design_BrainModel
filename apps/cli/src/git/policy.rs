use super::commands::GitCommand;

pub use crate::runtime::execution_governance::{CommandPolicy, CommandType};

pub fn classify(command: &GitCommand) -> CommandPolicy {
    let command_type = crate::runtime::execution_governance::classify_command(&command.canonical());
    crate::runtime::execution_governance::command_policy(command_type)
}
