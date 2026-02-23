use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::Value;

fn unique_store_dir(test_name: &str) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("design_v1_{test_name}_{nanos}")).to_string_lossy().to_string()
}

fn run_cli(store_dir: &str, args: &[&str]) -> (i32, Value) {
    let exe = env!("CARGO_BIN_EXE_design");
    let out = Command::new(exe)
        .args(args)
        .env("DESIGN_STORE_DIR", store_dir)
        .output()
        .expect("failed to run design cli");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    
    let code = out.status.code().unwrap_or(-1);
    let json: Value = if code == 0 {
        serde_json::from_str(&stdout).unwrap_or(Value::Null)
    } else {
        serde_json::from_str(&stderr).unwrap_or(Value::Null)
    };

    (code, json)
}

fn first_l2_card_id(analyze_json: &Value) -> Option<String> {
    let nodes = analyze_json["data"]["graph"]["nodes"].as_array()?;
    nodes
        .iter()
        .find(|n| n["type"].as_str() == Some("L2"))
        .and_then(|n| n["id"].as_str())
        .map(|s| s.to_string())
}

#[test]
fn schema_v1_wrapper_structure() {
    let store = unique_store_dir("wrapper");
    let (code, json) = run_cli(&store, &["--json", "analyze", "test text"]);
    
    assert_eq!(code, 0);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], "1.0");
    assert_eq!(json["command"], "analyze");
    assert!(json["data"].is_object());
    assert!(json["error"].is_null());
}

#[test]
fn analyze_data_schema() {
    let store = unique_store_dir("analyze");
    let (code, json) = run_cli(&store, &["--json", "analyze", "security is important"]);
    
    assert_eq!(code, 0);
    let data = &json["data"];
    assert!(data["l1_count"].is_number());
    assert!(data["l2_count"].is_number());
    assert!(data["stability_score"].is_number());
    assert!(data["ambiguity_score"].is_number());
    assert!(data["graph"]["nodes"].is_array());
    assert!(data["graph"]["edges"].is_array());
    assert!(data["snapshot"]["l1_hash"].is_string());
    assert_eq!(data["snapshot"]["version"], 2);
}

#[test]
fn explain_data_schema() {
    let store = unique_store_dir("explain");
    let _ = run_cli(&store, &["analyze", "goal: fast system"]);
    let (code, json) = run_cli(&store, &["--json", "explain"]);
    
    assert_eq!(code, 0);
    let data = &json["data"];
    assert!(data["stability_label"].is_string());
    assert!(data["ambiguity_label"].is_string());
    assert!(data["template_id"].is_string());
    assert_eq!(data["schema_version"], "1.6");
    assert_eq!(data["optimization"], "pareto_frontier");
    assert!(data["remediations"].is_array());
    assert!(data["missing_info"].is_array());
    assert!(data["drafts"].is_array());
    assert!(data["provocation"].is_string());
    assert!(data["cards"].is_array());
}

#[test]
fn design_cards_contain_l1_info() {
    let store = unique_store_dir("cards");
    let _ = run_cli(&store, &["analyze", "教室管理ツールを開発したい"]);
    let (code, json) = run_cli(&store, &["--json", "explain"]);
    
    assert_eq!(code, 0);
    let cards = &json["data"]["cards"];
    assert!(cards.is_array());
    assert!(cards.as_array().unwrap().len() >= 1);
    
    let first_card = &cards[0];
    assert!(first_card["id"].as_str().unwrap().starts_with("CARD-"));
    assert!(first_card["title"].is_string());
    assert!(first_card["overview"].is_string());
}

#[test]
fn error_json_structure() {
    let store = unique_store_dir("error");
    let (code, json) = run_cli(&store, &["--json", "invalid-cmd"]);
    
    assert_eq!(code, 3);
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], 3);
    assert_eq!(json["error"]["type"], "InvalidCommand");
    assert!(json["error"]["message"].is_string());
}

