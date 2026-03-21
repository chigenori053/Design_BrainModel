/// Phase3: DesignBrainModel インターフェースモジュール
///
/// CLIをDBM（DesignBrainModel Core）に接続するアダプタ群。
///
/// - client     : DBM Core との通信（CoreRuntime ラッパー）
/// - translator : DBM 出力 → CLI Plan 変換
/// - analyzer   : ファイルシステムベースのコード解析
pub mod analyzer;
pub mod client;
pub mod translator;

pub use analyzer::{
    AnalysisResult, Complexity, DependencyEdge, FileAnalysis, Language, Module, ModuleInfo,
    ProjectAnalysisResult, ProjectSummary,
};
pub use client::{ArchitectureAction, ArchitectureResult, Constraints, DBMClient, SearchResult};
pub use translator::Translator;
