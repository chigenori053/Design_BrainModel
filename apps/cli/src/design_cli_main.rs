//! design_cli — Design Brain Model (DBM) インタラクティブ CLI
//!
//! Claude Code と同様に自然言語で DBM Core 機能を操作できるツール。
//! 実行コマンド: `design_cli [path]`

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::io::{BufRead, Write};

use agent_core::{HvPolicy, Phase1Config, run_phase1_matrix};
use clap::Parser;
use design_search_engine::{
    BeamSearchController, SearchConfig as DesignSearchConfig, SearchController as _, rank_candidates,
};
use runtime_core::{ModalityInput, RuntimeStage};
use runtime_vm::{ExecutionMode as RuntimeExecutionMode, HybridVm as RuntimeHybridVm, Phase9RuntimeAdapter};
use world_model_core::{
    ConsistencyEvaluator, DeltaConsistencyEvaluator, DeterministicWorldModel,
    HypothesisGenerator, SimpleHypothesisGenerator, WorldModel,
};

const VERSION: &str = "0.1.0";
const PARETO_EPS: f64 = 1e-12;
const INTENT_THRESHOLD: f32 = 0.60;
const INTENT_GAP: f32 = 0.12;

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
struct ObjectiveVec {
    raw: [f64; 4],
    normalized: [f64; 4],
    clamped: [f64; 4],
}

#[derive(Clone, Debug)]
struct ObjCase {
    case_id: String,
    category: String,
    objective: ObjectiveVec,
}

// ── CLI args ──────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "design_cli",
    about = "DBM Design Brain Model — Claude Code-like interactive CLI",
    version = VERSION,
    disable_version_flag = false
)]
struct Args {
    /// 解析対象ディレクトリ (デフォルト: カレントディレクトリ)
    path: Option<String>,

    /// ビームサーチ幅 (デフォルト: 5)
    #[arg(long = "beam", default_value_t = 5)]
    beam_width: usize,

    /// 乱数シード (デフォルト: 42)
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// 最大ステップ数 (デフォルト: 25)
    #[arg(long = "steps", default_value_t = 25)]
    max_steps: usize,
}

// ── Session ───────────────────────────────────────────────────────────────────

struct Session {
    working_dir: String,
    seed: u64,
    beam_width: usize,
    max_steps: usize,
    history: Vec<String>,
    turn: usize,
}

// ── Intent ────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum Intent {
    Analyze { path: Option<String> },
    Simulate,
    Explain,
    Phase9 { text: String },
    Help,
    Status,
    Clear,
    Exit,
    Unknown,
}

struct IntentCandidate {
    intent: Intent,
    score: f32,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    ctrlc::set_handler(|| {
        eprintln!("\n[dbm] Ctrl+C を受信しました。/exit で終了してください。");
    })
    .expect("Ctrl-C ハンドラの設定に失敗しました");

    let args = Args::parse();
    let working_dir = args.path.clone().unwrap_or_else(|| ".".to_string());

    let mut session = Session {
        working_dir,
        seed: args.seed,
        beam_width: args.beam_width,
        max_steps: args.max_steps,
        history: Vec::new(),
        turn: 0,
    };

    if let Err(e) = run_session(&mut session) {
        eprintln!("[dbm] Fatal: {e}");
        std::process::exit(1);
    }
}

// ── Session loop ──────────────────────────────────────────────────────────────

fn run_session(session: &mut Session) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();

    print_banner(&mut out, session)?;

    loop {
        write!(out, "\ndbm> ").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }

        let input = line.trim().to_string();
        if input.is_empty() {
            continue;
        }

        session.history.push(input.clone());
        session.turn += 1;

        let should_exit = if input.starts_with('/') {
            handle_slash(&input, session, &mut out)?
        } else {
            handle_natural_language(&input, session, &mut out)?
        };

        if should_exit {
            break;
        }
    }

    writeln!(out, "\n[dbm] セッション終了。").map_err(|e| e.to_string())?;
    Ok(())
}

