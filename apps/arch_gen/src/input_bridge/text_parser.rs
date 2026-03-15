use std::fs;

/// Phase9パイプラインへ渡す生成リクエストの全パラメータ。
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    /// 解決済みの要件テキスト（`@path` 展開後）
    pub raw_text: String,
    pub beam_width: usize,
    pub max_depth: usize,
    pub candidates: usize,
    pub no_code: bool,
    pub verbose: bool,
}

impl GenerateRequest {
    pub fn new(
        raw_text: String,
        beam_width: usize,
        max_depth: usize,
        candidates: usize,
        no_code: bool,
        verbose: bool,
    ) -> Self {
        Self {
            raw_text,
            beam_width,
            max_depth,
            candidates,
            no_code,
            verbose,
        }
    }

    /// Phase9パイプライン（`RuntimeHybridVm::set_input_text`）へ渡すテキストを返す。
    pub fn input_text(&self) -> &str {
        &self.raw_text
    }
}

/// `@path` 形式を解決して要件テキストを返す。
/// 通常テキストはそのまま返す（空文字はエラー）。
pub fn resolve_requirement(requirement: &str) -> Result<String, String> {
    if let Some(path) = requirement.strip_prefix('@') {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("failed to read requirement file '{path}': {e}"))?;
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return Err(format!("requirement file '{path}' is empty"));
        }
        Ok(trimmed)
    } else {
        let text = requirement.trim().to_string();
        if text.is_empty() {
            return Err("requirement text must not be empty".to_string());
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_resolve_plain_text() {
        let result = resolve_requirement("ECサイトを設計する");
        assert_eq!(result.unwrap(), "ECサイトを設計する");
    }

    #[test]
    fn test_resolve_trims_whitespace() {
        let result = resolve_requirement("  Webアプリ  ");
        assert_eq!(result.unwrap(), "Webアプリ");
    }

    #[test]
    fn test_resolve_empty_is_error() {
        let result = resolve_requirement("   ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn test_resolve_file_reference() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "マイクロサービスを設計する").unwrap();
        let path = format!("@{}", tmp.path().display());
        let result = resolve_requirement(&path);
        assert_eq!(result.unwrap(), "マイクロサービスを設計する");
    }

    #[test]
    fn test_resolve_missing_file_is_error() {
        let result = resolve_requirement("@/nonexistent/file.txt");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("failed to read requirement file")
        );
    }

    #[test]
    fn test_generate_request_input_text() {
        let req = GenerateRequest::new("ECサイトを設計する".to_string(), 10, 5, 3, false, false);
        assert_eq!(req.input_text(), "ECサイトを設計する");
        assert_eq!(req.beam_width, 10);
        assert_eq!(req.max_depth, 5);
        assert_eq!(req.candidates, 3);
    }
}
