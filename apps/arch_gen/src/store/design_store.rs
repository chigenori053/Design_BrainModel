use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::input_bridge::{SavedDesign, load_design_file, save_design_file};

// ─── 型定義 ────────────────────────────────────────────────────────────────────

/// ストアの一覧エントリ（index.json に保存）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreEntry {
    pub name: String,
    pub saved_at: u64,
    /// 要件テキストの先頭 80 文字
    pub input_summary: String,
    pub candidate_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoreIndex {
    entries: Vec<StoreEntry>,
}

// ─── DesignStore ──────────────────────────────────────────────────────────────

/// 名前付き設計ファイルを `.arch_gen/store/` 配下で管理するストア。
pub struct DesignStore {
    store_dir: PathBuf,
}

impl DesignStore {
    /// カレントディレクトリの `.arch_gen/store/` をストアとして使う。
    pub fn new() -> Self {
        let store_dir = PathBuf::from(".arch_gen").join("store");
        Self { store_dir }
    }

    /// 任意のパスを指定して生成（テスト用）。
    #[cfg(test)]
    pub fn with_dir(store_dir: impl Into<PathBuf>) -> Self {
        Self { store_dir: store_dir.into() }
    }

    /// 名前付きで設計を保存し、保存先パスを返す。
    pub fn save(&self, name: &str, design: &SavedDesign) -> Result<PathBuf, String> {
        let name = sanitize_name(name);
        if name.is_empty() {
            return Err("store name must not be empty".to_string());
        }

        fs::create_dir_all(&self.store_dir)
            .map_err(|e| format!("failed to create store dir: {e}"))?;

        let file_path = self.store_dir.join(format!("{name}.json"));
        save_design_file(design, &file_path)?;

        // インデックス更新
        let mut index = self.read_index()?;
        let now = now_epoch();
        let summary = design.input.chars().take(80).collect::<String>();

        // 同名が既にあれば上書き
        if let Some(entry) = index.entries.iter_mut().find(|e| e.name == name) {
            entry.saved_at = now;
            entry.input_summary = summary;
            entry.candidate_count = design.candidates.len();
        } else {
            index.entries.push(StoreEntry {
                name: name.clone(),
                saved_at: now,
                input_summary: summary,
                candidate_count: design.candidates.len(),
            });
        }
        // 新しい順にソート
        index.entries.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        self.write_index(&index)?;

        Ok(file_path)
    }

    /// 名前で設計を読み込む。
    pub fn load(&self, name: &str) -> Result<SavedDesign, String> {
        let name = sanitize_name(name);
        let file_path = self.store_dir.join(format!("{name}.json"));
        load_design_file(&file_path)
            .map_err(|_| format!("store entry '{name}' not found. Use `saves` to list entries."))
    }

    /// 保存一覧を新しい順で返す。
    pub fn list(&self) -> Result<Vec<StoreEntry>, String> {
        if !self.store_dir.exists() {
            return Ok(vec![]);
        }
        let index = self.read_index()?;
        Ok(index.entries)
    }

    /// 名前付きエントリを削除する。
    pub fn delete(&self, name: &str) -> Result<(), String> {
        let name = sanitize_name(name);
        let file_path = self.store_dir.join(format!("{name}.json"));
        if file_path.exists() {
            fs::remove_file(&file_path)
                .map_err(|e| format!("failed to delete '{name}': {e}"))?;
        }
        let mut index = self.read_index()?;
        index.entries.retain(|e| e.name != name);
        self.write_index(&index)
    }

    // ─── private ──────────────────────────────────────────────────────────

    fn index_path(&self) -> PathBuf {
        self.store_dir.join("index.json")
    }

    fn read_index(&self) -> Result<StoreIndex, String> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(StoreIndex::default());
        }
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read store index: {e}"))?;
        serde_json::from_str(&json)
            .map_err(|e| format!("failed to parse store index: {e}"))
    }

    fn write_index(&self, index: &StoreIndex) -> Result<(), String> {
        let json = serde_json::to_string_pretty(index)
            .map_err(|e| format!("failed to serialize store index: {e}"))?;
        let path = self.index_path();
        fs::write(&path, json)
            .map_err(|e| format!("failed to write store index: {e}"))
    }
}

