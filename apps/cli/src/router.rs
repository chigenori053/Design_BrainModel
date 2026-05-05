use crate::core::CoreRequestKind;

/// Phase2.1: Slash Command Mapping
///
/// 独立したコマンドを他のコマンドのサブコマンドへ透過的にマッピングする。
/// 例：`/structure view .` → `/design structure view .`
pub fn map_slash_command(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("/structure") {
        trimmed.replacen("/structure", "/design structure", 1)
    } else {
        trimmed.to_string()
    }
}

/// Phase1 Router with Reason
///
/// ルーティングルール（DBM-CLI-CORE-UNIFICATION-PHASE1 §5.1 / DBM-CLI-OBSERVABILITY-PHASE1_5-EXT）：
/// - `/...` → SlashCommand (starts_with('/'))
/// - `apply` → Apply (exact_match('apply'/'y'/'yes'))
/// - contextあり → Followup (has_previous_context)
/// - その他 → NaturalLanguage (default)
pub fn route(input: &str, has_context: bool) -> (CoreRequestKind, &'static str) {
    let trimmed = input.trim();
    if trimmed.starts_with('/') {
        (CoreRequestKind::SlashCommand, "starts_with('/')")
    } else if trimmed.eq_ignore_ascii_case("apply")
        || trimmed.eq_ignore_ascii_case("y")
        || trimmed.eq_ignore_ascii_case("yes")
    {
        (CoreRequestKind::Apply, "exact_match('apply'/'y'/'yes')")
    } else if has_context {
        (CoreRequestKind::Followup, "has_previous_context")
    } else {
        (CoreRequestKind::NaturalLanguage, "default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing() {
        assert_eq!(route("/help", false).0, CoreRequestKind::SlashCommand);
        assert_eq!(route("apply", false).0, CoreRequestKind::Apply);
        assert_eq!(route("y", false).0, CoreRequestKind::Apply);
        assert_eq!(route("hello", false).0, CoreRequestKind::NaturalLanguage);
        assert_eq!(route("hello", true).0, CoreRequestKind::Followup);
    }
}
