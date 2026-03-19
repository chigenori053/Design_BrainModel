use execution_core::engine::execution_plan::ExecutionPlan;
use execution_core::validation::parse_command;

pub fn validate_execution_plan(plan: &ExecutionPlan) -> Result<(), String> {
    if !plan.project_root.exists() {
        return Err(format!(
            "working directory does not exist: {}",
            plan.project_root.display()
        ));
    }
    for command in &plan.dependency_plan.install_commands {
        parse_command(command)?;
    }
    for command in &plan.build_plan.build_commands {
        parse_command(command)?;
    }
    for command in &plan.run_plan.run_commands {
        parse_command(command)?;
    }
    for command in &plan.test_plan.test_commands {
        parse_command(command)?;
    }
    Ok(())
}
