use std::io::{BufRead, Write};

use super::templates::{DesignTemplate, TemplateField};

/// テンプレートへの記入結果
#[derive(Debug, Clone)]
pub struct FilledTemplate {
    /// (field.key, ユーザー入力または default) のペア
    pub answers: Vec<(String, String)>,
}

/// テンプレート補強後のパラメータ
#[derive(Debug, Clone)]
pub struct EnrichedParams {
    /// 元テキスト + 記入内容を結合した要件文
    pub enriched_text: String,
    pub beam_width_bonus: usize,
    pub max_depth_bonus: usize,
}

impl EnrichedParams {
    /// テンプレートを使わないパススルー（非対話 / --no-template 時）
    pub fn passthrough(raw_text: &str) -> Self {
        Self {
            enriched_text: raw_text.to_string(),
            beam_width_bonus: 0,
            max_depth_bonus: 0,
        }
    }
}

/// テンプレートをターミナルに表示してユーザー入力を収集する。
pub fn prompt_and_fill(
    template: &DesignTemplate,
    stdin: &mut impl BufRead,
    stdout: &mut impl Write,
) -> Result<FilledTemplate, String> {
    writeln!(stdout).ok();
    writeln!(stdout, "┌─ 設計テンプレート: {} ─────────────────────────", template.name).ok();
    writeln!(stdout, "│ {}", template.description).ok();
    writeln!(stdout, "│ ※ 空Enter でデフォルト値を使用します").ok();
    writeln!(stdout, "└───────────────────────────────────────────────────").ok();
    writeln!(stdout).ok();
    stdout.flush().ok();

    let mut answers = Vec::new();

    for field in template.fields {
        let answer = prompt_field(field, stdin, stdout)?;
        if !answer.is_empty() {
            answers.push((field.key.to_string(), answer));
        }
    }

    writeln!(stdout).ok();
    writeln!(stdout, "テンプレート記入完了。探索を開始します...").ok();
    writeln!(stdout).ok();
    stdout.flush().ok();

    Ok(FilledTemplate { answers })
}

fn prompt_field(
    field: &TemplateField,
    stdin: &mut impl BufRead,
    stdout: &mut impl Write,
) -> Result<String, String> {
    // プロンプト表示
    if let Some(default) = field.default {
        write!(stdout, "  {} [{}]: ", field.prompt, default).ok();
    } else {
        write!(stdout, "  {}: ", field.prompt).ok();
    }
    stdout.flush().ok();

    let mut line = String::new();
    stdin
        .read_line(&mut line)
        .map_err(|e| format!("failed to read input: {e}"))?;
    let trimmed = line.trim().to_string();

    if trimmed.is_empty() {
        // デフォルト値を使用
        Ok(field.default.unwrap_or("").to_string())
    } else {
        Ok(trimmed)
    }
}

/// 記入済みテンプレートから `EnrichedParams` を構築する。
pub fn enrich(
    raw_text: &str,
    filled: &FilledTemplate,
    template: &DesignTemplate,
) -> EnrichedParams {
    let enriched_text = build_enriched_text(raw_text, filled, template);
    EnrichedParams {
        enriched_text,
        beam_width_bonus: template.beam_width_bonus,
        max_depth_bonus: template.max_depth_bonus,
    }
}

/// 元テキスト + テンプレート回答 → 要件文字列を組み立てる。
fn build_enriched_text(
    raw_text: &str,
    filled: &FilledTemplate,
    template: &DesignTemplate,
) -> String {
    if filled.answers.is_empty() {
        return raw_text.to_string();
    }

    let mut parts: Vec<String> = vec![raw_text.to_string()];

    // テンプレート種別のヒント
    parts.push(format!("設計ドメイン: {}", template.name));

    // 各フィールドの回答を「key: value」形式で追加
    for (key, value) in &filled.answers {
        if !value.is_empty() && value != "なし" {
            // field の prompt からラベルを取得
            let label = template
                .fields
                .iter()
                .find(|f| f.key == key.as_str())
                .map(|f| strip_prompt_prefix(f.prompt))
                .unwrap_or(key.as_str());
            parts.push(format!("{label}: {value}"));
        }
    }

    parts.join("\n")
}