// ── Banner ────────────────────────────────────────────────────────────────────

fn print_banner<W: Write>(out: &mut W, session: &Session) -> Result<(), String> {
    let w = 62usize;
    let sep = "─".repeat(w);

    writeln!(out, "\n╭{sep}╮").map_err(|e| e.to_string())?;
    writeln!(out, "│  DBM  Design Brain Model  v{VERSION:<33}│")
        .map_err(|e| e.to_string())?;
    writeln!(out, "│  Working directory: {:<42}│", truncate(&session.working_dir, 42))
        .map_err(|e| e.to_string())?;
    writeln!(out, "╰{sep}╯").map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())?;
    writeln!(
        out,
        "  自然言語または /コマンド で入力してください。/help でヘルプ表示。"
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Natural language ──────────────────────────────────────────────────────────

fn handle_natural_language<W: Write>(
    input: &str,
    session: &mut Session,
    out: &mut W,
) -> Result<bool, String> {
    let candidate = detect_intent(input);
    let score_pct = (candidate.score * 100.0) as u32;

    // Exit は早期リターン
    if let Intent::Exit = candidate.intent {
        writeln!(out, "\n  ✔ Intent: EXIT").map_err(|e| e.to_string())?;
        return Ok(true);
    }

    let result: Result<(), String> = match candidate.intent {
        Intent::Analyze { path } => {
            let target = path.unwrap_or_else(|| session.working_dir.clone());
            writeln!(out, "\n  ✔ Intent: ANALYZE  ({score_pct}% confidence) → {target}")
                .map_err(|e| e.to_string())?;
            do_analyze(&target, session, out)
        }
        Intent::Simulate => {
            writeln!(out, "\n  ✔ Intent: SIMULATE  ({score_pct}% confidence)")
                .map_err(|e| e.to_string())?;
            do_simulate(session, out)
        }
        Intent::Explain => {
            writeln!(out, "\n  ✔ Intent: EXPLAIN  ({score_pct}% confidence)")
                .map_err(|e| e.to_string())?;
            do_explain(session, out)
        }
        Intent::Phase9 { text } => {
            writeln!(out, "\n  ✔ Intent: PHASE9  ({score_pct}% confidence)")
                .map_err(|e| e.to_string())?;
            do_phase9(text, session, out)
        }
        Intent::Help => {
            writeln!(out, "\n  ✔ Intent: HELP").map_err(|e| e.to_string())?;
            print_help(out)
        }
        Intent::Status => {
            writeln!(out, "\n  ✔ Intent: STATUS").map_err(|e| e.to_string())?;
            print_status(session, out)
        }
        Intent::Clear => {
            writeln!(out, "\n  ✔ Intent: CLEAR").map_err(|e| e.to_string())?;
            session.history.clear();
            session.turn = 0;
            writeln!(out, "  コンテキストをクリアしました。").map_err(|e| e.to_string())
        }
        Intent::Unknown | Intent::Exit => {
            writeln!(
                out,
                "\n  ? 入力を理解できませんでした (confidence: {score_pct}%)"
            )
            .map_err(|e| e.to_string())?;
            writeln!(out, "    /help でコマンド一覧を確認してください。")
                .map_err(|e| e.to_string())
        }
    };

    if let Err(e) = result {
        writeln!(out, "\n  ✗ Error: {e}").map_err(|e2| e2.to_string())?;
    }
    Ok(false)
}

// ── Slash commands ────────────────────────────────────────────────────────────

fn handle_slash<W: Write>(
    input: &str,
    session: &mut Session,
    out: &mut W,
) -> Result<bool, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let cmd = tokens[0].trim_start_matches('/');
    let rest = &tokens[1..];

    let result = match cmd {
        "exit" | "quit" | "q" => return Ok(true),

        "help" | "h" => print_help(out),

        "status" => print_status(session, out),

        "clear" => {
            session.history.clear();
            session.turn = 0;
            writeln!(out, "\n  コンテキストをクリアしました。").map_err(|e| e.to_string())
        }

        "analyze" => {
            let path = rest
                .first()
                .map(|s| s.to_string())
                .unwrap_or_else(|| session.working_dir.clone());
            do_analyze(&path, session, out)
        }

        "simulate" => do_simulate(session, out),

        "explain" => do_explain(session, out),

        "phase9" => {
            let text = if rest.is_empty() {
                "architecture check".to_string()
            } else {
                rest.join(" ")
            };
            do_phase9(text, session, out)
        }

        "seed" => {
            if let Some(s) = rest.first() {
                match s.parse::<u64>() {
                    Ok(v) => {
                        session.seed = v;
                        writeln!(out, "\n  seed = {v}").map_err(|e| e.to_string())
                    }
                    Err(_) => writeln!(out, "\n  Error: seed は正整数で指定してください。")
                        .map_err(|e| e.to_string()),
                }
            } else {
                writeln!(out, "\n  seed = {}", session.seed).map_err(|e| e.to_string())
            }
        }

        "beam" => {
            if let Some(s) = rest.first() {
                match s.parse::<usize>() {
                    Ok(v) if v > 0 => {
                        session.beam_width = v;
                        writeln!(out, "\n  beam_width = {v}").map_err(|e| e.to_string())
                    }
                    Ok(_) => writeln!(out, "\n  Error: beam は 1 以上で指定してください。")
                        .map_err(|e| e.to_string()),
                    Err(_) => writeln!(out, "\n  Error: beam は正整数で指定してください。")
                        .map_err(|e| e.to_string()),
                }
            } else {
                writeln!(out, "\n  beam_width = {}", session.beam_width)
                    .map_err(|e| e.to_string())
            }
        }

        unknown => writeln!(
            out,
            "\n  Unknown command: /{unknown}  — /help でコマンド一覧を確認してください。"
        )
        .map_err(|e| e.to_string()),
    };

    if let Err(e) = result {
        writeln!(out, "\n  ✗ Error: {e}").map_err(|e2| e2.to_string())?;
    }
    Ok(false)
}

