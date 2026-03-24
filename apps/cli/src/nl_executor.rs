//! DBM_CLI: 自然言語プランを実際のCLIコマンド呼び出しに変換して実行する。
//!
//! plan.rs の CommandInvocation を受け取り、現在の実行ファイル（design バイナリ）を
//! サブプロセスとして呼び出すことで実際の出力を得る。

use std::path::PathBuf;

/// CommandInvocation を design CLI の実際のサブコマンドに変換して実行する
///
/// # マッピング
/// | CommandInvocation | design CLI 呼び出し |
/// |---|---|
/// | analyze, code\|project, [path] | design analyze <path> |
/// | generate, spec, [target] | design generate --type <target> --lang rust |
/// | generate, design, [path] | design design <path> |
/// | validate, _, [path] | design validate <path> |
/// | refactor, _, [path] | design refactor <path> |
/// | coding, _, [path] | design coding <path> |
/// | diff, _, [path] | design diff <path> |
/// | check, _, [path] | design check <path> |
/// | apply, _, [path] | design apply <path> --apply |
/// | exec, _, [path] | design run <path> |
/// | rules, list, [] | design rules list |
/// | rules, inspect, [id] | design rules inspect <id> |
/// | rules, validate, [id] | design rules validate <id> |
/// | rules, promote, [id?] | design rules promote [id] |
/// | rules, rollback, [id] | design rules rollback <id> |
/// | memory, import, [path] | design memory import <path> |
pub fn execute_plan_step(
    name: &str,
    subcommand: Option<&str>,
    args: &[String],
) -> Result<String, String> {
    let (cli_cmd, cli_args) = map_invocation_to_cli(name, subcommand, args)?;
    run_design_command(&cli_cmd, &cli_args)
}

fn map_invocation_to_cli(
    name: &str,
    subcommand: Option<&str>,
    args: &[String],
) -> Result<(String, Vec<String>), String> {
    match name {
        "analyze" => {
            // design analyze <path>  (code/project サブコマンドは CLI が自動判別)
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("analyze".to_string(), vec![path]))
        }
        "generate" => match subcommand {
            Some("spec") => {
                // design generate --type <target> --lang rust
                let target = args.first().cloned().unwrap_or_else(|| "api".to_string());
                Ok((
                    "generate".to_string(),
                    vec![
                        "--type".to_string(),
                        target,
                        "--lang".to_string(),
                        "rust".to_string(),
                    ],
                ))
            }
            _ => {
                // design design <path>
                let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
                Ok(("design".to_string(), vec![path]))
            }
        },
        "validate" => {
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("validate".to_string(), vec![path]))
        }
        "refactor" => {
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("refactor".to_string(), vec![path]))
        }
        "coding" => {
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("coding".to_string(), vec![path]))
        }
        "diff" => {
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("diff".to_string(), vec![path]))
        }
        "check" => {
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("check".to_string(), vec![path]))
        }
        "apply" => {
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("apply".to_string(), vec![path, "--apply".to_string()]))
        }
        "exec" => {
            let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
            Ok(("run".to_string(), vec![path]))
        }
        "rules" => {
            let sub = subcommand.unwrap_or("list");
            match sub {
                "list" => {
                    let mut cli_args = vec!["list".to_string()];
                    cli_args.extend_from_slice(args);
                    Ok(("rules".to_string(), cli_args))
                }
                "inspect" | "validate" | "rollback" => {
                    let mut cli_args = vec![sub.to_string()];
                    cli_args.extend_from_slice(args);
                    Ok(("rules".to_string(), cli_args))
                }
                "promote" => {
                    let mut cli_args = vec!["promote".to_string()];
                    cli_args.extend_from_slice(args);
                    Ok(("rules".to_string(), cli_args))
                }
                _ => {
                    let mut cli_args = vec!["list".to_string()];
                    cli_args.extend_from_slice(args);
                    Ok(("rules".to_string(), cli_args))
                }
            }
        }
        "memory" => {
            let sub = subcommand.unwrap_or("import");
            let mut cli_args = vec![sub.to_string()];
            cli_args.extend_from_slice(args);
            Ok(("memory".to_string(), cli_args))
        }
        _ => Err(format!("不明なコマンド: {name}")),
    }
}

