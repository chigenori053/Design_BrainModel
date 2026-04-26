use std::process::Command;

pub fn assert_break(scenario: &str, layer: &str, cause: &str) {
    let bin = env!("CARGO_BIN_EXE_verification_cli");
    let output = Command::new(bin)
        .args(["audit", "--scenario", scenario])
        .output()
        .expect("audit");

    assert!(
        output.status.success(),
        "audit failed for {scenario}: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is valid json");
    assert_eq!(report["result"], "nondeterministic");
    assert_eq!(report["layer"], layer);
    assert_eq!(report["cause"], cause);
}

pub fn assert_deterministic(scenario: &str) {
    let bin = env!("CARGO_BIN_EXE_verification_cli");
    let output = Command::new(bin)
        .args(["audit", "--scenario", scenario])
        .output()
        .expect("audit");

    assert!(
        output.status.success(),
        "audit failed for {scenario}: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is valid json");
    assert_eq!(report["result"], "deterministic");
    assert!(report["layer"].is_null());
    assert!(report["cause"].is_null());
}
