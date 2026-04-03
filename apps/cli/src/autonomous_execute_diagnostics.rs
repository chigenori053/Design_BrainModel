use super::*;

pub(super) struct MultiErrorAnalyzer;

impl MultiErrorAnalyzer {
    pub(super) fn analyze(report: &ExecReport, context: &ContextState) -> DebugResult {
        let hint = parse_fix_hint(&report.stderr);
        let errors = extract_error_lines(report.project_type, &report.stderr);
        let mut candidates = errors
            .iter()
            .map(|line| classify_error_line(report.project_type, line, hint.clone()))
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            candidates.push(fallback_candidate(report, hint));
        }

        candidates.sort_by(|lhs, rhs| {
            rhs.priority
                .cmp(&lhs.priority)
                .then_with(|| rhs.confidence.total_cmp(&lhs.confidence))
                .then_with(|| lhs.signature.cmp(&rhs.signature))
        });

        let previous_action = context
            .attempts
            .iter()
            .rev()
            .find(|attempt| !attempt.success)
            .map(|attempt| attempt.action.as_str());
        let action_counts =
            candidates
                .iter()
                .fold(BTreeMap::<String, usize>::new(), |mut counts, candidate| {
                    *counts.entry(candidate.action.clone()).or_insert(0) += 1;
                    counts
                });
        for candidate in &mut candidates {
            let mut adjusted = false;
            if context
                .seen_signatures
                .iter()
                .any(|signature| signature == &candidate.signature)
            {
                candidate.confidence = (candidate.confidence - 0.2).max(0.0);
                adjusted = true;
            }
            if matches!(previous_action, Some("install_dependency"))
                && matches!(candidate.action.as_str(), "add_use" | "add_trait_import")
            {
                candidate.priority += 5;
                adjusted = true;
            }
            if action_counts.get(&candidate.action).copied().unwrap_or(0) > 1 {
                candidate.confidence = (candidate.confidence + 0.1).min(0.95);
                adjusted = true;
            }
            if adjusted && candidate.priority < 0 {
                candidate.priority = 0;
            }
        }

        candidates.sort_by(|lhs, rhs| {
            rhs.priority
                .cmp(&lhs.priority)
                .then_with(|| rhs.confidence.total_cmp(&lhs.confidence))
                .then_with(|| lhs.signature.cmp(&rhs.signature))
        });

        let primary = candidates.remove(0);
        let recent_same_action_failures = context
            .attempts
            .iter()
            .rev()
            .take(2)
            .filter(|attempt| !attempt.success && attempt.action == primary.action)
            .count();
        let same_action_repeated = recent_same_action_failures >= 2;
        let context_adjusted = context
            .seen_signatures
            .iter()
            .any(|signature| signature == &primary.signature)
            || matches!(previous_action, Some("install_dependency"))
                && matches!(primary.action.as_str(), "add_use" | "add_trait_import")
            || same_action_repeated;
        DebugResult {
            confidence: primary.confidence,
            retryable: primary.retryable && !same_action_repeated,
            primary,
            secondary: candidates,
            context_adjusted,
        }
    }
}

pub(super) struct DebugEngine;

impl DebugEngine {
    pub(super) fn analyze(report: &ExecReport, context: &ContextState) -> DebugResult {
        MultiErrorAnalyzer::analyze(report, context)
    }
}

pub(super) struct FixGenerator;

