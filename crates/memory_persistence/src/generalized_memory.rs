//! GeneralizedMemory — 汎化された永続化記憶の型定義

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// 汎化された永続化記憶エントリ。
///
/// 複数の類似する元記憶から共通の特徴を抽出して保持し、
/// 想起しやすいよう抽象的な形に一般化されている。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneralizedMemory {
    /// 永続化記憶の一意 ID
    pub id: String,
    /// 汎化されたテキスト要約
    pub summary: String,
    /// 共通タグ (ソート済み・重複なし)
    pub abstract_tags: Vec<String>,
    /// 重心埋め込みベクトル (元記憶の平均)
    pub centroid_embedding: Vec<f32>,
    /// バージョン番号 (アップグレードの度にインクリメント)
    pub version: u32,
    /// マージ元の記憶件数
    pub source_count: usize,
    /// この記憶が想起された回数
    pub recall_count: usize,
    /// Unix エポック秒 (作成時刻)
    pub created_epoch: u64,
    /// Unix エポック秒 (最終アップグレード時刻)
    pub last_upgraded_epoch: u64,
    /// 追加属性メタデータ
    pub attributes: BTreeMap<String, String>,
}

impl GeneralizedMemory {
    /// 元記憶から新規の汎化記憶を生成する。
    pub fn from_record(id: String, text: &str, tags: &[String], embedding: &[f32]) -> Self {
        let now = epoch_now();
        Self {
            id,
            summary: generalize_text(text),
            abstract_tags: generalize_tags(tags),
            centroid_embedding: embedding.to_vec(),
            version: 1,
            source_count: 1,
            recall_count: 0,
            created_epoch: now,
            last_upgraded_epoch: now,
            attributes: BTreeMap::new(),
        }
    }

    /// 新しい記憶で既存の汎化記憶をアップグレードする。
    ///
    /// - タグをマージする
    /// - 重心埋め込みを指数移動平均で更新する
    /// - テキスト要約は新しい方が長い場合に上書きする
    pub fn upgrade(&mut self, text: &str, tags: &[String], embedding: &[f32]) {
        // タグをマージ
        let new_tags = generalize_tags(tags);
        for tag in new_tags {
            if !self.abstract_tags.contains(&tag) {
                self.abstract_tags.push(tag);
            }
        }
        self.abstract_tags.sort();

        // 重心埋め込みを指数移動平均で更新 (α = 1 / (source_count + 1))
        if !embedding.is_empty() && embedding.len() == self.centroid_embedding.len() {
            let n = self.source_count as f32;
            for (centroid, new_val) in self.centroid_embedding.iter_mut().zip(embedding.iter()) {
                *centroid = (*centroid * n + new_val) / (n + 1.0);
            }
        }

        // より長い要約があれば更新
        let new_summary = generalize_text(text);
        if new_summary.len() > self.summary.len() {
            self.summary = new_summary;
        }

        self.source_count += 1;
        self.version += 1;
        self.last_upgraded_epoch = epoch_now();
    }

    /// 想起カウンタをインクリメントする。
    pub fn bump_recall(&mut self) {
        self.recall_count += 1;
    }
}

/// テキストから汎化された要約を生成する。
///
/// 最初の文 (句読点で区切る) を取り出し、120 文字以内に丸める。
fn generalize_text(text: &str) -> String {
    let sentence = text
        .split(|c: char| c == '.' || c == '!' || c == '\n')
        .next()
        .unwrap_or(text)
        .trim();
    if sentence.len() > 120 {
        format!("{}…", &sentence[..120])
    } else {
        sentence.to_string()
    }
}

/// タグリストから汎化されたタグセットを生成する。
///
/// 小文字化 → ソート → 重複除去。
fn generalize_tags(tags: &[String]) -> Vec<String> {
    let mut result: Vec<String> = tags
        .iter()
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    result.sort();
    result.dedup();
    result
}

fn epoch_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_record_sets_fields() {
        let mem = GeneralizedMemory::from_record(
            "gm_001".to_string(),
            "Design a REST API for user management.",
            &["api".to_string(), "REST".to_string()],
            &[0.1, 0.2, 0.3],
        );
        assert_eq!(mem.version, 1);
        assert_eq!(mem.source_count, 1);
        assert_eq!(mem.abstract_tags, vec!["api", "rest"]);
        assert!(mem.summary.contains("Design a REST"));
    }

    #[test]
    fn upgrade_merges_tags_and_increments_version() {
        let mut mem = GeneralizedMemory::from_record(
            "gm_002".to_string(),
            "GraphQL schema design",
            &["graphql".to_string()],
            &[0.5, 0.5],
        );
        mem.upgrade(
            "Add mutations to GraphQL schema",
            &["graphql".to_string(), "mutation".to_string()],
            &[0.6, 0.4],
        );
        assert_eq!(mem.version, 2);
        assert_eq!(mem.source_count, 2);
        assert!(mem.abstract_tags.contains(&"mutation".to_string()));
    }

    #[test]
    fn upgrade_updates_centroid() {
        let mut mem =
            GeneralizedMemory::from_record("gm_003".to_string(), "cache layer", &[], &[1.0, 0.0]);
        mem.upgrade("redis cache", &[], &[0.0, 1.0]);
        // centroid should be (1.0 * 1 + 0.0) / 2 = 0.5 for index 0
        assert!((mem.centroid_embedding[0] - 0.5).abs() < 1e-5);
        assert!((mem.centroid_embedding[1] - 0.5).abs() < 1e-5);
    }
}