// ── Help / Status ─────────────────────────────────────────────────────────────

fn print_help<W: Write>(out: &mut W) -> Result<(), String> {
    let lines = [
        "┌── DBM コマンド一覧 ───────────────────────────────────────┐",
        "│  /analyze [path]   Phase1 多目的解析を実行               │",
        "│  /simulate         Phase1 シミュレーションを実行         │",
        "│  /explain          Phase1 結果を説明                     │",
        "│  /phase9 [text]    Phase9-D アーキテクチャ解析を実行     │",
        "│  /seed [N]         乱数シードを設定・確認                │",
        "│  /beam [N]         ビームサーチ幅を設定・確認            │",
        "│  /status           セッション情報を表示                  │",
        "│  /clear            コンテキストをクリア                  │",
        "│  /exit             終了                                   │",
        "└──────────────────────────────────────────────────────────┘",
    ];
    writeln!(out).map_err(|e| e.to_string())?;
    for line in &lines {
        writeln!(out, "  {line}").map_err(|e| e.to_string())?;
    }
    writeln!(out, "\n  自然言語入力例:").map_err(|e| e.to_string())?;
    writeln!(out, "    「このプロジェクトを解析して」  →  /analyze").map_err(|e| e.to_string())?;
    writeln!(out, "    「シミュレーションを実行して」  →  /simulate").map_err(|e| e.to_string())?;
    writeln!(out, "    「説明して」                    →  /explain").map_err(|e| e.to_string())?;
    writeln!(out, "    「アーキテクチャを確認して」    →  /phase9").map_err(|e| e.to_string())?;
    Ok(())
}