#[test]
fn session_json_file_schema() {
    let store = unique_store_dir("session_file");
    let _ = run_cli(&store, &["analyze", "session test"]);
    let _ = run_cli(&store, &["--session", "mysess", "session", "save"]);
    
    let path = std::path::Path::new(&store).join("session_mysess.json");
    assert!(path.exists());
    
    let raw = std::fs::read_to_string(path).unwrap();
    let json: Value = serde_json::from_str(&raw).unwrap();
    
    assert_eq!(json["schema_version"], "1.0");
    assert_eq!(json["snapshot_version"], 2);
    assert_eq!(json["id"], "mysess");
    assert!(json["l1_units"].is_array());
    assert!(json["l2_units"].is_array());
    assert!(json["snapshot"]["l1_hash"].is_string());
    assert!(json["feedback_entries"].is_array());
}

#[test]
fn simulate_data_schema_v1_3() {
    let store = unique_store_dir("simulate");
    let _ = run_cli(&store, &["analyze", "high availability with strict memory limit"]);
    let (code, json) = run_cli(&store, &["--json", "simulate", "--target", "1", "--delta", "0.1"]);

    assert_eq!(code, 0);
    let data = &json["data"];
    assert_eq!(data["schema_version"], "1.3");
    assert!(data["impact_summary"]["stability_delta"].is_number());
    assert!(data["impact_summary"]["risk_delta"].is_number());
    assert!(data["affected_concepts"].is_array());
    assert!(data["blast_radius"]["coverage"].is_number());
    assert!(data["blast_radius"]["intensity"].is_number());
    assert!(data["blast_radius"]["structural_risk"].is_number());
    assert!(data["blast_radius"]["total_score"].is_number());
}

#[test]
fn explain_generates_missing_info_for_develop_intent() {
    let store = unique_store_dir("missing_info");
    let _ = run_cli(&store, &["analyze", "このシステムを開発したい"]);
    let (code, json) = run_cli(&store, &["--json", "explain"]);
    assert_eq!(code, 0);
    let missing = json["data"]["missing_info"].as_array().expect("missing_info array");
    assert!(!missing.is_empty());
}

#[test]
fn analyze_is_cumulative_in_same_session() {
    let store = unique_store_dir("cumulative");
    let (c1, j1) = run_cli(&store, &["--json", "analyze", "要件A"]);
    assert_eq!(c1, 0);
    let l1_first = j1["data"]["l1_count"].as_u64().unwrap_or(0);

    let (c2, j2) = run_cli(&store, &["--json", "analyze", "要件B"]);
    assert_eq!(c2, 0);
    let l1_second = j2["data"]["l1_count"].as_u64().unwrap_or(0);
    assert!(l1_second >= l1_first + 1);
}

#[test]
fn clear_resets_context() {
    let store = unique_store_dir("clear");
    let _ = run_cli(&store, &["analyze", "要件A"]);
    let _ = run_cli(&store, &["clear"]);
    let (code, json) = run_cli(&store, &["--json", "analyze", "要件B"]);
    assert_eq!(code, 0);
    let l1_count = json["data"]["l1_count"].as_u64().unwrap_or(0);
    assert_eq!(l1_count, 1);
}

#[test]
fn adopt_draft_increases_l1_units() {
    let store = unique_store_dir("adopt");
    let _ = run_cli(&store, &["--json", "analyze", "ダッシュボードを作りたい"]);
    let (_, explain) = run_cli(&store, &["--json", "explain"]);
    let drafts = explain["data"]["drafts"].as_array().expect("drafts");
    if drafts.is_empty() {
        return;
    }
    let draft_id = drafts[0]["draft_id"].as_str().unwrap();
    let (code, adopted) = run_cli(&store, &["--json", "adopt", "--draft-id", draft_id]);
    assert_eq!(code, 0);
    assert_eq!(adopted["data"]["adopted"], true);
    assert!(adopted["data"]["l1_count"].as_u64().unwrap_or(0) >= 2);
}