impl FixGenerator {
    pub(super) fn generate(
        candidate: &ErrorCandidate,
        report: &ExecReport,
        project_type: ProjectType,
        context: &ContextState,
    ) -> Option<Fix> {
        if let Some(hint) = &candidate.hint {
            let fix = fix_from_hint(hint)?;
            if context
                .applied_fixes
                .iter()
                .any(|applied| applied == &fix.content)
            {
                return None;
            }
            return Some(fix);
        }

        let fix = match candidate.action.as_str() {
            "install_tool" => Some(Fix {
                r#type: "command".to_string(),
                content: format!(
                    "Install required tool manually: {}",
                    report.command.as_deref().unwrap_or("unknown")
                ),
                executable: None,
                patch: None,
            }),
            "install_dependency" => install_dependency_fix(report, project_type),
            "add_use" => generate_add_use_fix(report),
            "add_trait_import" => generate_trait_import_fix(report),
            "fix_reference" => generate_reference_fix(report),
            "fix_borrow" => generate_borrow_fix(report),
            "fix_type" | "fix_compile" | "fix_syntax" | "fix_logic" => None,
            _ => None,
        }?;

        if context
            .applied_fixes
            .iter()
            .any(|applied| applied == &fix.content)
        {
            return install_dependency_fallback(candidate, report, project_type, context);
        }

        Some(fix)
    }
}

pub(super) fn execute_with_incident_recorder(
    original_root: &Path,
    sandbox_root: &Path,
    project_type: ProjectType,
    command: Vec<String>,
    timeout_ms: u64,
) -> Result<ExecReport, String> {
    let meta_dir = sandbox_root.join(".dbm_autonomous_execute");
    fs::create_dir_all(&meta_dir)
        .map_err(|err| format!("failed to prepare adapter metadata: {err}"))?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_nanos();
    let spec_path = meta_dir.join(format!("spec_{seed}.json"));
    let task_path = meta_dir.join(format!("task_{seed}.json"));
    fs::write(&spec_path, "{\"system\":\"autonomous_execute_v0_2\"}\n")
        .map_err(|err| format!("failed to write spec file: {err}"))?;
    fs::write(
        &task_path,
        format!("{{\"task\":\"{}\"}}\n", command.join(" ")),
    )
    .map_err(|err| format!("failed to write task file: {err}"))?;

    let script_path = workspace_root().join("scripts/incident_recorder.py");
    let resolved_python = resolve_python_command()?;
    let timeout_sec = timeout_ms.div_ceil(1000).max(1);
    let mut args = vec![
        script_path.display().to_string(),
        "--task-id".to_string(),
        "autonomous_execute".to_string(),
        "--spec".to_string(),
        spec_path.display().to_string(),
        "--task".to_string(),
        task_path.display().to_string(),
        "--timeout-sec".to_string(),
        timeout_sec.to_string(),
        "--".to_string(),
    ];
    args.extend(command.clone());
    let config = ExecutionConfig {
        command: resolved_python,
        args,
        working_dir: sandbox_root.display().to_string(),
        timeout_ms,
        env: fixed_env(),
        clean_env: true,
        output_mode: OutputMode::Buffered,
    };
    let policy = SandboxPolicy {
        allow_network: allow_network_for_command(&command),
        allow_fs_write: true,
        allowed_paths: vec![sandbox_root.display().to_string()],
    };
    let timeout = TimeoutConfig {
        timeout_ms,
        kill_signal: "kill".to_string(),
    };
    let started = Instant::now();
    let result = run_command(
        &config,
        &timeout,
        &policy,
        sandbox_root,
        sandbox_mode_hint(),
    )
    .map_err(|err| err.to_string())?;
    let action = infer_exec_action(&command);
    let error_type = classify_exec_error(&command, &result.status, &result.stderr);

    Ok(ExecReport {
        root: original_root.display().to_string(),
        project_type,
        action,
        status: result.status.clone(),
        success: result.status == "success",
        error_type,
        exit_code: result.exit_code,
        duration_ms: started.elapsed().as_millis(),
        stdout: result.stdout,
        stderr: clean_incident_stderr(&result.stderr),
        truncated: result.output_meta.truncated || result.stderr_meta.truncated,
        command: command.first().cloned(),
        args: command[1..].to_vec(),
        output_meta: result.output_meta,
        stderr_meta: result.stderr_meta,
        sandbox_mode: Some(result.sandbox_mode),
        telemetry: Some(result.telemetry),
        deterministic: true,
    })
}

