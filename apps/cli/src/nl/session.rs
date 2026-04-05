use std::path::PathBuf;

use super::types::CommandPlan;

/// 直前の Coding dry-run step の状態を保持する。
/// `coding --apply` の follow-up apply promotion (R1–R6) に使用される。
#[derive(Clone, Debug, Default)]
pub struct LastCodingTransaction {
    pub target: PathBuf,
    pub request: Option<String>,
    pub safe: bool,
    /// dry-run で得られた canonical patch 数。0 の場合は apply no-op (R5)。
    pub patch_count: usize,
    /// apply 済みフラグ。true の場合は再 apply を no-op とする (R6)。
    pub applied: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConversationState {
    pub autonomous_label: Option<String>,
    pub last_target: Option<PathBuf>,
    pub last_node: Option<String>,
    pub last_plan: Option<CommandPlan>,
    pub last_viewer_session: Option<String>,
    pub last_analysis_summary: Option<String>,
    /// R3: 直前の Coding dry-run transaction。apply promotion に再利用される。
    pub last_coding_transaction: Option<LastCodingTransaction>,
}

impl PartialEq for LastCodingTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
            && self.request == other.request
            && self.safe == other.safe
            && self.patch_count == other.patch_count
            && self.applied == other.applied
    }
}

impl Eq for LastCodingTransaction {}

impl ConversationState {
    pub fn prompt_label(&self) -> Option<&str> {
        self.autonomous_label.as_deref().or_else(|| {
            self.last_node.as_deref().or_else(|| {
                self.last_target
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
            })
        })
    }

    /// R1 条件: 前回 checked && !applied の coding transaction が存在するか。
    pub fn has_pending_coding_transaction(&self) -> bool {
        self.last_coding_transaction
            .as_ref()
            .map(|tx| !tx.applied)
            .unwrap_or(false)
    }

    /// R1 拡張: apply 済み transaction でも same transaction への再 apply guard を維持する。
    pub fn has_reapply_guard(&self) -> bool {
        self.last_coding_transaction
            .as_ref()
            .map(|tx| tx.applied)
            .unwrap_or(false)
    }
}