fn print_status<W: Write>(session: &Session, out: &mut W) -> Result<(), String> {
    writeln!(out, "\n  ── セッション状態 ──────────────────────────────").map_err(|e| e.to_string())?;
    writeln!(out, "  Working dir  : {}", session.working_dir).map_err(|e| e.to_string())?;
    writeln!(out, "  Seed         : {}", session.seed).map_err(|e| e.to_string())?;
    writeln!(out, "  Beam width   : {}", session.beam_width).map_err(|e| e.to_string())?;
    writeln!(out, "  Max steps    : {}", session.max_steps).map_err(|e| e.to_string())?;
    writeln!(out, "  Turn         : {}", session.turn).map_err(|e| e.to_string())?;
    writeln!(out, "  History      : {} 件", session.history.len()).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Intent detection ──────────────────────────────────────────────────────────

fn detect_intent(input: &str) -> IntentCandidate {
    let lower = input.to_lowercase();
    let mut candidates: Vec<IntentCandidate> = Vec::new();

    // Analyze
    if input.contains("解析")
        || input.contains("分析")
        || lower.contains("analyze")
        || lower.contains("analysis")
    {
        let has_target = input.contains("このプロジェクト")
            || input.contains("これ")
            || lower.contains("this")
            || lower.contains("project");
        let score = if has_target { 0.95 } else { 0.80 };
        let path = extract_path_token(input);
        candidates.push(IntentCandidate {
            intent: Intent::Analyze { path },
            score,
        });
    }

    // Simulate
    if input.contains("シミュレ")
        || lower.contains("simulat")
        || (lower.contains("run") && !lower.contains("analyze"))
    {
        candidates.push(IntentCandidate {
            intent: Intent::Simulate,
            score: 0.85,
        });
    }

    // Explain
    if input.contains("説明")
        || input.contains("教えて")
        || input.contains("どういう")
        || lower.contains("explain")
        || lower.contains("describe")
        || lower.contains("what is")
    {
        candidates.push(IntentCandidate {
            intent: Intent::Explain,
            score: 0.82,
        });
    }

    // Phase9
    if input.contains("アーキテクチャ")
        || input.contains("phase9")
        || lower.contains("architecture")
        || lower.contains("phase9")
    {
        candidates.push(IntentCandidate {
            intent: Intent::Phase9 {
                text: input.to_string(),
            },
            score: 0.90,
        });
    }

    // Help
    if input.contains("ヘルプ")
        || input.contains("使い方")
        || input.contains("コマンド")
        || lower.contains("help")
        || lower.contains("command")
    {
        candidates.push(IntentCandidate {
            intent: Intent::Help,
            score: 0.95,
        });
    }

    // Status
    if input.contains("状態")
        || input.contains("ステータス")
        || lower.contains("status")
        || lower.contains("info")
    {
        candidates.push(IntentCandidate {
            intent: Intent::Status,
            score: 0.90,
        });
    }

    // Clear
    if input.contains("クリア")
        || input.contains("リセット")
        || lower.contains("clear")
        || lower.contains("reset")
    {
        candidates.push(IntentCandidate {
            intent: Intent::Clear,
            score: 0.90,
        });
    }

    // Exit
    if input.contains("終了")
        || input.contains("やめ")
        || input.contains("閉じ")
        || lower.contains("exit")
        || lower.contains("quit")
        || lower.contains("bye")
    {
        candidates.push(IntentCandidate {
            intent: Intent::Exit,
            score: 0.95,
        });
    }

    if candidates.is_empty() {
        return IntentCandidate {
            intent: Intent::Unknown,
            score: 0.0,
        };
    }

    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));

    let top = candidates.remove(0);
    if top.score < INTENT_THRESHOLD {
        return IntentCandidate {
            intent: Intent::Unknown,
            score: top.score,
        };
    }

    // Ambiguity check
    if !candidates.is_empty() && (top.score - candidates[0].score) < INTENT_GAP {
        // Still proceed with the top candidate but at reduced confidence
        return IntentCandidate {
            score: top.score * 0.85,
            ..top
        };
    }

    top
}

fn extract_path_token(input: &str) -> Option<String> {
    input
        .split_whitespace()
        .find(|t| {
            t.starts_with('.') || t.starts_with('/') || (t.contains('/') && !t.contains(':'))
        })
        .map(|s| s.to_string())
}

// ── DBM operations ────────────────────────────────────────────────────────────

