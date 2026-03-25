//! similarity — 記憶間の類似度計算

use std::collections::BTreeSet;

/// タグの Jaccard 類似度を計算する。
///
/// |A ∩ B| / |A ∪ B| を返す。両方空の場合は 1.0。
pub fn jaccard_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let a_set: BTreeSet<String> = a.iter().map(|s| s.to_ascii_lowercase()).collect();
    let b_set: BTreeSet<String> = b.iter().map(|s| s.to_ascii_lowercase()).collect();
    let intersection = a_set.intersection(&b_set).count() as f32;
    let union = a_set.union(&b_set).count() as f32;
    if union == 0.0 { 0.0 } else { intersection / union }
}

/// ベクトルのコサイン類似度を計算する。
///
/// 長さが異なる場合や空の場合は 0.0 を返す。結果は [0, 1] にクランプされる。
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < f32::EPSILON || norm_b < f32::EPSILON {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
}

/// 2 つの汎化記憶間の複合類似度を計算する。
///
/// combined = 0.4 × jaccard(tags) + 0.6 × cosine(embedding)
pub fn combined_similarity(
    a_tags: &[String],
    a_embed: &[f32],
    b_tags: &[String],
    b_embed: &[f32],
) -> f32 {
    let tag_sim = jaccard_similarity(a_tags, b_tags);
    let embed_sim = cosine_similarity(a_embed, b_embed);
    0.4 * tag_sim + 0.6 * embed_sim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jaccard_identical() {
        let a = vec!["foo".to_string(), "bar".to_string()];
        assert!((jaccard_similarity(&a, &a) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn jaccard_disjoint() {
        let a = vec!["foo".to_string()];
        let b = vec!["bar".to_string()];
        assert!((jaccard_similarity(&a, &b)).abs() < 1e-5);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let a = vec!["a".to_string(), "b".to_string()];
        let b = vec!["b".to_string(), "c".to_string()];
        // intersection=1, union=3
        assert!((jaccard_similarity(&a, &b) - 1.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_identical() {
        let v = vec![1.0, 0.0, 1.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-5);
    }

    #[test]
    fn cosine_length_mismatch_returns_zero() {
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }
}
