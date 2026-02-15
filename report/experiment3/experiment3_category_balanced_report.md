# 実験3 検証レポート（カテゴリ均等展開）

## 実施内容
- 変更点:
  - `applicable_rules()` 直後にカテゴリ均等サンプラを適用
  - `m=1` でカテゴリごとに最低1本選択
  - Memory系OFF条件は実験2と同一維持
- OFF維持:
  - DHM OFF
  - Field距離制限 OFF
  - Diversity Pressure OFF
- ON維持:
  - SHM/CHM, Objective評価, Pareto, Beam, λ制御

## 実行コマンド
```bash
cargo run -p design_cli --release -- --trace --baseline-off --category-balanced --category-m 1 --trace-depth 100 --trace-beam 5 --trace-output trace_depth100_experiment3_category_balanced.csv
cargo run -p design_cli --release -- --bench --baseline-off --category-balanced --category-m 1 --bench-depth 100 --bench-beam 5 --bench-iter 20 --bench-warmup 3 > bench_depth100_experiment3_category_balanced.txt
```

## 指標結果
- diversity_mean: `0.060964541`
- diversity_min: `0.000000000`
- pareto_mean: `11.940000000`
- lambda_variance: `0.025713207`
- avg_total_ms: `26.871`
- avg_pareto_us: `6.238`

追加ログ:
- expanded_categories_count_mean: `6.000000000`
- selected_rules_count_mean: `28.020000000`
- per_category_selected の代表パターン:
  - `ConstraintPropagation:5|Cost:5|Performance:5|Refactor:5|Reliability:5|Structural:5`（100depth中89回）

## 判定
- 成功基準: `diversity_mean >= 0.01`
- 実測: `0.060964541`
- 判定: **成功（生成偏りが主因）**

## 比較（実験2 Baseline）
- 実験2 diversity_mean: `0.002648081`
- 実験3 diversity_mean: `0.060964541`
- 差分: `+0.058316460`

## 生成物
- trace: `trace_depth100_experiment3_category_balanced.csv`
- bench: `bench_depth100_experiment3_category_balanced.txt`
- metrics: `experiment3_category_balanced_metrics.csv`