/// "[必須] 主な編集対象 (例: ...)" → "主な編集対象" を抽出する。
fn strip_prompt_prefix(prompt: &str) -> &str {
    let s = prompt
        .trim_start_matches("[必須]")
        .trim_start_matches("[任意]")
        .trim();
    // "(例: ...)" の手前まで
    if let Some(pos) = s.find('(') {
        s[..pos].trim()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::templates::TEMPLATE_EDITOR;

    fn make_filled(pairs: &[(&str, &str)]) -> FilledTemplate {
        FilledTemplate {
            answers: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn test_passthrough_has_zero_bonus() {
        let p = EnrichedParams::passthrough("test input");
        assert_eq!(p.enriched_text, "test input");
        assert_eq!(p.beam_width_bonus, 0);
        assert_eq!(p.max_depth_bonus, 0);
    }

    #[test]
    fn test_enrich_applies_template_bonus() {
        let filled = make_filled(&[("ui", "tui"), ("plugin", "yes")]);
        let params = enrich("NeoVim風エディタ", &filled, &TEMPLATE_EDITOR);
        assert_eq!(params.beam_width_bonus, TEMPLATE_EDITOR.beam_width_bonus);
        assert_eq!(params.max_depth_bonus, TEMPLATE_EDITOR.max_depth_bonus);
    }

    #[test]
    fn test_enriched_text_contains_original() {
        let filled = make_filled(&[("ui", "tui")]);
        let params = enrich("NeoVim風エディタ", &filled, &TEMPLATE_EDITOR);
        assert!(params.enriched_text.contains("NeoVim風エディタ"));
    }

    #[test]
    fn test_enriched_text_contains_answers() {
        let filled = make_filled(&[("ui", "tui"), ("plugin", "yes")]);
        let params = enrich("エディタ", &filled, &TEMPLATE_EDITOR);
        assert!(params.enriched_text.contains("tui"));
        assert!(params.enriched_text.contains("yes"));
    }

    #[test]
    fn test_enriched_text_skips_empty_answers() {
        let filled = make_filled(&[("ui", ""), ("target", "ソースコード")]);
        let params = enrich("エディタ", &filled, &TEMPLATE_EDITOR);
        assert!(params.enriched_text.contains("ソースコード"));
        // empty answer は含まれない
        let lines: Vec<&str> = params.enriched_text.lines().collect();
        assert!(!lines.iter().any(|l| l.trim().is_empty() && l.contains(':')));
    }

    #[test]
    fn test_prompt_and_fill_with_all_defaults() {
        use std::io::Cursor;
        // 全フィールドで空Enterを渡す
        let input = "\n\n\n\n\n\n";
        let mut stdin = Cursor::new(input);
        let mut stdout = Vec::<u8>::new();

        let filled = prompt_and_fill(&TEMPLATE_EDITOR, &mut stdin, &mut stdout).unwrap();
        // デフォルト値のあるフィールドはanswerに含まれる
        let keys: Vec<&str> = filled.answers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"ui"));      // default = "tui"
        assert!(keys.contains(&"plugin"));  // default = "no"
    }

    #[test]
    fn test_prompt_and_fill_with_custom_input() {
        use std::io::Cursor;
        let input = "ソースコード\ngui\nyes\nyes\nyes\n起動 < 50ms\n";
        let mut stdin = Cursor::new(input);
        let mut stdout = Vec::<u8>::new();

        let filled = prompt_and_fill(&TEMPLATE_EDITOR, &mut stdin, &mut stdout).unwrap();
        let map: std::collections::HashMap<_, _> =
            filled.answers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(map["target"], "ソースコード");
        assert_eq!(map["ui"], "gui");
        assert_eq!(map["lsp"], "yes");
    }

    #[test]
    fn test_strip_prompt_prefix() {
        assert_eq!(strip_prompt_prefix("[必須] 主な編集対象 (例: foo)"), "主な編集対象");
        assert_eq!(strip_prompt_prefix("[任意] プラグイン機能 (yes/no)"), "プラグイン機能");
        assert_eq!(strip_prompt_prefix("[必須] UIの形式 (tui / gui)"), "UIの形式");
    }
}
