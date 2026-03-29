//! optimizer — 永続化記憶の意思決定エンジン
//!
//! # 設計思想
//!
//! 単純な閾値比較ではなく、複数の類似度次元を統合した
//! **多基準意思決定 (Multi-Criteria Decision Making; MCDM)** を採用する。
//!
//! ## 意思決定フロー
//!
//! ```text
//!              ┌─────────────────────────────────────┐
//!  ingest(r)   │  SimilarityProfile                   │
//!  ─────────►  │  ・tag_jaccard  (タグ Jaccard 類似度)│
//!              │  ・embed_cosine (埋め込みコサイン類似)│
//!              │  ・text_overlap (テキスト単語重複率)  │
//!              └────────────────┬────────────────────┘
//!                               │
//!                        score = weighted_sum
//!                               │
//!              ┌────────────────▼────────────────────┐
//!              │           DecisionPolicy              │
//!              │  ┌──────────────────────────────┐   │
//!              │  │  score < unique_threshold     │──►│ Store (新規保存)
//!              │  │  score >= duplicate_threshold │──►│ Skipped (重複排除)
//!              │  │  else → upgrade_gate          │   │
//!              │  └──────────┬───────────────────┘   │
//!              └─────────────┼──────────────────────┘
//!                            │
//!              ┌─────────────▼────────────────────────┐
//!              │  UpgradeGate (アップグレード判定)      │
//!              │  embed_cosine >= embed_gate AND       │
//!              │  tag_jaccard >= tag_gate              │
//!              │  → Upgraded / Store (コンセプト乖離)  │
//!              └──────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};

use crate::generalized_memory::GeneralizedMemory;
use crate::similarity::{combined_similarity, cosine_similarity, jaccard_similarity};

// ── デフォルト閾値 ────────────────────────────────────────────────────────────

/// combined スコアがこれを下回る場合は「ユニーク」と判定し、新規保存する
pub const UNIQUE_THRESHOLD: f32 = 0.30;

/// combined スコアがこれを上回る場合は「重複」と判定し、スキップする
pub const DUPLICATE_THRESHOLD: f32 = 0.85;

// ── 類似度プロファイル ──────────────────────────────────────────────────────────

/// 複数の次元での類似度スコアをまとめた構造体
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimilarityProfile {
    /// タグの Jaccard 類似度 [0, 1]
    pub tag_jaccard: f32,
    /// 埋め込みベクトルのコサイン類似度 [0, 1]
    pub embed_cosine: f32,
    /// テキスト単語レベルの重複率 [0, 1]
    pub text_overlap: f32,
    /// ポリシーによる加重和スコア [0, 1]
    pub combined: f32,
}

impl SimilarityProfile {
    /// 汎化記憶と新規記録の間の類似度プロファイルを計算する。
    pub fn compute(
        existing: &GeneralizedMemory,
        new_tags: &[String],
        new_embed: &[f32],
        new_text: &str,
        policy: &DecisionPolicy,
    ) -> Self {
        let tag_jaccard = jaccard_similarity(&existing.abstract_tags, new_tags);
        let embed_cosine = cosine_similarity(&existing.centroid_embedding, new_embed);
        let text_overlap = text_word_overlap(&existing.summary, new_text);
        let combined = policy.weight_tag * tag_jaccard
            + policy.weight_embed * embed_cosine
            + policy.weight_text * text_overlap;

        Self {
            tag_jaccard,
            embed_cosine,
            text_overlap,
            combined: combined.clamp(0.0, 1.0),
        }
    }
}

// ── アップグレードゲート ────────────────────────────────────────────────────────

/// アップグレードを許可するための追加条件
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpgradeGate {
    /// 埋め込みコサイン類似度の最低要件 (これを下回るとコンセプト乖離とみなす)
    pub min_embed_cosine: f32,
    /// タグ Jaccard の最低要件 (これを下回ると意味領域が異なるとみなす)
    pub min_tag_jaccard: f32,
}

impl Default for UpgradeGate {
    fn default() -> Self {
        Self {
            min_embed_cosine: 0.20,
            min_tag_jaccard: 0.10,
        }
    }
}

impl UpgradeGate {
    /// プロファイルがアップグレード条件を満たすか判定する。
    ///
    /// 両条件を満たす場合のみ Upgrade を許可し、
    /// 満たさない場合はコンセプト乖離として新規保存する。
    pub fn allows(&self, profile: &SimilarityProfile) -> bool {
        profile.embed_cosine >= self.min_embed_cosine && profile.tag_jaccard >= self.min_tag_jaccard
    }
}

