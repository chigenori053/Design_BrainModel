use super::templates::{
    DesignTemplate, TEMPLATE_DATA_PIPELINE, TEMPLATE_EDITOR, TEMPLATE_GENERIC,
    TEMPLATE_WEB_SERVICE,
};

/// キーワードマッチでドメインを推定し、適切なテンプレートを返す。
pub fn select_template(text: &str) -> &'static DesignTemplate {
    let lower = text.to_lowercase();

    if is_editor_domain(&lower) {
        &TEMPLATE_EDITOR
    } else if is_data_pipeline_domain(&lower) {
        &TEMPLATE_DATA_PIPELINE
    } else if is_web_service_domain(&lower) {
        &TEMPLATE_WEB_SERVICE
    } else {
        &TEMPLATE_GENERIC
    }
}

fn is_editor_domain(lower: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "editor",
        "エディタ",
        "エディター",
        "vim",
        "neovim",
        "emacs",
        "テキスト編集",
        "text editor",
        "code editor",
        "ide",
        "編集",
        "入力補完",
        "シンタックスハイライト",
        "syntax",
        "lsp",
        "バッファ",
        "buffer",
    ];
    KEYWORDS.iter().any(|kw| lower.contains(kw))
}

fn is_data_pipeline_domain(lower: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "pipeline",
        "パイプライン",
        "etl",
        "バッチ",
        "batch",
        "ストリーム",
        "stream",
        "データ処理",
        "data processing",
        "インジェスト",
        "ingest",
        "dwh",
        "データウェアハウス",
        "spark",
        "kafka",
        "flink",
        "集計",
        "変換",
    ];
    KEYWORDS.iter().any(|kw| lower.contains(kw))
}

fn is_web_service_domain(lower: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "api",
        "rest",
        "graphql",
        "grpc",
        "http",
        "server",
        "サーバー",
        "サービス",
        "service",
        "web",
        "ウェブ",
        "マイクロサービス",
        "microservice",
        "バックエンド",
        "backend",
        "認証",
        "auth",
        "endpoint",
        "エンドポイント",
    ];
    KEYWORDS.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_editor_by_neovim() {
        assert_eq!(select_template("NeoVim風のEditorを作りたい").id, "editor");
    }

    #[test]
    fn test_select_editor_by_japanese() {
        assert_eq!(select_template("テキスト編集ツールを設計したい").id, "editor");
    }

    #[test]
    fn test_select_web_service() {
        assert_eq!(select_template("REST APIサーバーを設計する").id, "web_service");
    }

    #[test]
    fn test_select_data_pipeline() {
        assert_eq!(select_template("Kafkaを使ったストリームパイプラインを構築したい").id, "data_pipeline");
    }

    #[test]
    fn test_select_generic_fallback() {
        assert_eq!(select_template("社内ツールを作りたい").id, "generic");
    }

    #[test]
    fn test_editor_takes_priority_over_web() {
        // "editor" と "api" 両方含む場合 → editor が優先
        assert_eq!(select_template("editor with LSP and REST api support").id, "editor");
    }

    #[test]
    fn test_data_pipeline_before_web() {
        // pipeline は web より先に評価される
        assert_eq!(select_template("Kafka pipeline with REST ingest API").id, "data_pipeline");
    }
}