// ─── ユーティリティ ───────────────────────────────────────────────────────────

/// ファイル名として安全な文字列に変換する（英数字・ハイフン・アンダースコアのみ）。
fn sanitize_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 保存一覧を人間が読める形式で整形する。
pub fn format_store_list(entries: &[StoreEntry]) -> String {
    if entries.is_empty() {
        return "No saved designs. Use `save <name>` in interactive mode.".to_string();
    }
    let mut out = format!("{} saved design(s):\n", entries.len());
    out.push_str(&"─".repeat(55));
    out.push('\n');
    for entry in entries {
        let ts = format_epoch(entry.saved_at);
        out.push_str(&format!(
            "  {:20}  {}  {} candidate(s)\n",
            entry.name, ts, entry.candidate_count
        ));
        out.push_str(&format!("    {}\n", entry.input_summary));
    }
    out
}

fn format_epoch(secs: u64) -> String {
    // UNIX時刻をシンプルな日時文字列に変換（外部クレート不使用）
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let year = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02}")
}

// ─── テスト ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_bridge::{SavedCandidate, SavedCodeMetrics, SavedEvaluation};

    fn make_design(input: &str, n: usize) -> SavedDesign {
        SavedDesign {
            version: "1.0".to_string(),
            generated_at: "0".to_string(),
            input: input.to_string(),
            search_states: 10,
            candidates: (1..=n)
                .map(|i| SavedCandidate {
                    id: i,
                    score: 0.8,
                    pareto_rank: 0,
                    evaluation: SavedEvaluation {
                        structural_quality: 0.9,
                        dependency_quality: 0.8,
                        constraint_satisfaction: 0.9,
                        complexity: 0.3,
                        simulation_quality: 0.9,
                        total: 0.8,
                    },
                    components: vec!["service_1".to_string()],
                    dependencies: vec![],
                    code_metrics: SavedCodeMetrics::default(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_save_and_load() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = DesignStore::with_dir(tmp.path());
        let design = make_design("エディタを設計したい", 3);

        let path = store.save("my-editor", &design).unwrap();
        assert!(path.exists());

        let loaded = store.load("my-editor").unwrap();
        assert_eq!(loaded.input, "エディタを設計したい");
        assert_eq!(loaded.candidates.len(), 3);
    }

    #[test]
    fn test_list_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = DesignStore::with_dir(tmp.path());
        let entries = store.list().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_list_after_save() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = DesignStore::with_dir(tmp.path());
        store.save("design-a", &make_design("API設計", 2)).unwrap();
        store.save("design-b", &make_design("エディタ設計", 3)).unwrap();

        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_delete() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = DesignStore::with_dir(tmp.path());
        store.save("temp", &make_design("test", 1)).unwrap();
        store.delete("temp").unwrap();

        let entries = store.list().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_overwrite_same_name() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = DesignStore::with_dir(tmp.path());
        store.save("v1", &make_design("old", 1)).unwrap();
        store.save("v1", &make_design("new", 5)).unwrap();

        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].candidate_count, 5);
    }

    #[test]
    fn test_sanitize_name_replaces_special_chars() {
        assert_eq!(sanitize_name("my editor v1"), "my_editor_v1");
        assert_eq!(sanitize_name("設計案"), "");  // 非ASCII → _ → trim_matches → 空
        assert_eq!(sanitize_name("my-design_1"), "my-design_1");
        assert_eq!(sanitize_name("editor v2"), "editor_v2");
    }

    #[test]
    fn test_load_nonexistent_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = DesignStore::with_dir(tmp.path());
        assert!(store.load("nonexistent").is_err());
    }

    #[test]
    fn test_format_store_list_empty() {
        let result = format_store_list(&[]);
        assert!(result.contains("No saved designs"));
    }

    #[test]
    fn test_format_store_list_nonempty() {
        let entries = vec![StoreEntry {
            name: "my-editor".to_string(),
            saved_at: 1700000000,
            input_summary: "NeoVim風エディタ".to_string(),
            candidate_count: 3,
        }];
        let result = format_store_list(&entries);
        assert!(result.contains("my-editor"));
        assert!(result.contains("3 candidate(s)"));
    }
}
