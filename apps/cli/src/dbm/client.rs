/// Phase3: DBM Client
///
/// DesignBrainModel Core とのインターフェース。
/// - analyze_code   : ファイルシステム解析
/// - generate_architecture : CoreRuntime 経由のアーキテクチャ生成
/// - search_design  : 設計空間検索
use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use memory_engine::InMemoryEngine;
use runtime_core::{ChatContext, CoreRuntime, RuntimeExecutionResult};
use std::sync::Arc;

use crate::dbm::analyzer;

pub use crate::dbm::analyzer::{AnalysisResult, ModuleInfo};

/// DBM接続クライアント
pub struct DBMClient {
    runtime: CoreRuntime,
}

/// アーキテクチャ生成結果
#[derive(Clone, Debug)]
pub struct ArchitectureResult {
    pub intent: String,
    pub components: Vec<String>,
    pub layers: Vec<String>,
    pub actions: Vec<ArchitectureAction>,
}

/// アーキテクチャアクション（Translator が CLI コマンドへ変換する）
#[derive(Clone, Debug)]
pub struct ArchitectureAction {
    pub action_type: String,
    pub target: String,
}

/// 設計検索制約
#[derive(Clone, Debug, Default)]
pub struct Constraints {
    pub keywords: Vec<String>,
    pub requirements: Vec<String>,
}

impl Constraints {
    pub fn from_input(input: &str) -> Self {
        Self {
            keywords: input.split_whitespace().map(String::from).collect(),
            requirements: vec![],
        }
    }

    fn to_query(&self) -> String {
        self.keywords.join(" ")
    }
}

/// 設計検索結果
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub candidates: Vec<String>,
    pub top_candidate: Option<String>,
    pub confidence: f64,
}

impl DBMClient {
    pub fn new() -> Self {
        let runtime = CoreRuntime::new_with_defaults(
            Arc::new(InMemoryEngine::default()),
            Arc::new(DeterministicBeamSearchEngine::default()),
        );
        Self { runtime }
    }

    /// パスのコードを解析する（旧 API）
    pub fn analyze_code(&self, path: &str) -> Result<AnalysisResult, String> {
        analyzer::analyze_path(path)
    }

    /// プロジェクト全体を解析する（Phase3.1）
    pub fn analyze_project(
        &self,
        root_path: &str,
    ) -> Result<crate::dbm::analyzer::ProjectAnalysisResult, String> {
        analyzer::analyze_project(root_path)
    }

    /// 自然言語の intent から Architecture を生成する
    ///
    /// CoreRuntime のフルパイプライン（意図解析 → 検索 → 制約評価）を実行する。
    /// Clarification が必要な場合や実行エラー時は Err を返す。
    pub fn generate_architecture(&self, intent: &str) -> Result<ArchitectureResult, String> {
        let context = ChatContext::default();
        match self.runtime.execute_from_text(intent, &context) {
            Ok(RuntimeExecutionResult::Executed(result)) => {
                let components: Vec<String> = result
                    .project_layout
                    .files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect();
                let layers = derive_layers(&components);
                let actions = derive_actions(intent, &components);
                Ok(ArchitectureResult {
                    intent: intent.to_string(),
                    components,
                    layers,
                    actions,
                })
            }
            Ok(RuntimeExecutionResult::Clarification(c)) => {
                Err(format!("clarification needed: {}", c.message))
            }
            Err(e) => Err(format!("runtime error: {e:?}")),
        }
    }

    /// 制約に基づく設計空間検索
    pub fn search_design(&self, constraints: Constraints) -> Result<SearchResult, String> {
        let query = constraints.to_query();
        let context = ChatContext::default();
        match self.runtime.execute_from_text(&query, &context) {
            Ok(RuntimeExecutionResult::Executed(result)) => {
                let candidates: Vec<String> = result
                    .project_layout
                    .files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect();
                let top = candidates.first().cloned();
                Ok(SearchResult {
                    candidates,
                    top_candidate: top,
                    confidence: 0.8,
                })
            }
            Ok(RuntimeExecutionResult::Clarification(_)) => {
                Err("search needs clarification".to_string())
            }
            Err(e) => Err(format!("search error: {e:?}")),
        }
    }
}

