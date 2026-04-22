use serde_json::Value;
use std::process::Command;

// The CLI uses try_parse() which routes --help through the JSON error handler.
// Help text is embedded in the "error.message" field of the stderr JSON.
fn help_text() -> String {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe)
        .arg("--help")
        .output()
        .expect("run design --help");
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Ok(json) = serde_json::from_str::<Value>(&stderr) {
        if let Some(msg) = json["error"]["message"].as_str() {
            return msg.to_owned();
        }
    }
    // Fallback: check stdout as well (in case future versions change behavior)
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn help_lists_replay_command() {
    let help = help_text();
    assert!(
        help.contains("replay"),
        "help output must include 'replay' command, got:\n{help}"
    );
}

#[test]
fn help_lists_all_expected_commands() {
    let help = help_text();
    for cmd in &["analyze", "explain", "simulate", "replay", "export", "phase9"] {
        assert!(
            help.contains(cmd),
            "help output must include '{cmd}', got:\n{help}"
        );
    }
}
