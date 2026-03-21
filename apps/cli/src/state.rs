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
}

impl Context {
    pub fn push(&mut self, input: impl Into<String>) {
        self.history.push(input.into());
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
    }

    #[test]
    fn context_push_appends_history() {
        let mut ctx = Context::default();
        ctx.push("input1");
        ctx.push("input2");
        assert_eq!(ctx.history, vec!["input1", "input2"]);
    }
}
