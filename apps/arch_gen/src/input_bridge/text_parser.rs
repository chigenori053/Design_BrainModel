use std::fs;

/// `@path` 形式のファイル参照を解決し、要件テキストを返す。
pub fn resolve_requirement(requirement: &str) -> Result<String, String> {
    if let Some(path) = requirement.strip_prefix('@') {
        fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .map_err(|e| format!("failed to read requirement file '{path}': {e}"))
    } else {
        let text = requirement.trim().to_string();
        if text.is_empty() {
            return Err("requirement text must not be empty".to_string());
        }
        Ok(text)
    }
}