// ── 意思決定ポリシー ────────────────────────────────────────────────────────────

/// 意思決定に使用するポリシー設定
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionPolicy {
    /// combined スコアがこれを下回ると新規保存
    pub unique_threshold: f32,
    /// combined スコアがこれを上回ると重複スキップ
    pub duplicate_threshold: f32,
    /// タグ Jaccard の重み
    pub weight_tag: f32,
    /// 埋め込みコサインの重み
    pub weight_embed: f32,
    /// テキスト重複の重み
    pub weight_text: f32,
    /// アップグレードゲート条件
    pub upgrade_gate: UpgradeGate,
}

impl Default for DecisionPolicy {
    fn default() -> Self {
        Self {
            unique_threshold: UNIQUE_THRESHOLD,
            duplicate_threshold: DUPLICATE_THRESHOLD,
            weight_tag: 0.30,
            weight_embed: 0.55,
            weight_text: 0.15,
            upgrade_gate: UpgradeGate::default(),
        }
    }
}

impl DecisionPolicy {
    /// 重みの合計を検証する (テスト・デバッグ用)
    pub fn validate(&self) -> Result<(), String> {
        let sum = self.weight_tag + self.weight_embed + self.weight_text;
        if (sum - 1.0).abs() > 1e-3 {
            return Err(format!("weights must sum to 1.0, got {sum:.3}"));
        }
        if self.unique_threshold >= self.duplicate_threshold {
            return Err(format!(
                "unique_threshold ({}) must be < duplicate_threshold ({})",
                self.unique_threshold, self.duplicate_threshold
            ));
        }
        Ok(())
    }
}

// ── 意思決定結果 ────────────────────────────────────────────────────────────────

/// 意思決定の根拠情報
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionEvidence {
    /// 最も類似する既存記憶のインデックス (None = 記憶なし)
    pub best_match_idx: Option<usize>,
    /// 最も類似する既存記憶の ID (None = 記憶なし)
    pub best_match_id: Option<String>,
    /// 類似度プロファイル (None = 記憶なし)
    pub profile: Option<SimilarityProfile>,
    /// 意思決定の理由テキスト
    pub reason: String,
}

/// 取り込み処理の結果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IngestResult {
    /// 新規ユニーク記憶として保存された (ID を返す)
    Stored(String),
    /// 既存の汎化記憶がアップグレードされた (ID と新バージョン番号を返す)
    Upgraded { id: String, version: u32 },
    /// 重複のためスキップされた (マッチした記憶の ID と類似度を返す)
    Skipped { matched_id: String, similarity: f32 },
}

impl IngestResult {
    /// 保存またはアップグレードされた記憶の ID を返す。スキップの場合は None。
    pub fn stored_or_upgraded_id(&self) -> Option<&str> {
        match self {
            IngestResult::Stored(id) => Some(id),
            IngestResult::Upgraded { id, .. } => Some(id),
            IngestResult::Skipped { .. } => None,
        }
    }

    pub fn is_stored(&self) -> bool {
        matches!(self, IngestResult::Stored(_))
    }

    pub fn is_upgraded(&self) -> bool {
        matches!(self, IngestResult::Upgraded { .. })
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self, IngestResult::Skipped { .. })
    }
}

// ── 意思決定エンジン ────────────────────────────────────────────────────────────

/// 多基準意思決定エンジン
///
/// [`DecisionPolicy`] に従い、新規記録を既存の汎化記憶と比較して
/// 保存 / アップグレード / スキップ のいずれかを決定する。
#[derive(Debug)]
pub struct DecisionEngine {
    pub policy: DecisionPolicy,
}

impl DecisionEngine {
    pub fn new(policy: DecisionPolicy) -> Self {
        Self { policy }
    }

    pub fn with_defaults() -> Self {
        Self::new(DecisionPolicy::default())
    }

