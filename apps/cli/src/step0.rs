use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::Serialize;

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
        errors.push(format!("N mismatch: expected {EXPECTED_TOTAL}, got {}", cases.len()));
    }

    let mut counts = BTreeMap::<String, usize>::new();
    for c in cases {
        *counts.entry(c.category.clone()).or_insert(0) += 1;
    }
    for (cat, expected) in EXPECTED_DISTRIBUTION {
        let got = counts.get(cat).copied().unwrap_or(0);
        if got != expected {
            errors.push(format!("category {cat} mismatch: expected {expected}, got {got}"));
        }
    }
    for cat in counts.keys() {
        if !EXPECTED_DISTRIBUTION.iter().any(|(k, _)| k == &cat.as_str()) {
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
        let h1 = sha256_hex(format!("{}|{}|{}", lhs.category, lhs.case_id, lhs.input_text).as_bytes());
        let h2 = sha256_hex(format!("{}|{}|{}", rhs.category, rhs.case_id, rhs.input_text).as_bytes());
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
    let min_unique: BTreeMap<&str, usize> = BTreeMap::from([
        ("A", 5),
        ("B", 8),
        ("C", 5),
        ("D", 5),
        ("E", 3),
        ("F", 3),
    ]);
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
    let digest = sha256_bytes(input);
    hex_from_bytes(&digest)
}

fn hex_from_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn sha256_bytes(message: &[u8]) -> [u8; 32] {
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98,
        0x71374491,
        0xb5c0fbcf,
        0xe9b5dba5,
        0x3956c25b,
        0x59f111f1,
        0x923f82a4,
        0xab1c5ed5,
        0xd807aa98,
        0x12835b01,
        0x243185be,
        0x550c7dc3,
        0x72be5d74,
        0x80deb1fe,
        0x9bdc06a7,
        0xc19bf174,
        0xe49b69c1,
        0xefbe4786,
        0x0fc19dc6,
        0x240ca1cc,
        0x2de92c6f,
        0x4a7484aa,
        0x5cb0a9dc,
        0x76f988da,
        0x983e5152,
        0xa831c66d,
        0xb00327c8,
        0xbf597fc7,
        0xc6e00bf3,
        0xd5a79147,
        0x06ca6351,
        0x14292967,
        0x27b70a85,
        0x2e1b2138,
        0x4d2c6dfc,
        0x53380d13,
        0x650a7354,
        0x766a0abb,
        0x81c2c92e,
        0x92722c85,
        0xa2bfe8a1,
        0xa81a664b,
        0xc24b8b70,
        0xc76c51a3,
        0xd192e819,
        0xd6990624,
        0xf40e3585,
        0x106aa070,
        0x19a4c116,
        0x1e376c08,
        0x2748774c,
        0x34b0bcb5,
        0x391c0cb3,
        0x4ed8aa4a,
        0x5b9cca4f,
        0x682e6ff3,
        0x748f82ee,
        0x78a5636f,
        0x84c87814,
        0x8cc70208,
        0x90befffa,
        0xa4506ceb,
        0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (message.len() as u64) * 8;
    let mut padded = message.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = H0;
    let mut w = [0u32; 64];
    for chunk in padded.chunks_exact(64) {
        for (i, bytes) in chunk.chunks_exact(4).take(16).enumerate() {
            w[i] = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16]
                .wrapping_add(s0)
                .wrapping_add(w[t - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for t in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[t])
                .wrapping_add(w[t]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        let b = word.to_be_bytes();
        out[i * 4..i * 4 + 4].copy_from_slice(&b);
    }
    out
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
        let ids = cases.iter().map(|c| c.case_id.clone()).collect::<BTreeSet<_>>();
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