impl Default for DBMClient {
    fn default() -> Self {
        Self::new()
    }
}

/// コンポーネントリストからレイヤー構造を推定する
fn derive_layers(components: &[String]) -> Vec<String> {
    let mut layers = Vec::new();
    if components
        .iter()
        .any(|c| c.contains("src/") || c.ends_with(".rs"))
    {
        layers.push("Implementation".to_string());
    }
    if components
        .iter()
        .any(|c| c.contains("tests/") || c.contains("test"))
    {
        layers.push("Test".to_string());
    }
    if components
        .iter()
        .any(|c| c.ends_with("Cargo.toml") || c.ends_with("package.json") || c.ends_with(".toml"))
    {
        layers.push("Configuration".to_string());
    }
    layers
}

/// intent とコンポーネントリストから ArchitectureAction を推定する
fn derive_actions(intent: &str, _components: &[String]) -> Vec<ArchitectureAction> {
    let lower = intent.to_lowercase();
    let target = intent
        .split_whitespace()
        .last()
        .unwrap_or("target")
        .to_string();
    let mut actions = Vec::new();

    if lower.contains("spec") || lower.contains("仕様") {
        actions.push(ArchitectureAction {
            action_type: "generate_spec".to_string(),
            target: target.clone(),
        });
    }
    if lower.contains("design") || lower.contains("設計") {
        actions.push(ArchitectureAction {
            action_type: "generate_design".to_string(),
            target: target.clone(),
        });
    }
    if lower.contains("analyze") || lower.contains("分析") || lower.contains("解析") {
        actions.push(ArchitectureAction {
            action_type: "analyze_code".to_string(),
            target: target.clone(),
        });
    }

    if actions.is_empty() {
        // コンポーネントが得られた場合はそれに基づく spec 生成
        // 得られない場合もデフォルト generate spec
        actions.push(ArchitectureAction {
            action_type: "generate_spec".to_string(),
            target,
        });
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_new_works() {
        let _client = DBMClient::new();
    }

    #[test]
    fn analyze_code_nonexistent_returns_empty() {
        let client = DBMClient::new();
        let result = client.analyze_code("/nonexistent/xyz123").unwrap();
        assert!(result.modules.is_empty());
    }

    #[test]
    fn analyze_code_src_dir_finds_files() {
        let client = DBMClient::new();
        let result = client.analyze_code("src/").unwrap();
        assert!(!result.modules.is_empty());
        assert!(result.total_lines > 0);
    }

    #[test]
    fn derive_layers_detects_implementation() {
        let components = vec!["src/main.rs".to_string()];
        let layers = derive_layers(&components);
        assert!(layers.contains(&"Implementation".to_string()));
    }

    #[test]
    fn derive_layers_detects_configuration() {
        let components = vec!["Cargo.toml".to_string()];
        let layers = derive_layers(&components);
        assert!(layers.contains(&"Configuration".to_string()));
    }

    #[test]
    fn derive_actions_spec_keyword() {
        let actions = derive_actions("write spec for api", &[]);
        assert_eq!(actions[0].action_type, "generate_spec");
        assert_eq!(actions[0].target, "api");
    }

    #[test]
    fn derive_actions_design_keyword() {
        let actions = derive_actions("design the database", &[]);
        assert_eq!(actions[0].action_type, "generate_design");
    }

    #[test]
    fn derive_actions_analyze_keyword() {
        let actions = derive_actions("analyze the code", &[]);
        assert_eq!(actions[0].action_type, "analyze_code");
    }

    #[test]
    fn derive_actions_default_is_spec() {
        let actions = derive_actions("build something", &[]);
        assert_eq!(actions[0].action_type, "generate_spec");
    }

    #[test]
    fn constraints_from_input() {
        let c = Constraints::from_input("build rust api");
        assert!(c.keywords.contains(&"rust".to_string()));
        assert!(c.keywords.contains(&"api".to_string()));
    }

    #[test]
    fn generate_architecture_returns_result_or_err() {
        let client = DBMClient::new();
        // CoreRuntime may clarify or execute — either is valid behavior
        let _result = client.generate_architecture("build rust cli");
        // Should not panic
    }
}