    /// 新規記録に対する取り込み決定を行う。
    ///
    /// # Arguments
    /// - `memories`        : 既存の汎化記憶スライス
    /// - `new_tags`        : 新規記録のタグ
    /// - `new_embedding`   : 新規記録の埋め込みベクトル
    /// - `new_text`        : 新規記録のテキスト
    ///
    /// # Returns
    /// `(action, evidence)` のタプル。`action` は保存先インデックスを示す [`DecisionAction`]。
    pub fn decide(
        &self,
        memories: &[GeneralizedMemory],
        new_tags: &[String],
        new_embedding: &[f32],
        new_text: &str,
    ) -> (DecisionAction, DecisionEvidence) {
        if memories.is_empty() {
            return (
                DecisionAction::StoreNew,
                DecisionEvidence {
                    best_match_idx: None,
                    best_match_id: None,
                    profile: None,
                    reason: "No existing memories; store as first entry.".to_string(),
                },
            );
        }

        // 全記憶との類似度プロファイルを計算し、最大スコアを求める
        let best = memories
            .iter()
            .enumerate()
            .map(|(idx, mem)| {
                let profile = SimilarityProfile::compute(
                    mem,
                    new_tags,
                    new_embedding,
                    new_text,
                    &self.policy,
                );
                (idx, profile)
            })
            .max_by(|a, b| {
                a.1.combined
                    .partial_cmp(&b.1.combined)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("memories is non-empty");

        let (best_idx, best_profile) = best;
        let best_id = memories[best_idx].id.clone();
        let score = best_profile.combined;

        if score >= self.policy.duplicate_threshold {
            // 重複
            return (
                DecisionAction::Skip {
                    matched_idx: best_idx,
                },
                DecisionEvidence {
                    best_match_idx: Some(best_idx),
                    best_match_id: Some(best_id),
                    profile: Some(best_profile.clone()),
                    reason: format!(
                        "Duplicate detected (combined={score:.3} >= threshold={:.3}). \
                         tag_jaccard={:.3}, embed_cosine={:.3}, text_overlap={:.3}.",
                        self.policy.duplicate_threshold,
                        best_profile.tag_jaccard,
                        best_profile.embed_cosine,
                        best_profile.text_overlap
                    ),
                },
            );
        }

        if score < self.policy.unique_threshold {
            // ユニーク
            return (
                DecisionAction::StoreNew,
                DecisionEvidence {
                    best_match_idx: Some(best_idx),
                    best_match_id: Some(best_id),
                    profile: Some(best_profile.clone()),
                    reason: format!(
                        "Unique concept (combined={score:.3} < threshold={:.3}). \
                         tag_jaccard={:.3}, embed_cosine={:.3}.",
                        self.policy.unique_threshold,
                        best_profile.tag_jaccard,
                        best_profile.embed_cosine
                    ),
                },
            );
        }

        // アップグレードゾーン: UpgradeGate で追加検証
        if self.policy.upgrade_gate.allows(&best_profile) {
            (
                DecisionAction::Upgrade {
                    target_idx: best_idx,
                },
                DecisionEvidence {
                    best_match_idx: Some(best_idx),
                    best_match_id: Some(best_id),
                    profile: Some(best_profile.clone()),
                    reason: format!(
                        "Upgrade: combined={score:.3} in [{:.3}, {:.3}), \
                         embed_cosine={:.3} >= gate={:.3}, \
                         tag_jaccard={:.3} >= gate={:.3}.",
                        self.policy.unique_threshold,
                        self.policy.duplicate_threshold,
                        best_profile.embed_cosine,
                        self.policy.upgrade_gate.min_embed_cosine,
                        best_profile.tag_jaccard,
                        self.policy.upgrade_gate.min_tag_jaccard
                    ),
                },
            )
        } else {
            // UpgradeGate 不通過 → コンセプト乖離として新規保存
            (
                DecisionAction::StoreNew,
                DecisionEvidence {
                    best_match_idx: Some(best_idx),
                    best_match_id: Some(best_id),
                    profile: Some(best_profile.clone()),
                    reason: format!(
                        "Concept drift: combined={score:.3} in upgrade zone but \
                         embed_cosine={:.3} < gate={:.3} or tag_jaccard={:.3} < gate={:.3}. \
                         Treating as new concept.",
                        best_profile.embed_cosine,
                        self.policy.upgrade_gate.min_embed_cosine,
                        best_profile.tag_jaccard,
                        self.policy.upgrade_gate.min_tag_jaccard
                    ),
                },
            )
        }
    }
}

/// 意思決定アクション
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionAction {
    /// 新規の汎化記憶として保存する
    StoreNew,
    /// 既存の汎化記憶をアップグレードする
    Upgrade { target_idx: usize },
    /// 重複のためスキップする
    Skip { matched_idx: usize },
}

// ── ユーティリティ ─────────────────────────────────────────────────────────────

/// テキスト間の単語レベル重複率を計算する。
///
/// 両テキストのユニーク単語集合の Jaccard 類似度を返す。
pub fn text_word_overlap(a: &str, b: &str) -> f32 {
    let words_a: std::collections::BTreeSet<String> = a
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_ascii_lowercase())
        .collect();
    let words_b: std::collections::BTreeSet<String> = b
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_ascii_lowercase())
        .collect();
    if words_a.is_empty() && words_b.is_empty() {
        return 0.0;
    }
    let intersection = words_a.intersection(&words_b).count() as f32;
    let union = words_a.union(&words_b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// 既存記憶の中からデフォルトポリシーで最も類似するエントリを探す。
///
/// Returns `(index, combined_similarity)` のタプル。記憶が空の場合は `None`。
pub fn find_best_match(
    memories: &[GeneralizedMemory],
    record_tags: &[String],
    record_embedding: &[f32],
) -> Option<(usize, f32)> {
    if memories.is_empty() {
        return None;
    }
    memories
        .iter()
        .enumerate()
        .map(|(idx, mem)| {
            let sim = combined_similarity(
                &mem.abstract_tags,
                &mem.centroid_embedding,
                record_tags,
                record_embedding,
            );
            (idx, sim)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

// ── テスト ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generalized_memory::GeneralizedMemory;

    fn make_mem(id: &str, tags: &[&str], embed: &[f32], summary: &str) -> GeneralizedMemory {
        let mut m = GeneralizedMemory::from_record(
            id.to_string(),
            summary,
            &tags.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            embed,
        );
        m.summary = summary.to_string();
        m
    }

    fn engine() -> DecisionEngine {
        DecisionEngine::with_defaults()
    }

    #[test]
    fn empty_memories_always_store_new() {
        let eng = engine();
        let (action, ev) = eng.decide(&[], &["api".to_string()], &[1.0, 0.0], "REST API design");
        assert_eq!(action, DecisionAction::StoreNew);
        assert!(ev.best_match_idx.is_none());
    }

    #[test]
    fn identical_input_is_skipped() {
        let eng = engine();
        let mem = make_mem("m1", &["api", "rest"], &[1.0, 0.0], "REST API design");
        let (action, _ev) = eng.decide(
            &[mem],
            &["api".to_string(), "rest".to_string()],
            &[1.0, 0.0],
            "REST API design",
        );
        assert!(matches!(action, DecisionAction::Skip { .. }));
    }

    #[test]
    fn orthogonal_input_stores_new() {
        let eng = engine();
        let mem = make_mem("m1", &["api"], &[1.0, 0.0], "REST API");
        let (action, ev) = eng.decide(
            &[mem],
            &["database".to_string(), "sql".to_string()],
            &[0.0, 1.0],
            "SQL schema design",
        );
        assert_eq!(action, DecisionAction::StoreNew);
        assert!(ev.profile.as_ref().unwrap().combined < UNIQUE_THRESHOLD);
    }

    #[test]
    fn similar_input_upgrades_existing() {
        let eng = engine();
        let mem = make_mem("m1", &["api", "rest"], &[1.0, 0.0, 0.0], "REST API design");
        // embed_cosine と tag_jaccard が高い → UpgradeGate を通過
        let (action, _ev) = eng.decide(
            &[mem],
            &["api".to_string(), "rest".to_string(), "auth".to_string()],
            &[0.95, 0.05, 0.0],
            "REST API with authentication",
        );
        assert!(matches!(action, DecisionAction::Upgrade { target_idx: 0 }));
    }

    #[test]
    fn policy_validation_catches_bad_weights() {
        let mut policy = DecisionPolicy::default();
        policy.weight_tag = 0.5;
        assert!(policy.validate().is_err());
    }

    #[test]
    fn upgrade_gate_blocks_concept_drift() {
        let mut policy = DecisionPolicy::default();
        // embed_cosine を非常に高いゲートに設定
        policy.upgrade_gate.min_embed_cosine = 0.99;
        let eng = DecisionEngine::new(policy);

        let mem = make_mem("m1", &["api", "rest"], &[1.0, 0.0], "REST API design");
        // combined はアップグレードゾーンだが embed_cosine が低い
        let (action, ev) = eng.decide(
            &[mem],
            &["api".to_string(), "rest".to_string()],
            &[0.5, 0.5], // embed_cosine = cos(45deg) ≈ 0.707 < 0.99
            "REST API improvements",
        );
        // ゲート不通過 → コンセプト乖離として StoreNew
        assert_eq!(action, DecisionAction::StoreNew);
        assert!(ev.reason.contains("Concept drift"));
    }

    #[test]
    fn text_word_overlap_symmetric() {
        let a = "REST API design for mobile";
        let b = "mobile API design patterns";
        let sim = text_word_overlap(a, b);
        assert!(sim > 0.0 && sim < 1.0);
        assert_eq!(text_word_overlap(a, b), text_word_overlap(b, a));
    }
}
