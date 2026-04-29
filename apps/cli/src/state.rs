/// Phase0: CLI状態機械の型定義
///
/// CLIはstatelessではなく状態を持つシステムとして扱う。
/// Phase2以降でPlanner/Executorと接続するための拡張ポイントを備える。
/// CLIセッションの状態遷移
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum State {
    /// 初期状態：入力待ち
    #[default]
    Idle,
    /// 計画中：Plannerが処理中
    Planning,
    /// 実行待機：Planが確定し実行可能な状態
    Ready,
    /// 実行中：Executorが処理中
    Running,
    /// 完了：実行が正常に終了
    Completed,
    /// エラー：回復可能なエラー状態
    Error,
    SpecReceived,
    DesignDeltaReady,
    MutationPlanned,
    MutationCandidatesReady,
    MutationRankingReady,
    BestMutationSelected,
    RationalityScored,
    PatchPlanReady,
    TestPlanReady,
    Repairing,
    CommitReady,
    /// Executor is blocked waiting for a Control Event response.
    Blocked,
    Failed,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Planning => "planning",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Error => "error",
            Self::SpecReceived => "spec_received",
            Self::DesignDeltaReady => "design_delta_ready",
            Self::MutationPlanned => "mutation_planned",
            Self::MutationCandidatesReady => "mutation_candidates_ready",
            Self::MutationRankingReady => "mutation_ranking_ready",
            Self::BestMutationSelected => "best_mutation_selected",
            Self::RationalityScored => "rationality_scored",
            Self::PatchPlanReady => "patch_plan_ready",
            Self::TestPlanReady => "test_plan_ready",
            Self::Repairing => "repairing",
            Self::CommitReady => "commit_ready",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }
}

/// CLIの実行モード（Phase2で拡張）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// 計画モード（Phase0のデフォルト）
    #[default]
    Plan,
}

/// セッションに付随するコンテキスト情報
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// 入力履歴
    pub history: Vec<String>,
    /// 最後に操作したファイル/ディレクトリパス
    pub last_path: Option<String>,
    /// 最後に実行したコマンド名
    pub last_command: Option<String>,
}

impl Context {
    pub fn push(&mut self, input: impl Into<String>) {
        self.history.push(input.into());
    }

    /// 最後に使ったパスを保存する
    pub fn set_last_path(&mut self, path: &str) {
        if !path.is_empty() && path != "." {
            self.last_path = Some(path.to_string());
        }
    }

    /// 最後に使ったパスを返す（なければ "."）
    pub fn last_path_or_default(&self) -> &str {
        self.last_path.as_deref().unwrap_or(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_idle() {
        assert_eq!(State::default(), State::Idle);
    }

    #[test]
    fn default_mode_is_plan() {
        assert_eq!(Mode::default(), Mode::Plan);
    }

    #[test]
    fn state_as_str() {
        assert_eq!(State::Idle.as_str(), "idle");
        assert_eq!(State::Planning.as_str(), "planning");
        assert_eq!(State::Ready.as_str(), "ready");
        assert_eq!(State::Running.as_str(), "running");
        assert_eq!(State::Completed.as_str(), "completed");
        assert_eq!(State::Error.as_str(), "error");
        assert_eq!(State::SpecReceived.as_str(), "spec_received");
        assert_eq!(State::DesignDeltaReady.as_str(), "design_delta_ready");
        assert_eq!(State::MutationPlanned.as_str(), "mutation_planned");
        assert_eq!(
            State::MutationCandidatesReady.as_str(),
            "mutation_candidates_ready"
        );
        assert_eq!(
            State::MutationRankingReady.as_str(),
            "mutation_ranking_ready"
        );
        assert_eq!(
            State::BestMutationSelected.as_str(),
            "best_mutation_selected"
        );
        assert_eq!(State::RationalityScored.as_str(), "rationality_scored");
        assert_eq!(State::PatchPlanReady.as_str(), "patch_plan_ready");
        assert_eq!(State::TestPlanReady.as_str(), "test_plan_ready");
        assert_eq!(State::Repairing.as_str(), "repairing");
        assert_eq!(State::CommitReady.as_str(), "commit_ready");
    }

    #[test]
    fn context_push_appends_history() {
        let mut ctx = Context::default();
        ctx.push("input1");
        ctx.push("input2");
        assert_eq!(ctx.history, vec!["input1", "input2"]);
    }

    #[test]
    fn context_set_last_path_stores_path() {
        let mut ctx = Context::default();
        ctx.set_last_path("src/main.rs");
        assert_eq!(ctx.last_path, Some("src/main.rs".to_string()));
    }

    #[test]
    fn context_set_last_path_ignores_dot() {
        let mut ctx = Context::default();
        ctx.set_last_path(".");
        assert_eq!(ctx.last_path, None);
    }

    #[test]
    fn context_last_path_or_default_returns_dot_when_none() {
        let ctx = Context::default();
        assert_eq!(ctx.last_path_or_default(), ".");
    }

    #[test]
    fn context_last_path_or_default_returns_stored_path() {
        let mut ctx = Context::default();
        ctx.set_last_path("src/lib.rs");
        assert_eq!(ctx.last_path_or_default(), "src/lib.rs");
    }
}
