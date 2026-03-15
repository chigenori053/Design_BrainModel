/// コンポーネント名一覧と依存関係ペアから PlantUML `@startuml` 記法を生成する。
pub fn build_plantuml(component_names: &[String], dependency_pairs: &[(String, String)]) -> String {
    let mut out = String::from("@startuml\n");

    for name in component_names {
        out.push_str(&format!("component [{}]\n", name));
    }

    if !dependency_pairs.is_empty() {
        out.push('\n');
        for (from, to) in dependency_pairs {
            out.push_str(&format!("[{}] --> [{}]\n", from, to));
        }
    }

    out.push_str("@enduml\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_plantuml_with_dependencies() {
        let names = vec!["api_gateway".to_string(), "order_service".to_string()];
        let deps = vec![("api_gateway".to_string(), "order_service".to_string())];
        let out = build_plantuml(&names, &deps);
        assert!(out.starts_with("@startuml\n"));
        assert!(out.ends_with("@enduml\n"));
        assert!(out.contains("component [api_gateway]"));
        assert!(out.contains("component [order_service]"));
        assert!(out.contains("[api_gateway] --> [order_service]"));
    }

    #[test]
    fn test_build_plantuml_no_dependencies() {
        let names = vec!["service_1".to_string(), "database_2".to_string()];
        let out = build_plantuml(&names, &[]);
        assert!(out.contains("component [service_1]"));
        assert!(out.contains("component [database_2]"));
        assert!(!out.contains("-->"));
    }

    #[test]
    fn test_build_plantuml_empty() {
        let out = build_plantuml(&[], &[]);
        assert_eq!(out, "@startuml\n@enduml\n");
    }

    #[test]
    fn test_build_plantuml_multiple_deps() {
        let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let deps = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "c".to_string()),
        ];
        let out = build_plantuml(&names, &deps);
        assert!(out.contains("[a] --> [b]"));
        assert!(out.contains("[b] --> [c]"));
    }
}
