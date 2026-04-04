#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalType {
    EliminateCycles,
    ReduceUnsafe,
    StabilizeViewerDispatch,
    ImproveTestPassRate,
    PrepareCommitAndPR,
}

pub fn detect_goal(input: &str) -> Option<GoalType> {
    let lower = input.to_lowercase();
    if (lower.contains("循環依存") || lower.contains("cycle"))
        && (lower.contains("ゼロ") || lower.contains("なくして") || lower.contains("zero"))
    {
        return Some(GoalType::EliminateCycles);
    }
    if lower.contains("unsafe") && (lower.contains("減ら") || lower.contains("improve")) {
        return Some(GoalType::ReduceUnsafe);
    }
    if (lower.contains("viewer dispatch") || lower.contains("dispatch") || lower.contains("viewer"))
        && (lower.contains("安定") || lower.contains("stabilize"))
    {
        return Some(GoalType::StabilizeViewerDispatch);
    }
    if (lower.contains("test") || lower.contains("pass rate"))
        && (lower.contains("改善") || lower.contains("improve"))
    {
        return Some(GoalType::ImproveTestPassRate);
    }
    if lower.contains("commit")
        || lower.contains("pr作")
        || lower.contains("prまで")
        || lower.contains("pull request")
        || lower.contains("コミット")
        || lower
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '。' | '、' | ';' | ':'))
            .any(|token| token == "pr")
    {
        return Some(GoalType::PrepareCommitAndPR);
    }
    None
}

pub fn goal_label(goal: GoalType) -> &'static str {
    match goal {
        GoalType::EliminateCycles => "cycles",
        GoalType::ReduceUnsafe => "unsafe",
        GoalType::StabilizeViewerDispatch => "viewer",
        GoalType::ImproveTestPassRate => "tests",
        GoalType::PrepareCommitAndPR => "git",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cycle_goal() {
        assert_eq!(
            detect_goal("この循環依存をゼロにして"),
            Some(GoalType::EliminateCycles)
        );
    }

    #[test]
    fn detects_unsafe_goal() {
        assert_eq!(
            detect_goal("unsafe を減らして"),
            Some(GoalType::ReduceUnsafe)
        );
    }
}