fn sandbox_mode_hint() -> SandboxMode {
    SandboxMode::Reuse
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn resolve_python_command() -> Result<String, String> {
    resolve_command("python")
        .or_else(|_| resolve_command("python3"))
        .map_err(|err| err.to_string())
}

fn infer_exec_action(command: &[String]) -> ExecAction {
    if command.len() >= 2 && is_install_command(command) {
        ExecAction::Install
    } else if command.iter().any(|arg| arg == "test") {
        ExecAction::Test
    } else if command.iter().any(|arg| arg == "build") {
        ExecAction::Build
    } else if command.iter().any(|arg| arg == "run" || arg == "start") {
        ExecAction::Run
    } else {
        ExecAction::Detect
    }
}

fn classify_exec_error(command: &[String], status: &str, stderr: &str) -> String {
    let lowered = stderr.to_lowercase();
    if status == "timeout" || lowered.contains("timed out") {
        return "Timeout".to_string();
    }
    if lowered.contains("not found") || lowered.contains("command not found") {
        return "MissingCommand".to_string();
    }
    if lowered.contains("cannot find crate")
        || lowered.contains("no matching package named")
        || lowered.contains("use of unresolved module")
        || lowered.contains("cannot find module")
        || lowered.contains("cannot resolve module")
    {
        return "DependencyError".to_string();
    }
    if lowered.contains("syntaxerror")
        || lowered.contains("expected `;`")
        || lowered.contains("unexpected token")
        || lowered.contains("expected expression")
    {
        return "SyntaxError".to_string();
    }
    if lowered.contains("test failed")
        || lowered.contains("assertion failed")
        || lowered.contains("failing tests")
    {
        return "TestFailure".to_string();
    }
    if status == "failure" {
        if command.iter().any(|arg| arg == "test") {
            return "TestFailure".to_string();
        }
        return "BuildError".to_string();
    }
    "Unknown".to_string()
}

fn is_retryable_action(action: &str) -> bool {
    !matches!(action, "retry_or_abort" | "abort")
}

pub(super) fn extract_error_lines(project_type: ProjectType, stderr: &str) -> Vec<String> {
    let mut extracted = stderr
        .lines()
        .filter_map(|line| match project_type {
            ProjectType::Rust if line.trim_start().starts_with("error[") => {
                Some(line.trim().to_string())
            }
            ProjectType::Rust if line.trim_start().starts_with("error:") => {
                Some(line.trim().to_string())
            }
            ProjectType::Node if line.contains("Error:") => Some(line.trim().to_string()),
            ProjectType::Node if line.contains("Module not found") => Some(line.trim().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if extracted.is_empty() {
        let trimmed = stderr.trim();
        if !trimmed.is_empty() {
            extracted.push(trimmed.lines().next().unwrap_or(trimmed).trim().to_string());
        }
    }

    extracted
}

pub(super) fn classify_error_line(
    project_type: ProjectType,
    error_line: &str,
    hint: Option<FixHint>,
) -> ErrorCandidate {
    if hint.is_some() {
        let signature = build_signature("Hint", "hint", error_line);
        return ErrorCandidate {
            signature,
            signature_hint: "hint".to_string(),
            action: "fix_compile".to_string(),
            priority: priority_for_action("fix_compile"),
            confidence: 0.95,
            retryable: true,
            hint,
        };
    }

    let lowered = error_line.to_lowercase();
    let (action, confidence, signature_hint) = match project_type {
        ProjectType::Rust => {
            if lowered.contains("cannot find crate") {
                ("install_dependency", 0.9, "cannot_find_crate")
            } else if lowered.contains("use of unresolved module or unlinked crate") {
                ("install_dependency", 0.9, "unresolved_module")
            } else if lowered.contains("unresolved import") {
                ("add_use", 0.9, "unresolved_import")
            } else if lowered.contains("no method named") {
                ("add_trait_import", 0.7, "no_method_named")
            } else if lowered.contains("mismatched types") {
                ("fix_type", 0.9, "mismatched_types")
            } else if lowered.contains("expected struct") {
                ("fix_type", 0.7, "expected_struct")
            } else if lowered.contains("expected enum") {
                ("fix_type", 0.7, "expected_enum")
            } else if lowered.contains("borrow of moved value")
                || lowered.contains("use of moved value")
                || lowered.contains("cannot borrow")
            {
                ("fix_borrow", 0.9, "fix_borrow")
            } else if lowered.contains("failed to compile") {
                ("fix_compile", 0.7, "failed_to_compile")
            } else {
                ("abort", 0.5, "unknown")
            }
        }
        ProjectType::Node => {
            if lowered.contains("module not found") {
                ("install_dependency", 0.9, "module_not_found")
            } else if lowered.contains("cannot find module") {
                ("install_dependency", 0.9, "cannot_find_module")
            } else if lowered.contains("undefined is not a function") {
                ("fix_reference", 0.9, "undefined_not_function")
            } else if lowered.contains("cannot read property") {
                ("fix_reference", 0.9, "cannot_read_property")
            } else if lowered.contains("unexpected token") {
                ("fix_syntax", 0.9, "unexpected_token")
            } else if lowered.contains("unexpected identifier") {
                ("fix_syntax", 0.7, "unexpected_identifier")
            } else if lowered.contains("is not defined") {
                ("fix_reference", 0.9, "is_not_defined")
            } else {
                ("abort", 0.5, "unknown")
            }
        }
        _ => ("abort", 0.5, "unknown"),
    };
    let signature = build_signature(
        &classify_exec_error(&[], "failure", error_line),
        signature_hint,
        error_line,
    );

    ErrorCandidate {
        signature,
        signature_hint: signature_hint.to_string(),
        action: action.to_string(),
        priority: priority_for_action(action),
        confidence,
        retryable: is_retryable_action(action),
        hint,
    }
}

fn fallback_candidate(report: &ExecReport, hint: Option<FixHint>) -> ErrorCandidate {
    if hint.is_some() {
        return ErrorCandidate {
            signature: build_signature(&report.error_type, "hint", &report.stderr),
            signature_hint: "hint".to_string(),
            action: "fix_compile".to_string(),
            priority: priority_for_action("fix_compile"),
            confidence: 0.95,
            retryable: true,
            hint,
        };
    }

    let (action, confidence, signature_hint) = match report.error_type.as_str() {
        "MissingCommand" => ("install_tool", 0.5, "missing_command"),
        "DependencyError" => ("install_dependency", 0.5, "dependency_error"),
        "SyntaxError" => ("fix_syntax", 0.5, "syntax_error"),
        "BuildError" => ("fix_compile", 0.5, "build_error"),
        "TestFailure" => ("fix_logic", 0.5, "test_failure"),
        "Timeout" => ("retry_or_abort", 0.5, "timeout"),
        _ => ("abort", 0.5, "unknown"),
    };

    ErrorCandidate {
        signature: build_signature(&report.error_type, signature_hint, &report.stderr),
        signature_hint: signature_hint.to_string(),
        action: action.to_string(),
        priority: priority_for_action(action),
        confidence,
        retryable: is_retryable_action(action),
        hint,
    }
}

fn priority_for_action(action: &str) -> i32 {
    match action {
        "install_tool" => 100,
        "install_dependency" => 90,
        "add_use" | "add_trait_import" => 80,
        "fix_syntax" => 70,
        "fix_compile" | "fix_type" | "fix_reference" | "fix_borrow" => 60,
        "fix_logic" => 50,
        _ => 0,
    }
}

pub(super) fn has_progressed(attempts: &[ExecuteAttempt], debug: &DebugResult) -> bool {
    let current_count = 1 + debug.secondary.len();
    let previous_debug = attempts
        .iter()
        .rev()
        .find_map(|attempt| attempt.debug.as_ref());
    match previous_debug {
        Some(previous) => {
            current_count < 1 + previous.secondary.len()
                || debug.primary.signature != previous.primary.signature
                || debug.primary.action != previous.primary.action
        }
        None => true,
    }
}

fn build_signature(error_type: &str, primary_keyword: &str, message: &str) -> String {
    let normalized = normalize_message(message);
    let mut hasher = Sha256::new();
    hasher.update(error_type.as_bytes());
    hasher.update(b"|");
    hasher.update(primary_keyword.as_bytes());
    hasher.update(b"|");
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();
    format!("{primary_keyword}:{}", hex_prefix(&digest))
}

fn normalize_message(message: &str) -> String {
    message
        .split_whitespace()
        .map(normalize_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_token(token: &str) -> String {
    if token.contains('/') || token.contains('\\') {
        return "<path>".to_string();
    }
    if token.chars().any(|ch| ch.is_ascii_digit()) {
        return "<num>".to_string();
    }
    if token.starts_with('`') || token.starts_with('\'') {
        return "<symbol>".to_string();
    }
    token.to_lowercase()
}

fn hex_prefix(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_fix_hint(stderr: &str) -> Option<FixHint> {
    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(payload) = trimmed.strip_prefix("DBM_COMMAND:") {
            return Some(FixHint {
                kind: "command".to_string(),
                payload: payload.trim().to_string(),
            });
        }
        if let Some(payload) = trimmed.strip_prefix("DBM_PATCH:") {
            return Some(FixHint {
                kind: "patch".to_string(),
                payload: payload.trim().to_string(),
            });
        }
        if let Some(payload) = trimmed.strip_prefix("DBM_CONFIG:") {
            return Some(FixHint {
                kind: "config".to_string(),
                payload: payload.trim().to_string(),
            });
        }
    }
    None
}

fn fix_from_hint(hint: &FixHint) -> Option<Fix> {
    match hint.kind.as_str() {
        "command" => Some(Fix {
            r#type: "command".to_string(),
            content: hint.payload.clone(),
            executable: Some(split_command(&hint.payload).ok()?),
            patch: None,
        }),
        "patch" | "config" => {
            let mut parts = hint.payload.splitn(3, '|');
            let path = parts.next()?.trim().to_string();
            let find = parts.next()?.to_string();
            let replace = parts.next()?.to_string();
            Some(Fix {
                r#type: if hint.kind == "config" {
                    "config".to_string()
                } else {
                    "patch".to_string()
                },
                content: hint.payload.clone(),
                executable: None,
                patch: Some(TextPatch {
                    path,
                    find,
                    replace,
                }),
            })
        }
        _ => None,
    }
}

fn install_dependency_fix(report: &ExecReport, project_type: ProjectType) -> Option<Fix> {
    let dependency = extract_dependency_name(&report.stderr)?;
    let executable = match project_type {
        ProjectType::Rust => vec!["cargo".to_string(), "add".to_string(), dependency.clone()],
        ProjectType::Node => vec!["npm".to_string(), "install".to_string(), dependency.clone()],
        _ => return None,
    };
    Some(Fix {
        r#type: "command".to_string(),
        content: executable.join(" "),
        executable: Some(executable),
        patch: None,
    })
}

fn install_dependency_fallback(
    candidate: &ErrorCandidate,
    report: &ExecReport,
    project_type: ProjectType,
    context: &ContextState,
) -> Option<Fix> {
    if candidate.action != "install_dependency" {
        return None;
    }

    let dependency = extract_dependency_name(&report.stderr)?;
    let alternative = alternative_dependency_name(project_type, &dependency)?;
    let fix = match project_type {
        ProjectType::Rust => Fix {
            r#type: "command".to_string(),
            content: format!("cargo add {alternative}"),
            executable: Some(vec!["cargo".to_string(), "add".to_string(), alternative]),
            patch: None,
        },
        ProjectType::Node => Fix {
            r#type: "command".to_string(),
            content: format!("npm install {alternative}"),
            executable: Some(vec!["npm".to_string(), "install".to_string(), alternative]),
            patch: None,
        },
        _ => return None,
    };

    if context
        .applied_fixes
        .iter()
        .any(|applied| applied == &fix.content)
    {
        return None;
    }

    Some(fix)
}

fn alternative_dependency_name(project_type: ProjectType, dependency: &str) -> Option<String> {
    match project_type {
        ProjectType::Rust => match dependency {
            "serde_derive" => Some("serde".to_string()),
            _ => None,
        },
        ProjectType::Node => match dependency {
            "node-fetch" => Some("undici".to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn generate_add_use_fix(report: &ExecReport) -> Option<Fix> {
    let import_path = extract_rust_import_candidate(&report.stderr)?;
    let source_file = extract_rust_source_path(&report.stderr)?;
    let use_stmt = format!("use {import_path};\n");
    Some(Fix {
        r#type: "patch".to_string(),
        content: format!("prepend `{}` to {}", use_stmt.trim_end(), source_file),
        executable: None,
        patch: Some(TextPatch {
            path: source_file,
            find: String::new(),
            replace: use_stmt,
        }),
    })
}

fn generate_trait_import_fix(report: &ExecReport) -> Option<Fix> {
    if let Some(import_path) = extract_rust_import_candidate(&report.stderr) {
        let source_file = extract_rust_source_path(&report.stderr)?;
        let use_stmt = format!("use {import_path};\n");
        return Some(Fix {
            r#type: "patch".to_string(),
            content: use_stmt.trim_end().to_string(),
            executable: None,
            patch: Some(TextPatch {
                path: source_file,
                find: String::new(),
                replace: use_stmt,
            }),
        });
    }
    Some(Fix {
        r#type: "patch".to_string(),
        content: "use std::fmt::Display;".to_string(),
        executable: None,
        patch: Some(TextPatch {
            path: extract_rust_source_path(&report.stderr)?,
            find: String::new(),
            replace: "use std::fmt::Display;\n".to_string(),
        }),
    })
}

fn generate_reference_fix(report: &ExecReport) -> Option<Fix> {
    if let Some(hint) = parse_fix_hint(&report.stderr) {
        return fix_from_hint(&hint);
    }
    if report.project_type == ProjectType::Node {
        let module = extract_node_module_name(&report.stderr).unwrap_or_else(|| "fs".to_string());
        return Some(Fix {
            r#type: "patch".to_string(),
            content: format!("const {module} = require('{module}');"),
            executable: None,
            patch: Some(TextPatch {
                path: extract_node_source_path(&report.stderr)?,
                find: String::new(),
                replace: format!("const {module} = require('{module}');\n"),
            }),
        });
    }
    None
}

fn generate_borrow_fix(report: &ExecReport) -> Option<Fix> {
    let source_file = extract_rust_source_path(&report.stderr)?;
    let symbol = extract_rust_moved_value(&report.stderr)?;
    Some(Fix {
        r#type: "patch".to_string(),
        content: format!("{symbol}.clone()"),
        executable: None,
        patch: Some(TextPatch {
            path: source_file,
            find: format!("&{symbol}"),
            replace: format!("{symbol}.clone()"),
        }),
    })
}

fn extract_dependency_name(stderr: &str) -> Option<String> {
    for marker in [
        "cannot find crate `",
        "use of unresolved module or unlinked crate `",
        "no matching package named `",
        "Cannot find module '",
        "cannot find module '",
    ] {
        if let Some(name) = capture_between(stderr, marker, marker_end(marker)) {
            return Some(name);
        }
    }
    None
}

fn extract_rust_import_candidate(stderr: &str) -> Option<String> {
    for marker in [
        "help: consider importing this trait: `",
        "help: consider importing this struct: `",
        "help: consider importing this enum: `",
        "help: consider importing this module: `",
        "help: consider importing this unresolved item through its public re-export: `",
        "help: a similar path exists: `",
    ] {
        if let Some(path) = capture_between(stderr, marker, "`") {
            return Some(path);
        }
    }
    None
}

fn extract_rust_source_path(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(remainder) = trimmed.strip_prefix("--> ") {
            let candidate = remainder.split(':').next()?.trim();
            if !candidate.is_empty() && !candidate.starts_with('<') {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn extract_node_source_path(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.contains(".js:") || trimmed.contains(".ts:") {
            return Some(trimmed.split(':').next()?.to_string());
        }
    }
    None
}

fn extract_rust_moved_value(stderr: &str) -> Option<String> {
    for marker in ["borrow of moved value: `", "use of moved value: `"] {
        if let Some(value) = capture_between(stderr, marker, "`") {
            return Some(value);
        }
    }
    None
}

fn extract_node_module_name(stderr: &str) -> Option<String> {
    for marker in ["Cannot find module '", "cannot find module '"] {
        if let Some(name) = capture_between(stderr, marker, "'") {
            return Some(name);
        }
    }
    if stderr.contains("cannot read property") || stderr.contains("undefined is not a function") {
        return Some("fs".to_string());
    }
    None
}

fn marker_end(marker: &str) -> &'static str {
    if marker.ends_with('`') { "`" } else { "'" }
}

fn capture_between(haystack: &str, start: &str, end: &str) -> Option<String> {
    let start_index = haystack.find(start)? + start.len();
    let remainder = &haystack[start_index..];
    let end_index = remainder.find(end)?;
    Some(remainder[..end_index].to_string())
}

pub(super) fn apply_text_patch(root: &Path, patch: &TextPatch) -> Result<(), String> {
    let file_path = root.join(&patch.path);
    let original = fs::read_to_string(&file_path)
        .map_err(|err| format!("failed to read patch target {}: {err}", file_path.display()))?;
    if patch.find.is_empty() {
        if original.starts_with(&patch.replace) {
            return Ok(());
        }
        let updated = format!("{}{}", patch.replace, original);
        return fs::write(&file_path, updated).map_err(|err| {
            format!(
                "failed to write patch target {}: {err}",
                file_path.display()
            )
        });
    }
    if !original.contains(&patch.find) {
        return Err(format!(
            "patch target {} does not contain expected text",
            file_path.display()
        ));
    }
    let updated = original.replacen(&patch.find, &patch.replace, 1);
    fs::write(&file_path, updated).map_err(|err| {
        format!(
            "failed to write patch target {}: {err}",
            file_path.display()
        )
    })
}

fn allow_network_for_command(command: &[String]) -> bool {
    is_install_command(command)
}

fn is_install_command(command: &[String]) -> bool {
    matches!(
        command,
        [tool, action, ..]
            if (tool == "cargo" && action == "add")
                || (tool == "npm" && (action == "install" || action == "i"))
    )
}

pub(super) fn split_command(command: &str) -> Result<Vec<String>, String> {
    let parts = command
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err("command is empty".to_string());
    }
    Ok(parts)
}

fn clean_incident_stderr(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| !line.starts_with("Incident recorded: "))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn matches_any(input: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| input.contains(pattern))
}

pub(super) fn push_command(tasks: &mut Vec<String>, command: Option<&Vec<String>>) {
    if let Some(command) = command {
        let joined = command.join(" ");
        if !tasks.iter().any(|task| task == &joined) {
            tasks.push(joined);
        }
    }
}

pub(super) fn default_tasks(commands: &CommandSet) -> Vec<&Vec<String>> {
    [
        commands.build.as_ref(),
        commands.test.as_ref(),
        commands.run.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(super) fn normalize_fix_chain_step(action: &str) -> String {
    match action {
        "install_dependency" => "dependency".to_string(),
        "add_use" | "add_trait_import" => "import".to_string(),
        "fix_syntax" => "syntax".to_string(),
        "fix_borrow" => "borrow".to_string(),
        _ => "build".to_string(),
    }
}
