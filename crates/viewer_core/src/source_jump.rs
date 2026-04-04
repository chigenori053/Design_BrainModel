use std::path::Path;
use std::process::Command;

use crate::model::SourceBinding;

pub fn open_source(binding: &SourceBinding, root: Option<&Path>) -> Result<(), String> {
    let absolute = if binding.file.is_absolute() {
        binding.file.clone()
    } else {
        root.map(|root| root.join(&binding.file))
            .unwrap_or_else(|| binding.file.clone())
    };

    if try_vscode_jump(&absolute, binding.line_start, binding.line_end) {
        return Ok(());
    }

    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(&absolute).status()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "", &absolute.display().to_string()])
            .status()
    } else {
        Command::new("xdg-open").arg(&absolute).status()
    }
    .map_err(|err| format!("failed to open {}: {err}", absolute.display()))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("source jump failed for {}", absolute.display()))
    }
}

fn try_vscode_jump(path: &Path, line_start: usize, _line_end: usize) -> bool {
    let target = format!("{}:{}", path.display(), line_start.max(1));
    Command::new("code")
        .args(["--goto", &target])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
