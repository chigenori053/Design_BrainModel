# 実験4 検証レポート（Soft Category Balancing）

## 仕様反映
- 追加CLI:
  - `--category-soft`
  - `--category-alpha` (default: 3.0)
  - `--temperature` (default: 0.8)
  - `--entropy-beta` (default: 0.02)
- 追加traceログ:
  - `entropy_per_depth`
  - `unique_category_count_per_depth`
  - `pareto_front_size_per_depth`
  - 既存の `expanded_categories_count`, `selected_rules_count`, `per_category_selected`

## 実行コマンド
```bash
cargo run -p design_cli --release -- \
  --trace --baseline-off --category-soft \
  --category-alpha 3.0 --temperature 0.8 --entropy-beta 0.02 \
  --trace-depth 100 --trace-beam 5 \
  --trace-output report/trace_depth100_experiment4_category_soft.csv

cargo run -p design_cli --release -- \
  --bench --baseline-off --category-soft \
  --category-alpha 3.0 --temperature 0.8 --entropy-beta 0.02 \
  --bench-depth 100 --bench-beam 5 --bench-iter 20 --bench-warmup 3 \
  > report/bench_depth100_experiment4_category_soft.txt
```

## 実測結果
- diversity_mean: `0.000005394`
- diversity_min: `0.000000000`
- pareto_mean: `4.910000000`
- lambda_variance: `0.025418750`
- avg_total_ms: `12.060`
- avg_pareto_us: `2.787`

追加診断:
- entropy_per_depth_mean: `1.332179070`
- unique_category_count_per_depth_mean: `4.000000000`
- zero-diversity depth 数: `95`
- zero-diversity depth の entropy 平均: `1.332179070`
- zero-diversity depth の pareto_front_size 平均: `4.957894737`

## 判定
- 成功基準: diversity_mean >= 0.01
- 実測: `0.000005394`
- 判定: **FAIL**

## 所見
- entropy はゼロではなく（約1.33）、カテゴリ単一化は主因ではありません。
- 一方で diversity はほぼ0で、zero-diversity depth が95/100。
- この条件では、カテゴリ分散はある程度維持されても、目的空間での状態差が潰れて pareto 側で実質同質化している挙動です。
- ハード均等（実験3: diversity_mean=`0.060964541`）と比べ、Soft設定（alpha=3.0, T=0.8, beta=0.02）は探索多様性を維持できませんでした。

## 成果物
- trace: `report/trace_depth100_experiment4_category_soft.csv`
- bench: `report/bench_depth100_experiment4_category_soft.txt`
- metrics: `report/experiment4_category_soft_metrics.csv`
- report: `report/experiment4_category_soft_report.md`
