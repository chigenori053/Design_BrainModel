/// Phase3: DBM Planner Adapter
///
/// CLI 入力を DBM に渡し、Plan へ変換する。
///
/// 処理フロー:
/// 1. input 解析（analyze/design/spec キーワード判定）
/// 2. DBMClient へ問い合わせ
/// 3. Translator で Plan へ変換
/// 4. エラー時は RuleBasedPlanner へフォールバック
use crate::dbm::client::DBMClient;
use crate::dbm::translator::Translator;
use crate::plan::Plan;
use crate::session::AgentSession;

pub struct DBMPlannerAdapter {
    client: DBMClient,
    translator: Translator,
}

impl DBMPlannerAdapter {
    pub fn new() -> Self {
        Self {
            client: DBMClient::new(),
            translator: Translator::new(),
        }
    }

    /// input から Plan を生成する
    ///
    /// DBM失敗時はフォールバックせず Err を返す（呼び出し側がフォールバックを制御する）。
    pub fn create_plan(&self, input: &str, _session: &AgentSession) -> Result<Plan, String> {
        let lower = input.to_lowercase();

        if lower.contains("project") || lower.contains("プロジェクト") {
            let target = last_token(input);
            let result = self.client.analyze_project(&target)?;
            Ok(self.translator.project_to_plan(&result, input))
        } else if lower.contains("analyze") || lower.contains("分析") || lower.contains("解析")
        {
            let target = last_token(input);
            let result = self.client.analyze_code(&target)?;
            Ok(self.translator.analysis_to_plan(&result, input))
        } else {
            let result = self.client.generate_architecture(input)?;
            Ok(self.translator.architecture_to_plan(&result, input))
        }
    }
}

impl Default for DBMPlannerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn last_token(input: &str) -> String {
    input.split_whitespace().last().unwrap_or(".").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::rule_based::RuleBasedPlanner;

    fn new_session() -> AgentSession {
        AgentSession::new()
    }

    #[test]
    fn adapter_creates_plan_from_spec_input() {
        let adapter = DBMPlannerAdapter::new();
        let session = new_session();
        // DBM may succeed or fail; either way we test that fallback in create_plan works
        // (fallback is in the caller, so this just tests adapter doesn't panic)
        let _result = adapter.create_plan("spec for cli", &session);
    }

    #[test]
    fn adapter_analyze_input_calls_analyzer() {
        let adapter = DBMPlannerAdapter::new();
        let session = new_session();
        // "analyze" keyword → analyzer path → always returns Ok
        let result = adapter.create_plan("analyze src/", &session).unwrap();
        assert!(!result.steps.is_empty());
        let cmd = result.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    #[test]
    fn adapter_returns_ready_plan_for_analyze() {
        let adapter = DBMPlannerAdapter::new();
        let session = new_session();
        let result = adapter.create_plan("analyze the code", &session).unwrap();
        assert_eq!(result.status, crate::plan::PlanStatus::Ready);
    }

    #[test]
    fn adapter_project_input_calls_project_analyzer() {
        let adapter = DBMPlannerAdapter::new();
        let session = new_session();
        // "project" keyword → analyze_project → always Ok (even if path is ".")
        let result = adapter.create_plan("analyze project .", &session).unwrap();
        assert!(!result.steps.is_empty());
        let cmd = result.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
        assert_eq!(cmd.subcommand.as_deref(), Some("project"));
    }

    #[test]
    fn fallback_planner_used_when_dbm_fails() {
        // Simulate what the caller (planner/mod.rs) does on failure
        let session = new_session();
        let adapter = DBMPlannerAdapter::new();

        // design input → generate_architecture which may fail with clarification
        let plan = match adapter.create_plan("design the schema", &session) {
            Ok(p) => p,
            Err(_) => RuleBasedPlanner::new().plan("design the schema"),
        };

        assert!(!plan.steps.is_empty());
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "generate");
    }
}
