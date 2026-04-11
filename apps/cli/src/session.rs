use runtime_core::stable_v03::RuntimeResult;
use runtime_core::{ChatContext, Clarification, SlotMap, SlotValue};

use crate::design_delta::{
    DesignGraph, MutationCandidate, MutationPlan, MutationSearchResult, RationalityScore,
    TradeoffExplanation,
};
use crate::plan::Plan;
use crate::state::{Context, Mode, State};

/// Phase0 Patch: タスク（Phase4で本格実装）
///
/// CommandやAgentが生成する作業単位のプレースホルダ。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Task {
    pub description: String,
    pub completed: bool,
}

impl Task {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            completed: false,
        }
    }
}

/// Phase0 Patch: Agent型CLIセッション（拡張版）
///
/// CLIを状態機械として扱う。Phase2以降でPlannerとExecutorに接続される。
///
/// # 拡張ポイント
/// - `history` : ユーザー入力の履歴（Phase1でCommand履歴フィルタリングに使用）
/// - `transcript` : REPL が出力した応答ログ
/// - `tasks`   : タスクリスト（Phase4で本格実装）
#[derive(Clone, Debug, Default)]
pub struct AgentSession {
    pub workspace_root: Option<std::path::PathBuf>,
    pub design_baseline: Option<DesignGraph>,
    pub last_rationality_score: Option<RationalityScore>,
    pub active_mutation_plan: Option<MutationPlan>,
    pub mutation_candidates: Vec<MutationCandidate>,
    pub selected_mutation: Option<MutationCandidate>,
    pub mutation_search_depth: usize,
    pub last_mutation_search_result: Option<MutationSearchResult>,
    pub last_tradeoff_explanation: Option<TradeoffExplanation>,
    /// 現在の状態
    pub state: State,
    /// 実行中のプラン（Phase2で使用）
    pub current_plan: Option<Plan>,
    /// 実行モード（Phase2で使用）
    pub mode: Mode,
    /// セッションコンテキスト（スロット・推論情報）
    pub context: Context,
    /// 入力履歴（user input only）
    pub history: Vec<String>,
    /// 出力履歴（agent/system output）
    pub transcript: Vec<String>,
    /// タスクリスト（Phase4で使用）
    pub tasks: Vec<Task>,
}

impl AgentSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_root(root: std::path::PathBuf) -> Self {
        let mut s = Self::new();
        s.workspace_root = Some(root);
        s
    }

    pub fn record(&mut self, input: &str) {
        self.history.push(input.to_string());
    }

    pub fn record_output(&mut self, output: impl Into<String>) {
        self.transcript.push(output.into());
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatSession {
    pub history: Vec<String>,
    pub slot_state: Option<SlotMap>,
    pub pending_clarification: Option<Clarification>,
}

impl ChatSession {
    pub fn new() -> Self {
        Self {
            history: vec![],
            slot_state: None,
            pending_clarification: None,
        }
    }

    pub fn to_context(&self) -> ChatContext {
        ChatContext {
            history: self.history.clone(),
            last_slots: self.slot_state.clone(),
        }
    }

    pub fn update_success(&mut self, input: &str, result: &RuntimeResult) {
        self.history.push(input.to_string());
        if let Some(trace) = &result.intent_trace {
            self.slot_state = Some(trace.final_slots.clone());
        }
        self.resolve_clarification();
    }

    pub fn update_pending(
        &mut self,
        input: &str,
        merged_slots: Option<SlotMap>,
        clarification: Clarification,
    ) {
        self.history.push(input.to_string());
        self.slot_state = merged_slots;
        self.update_clarification(clarification);
    }

    pub fn update_clarification(&mut self, clarification: Clarification) {
        self.pending_clarification = Some(clarification);
    }

    pub fn resolve_clarification(&mut self) {
        self.pending_clarification = None;
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

pub fn merge_slots(prev: &SlotMap, new: &SlotMap) -> SlotMap {
    let mut merged = prev.clone();
    merge_slot_values(&mut merged.core, &new.core);
    merge_slot_values(&mut merged.system, &new.system);
    merge_slot_values(&mut merged.quality, &new.quality);
    merge_slot_values(&mut merged.optional, &new.optional);
    merged
}

fn merge_slot_values<K>(
    target: &mut std::collections::HashMap<K, SlotValue>,
    incoming: &std::collections::HashMap<K, SlotValue>,
) where
    K: std::cmp::Eq + std::hash::Hash + Copy,
{
    for (slot, value) in incoming {
        if value.value.trim().is_empty() {
            continue;
        }
        target.insert(*slot, value.clone());
    }
}
