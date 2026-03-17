/// テンプレートの1フィールド定義
#[derive(Debug, Clone)]
pub struct TemplateField {
    pub key: &'static str,
    pub prompt: &'static str,
    #[allow(dead_code)]
    pub required: bool,
    pub default: Option<&'static str>,
}

/// ドメイン別設計テンプレート
#[derive(Debug, Clone)]
pub struct DesignTemplate {
    /// テンプレート識別子（selector テストで使用）
    #[allow(dead_code)]
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub fields: &'static [TemplateField],
    /// BeamSearch幅への加算ボーナス
    pub beam_width_bonus: usize,
    /// 探索深度への加算ボーナス
    pub max_depth_bonus: usize,
}

// ─── テンプレート定義 ──────────────────────────────────────────────────────────

const EDITOR_FIELDS: &[TemplateField] = &[
    TemplateField {
        key: "target",
        prompt: "[必須] 主な編集対象 (例: テキストファイル, ソースコード, Markdown)",
        required: true,
        default: None,
    },
    TemplateField {
        key: "ui",
        prompt: "[必須] UIの形式 (tui / gui / web / cli)",
        required: true,
        default: Some("tui"),
    },
    TemplateField {
        key: "plugin",
        prompt: "[任意] プラグイン機能 (yes / no)",
        required: false,
        default: Some("no"),
    },
    TemplateField {
        key: "multi_buffer",
        prompt: "[任意] 複数バッファ対応 (yes / no)",
        required: false,
        default: Some("no"),
    },
    TemplateField {
        key: "lsp",
        prompt: "[任意] 言語サーバー(LSP)連携 (yes / no)",
        required: false,
        default: Some("no"),
    },
    TemplateField {
        key: "perf",
        prompt: "[任意] 起動・応答性能要件 (例: 起動 < 100ms、なければ空Enter)",
        required: false,
        default: None,
    },
];

pub const TEMPLATE_EDITOR: DesignTemplate = DesignTemplate {
    id: "editor",
    name: "エディタ系",
    description: "テキスト編集・コードエディタ・IDE 相当のシステム",
    fields: EDITOR_FIELDS,
    beam_width_bonus: 8,
    max_depth_bonus: 2,
};

const WEB_SERVICE_FIELDS: &[TemplateField] = &[
    TemplateField {
        key: "protocol",
        prompt: "[必須] 主要プロトコル (例: REST / GraphQL / gRPC / WebSocket)",
        required: true,
        default: Some("REST"),
    },
    TemplateField {
        key: "auth",
        prompt: "[必須] 認証方式 (例: JWT / OAuth2 / セッション / なし)",
        required: true,
        default: Some("JWT"),
    },
    TemplateField {
        key: "db",
        prompt: "[任意] 主なデータストア (例: PostgreSQL, Redis, MongoDB)",
        required: false,
        default: Some("PostgreSQL"),
    },
    TemplateField {
        key: "scale",
        prompt: "[任意] スケーリング方針 (例: 水平スケール, シングルインスタンス)",
        required: false,
        default: None,
    },
    TemplateField {
        key: "async",
        prompt: "[任意] 非同期処理・キュー (例: Kafka, RabbitMQ, なし)",
        required: false,
        default: Some("なし"),
    },
    TemplateField {
        key: "nonfunc",
        prompt: "[任意] 非機能要件 (例: レイテンシ < 200ms, 可用性 99.9%)",
        required: false,
        default: None,
    },
];

pub const TEMPLATE_WEB_SERVICE: DesignTemplate = DesignTemplate {
    id: "web_service",
    name: "Webサービス・API系",
    description: "REST API / マイクロサービス / Webバックエンド",
    fields: WEB_SERVICE_FIELDS,
    beam_width_bonus: 6,
    max_depth_bonus: 2,
};