fn do_analyze<W: Write>(path: &str, session: &Session, out: &mut W) -> Result<(), String> {
    writeln!(
        out,
        "  ⟳ Phase1 解析中: {path}  (seed={}, beam={}, steps={})...",
        session.seed, session.beam_width, session.max_steps
    )
    .map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;

    let (cases, frontier, hv, hash) = run_phase1_analysis(session)?;

    let w = 54usize;
    writeln!(out, "\n  ┌── Phase1 解析結果 {}", "─".repeat(w - 18)).map_err(|e| e.to_string())?;
    writeln!(out, "  │  対象パス    : {}", truncate(path, 38)).map_err(|e| e.to_string())?;
    writeln!(out, "  │  候補ケース数: {}", cases.len()).map_err(|e| e.to_string())?;
    writeln!(out, "  │  フロンティア: {} ケース", frontier.len()).map_err(|e| e.to_string())?;
    writeln!(out, "  │  超体積 (HV) : {:.6}", hv).map_err(|e| e.to_string())?;
    writeln!(out, "  │  Hash        : {}", &hash[..16.min(hash.len())]).map_err(|e| e.to_string())?;
    writeln!(out, "  └{}", "─".repeat(w)).map_err(|e| e.to_string())?;

    writeln!(out, "\n  フロンティア上位5件 (SI / CS / RP / ER):").map_err(|e| e.to_string())?;
    for (i, case) in frontier.iter().take(5).enumerate() {
        let o = case.objective.clamped;
        writeln!(
            out,
            "  [{i}] {:<26}  {:.3} / {:.3} / {:.3} / {:.3}",
            truncate(&case.case_id, 26),
            o[0],
            o[1],
            o[2],
            o[3]
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn do_simulate<W: Write>(session: &Session, out: &mut W) -> Result<(), String> {
    writeln!(
        out,
        "  ⟳ Phase1 シミュレーション実行中 (seed={}, beam={}, steps={})...",
        session.seed, session.beam_width, session.max_steps
    )
    .map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;

    let (cases, frontier, hv, _) = run_phase1_analysis(session)?;
    let mean = objective_mean(&frontier);

    let w = 54usize;
    writeln!(out, "\n  ┌── シミュレーション結果 {}", "─".repeat(w - 22)).map_err(|e| e.to_string())?;
    writeln!(out, "  │  探索ステップ: {}", cases.len()).map_err(|e| e.to_string())?;
    writeln!(out, "  │  フロンティア: {} ケース", frontier.len()).map_err(|e| e.to_string())?;
    writeln!(out, "  │  超体積 (HV) : {:.6}", hv).map_err(|e| e.to_string())?;
    writeln!(out, "  └{}", "─".repeat(w)).map_err(|e| e.to_string())?;

    if let Some(best) = frontier.first() {
        let o = best.objective.clamped;
        writeln!(out, "\n  最良候補: {}", truncate(&best.case_id, 40)).map_err(|e| e.to_string())?;
        writeln!(out, "    SI={:.3}  CS={:.3}  RP={:.3}  ER={:.3}", o[0], o[1], o[2], o[3])
            .map_err(|e| e.to_string())?;
    }

    writeln!(
        out,
        "\n  フロンティア平均: SI={:.3}  CS={:.3}  RP={:.3}  ER={:.3}",
        mean[0], mean[1], mean[2], mean[3]
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn do_explain<W: Write>(session: &Session, out: &mut W) -> Result<(), String> {
    writeln!(
        out,
        "  ⟳ Phase1 説明生成中 (seed={}, beam={}, steps={})...",
        session.seed, session.beam_width, session.max_steps
    )
    .map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;

    let (cases, frontier, hv, hash) = run_phase1_analysis(session)?;
    let mean = objective_mean(&frontier);

    let w = 54usize;
    writeln!(out, "\n  ┌── Phase1 説明 {}", "─".repeat(w - 14)).map_err(|e| e.to_string())?;
    writeln!(out, "  │  多目的最適化 (Phase1 決定論的ビームサーチ)").map_err(|e| e.to_string())?;
    writeln!(out, "  │").map_err(|e| e.to_string())?;
    writeln!(out, "  │  目的空間 (4次元):").map_err(|e| e.to_string())?;
    writeln!(out, "  │    SI  Structural Integrity  (構造整合性)").map_err(|e| e.to_string())?;
    writeln!(out, "  │    CS  Cognitive Stability   (認知安定性)").map_err(|e| e.to_string())?;
    writeln!(out, "  │    RP  Revision Pressure     (改訂圧力)").map_err(|e| e.to_string())?;
    writeln!(out, "  │    ER  Exploration Readiness (探索準備度)").map_err(|e| e.to_string())?;
    writeln!(out, "  │").map_err(|e| e.to_string())?;
    writeln!(out, "  │  探索結果:").map_err(|e| e.to_string())?;
    writeln!(out, "  │    候補数       : {}", cases.len()).map_err(|e| e.to_string())?;
    writeln!(out, "  │    フロンティア : {} ケース", frontier.len()).map_err(|e| e.to_string())?;
    writeln!(out, "  │    超体積 (HV)  : {:.6}", hv).map_err(|e| e.to_string())?;
    writeln!(out, "  │    Hash         : {}", &hash[..16.min(hash.len())]).map_err(|e| e.to_string())?;
    writeln!(out, "  │").map_err(|e| e.to_string())?;
    writeln!(
        out,
        "  │  フロンティア平均: SI={:.3} CS={:.3} RP={:.3} ER={:.3}",
        mean[0], mean[1], mean[2], mean[3]
    )
    .map_err(|e| e.to_string())?;
    writeln!(out, "  └{}", "─".repeat(w)).map_err(|e| e.to_string())?;

    Ok(())
}

fn do_phase9<W: Write>(text: String, _session: &Session, out: &mut W) -> Result<(), String> {
    writeln!(out, "  ⟳ Phase9-D アーキテクチャ解析実行中...").map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;

    let accepted = ModalityInput::accepted_modalities();

    let mut vm = RuntimeHybridVm::new(RuntimeExecutionMode::Reasoning);
    vm.set_input_text(text.clone());
    vm.execute();

    let ctx = Phase9RuntimeAdapter::from_legacy(vm.context());

    let current_state = ctx
        .world_state
        .clone()
        .unwrap_or_else(|| world_model_core::WorldState::new(1, vec![1.0, 1.0, 1.0]));

    let generator = SimpleHypothesisGenerator;
    let generated = generator
        .generate(&current_state, ctx.recall_result.as_ref())
        .map_err(|e| format!("仮説生成エラー: {e}"))?;

    let selected = generated
        .first()
        .cloned()
        .ok_or_else(|| "仮説が生成されませんでした".to_string())?;

    let model = DeterministicWorldModel;
    let prediction = model
        .transition(&current_state, &selected)
        .map_err(|e| format!("遷移評価エラー: {e}"))?;

    let evaluator = DeltaConsistencyEvaluator;
    let consistency = evaluator
        .evaluate(&current_state, &prediction)
        .map_err(|e| format!("整合性評価エラー: {e}"))?;

    let search_controller = BeamSearchController::default();
    let search_config = DesignSearchConfig::default();
    let search_states = search_controller.search(
        current_state,
        ctx.recall_result.as_ref(),
        &search_config,
    );
    let ranked = rank_candidates(search_states.clone());
    let best_score = ranked.first().map(|c| c.score).unwrap_or(0.0);

    let stage_str = match ctx.stage {
        RuntimeStage::Input => "input",
        RuntimeStage::Normalize => "normalize",
        RuntimeStage::Recall => "recall",
        RuntimeStage::HypothesisGeneration => "hypothesis_generation",
        RuntimeStage::Search => "search",
        RuntimeStage::Simulation => "simulation",
        RuntimeStage::Evaluation => "evaluation",
        RuntimeStage::Ranking => "ranking",
        RuntimeStage::TransitionEvaluation => "transition_evaluation",
        RuntimeStage::ConsistencyEvaluation => "consistency_evaluation",
        RuntimeStage::Output => "output",
    };

    let w = 54usize;
    writeln!(out, "\n  ┌── Phase9-D アーキテクチャ報告 {}", "─".repeat(w - 30))
        .map_err(|e| e.to_string())?;
    writeln!(out, "  │  入力文字列   : {}", truncate(&text, 38)).map_err(|e| e.to_string())?;
    writeln!(out, "  │  Request ID   : {}", ctx.request_id.0).map_err(|e| e.to_string())?;
    writeln!(out, "  │  ステージ     : {stage_str}").map_err(|e| e.to_string())?;
    writeln!(out, "  │  モダリティ数 : {}", accepted.len()).map_err(|e| e.to_string())?;
    writeln!(
        out,
        "  │  仮説数       : {}",
        ctx.hypotheses.len() + generated.len()
    )
    .map_err(|e| e.to_string())?;
    writeln!(out, "  │  探索状態数   : {}", search_states.len()).map_err(|e| e.to_string())?;
    writeln!(out, "  │  最良スコア   : {:.6}", round6(best_score)).map_err(|e| e.to_string())?;
    writeln!(out, "  │  整合性スコア : {:.6}", round6(consistency.value))
        .map_err(|e| e.to_string())?;
    writeln!(out, "  └{}", "─".repeat(w)).map_err(|e| e.to_string())?;

    Ok(())
}

// ── Phase1 engine wrapper ─────────────────────────────────────────────────────

fn run_phase1_analysis(
    session: &Session,
) -> Result<(Vec<ObjCase>, Vec<ObjCase>, f64, String), String> {
    let cfg = Phase1Config {
        beam_width: session.beam_width,
        max_steps: session.max_steps,
        hv_policy: HvPolicy::Legacy,
        seed: session.seed,
        norm_alpha: 0.1,
        alpha: 3.0,
        temperature: 0.1,
        entropy_beta: 0.03,
        lambda_min: 0.2,
        lambda_target_entropy: 1.2,
        lambda_k: 0.2,
        lambda_ema: 0.4,
    };
    if !cfg.is_valid() {
        return Err("Phase1Config が無効です (beam_width / max_steps を確認してください)".to_string());
    }

    let (rows, _) = run_phase1_matrix(cfg);
    if rows.is_empty() {
        return Err("Phase1 エンジンが結果を返しませんでした".to_string());
    }

    let mut raw_cases: Vec<ObjCase> = Vec::with_capacity(rows.len());
    for (idx, row) in rows.iter().enumerate() {
        let raw = parse_vec4_pipe(&row.objective_vector_raw)
            .ok_or_else(|| format!("row {idx}: objective_vector_raw の解析に失敗しました"))?;
        raw_cases.push(ObjCase {
            case_id: format!("{}-{:04}-{:04}", row.variant, row.depth, row.beam_index),
            category: row.variant.clone(),
            objective: ObjectiveVec { raw, normalized: raw, clamped: raw },
        });
    }

    let cases = normalize_cases(raw_cases)?;
    let frontier = pareto_frontier(&cases);
    let hv = hypervolume_4d(&frontier);
    let hash = frontier_hash(&frontier);

    Ok((cases, frontier, hv, hash))
}

// ── Math utilities ────────────────────────────────────────────────────────────

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn parse_vec4_pipe(s: &str) -> Option<[f64; 4]> {
    let parts: Vec<&str> = s.split('|').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0.0f64; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p.parse().ok()?;
    }
    Some(out)
}

fn normalize_cases(mut cases: Vec<ObjCase>) -> Result<Vec<ObjCase>, String> {
    if cases.is_empty() {
        return Ok(cases);
    }
    let mut mins = [f64::INFINITY; 4];
    let mut maxs = [f64::NEG_INFINITY; 4];
    for c in &cases {
        for i in 0..4 {
            let v = c.objective.raw[i];
            if !v.is_finite() {
                return Err(format!("非有限値: case_id={} dim={i}", c.case_id));
            }
            mins[i] = mins[i].min(v);
            maxs[i] = maxs[i].max(v);
        }
    }
    for c in &mut cases {
        let mut norm = [0.0f64; 4];
        for i in 0..4 {
            norm[i] = if (maxs[i] - mins[i]).abs() > PARETO_EPS {
                (c.objective.raw[i] - mins[i]) / (maxs[i] - mins[i])
            } else {
                0.5
            };
        }
        c.objective.normalized = norm;
        c.objective.clamped = norm.map(clamp01);
    }
    Ok(cases)
}

fn dominates(a: &ObjectiveVec, b: &ObjectiveVec) -> bool {
    (0..4).all(|i| a.clamped[i] + PARETO_EPS >= b.clamped[i])
        && (0..4).any(|i| a.clamped[i] > b.clamped[i] + PARETO_EPS)
}

fn pareto_frontier(cases: &[ObjCase]) -> Vec<ObjCase> {
    let mut sorted = cases.to_vec();
    sorted.sort_by(|a, b| a.case_id.cmp(&b.case_id));

    let mut dedup: Vec<ObjCase> = Vec::new();
    let mut seen = BTreeSet::<String>::new();
    for c in sorted {
        if seen.insert(c.case_id.clone()) {
            dedup.push(c);
        }
    }

    let mut front: Vec<ObjCase> = Vec::new();
    for i in 0..dedup.len() {
        let dominated = (0..dedup.len())
            .filter(|&j| j != i)
            .any(|j| dominates(&dedup[j].objective, &dedup[i].objective));
        if !dominated {
            front.push(dedup[i].clone());
        }
    }
    front.sort_by(|a, b| a.case_id.cmp(&b.case_id));
    front
}

fn frontier_hash(frontier: &[ObjCase]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for item in frontier {
        fnv1a(&mut h, item.case_id.as_bytes());
        fnv1a(&mut h, b"|");
        for v in item.objective.normalized {
            fnv1a(&mut h, &v.to_bits().to_le_bytes());
        }
        fnv1a(&mut h, b"\n");
    }
    format!("{h:016x}")
}

fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash ^= *b as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

fn hypervolume_4d(frontier: &[ObjCase]) -> f64 {
    let points: Vec<[f64; 4]> = frontier.iter().map(|c| c.objective.clamped).collect();
    round6(hv_recursive(&points, 4))
}

fn hv_recursive(points: &[[f64; 4]], dim: usize) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if dim == 1 {
        return points.iter().map(|p| p[0]).fold(0.0_f64, f64::max);
    }
    let axis = 4 - dim;
    let mut coords: Vec<f64> = points.iter().map(|p| p[axis]).collect();
    coords.sort_by(|a, b| a.total_cmp(b));
    coords.dedup_by(|a, b| (*a - *b).abs() <= PARETO_EPS);

    let mut prev = 0.0;
    let mut volume = 0.0;
    for c in coords {
        let width = c - prev;
        if width > PARETO_EPS {
            let projected: Vec<[f64; 4]> = points
                .iter()
                .filter(|p| p[axis] + PARETO_EPS >= c)
                .map(|p| {
                    let mut q = [0.0f64; 4];
                    q[..(dim - 1)].copy_from_slice(&p[(axis + 1)..(axis + dim)]);
                    q
                })
                .collect();
            volume += width * hv_recursive(&projected, dim - 1);
        }
        prev = c;
    }
    volume
}

fn objective_mean(frontier: &[ObjCase]) -> [f64; 4] {
    if frontier.is_empty() {
        return [0.0; 4];
    }
    let mut sum = [0.0f64; 4];
    for c in frontier {
        for i in 0..4 {
            sum[i] += c.objective.clamped[i];
        }
    }
    let n = frontier.len() as f64;
    sum.map(|v| round6(v / n))
}

// ── String helpers ────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        chars[..max.saturating_sub(2)].iter().collect::<String>() + ".."
    }
}
