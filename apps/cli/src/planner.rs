/// Phase3: Planner モジュール
///
/// PlannerMode に応じて RuleBased / DBM の2種類の Planner を切り替える。
/// Strategy Pattern による差し替え可能設計。
///
/// フォールバック戦略:
/// DBM失敗 → RuleBased（常に成功）
pub mod dbm_adapter;
pub mod rule_based;

pub use dbm_adapter::DBMPlannerAdapter;
pub use rule_based::RuleBasedPlanner;

use crate::plan::Plan;
use crate::session::AgentSession;

/// Planner の動作モード
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlannerMode {
    /// ルールベース（キーワードマッチング、Phase2）
    #[default]
    RuleBased,
    /// DBM推論ベース（CoreRuntime経由、Phase3）
    DBM,
}

impl PlannerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RuleBased => "rule_based",
            Self::DBM => "dbm",
        }
    }

    /// 文字列から PlannerMode をパースする
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "rule" | "rule_based" => Some(Self::RuleBased),
            "dbm" => Some(Self::DBM),
            _ => None,
        }
    }
}

/// mode に応じた Planner で Plan を生成する
///
/// DBM が失敗した場合は RuleBased にフォールバックする。
pub fn create_plan(input: &str, session: &AgentSession, mode: PlannerMode) -> Plan {
    let last_path = session.context.last_path.as_deref();
    match mode {
        PlannerMode::RuleBased => RuleBasedPlanner::new().plan(input, last_path),
        PlannerMode::DBM => {
            let adapter = DBMPlannerAdapter::new();
            match adapter.create_plan(input, session) {
                Ok(plan) => plan,
                Err(_) => {
                    // DBM失敗 → RuleBasedにフォールバック
                    RuleBasedPlanner::new().plan(input, last_path)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_session() -> AgentSession {
        AgentSession::new()
    }

    #[test]
    fn default_mode_is_rule_based() {
        assert_eq!(PlannerMode::default(), PlannerMode::RuleBased);
    }

    #[test]
    fn mode_as_str() {
        assert_eq!(PlannerMode::RuleBased.as_str(), "rule_based");
        assert_eq!(PlannerMode::DBM.as_str(), "dbm");
    }

    #[test]
    fn mode_from_str_rule() {
        assert_eq!(PlannerMode::parse("rule"), Some(PlannerMode::RuleBased));
        assert_eq!(
            PlannerMode::parse("rule_based"),
            Some(PlannerMode::RuleBased)
        );
    }

    #[test]
    fn mode_from_str_dbm() {
        assert_eq!(PlannerMode::parse("dbm"), Some(PlannerMode::DBM));
    }

    #[test]
    fn mode_from_str_unknown_returns_none() {
        assert_eq!(PlannerMode::parse("unknown"), None);
    }

    #[test]
    fn create_plan_rule_based_returns_plan() {
        let session = new_session();
        let plan = create_plan("design the api", &session, PlannerMode::RuleBased);
        assert!(!plan.steps.is_empty());
        assert_eq!(plan.status, crate::plan::PlanStatus::Ready);
    }

    #[test]
    fn create_plan_dbm_returns_plan_with_fallback() {
        let session = new_session();
        // DBM may fail → falls back to rule-based; either way a plan is returned
        let plan = create_plan("spec for cli", &session, PlannerMode::DBM);
        assert!(!plan.steps.is_empty());
    }

    #[test]
    fn create_plan_rule_based_analyze_keyword() {
        let session = new_session();
        let plan = create_plan("analyze the code", &session, PlannerMode::RuleBased);
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }

    #[test]
    fn create_plan_dbm_analyze_falls_back_or_succeeds() {
        let session = new_session();
        // "analyze src/" → DBM adapter uses filesystem analyzer → always succeeds
        let plan = create_plan("analyze src/", &session, PlannerMode::DBM);
        assert!(!plan.steps.is_empty());
        let cmd = plan.steps[0].command.as_ref().unwrap();
        assert_eq!(cmd.name, "analyze");
    }
}
