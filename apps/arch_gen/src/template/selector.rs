use design_reasoning::{
    IssueType, ReasoningAxis, StructuredReasoningEngine, StructuredReasoningInput,
};

use super::templates::{
    DesignTemplate, DynamicTemplate, DynamicTemplateField, TEMPLATE_DATA_PIPELINE, TEMPLATE_EDITOR,
    TEMPLATE_GENERIC, TEMPLATE_WEB_SERVICE,
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

// ─── 推論ドリブンのテンプレート生成 ──────────────────────────────────────────

/// `StructuredReasoningEngine` で入力テキストを分析し、
/// ドメインベースのフィールドに加えて推論で検出した不足軸を追加した
/// `DynamicTemplate` を返す。
pub fn infer_template(text: &str) -> DynamicTemplate {
    // 1. 既存キーワードマッチでベースドメインを取得
    let base = select_template(text);

    // 2. SRT 推論入力を構築（スコアは中立値で初期化）
    let srt_input = StructuredReasoningInput {
        source_text: text.to_string(),
        selected_objective: text.lines().next().map(|l| l.trim().to_string()),
        requirement_count: text.split_whitespace().count().saturating_div(5).max(1),
        stability_score: 0.5,
        ambiguity_score: 0.3,
        evidence_spans: vec![text.to_string()],
    };

    let engine = StructuredReasoningEngine::default();
    let srt = engine.build_srt(&srt_input);

    // 3. ベーステンプレートのフィールドを動的型に変換
    let mut fields: Vec<DynamicTemplateField> = base
        .fields
        .iter()
        .map(|f| DynamicTemplateField {
            key: f.key.to_string(),
            prompt: f.prompt.to_string(),
            required: f.required,
            default: f.default.map(|d| d.to_string()),
        })
        .collect();

    // 4. 重要度の高い SRT issue をテンプレートフィールドとして追加
    //    すでに同じキーのフィールドがある場合は追加しない
    for issue in srt.issues.iter().filter(|i| i.severity >= 0.45).take(3) {
        let key = axis_to_field_key(issue.axis);
        if fields.iter().any(|f| f.key == key) {
            continue;
        }
        fields.push(DynamicTemplateField {
            key: key.to_string(),
            prompt: axis_to_prompt_text(issue.axis, issue.issue_type),
            required: issue.severity >= 0.7,
            default: None,
        });
    }

    // 5. 推論状態の説明をテンプレート description に付加
    let readiness = match srt.overall_state {
        design_reasoning::OverallState::Ready => "要件は整理されています",
        design_reasoning::OverallState::PartialReady => "一部の要件に不足があります",
        design_reasoning::OverallState::Insufficient => "要件の基礎要素が不足しています",
    };

    DynamicTemplate {
        name: base.name.to_string(),
        description: format!("{}  [{readiness}]", base.description),
        fields,
        beam_width_bonus: base.beam_width_bonus,
        max_depth_bonus: base.max_depth_bonus,
    }
}

fn axis_to_field_key(axis: ReasoningAxis) -> &'static str {
    match axis {
        ReasoningAxis::TargetUser => "target_user",
        ReasoningAxis::SuccessMetric => "success_metric",
        ReasoningAxis::ScopeBoundary => "scope_boundary",
        ReasoningAxis::Constraint => "srt_constraint",
        ReasoningAxis::TechnicalStrategy => "tech_strategy",
        ReasoningAxis::RiskAssumption => "risk_assumption",
        ReasoningAxis::ProblemDefinition => "problem_def",
        ReasoningAxis::ValueProposition => "value_prop",
    }
}

fn axis_to_prompt_text(axis: ReasoningAxis, issue_type: IssueType) -> String {
    let prefix = match issue_type {
        IssueType::Missing => "[必須] ",
        IssueType::Ambiguous => "[要明確化] ",
        IssueType::Weak => "[補強推奨] ",
        IssueType::Minor => "[任意] ",
    };
    let question = match axis {
        ReasoningAxis::TargetUser =>
            "対象ユーザーの属性と利用場面 (例: 個人開発者・チーム開発)",
        ReasoningAxis::SuccessMetric =>
            "成功条件・達成基準 (例: 起動 < 100ms、処理 < 1s)",
        ReasoningAxis::ScopeBoundary =>
            "スコープ境界（含む / 含まない機能）",
        ReasoningAxis::Constraint =>
            "技術・性能・予算制約 (例: オフライン必須、Rust製)",
        ReasoningAxis::TechnicalStrategy =>
            "技術方針・優先するアーキテクチャ特性 (例: モジュール性重視)",
        ReasoningAxis::RiskAssumption =>
            "主要な不確実性・外部依存 (例: OS依存、外部API)",
        ReasoningAxis::ProblemDefinition =>
            "解決する課題の具体的な説明",
        ReasoningAxis::ValueProposition =>
            "既存ソリューションとの差別化ポイント",
    };
    format!("{prefix}{question}")
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
