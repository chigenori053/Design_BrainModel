use crate::engine::execution_plan::ExecutionPlan;

pub fn validate_execution_plan(plan: &ExecutionPlan) -> Result<(), String> {
    if plan.dependency_plan.manifest_file.trim().is_empty() {
        return Err("dependency manifest_file must not be empty".into());
    }
    if plan
        .build_plan
        .build_commands
        .iter()
        .any(|c| c.trim().is_empty())
    {
        return Err("build plan contains an empty build command".into());
    }
    if plan
        .run_plan
        .run_commands
        .iter()
        .any(|c| c.trim().is_empty())
    {
        return Err("run plan contains an empty run command".into());
    }
    if plan
        .test_plan
        .test_files
        .iter()
        .any(|path| path.trim().is_empty())
    {
        return Err("test plan contains an empty test file path".into());
    }
    if plan
        .test_plan
        .test_commands
        .iter()
        .any(|c| c.trim().is_empty())
    {
        return Err("test plan contains an empty test command".into());
    }
    Ok(())
}

pub fn parse_command(command: &str) -> Result<(String, Vec<String>), String> {
    let parts = command
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err("command must not be empty".into());
    }
    let program = parts[0].clone();
    let args = parts[1..].to_vec();
    Ok((program, args))
}