#[test]
fn reject_records_feedback_and_deprioritizes_draft() {
    let store = unique_store_dir("reject");
    let _ = run_cli(&store, &["--json", "analyze", "ダッシュボードを作りたい"]);
    let (_, explain1) = run_cli(&store, &["--json", "explain"]);
    let drafts = explain1["data"]["drafts"].as_array().expect("drafts");
    if drafts.is_empty() {
        return;
    }
    let rejected = drafts[0]["draft_id"].as_str().unwrap().to_string();

    for _ in 0..5 {
        let (_, latest) = run_cli(&store, &["--json", "explain"]);
        let current = latest["data"]["drafts"].as_array().expect("drafts");
        if current.iter().any(|d| d["draft_id"].as_str() == Some(rejected.as_str())) {
            let (code, _) = run_cli(&store, &["--json", "reject", "--draft-id", &rejected]);
            assert_eq!(code, 0);
        } else {
            break;
        }
    }

    let path = std::path::Path::new(&store).join("session_default.json");
    let raw = std::fs::read_to_string(path).unwrap();
    let json: Value = serde_json::from_str(&raw).unwrap();
    let entries = json["feedback_entries"].as_array().expect("feedback_entries");
    assert!(!entries.is_empty());
    assert_eq!(entries[0]["action"], "Reject");

    let (_, explain2) = run_cli(&store, &["--json", "explain"]);
    let top_after = explain2["data"]["drafts"][0]["draft_id"].as_str().unwrap_or("");
    assert_ne!(top_after, rejected);
}

#[test]
fn export_artifacts_rust_writes_files() {
    let store = unique_store_dir("export_artifacts_rust");
    let _ = run_cli(&store, &["analyze", "高性能で信頼性の高いAPIサーバー"]);
    let out_dir = std::path::Path::new(&store).join("artifacts_rust");
    let out_arg = out_dir.to_string_lossy().to_string();

    let (code, json) = run_cli(
        &store,
        &["--json", "export", "--format", "rust", "--out", &out_arg],
    );
    assert_eq!(code, 0);
    assert_eq!(json["data"]["format"], "rust");
    let files = json["data"]["files"].as_array().expect("files");
    assert!(!files.is_empty());
    for file in files {
        let p = std::path::Path::new(file.as_str().unwrap());
        assert!(p.exists());
        assert_eq!(p.extension().and_then(|s| s.to_str()), Some("rs"));
    }
}

#[test]
fn search_command_requires_permission_flag() {
    let store = unique_store_dir("search_permission");
    let (_, analyze) = run_cli(&store, &["--json", "analyze", "高信頼APIを設計する"]);
    let card = match first_l2_card_id(&analyze) {
        Some(c) => c,
        None => return,
    };
    let (code, json) = run_cli(&store, &["--json", "search", "--card", &card]);
    assert_eq!(code, 3);
    assert_eq!(json["error"]["type"], "InvalidCommand");
}

#[test]
fn search_and_refine_update_card_state() {
    let store = unique_store_dir("search_refine");
    let (_, analyze) = run_cli(&store, &["--json", "analyze", "監査可能で高速な認証基盤"]);
    let card = match first_l2_card_id(&analyze) {
        Some(c) => c,
        None => return,
    };

    let (s_code, s_json) = run_cli(
        &store,
        &["--json", "search", "--card", &card, "--query", "auth security benchmark", "--allow"],
    );
    assert_eq!(s_code, 0);
    assert_eq!(s_json["data"]["grounded"], true);
    assert!(s_json["data"]["results"].as_array().map(|a| !a.is_empty()).unwrap_or(false));

    let (r_code, r_json) = run_cli(
        &store,
        &["--json", "refine", "--card", &card, "--text", "p99 latency < 120ms を追加"],
    );
    assert_eq!(r_code, 0);
    assert_eq!(r_json["data"]["refined"], true);
    assert!(r_json["data"]["stability_score"].is_number());
}
