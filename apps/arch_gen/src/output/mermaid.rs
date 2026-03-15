/// Mermaid ノードIDとして安全な文字列に変換する。
/// スペース・ハイフン・ドット・スラッシュをアンダースコアに置換。
pub fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// コンポーネント名一覧と依存関係ペアから Mermaid `graph TD` 記法を生成する。
pub fn build_mermaid(
    component_names: &[String],
    dependency_pairs: &[(String, String)],
) -> String {
    let mut out = String::from("graph TD\n");

    if dependency_pairs.is_empty() {
        // 依存関係なし: ノードのみ列挙
        for name in component_names {
            let id = sanitize_id(name);
            out.push_str(&format!("  {id}[\"{name}\"]\n"));
        }
    } else {
        for (from, to) in dependency_pairs {
            let from_id = sanitize_id(from);
            let to_id = sanitize_id(to);
            out.push_str(&format!("  {from_id}[\"{from}\"] --> {to_id}[\"{to}\"]\n"));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_id_replaces_special_chars() {
        assert_eq!(sanitize_id("api-gateway"), "api_gateway");
        assert_eq!(sanitize_id("user service"), "user_service");
        assert_eq!(sanitize_id("order/service"), "order_service");
        assert_eq!(sanitize_id("event.bus"), "event_bus");
        assert_eq!(sanitize_id("valid_name"), "valid_name");
    }

    #[test]
    fn test_build_mermaid_with_dependencies() {
        let names = vec!["api_gateway".to_string(), "order_service".to_string()];
        let deps = vec![("api_gateway".to_string(), "order_service".to_string())];
        let out = build_mermaid(&names, &deps);
        assert!(out.starts_with("graph TD\n"));
        assert!(out.contains("api_gateway[\"api_gateway\"] --> order_service[\"order_service\"]"));
    }

    #[test]
    fn test_build_mermaid_no_dependencies() {
        let names = vec!["service_1".to_string(), "database_2".to_string()];
        let out = build_mermaid(&names, &[]);
        assert!(out.contains("service_1[\"service_1\"]"));
        assert!(out.contains("database_2[\"database_2\"]"));
        assert!(!out.contains("-->"));
    }

    #[test]
    fn test_build_mermaid_empty() {
        let out = build_mermaid(&[], &[]);
        assert_eq!(out, "graph TD\n");
    }

    #[test]
    fn test_sanitize_id_japanese_becomes_underscore() {
        // 日本語など非ASCII はアンダースコアに変換
        let result = sanitize_id("サービス");
        assert!(result.chars().all(|c| c == '_'));
    }
}
