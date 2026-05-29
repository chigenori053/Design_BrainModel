//! CapabilityExecutionTelemetry
//!
//! DBM-ANALYZE-CAPABILITY-ISOLATION v1.2 §4 に基づく Capability 実行テレメトリ。
//!
//! Capability の実行結果（成功・失敗）を JSON 形式で記録する。
//!
//! ## 成功時の JSON フォーマット
//!
//! ```json
//! {"intent":"AnalyzeTests","capability":"AnalyzeTestsCapability","result_type":"TestInventoryResult"}
//! ```
//!
//! ## 失敗時の JSON フォーマット
//!
//! ```json
//! {"intent":"AnalyzeTests","capability":"AnalyzeTestsCapability","result_type":"ProjectStructureAnalysisResult","error":"capability_mismatch"}
//! ```

use serde_json::{Value, json};

/// Capability 実行テレメトリ。
///
/// 成功・失敗双方の実行ログを保持する。
/// `error` フィールドが `None` の場合は成功、`Some` の場合は失敗を示す。
#[derive(Debug, Clone)]
pub struct CapabilityExecutionTelemetry {
    /// 元の Intent 名（例: `"AnalyzeTests"`）。
    pub intent: String,
    /// 使用された Capability 名（例: `"AnalyzeTestsCapability"`）。
    pub capability: String,
    /// 実際の Output 型名（例: `"TestInventoryResult"`）。
    pub result_type: String,
    /// エラー種別。成功時は `None`、失敗時は `Some("capability_mismatch")` など。
    pub error: Option<String>,
}

impl CapabilityExecutionTelemetry {
    /// 成功時のテレメトリを生成する。
    pub fn record_success(intent: &str, capability: &str, result_type: &str) -> Self {
        Self {
            intent: intent.to_string(),
            capability: capability.to_string(),
            result_type: result_type.to_string(),
            error: None,
        }
    }

    /// Capability ミスマッチ失敗時のテレメトリを生成する。
    pub fn record_mismatch(intent: &str, capability: &str, result_type: &str) -> Self {
        Self {
            intent: intent.to_string(),
            capability: capability.to_string(),
            result_type: result_type.to_string(),
            error: Some("capability_mismatch".to_string()),
        }
    }

    /// テレメトリを compact JSON 文字列にシリアライズする。
    pub fn to_json(&self) -> String {
        self.to_json_value().to_string()
    }

    /// テレメトリを `serde_json::Value` として返す。
    pub fn to_json_value(&self) -> Value {
        if let Some(ref error) = self.error {
            json!({
                "intent": self.intent,
                "capability": self.capability,
                "result_type": self.result_type,
                "error": error,
            })
        } else {
            json!({
                "intent": self.intent,
                "capability": self.capability,
                "result_type": self.result_type,
            })
        }
    }

    /// 成功テレメトリか否か。
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// spec §Required Tests: telemetry_records_capability_execution
    ///
    /// 成功時: intent, capability, result_type が記録され error フィールドが存在しない。
    #[test]
    fn telemetry_records_capability_execution() {
        let t = CapabilityExecutionTelemetry::record_success(
            "AnalyzeTests",
            "AnalyzeTestsCapability",
            "TestInventoryResult",
        );
        assert!(t.is_success(), "record_success should be success");

        let v = t.to_json_value();
        assert_eq!(v["intent"], "AnalyzeTests");
        assert_eq!(v["capability"], "AnalyzeTestsCapability");
        assert_eq!(v["result_type"], "TestInventoryResult");
        // 成功時は "error" フィールドなし
        assert!(
            v.get("error").is_none(),
            "success telemetry must not have error field"
        );
    }

    /// spec §Required Tests: telemetry_records_capability_mismatch
    ///
    /// 失敗時: error フィールドが "capability_mismatch" で記録される。
    #[test]
    fn telemetry_records_capability_mismatch() {
        let t = CapabilityExecutionTelemetry::record_mismatch(
            "AnalyzeTests",
            "AnalyzeTestsCapability",
            "ProjectStructureAnalysisResult",
        );
        assert!(!t.is_success(), "record_mismatch should not be success");

        let v = t.to_json_value();
        assert_eq!(v["intent"], "AnalyzeTests");
        assert_eq!(v["capability"], "AnalyzeTestsCapability");
        assert_eq!(v["result_type"], "ProjectStructureAnalysisResult");
        assert_eq!(v["error"], "capability_mismatch");
    }

    /// 成功時の JSON 文字列に "error" キーが含まれない。
    #[test]
    fn success_json_has_no_error_key() {
        let t = CapabilityExecutionTelemetry::record_success(
            "AnalyzeProject",
            "AnalyzeProjectCapability",
            "ProjectStructureAnalysisResult",
        );
        let json_str = t.to_json();
        assert!(
            !json_str.contains("\"error\""),
            "success JSON must not contain error key: {json_str}"
        );
    }

    /// 失敗時の JSON 文字列に "error" キーと "capability_mismatch" 値が含まれる。
    #[test]
    fn mismatch_json_contains_error_field() {
        let t = CapabilityExecutionTelemetry::record_mismatch(
            "AnalyzeTests",
            "AnalyzeTestsCapability",
            "ProjectStructureAnalysisResult",
        );
        let json_str = t.to_json();
        assert!(
            json_str.contains("\"error\""),
            "mismatch JSON must contain error key: {json_str}"
        );
        assert!(
            json_str.contains("capability_mismatch"),
            "mismatch JSON must contain capability_mismatch: {json_str}"
        );
    }

    /// AnalyzeProject 成功テレメトリのフィールド確認。
    #[test]
    fn analyze_project_success_telemetry_fields() {
        let t = CapabilityExecutionTelemetry::record_success(
            "AnalyzeProject",
            "AnalyzeProjectCapability",
            "ProjectStructureAnalysisResult",
        );
        assert_eq!(t.intent, "AnalyzeProject");
        assert_eq!(t.capability, "AnalyzeProjectCapability");
        assert_eq!(t.result_type, "ProjectStructureAnalysisResult");
        assert!(t.error.is_none());
    }
}