/// 現在の実行ファイルを使ってサブコマンドを実行し、出力を返す
pub fn run_design_command(subcommand: &str, args: &[String]) -> Result<String, String> {
    let exe = current_exe()?;

    let output = std::process::Command::new(&exe)
        .arg(subcommand)
        .args(args)
        .output()
        .map_err(|e| format!("実行エラー: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    // info:/warn: 行のみ stderr から表示
    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
    let info_lines: Vec<&str> = stderr_str
        .lines()
        .filter(|l| l.starts_with("info:") || l.starts_with("warn:"))
        .collect();

    if output.status.success() {
        let mut result = String::new();
        if !info_lines.is_empty() {
            result.push_str(&info_lines.join("\n"));
            result.push('\n');
        }
        result.push_str(&stdout);
        Ok(result)
    } else {
        let err_raw = String::from_utf8_lossy(&output.stderr).to_string();
        let err = if !err_raw.trim().is_empty() {
            err_raw
        } else {
            stdout
        };
        Err(err.trim().to_string())
    }
}

fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("実行ファイルが見つかりません: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_analyze_code_to_analyze_cli() {
        let (cmd, args) =
            map_invocation_to_cli("analyze", Some("code"), &["src/".to_string()]).unwrap();
        assert_eq!(cmd, "analyze");
        assert_eq!(args, vec!["src/"]);
    }

    #[test]
    fn map_analyze_project_to_analyze_cli() {
        let (cmd, args) =
            map_invocation_to_cli("analyze", Some("project"), &[".".to_string()]).unwrap();
        assert_eq!(cmd, "analyze");
        assert_eq!(args, vec!["."]);
    }

    #[test]
    fn map_generate_spec_to_generate_cli() {
        let (cmd, args) =
            map_invocation_to_cli("generate", Some("spec"), &["api".to_string()]).unwrap();
        assert_eq!(cmd, "generate");
        assert!(args.contains(&"--type".to_string()));
        assert!(args.contains(&"api".to_string()));
        assert!(args.contains(&"--lang".to_string()));
        assert!(args.contains(&"rust".to_string()));
    }

    #[test]
    fn map_generate_design_to_design_cli() {
        let (cmd, args) =
            map_invocation_to_cli("generate", Some("design"), &[".".to_string()]).unwrap();
        assert_eq!(cmd, "design");
        assert_eq!(args, vec!["."]);
    }

    #[test]
    fn map_validate_to_validate_cli() {
        let (cmd, args) = map_invocation_to_cli("validate", None, &["src/".to_string()]).unwrap();
        assert_eq!(cmd, "validate");
        assert_eq!(args, vec!["src/"]);
    }

    #[test]
    fn map_refactor_to_refactor_cli() {
        let (cmd, args) = map_invocation_to_cli("refactor", None, &[".".to_string()]).unwrap();
        assert_eq!(cmd, "refactor");
        assert_eq!(args, vec!["."]);
    }

    #[test]
    fn map_exec_to_run_cli() {
        let (cmd, args) = map_invocation_to_cli("exec", None, &["main.rs".to_string()]).unwrap();
        assert_eq!(cmd, "run");
        assert_eq!(args, vec!["main.rs"]);
    }

    #[test]
    fn map_diff_to_diff_cli() {
        let (cmd, args) = map_invocation_to_cli("diff", None, &["src/".to_string()]).unwrap();
        assert_eq!(cmd, "diff");
        assert_eq!(args, vec!["src/"]);
    }

    #[test]
    fn map_check_to_check_cli() {
        let (cmd, args) = map_invocation_to_cli("check", None, &["src/".to_string()]).unwrap();
        assert_eq!(cmd, "check");
        assert_eq!(args, vec!["src/"]);
    }

    #[test]
    fn map_apply_includes_apply_flag() {
        let (cmd, args) = map_invocation_to_cli("apply", None, &["src/".to_string()]).unwrap();
        assert_eq!(cmd, "apply");
        assert!(args.contains(&"--apply".to_string()));
    }

    #[test]
    fn map_rules_list() {
        let (cmd, args) = map_invocation_to_cli("rules", Some("list"), &[]).unwrap();
        assert_eq!(cmd, "rules");
        assert_eq!(args[0], "list");
    }

    #[test]
    fn map_rules_inspect() {
        let (cmd, args) =
            map_invocation_to_cli("rules", Some("inspect"), &["rule-001".to_string()]).unwrap();
        assert_eq!(cmd, "rules");
        assert_eq!(args[0], "inspect");
        assert_eq!(args[1], "rule-001");
    }

    #[test]
    fn map_memory_import() {
        let (cmd, args) =
            map_invocation_to_cli("memory", Some("import"), &["seeds/k.json".to_string()]).unwrap();
        assert_eq!(cmd, "memory");
        assert_eq!(args[0], "import");
        assert_eq!(args[1], "seeds/k.json");
    }

    #[test]
    fn map_unknown_returns_error() {
        let result = map_invocation_to_cli("unknown_cmd", None, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不明なコマンド"));
    }

    #[test]
    fn analyze_default_path_is_dot() {
        let (_, args) = map_invocation_to_cli("analyze", Some("code"), &[]).unwrap();
        assert_eq!(args, vec!["."]);
    }

    #[test]
    fn generate_spec_default_target_is_api() {
        let (_, args) = map_invocation_to_cli("generate", Some("spec"), &[]).unwrap();
        assert!(args.contains(&"api".to_string()));
    }
}