const DATA_PIPELINE_FIELDS: &[TemplateField] = &[
    TemplateField {
        key: "source",
        prompt: "[必須] データソース (例: CSV, Kafka, RDB, S3)",
        required: true,
        default: None,
    },
    TemplateField {
        key: "sink",
        prompt: "[必須] 出力先 (例: DWH, Elasticsearch, RDB, S3)",
        required: true,
        default: None,
    },
    TemplateField {
        key: "mode",
        prompt: "[必須] 処理モード (batch / stream / hybrid)",
        required: true,
        default: Some("batch"),
    },
    TemplateField {
        key: "transform",
        prompt: "[任意] 主な変換処理 (例: 集計, フィルタ, JOIN, ML推論)",
        required: false,
        default: None,
    },
    TemplateField {
        key: "schedule",
        prompt: "[任意] 実行スケジュール (例: 毎時, 日次, リアルタイム)",
        required: false,
        default: Some("日次"),
    },
    TemplateField {
        key: "volume",
        prompt: "[任意] データ量 (例: 1GB/日, 100万件/時)",
        required: false,
        default: None,
    },
];

pub const TEMPLATE_DATA_PIPELINE: DesignTemplate = DesignTemplate {
    id: "data_pipeline",
    name: "データパイプライン・ETL系",
    description: "バッチ処理 / ストリーム処理 / データ変換",
    fields: DATA_PIPELINE_FIELDS,
    beam_width_bonus: 6,
    max_depth_bonus: 3,
};

const GENERIC_FIELDS: &[TemplateField] = &[
    TemplateField {
        key: "core_function",
        prompt: "[必須] 主要な機能・責務 (自由に記述)",
        required: true,
        default: None,
    },
    TemplateField {
        key: "users",
        prompt: "[任意] 主なユーザー・利用者 (例: エンジニア, 一般ユーザー, 内部システム)",
        required: false,
        default: None,
    },
    TemplateField {
        key: "scale",
        prompt: "[任意] 規模感 (例: 小チーム, 数百同時接続, 単一プロセス)",
        required: false,
        default: None,
    },
    TemplateField {
        key: "constraint",
        prompt: "[任意] 技術制約・非機能要件 (例: Rust製, オフライン動作)",
        required: false,
        default: None,
    },
];

pub const TEMPLATE_GENERIC: DesignTemplate = DesignTemplate {
    id: "generic",
    name: "汎用",
    description: "その他のシステム",
    fields: GENERIC_FIELDS,
    beam_width_bonus: 4,
    max_depth_bonus: 1,
};

// ─── 推論エンジンが動的に生成するテンプレート型 ──────────────────────────────

/// 推論で動的生成されるテンプレートフィールド（所有型）
#[derive(Debug, Clone)]
pub struct DynamicTemplateField {
    pub key: String,
    pub prompt: String,
    #[allow(dead_code)]
    pub required: bool,
    pub default: Option<String>,
}

/// 推論エンジンが動的に生成するテンプレート（所有型）
#[derive(Debug, Clone)]
pub struct DynamicTemplate {
    pub name: String,
    pub description: String,
    pub fields: Vec<DynamicTemplateField>,
    pub beam_width_bonus: usize,
    pub max_depth_bonus: usize,
}

impl DynamicTemplate {
    /// 静的 DesignTemplate を動的テンプレートに変換する
    #[allow(dead_code)]
    pub fn from_static(base: &'static DesignTemplate) -> Self {
        Self {
            name: base.name.to_string(),
            description: base.description.to_string(),
            fields: base.fields.iter().map(|f| DynamicTemplateField {
                key: f.key.to_string(),
                prompt: f.prompt.to_string(),
                required: f.required,
                default: f.default.map(|d| d.to_string()),
            }).collect(),
            beam_width_bonus: base.beam_width_bonus,
            max_depth_bonus: base.max_depth_bonus,
        }
    }
}

/// 全テンプレートのスライス（拡張時の列挙用）
#[allow(dead_code)]
pub const ALL_TEMPLATES: &[&DesignTemplate] = &[
    &TEMPLATE_EDITOR,
    &TEMPLATE_WEB_SERVICE,
    &TEMPLATE_DATA_PIPELINE,
    &TEMPLATE_GENERIC,
];
