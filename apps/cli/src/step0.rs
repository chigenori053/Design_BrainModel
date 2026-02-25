use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

const EXPECTED_TOTAL: usize = 100;
const EXPECTED_DISTRIBUTION: [(&str, usize); 6] = [
    ("A", 20),
    ("B", 25),
    ("C", 20),
    ("D", 15),
    ("E", 10),
    ("F", 10),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TestCase {
    pub case_id: String,
    pub category: String,
    pub input_text: String,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegritySummary {
    pub seed: u64,
    pub total_cases: usize,
    pub category_counts: BTreeMap<String, usize>,
    pub unique_case_id: bool,
    pub empty_input_text_count: usize,
    pub hash: String,
}

pub fn generate_cases(seed: u64) -> Vec<TestCase> {
    let mut out = Vec::with_capacity(EXPECTED_TOTAL);
    for (cat, count) in EXPECTED_DISTRIBUTION {
        for idx in 1..=count {
            let case_id = format!("{cat}-{idx:03}");
            let input_text = generate_input_text(seed, cat, idx);
            out.push(TestCase {
                case_id,
                category: cat.to_string(),
                input_text,
                seed,
            });
        }
    }
    out
}

pub fn validate_cases(cases: &[TestCase], seed: u64) -> Result<IntegritySummary, Vec<String>> {
    let mut errors = Vec::new();

    if cases.len() != EXPECTED_TOTAL {
        errors.push(format!(
            "N mismatch: expected {EXPECTED_TOTAL}, got {}",
            cases.len()
        ));
    }

    let mut counts = BTreeMap::<String, usize>::new();
    for c in cases {
        *counts.entry(c.category.clone()).or_insert(0) += 1;
    }
    for (cat, expected) in EXPECTED_DISTRIBUTION {
        let got = counts.get(cat).copied().unwrap_or(0);
        if got != expected {
            errors.push(format!(
                "category {cat} mismatch: expected {expected}, got {got}"
            ));
        }
    }
    for cat in counts.keys() {
        if !EXPECTED_DISTRIBUTION
            .iter()
            .any(|(k, _)| k == &cat.as_str())
        {
            errors.push(format!("unknown category: {cat}"));
        }
    }

    let mut seen = BTreeSet::<String>::new();
    let mut duplicates = Vec::new();
    let mut empty_input = Vec::new();
    for c in cases {
        if !seen.insert(c.case_id.clone()) {
            duplicates.push(c.case_id.clone());
        }
        if c.case_id.trim().is_empty() {
            errors.push("case_id missing".to_string());
        }
        if c.input_text.trim().is_empty() {
            empty_input.push(c.case_id.clone());
        }
    }
    if !duplicates.is_empty() {
        duplicates.sort();
        duplicates.dedup();
        errors.push(format!("duplicate case_id found: {}", duplicates.join(",")));
    }
    if !empty_input.is_empty() {
        errors.push(format!("empty input_text found: {}", empty_input.join(",")));
    }

    let summary = IntegritySummary {
        seed,
        total_cases: cases.len(),
        category_counts: counts,
        unique_case_id: duplicates.is_empty(),
        empty_input_text_count: empty_input.len(),
        hash: format!("sha256:{}", dataset_hash(cases)),
    };

    if errors.is_empty() {
        Ok(summary)
    } else {
        Err(errors)
    }
}

pub fn write_audit_logs(
    base_dir: &Path,
    seed: u64,
    cases: &[TestCase],
    summary: &IntegritySummary,
) -> Result<(), String> {
    let seed_dir = base_dir.join(format!("seed_{seed}"));
    fs::create_dir_all(&seed_dir).map_err(|e| format!("failed to create seed dir: {e}"))?;

    let summary_path = seed_dir.join("integrity_summary.json");
    let digest_path = seed_dir.join("case_digest.csv");
    let summary_json = serde_json::to_string_pretty(summary)
        .map_err(|e| format!("failed to serialize integrity summary: {e}"))?;
    fs::write(summary_path, summary_json).map_err(|e| format!("failed to write summary: {e}"))?;

    let mut writer = BufWriter::new(
        File::create(digest_path).map_err(|e| format!("failed to create digest csv: {e}"))?,
    );
    writer
        .write_all(b"case_id,category,input_len,input_hash_prefix\n")
        .map_err(|e| format!("failed to write digest header: {e}"))?;
    for c in cases {
        let input_hash_prefix = &sha256_hex(c.input_text.as_bytes())[..8];
        let line = format!(
            "{},{},{},{}\n",
            c.case_id,
            c.category,
            c.input_text.chars().count(),
            input_hash_prefix
        );
        writer
            .write_all(line.as_bytes())
            .map_err(|e| format!("failed to write digest row: {e}"))?;
    }
    writer
        .flush()
        .map_err(|e| format!("failed to flush digest csv: {e}"))?;
    Ok(())
}

pub fn input_hash_prefix(input: &str) -> String {
    sha256_hex(input.as_bytes())[..8].to_string()
}

pub fn verify_seed_reproducibility(seed: u64) -> Result<(), String> {
    let a = generate_cases(seed);
    let b = generate_cases(seed);
    if a.len() != b.len() {
        return Err("reproducibility mismatch: length differs".to_string());
    }
    for (idx, (lhs, rhs)) in a.iter().zip(b.iter()).enumerate() {
        let h1 =
            sha256_hex(format!("{}|{}|{}", lhs.category, lhs.case_id, lhs.input_text).as_bytes());
        let h2 =
            sha256_hex(format!("{}|{}|{}", rhs.category, rhs.case_id, rhs.input_text).as_bytes());
        if h1 != h2 {
            return Err(format!(
                "reproducibility mismatch at index {idx}: {} vs {}",
                lhs.case_id, rhs.case_id
            ));
        }
    }
    Ok(())
}

pub fn check_diversity_sanity(cases: &[TestCase]) -> Result<(), Vec<String>> {
    let min_unique: BTreeMap<&str, usize> =
        BTreeMap::from([("A", 5), ("B", 8), ("C", 5), ("D", 5), ("E", 3), ("F", 3)]);
    let mut uniq = BTreeMap::<String, BTreeSet<String>>::new();
    for c in cases {
        uniq.entry(c.category.clone())
            .or_default()
            .insert(sha256_hex(c.input_text.as_bytes()));
    }
    let mut errors = Vec::new();
    for (cat, min_required) in min_unique {
        let got = uniq.get(cat).map(|s| s.len()).unwrap_or(0);
        if got < min_required {
            errors.push(format!(
                "diversity below threshold for {cat}: got {got}, required {min_required}"
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn generate_input_text(seed: u64, category: &str, index: usize) -> String {
    let themes: &[&str] = match category {
        "A" => &[
            "顧客体験を改善するため、応答時間を200ms以内に維持する",
            "主要APIのSLOを99.95%に設定し、監視を強化する",
            "認証基盤を統合して運用負荷を下げる",
            "障害時の自動復旧時間を5分以内に短縮する",
            "継続的デリバリーの失敗率を低減する",
            "重要フローの観測性を標準化する",
        ],
        "B" => &[
            "検索性能を改善するためインデックス更新戦略を見直す",
            "キャッシュヒット率を向上するためTTLを最適化する",
            "バッチ処理のピーク負荷を平準化する",
            "セキュリティ要件を満たしつつデプロイ速度を確保する",
            "障害検知の誤検知率を低減する",
            "テスト実行時間を短縮しリリース頻度を上げる",
            "データ同期の整合性を維持しながら遅延を下げる",
            "監査ログの網羅性を保ってコストを抑える",
            "複数チームの開発ルールを共通化する",
        ],
        "C" => &[
            "要件の優先度が曖昧なため、判断基準を明確化したい",
            "クラウド依存の扱いを明文化しないと実装方針が揺れる",
            "性能目標とコスト制約のトレードオフを整理したい",
            "境界条件が不明確で再設計の可能性がある",
            "運用時の責任分界を定義したい",
            "品質指標の定義粒度を揃えたい",
        ],
        "D" => &[
            "依存関係が複雑で変更影響範囲が読みにくい",
            "テスト戦略が不足し回帰リスクが高い",
            "設計意図が文書化されておらず保守困難",
            "監視アラートの設計が一貫していない",
            "障害時の手順が属人化している",
            "CI失敗時の原因分類が曖昧で復旧が遅い",
        ],
        "E" => &[
            "仕様が断片的で全体整合が取れていない",
            "依存ルールが未定義で実装が衝突しやすい",
            "品質基準がなく判断が担当者依存になる",
            "要求変更時の評価手順が不足している",
        ],
        "F" => &[
            "目的と制約が未定義で設計検討が進められない",
            "要求の前提条件が曖昧で意思決定できない",
            "依存先システム情報が不足している",
            "品質目標が合意されていない",
        ],
        _ => &["定義外カテゴリ"],
    };

    let theme_idx = deterministic_index(seed, category, index, themes.len());
    let p1 = ["高", "中", "低"][deterministic_index(seed + 13, category, index, 3)];
    let p2 = ["短期", "中期", "長期"][deterministic_index(seed + 29, category, index, 3)];
    format!(
        "{}。優先度={}。評価期間={}。ケース{}。",
        themes[theme_idx], p1, p2, index
    )
}

fn deterministic_index(seed: u64, category: &str, index: usize, modulo: usize) -> usize {
    if modulo == 0 {
        return 0;
    }
    let cat_fold = category
        .as_bytes()
        .iter()
        .fold(0u64, |acc, &b| acc.wrapping_mul(131).wrapping_add(b as u64));
    let x = splitmix64(seed ^ cat_fold ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    (x as usize) % modulo
}

fn dataset_hash(cases: &[TestCase]) -> String {
    let mut bytes = Vec::<u8>::new();
    for c in cases {
        bytes.extend_from_slice(c.case_id.as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(c.category.as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(c.input_text.as_bytes());
        bytes.push(b'\n');
    }
    sha256_hex(&bytes)
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    format!("{:x}", hasher.finalize())
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t0_1_category_distribution_test() {
        let cases = generate_cases(42);
        let summary = validate_cases(&cases, 42).expect("integrity should pass");
        assert_eq!(summary.total_cases, 100);
        assert_eq!(summary.category_counts.get("A").copied().unwrap_or(0), 20);
        assert_eq!(summary.category_counts.get("B").copied().unwrap_or(0), 25);
        assert_eq!(summary.category_counts.get("C").copied().unwrap_or(0), 20);
        assert_eq!(summary.category_counts.get("D").copied().unwrap_or(0), 15);
        assert_eq!(summary.category_counts.get("E").copied().unwrap_or(0), 10);
        assert_eq!(summary.category_counts.get("F").copied().unwrap_or(0), 10);
    }

    #[test]
    fn t0_2_case_id_uniqueness_test() {
        let cases = generate_cases(42);
        let ids = cases
            .iter()
            .map(|c| c.case_id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), cases.len());
    }

    #[test]
    fn t0_3_non_empty_input_test() {
        let cases = generate_cases(42);
        assert!(cases.iter().all(|c| !c.input_text.trim().is_empty()));
    }

    #[test]
    fn t0_4_seed_reproducibility_test() {
        verify_seed_reproducibility(42).expect("seed reproducibility should pass");
    }

    #[test]
    fn t0_5_diversity_sanity_test() {
        let cases = generate_cases(42);
        check_diversity_sanity(&cases).expect("diversity sanity should pass");
    }
}
