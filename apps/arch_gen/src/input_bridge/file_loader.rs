use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ─── 保存形式の型定義 ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDesign {
    pub version: String,
    pub generated_at: String,
    pub input: String,
    pub search_states: usize,
    pub candidates: Vec<SavedCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCandidate {
    pub id: usize,
    pub score: f64,
    pub pareto_rank: usize,
    pub evaluation: SavedEvaluation,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<[String; 2]>,
    #[serde(default)]
    pub code_metrics: SavedCodeMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedCodeMetrics {
    pub coupling_score: f64,
    pub dependency_depth: usize,
    pub module_count: usize,
    pub dependency_cycles: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedEvaluation {
    pub structural_quality: f64,
    pub dependency_quality: f64,
    pub constraint_satisfaction: f64,
    pub complexity: f64,
    pub simulation_quality: f64,
    pub total: f64,
}

// ─── ロード / セーブ ────────────────────────────────────────────────────────

/// JSON設計ファイルを読み込んで `SavedDesign` を返す。
pub fn load_design_file(path: &Path) -> Result<SavedDesign, String> {
    let json = fs::read_to_string(path)
        .map_err(|e| format!("failed to read design file '{}': {e}", path.display()))?;
    serde_json::from_str(&json)
        .map_err(|e| format!("failed to parse design file '{}': {e}", path.display()))
}

/// `SavedDesign` をJSONファイルに書き出す。
pub fn save_design_file(design: &SavedDesign, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create dir '{}': {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(design)
        .map_err(|e| format!("failed to serialize design: {e}"))?;
    fs::write(path, json)
        .map_err(|e| format!("failed to write design file '{}': {e}", path.display()))
}

// ─── テスト ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_design() -> SavedDesign {
        SavedDesign {
            version: "1.0".to_string(),
            generated_at: "2026-03-15T00:00:00Z".to_string(),
            input: "ECサイトを設計する".to_string(),
            search_states: 10,
            candidates: vec![SavedCandidate {
                id: 1,
                score: 0.85,
                pareto_rank: 0,
                evaluation: SavedEvaluation {
                    structural_quality: 1.0,
                    dependency_quality: 0.8,
                    constraint_satisfaction: 0.9,
                    complexity: 0.3,
                    simulation_quality: 0.9,
                    total: 0.86,
                },
                components: vec![],
                dependencies: vec![],
                code_metrics: Default::default(),
            }],
        }
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let design = sample_design();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        save_design_file(&design, &path).unwrap();
        let loaded = load_design_file(&path).unwrap();

        assert_eq!(loaded.input, design.input);
        assert_eq!(loaded.candidates.len(), 1);
        assert!((loaded.candidates[0].score - 0.85).abs() < 1e-9);
    }

    #[test]
    fn test_load_missing_file_is_error() {
        let result = load_design_file(Path::new("/nonexistent/design.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to read design file"));
    }

    #[test]
    fn test_load_invalid_json_is_error() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "not valid json").unwrap();
        let result = load_design_file(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to parse design file"));
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let path = tmp_dir.path().join("nested/dir/design.json");
        let design = sample_design();
        save_design_file(&design, &path).unwrap();
        assert!(path.exists());
    }
}
