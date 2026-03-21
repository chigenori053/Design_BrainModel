#!/usr/bin/env python3
"""
Operational Phase — KPI Aggregation & Failure Analysis

Usage:
    python3 scripts/aggregate.py [log_dir] [--verbose] [--failures]

Examples:
    python3 scripts/aggregate.py logs/phase_a
    python3 scripts/aggregate.py logs/phase_b --failures
    python3 scripts/aggregate.py logs/          --verbose
"""

from __future__ import annotations

import argparse
import glob
import json
import math
import sys
from collections import Counter
from pathlib import Path


# ── CLI ────────────────────────────────────────────────────────────────────────

def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Aggregate operational run logs")
    p.add_argument("log_dir", nargs="?", default="logs", help="Directory containing *.json logs")
    p.add_argument("--verbose", "-v", action="store_true", help="Show per-run details")
    p.add_argument("--failures", "-f", action="store_true", help="Show failure breakdown")
    p.add_argument("--anomalies", "-a", action="store_true", help="Show anomaly details")
    return p.parse_args()


# ── KPI targets (from spec section 3) ─────────────────────────────────────────

KPI_SUCCESS_RATE_MIN  = 0.70
KPI_LATENCY_MAX_MS    = 1000
KPI_RECALL_RATE_MIN   = 0.60


# ── Aggregation ────────────────────────────────────────────────────────────────

def load_logs(log_dir: str) -> list[dict]:
    pattern = str(Path(log_dir) / "**" / "*.json")
    files = glob.glob(pattern, recursive=True)
    if not files:
        print(f"No JSON files found in {log_dir}", file=sys.stderr)
        sys.exit(1)

    logs = []
    for f in sorted(files):
        try:
            with open(f) as fh:
                logs.append(json.load(fh))
        except Exception as e:
            print(f"warn: cannot load {f}: {e}", file=sys.stderr)
    return logs


def percentile(data: list[float], p: float) -> float:
    if not data:
        return 0.0
    sorted_data = sorted(data)
    idx = (len(sorted_data) - 1) * p / 100
    lo, hi = int(idx), min(int(idx) + 1, len(sorted_data) - 1)
    return sorted_data[lo] + (sorted_data[hi] - sorted_data[lo]) * (idx - lo)


def aggregate(logs: list[dict]) -> None:
    total = len(logs)

    latencies: list[float]        = []
    nodes_explored: list[int]     = []
    recall_rates: list[float]     = []
    beam_avgs: list[float]        = []
    prune_rates: list[float]      = []
    success_count                  = 0
    failure_types: Counter        = Counter()
    anomaly_latency_count          = 0
    anomaly_explosion_count        = 0
    anomaly_recall_low_count       = 0

    for log in logs:
        metrics = log.get("metrics", {})
        trace_stats = log.get("trace_stats") or {}
        trace = log.get("trace") or {}
        anomalies = log.get("anomalies", {})
        failure = log.get("failure")

        if metrics.get("success"):
            success_count += 1

        lat = metrics.get("latency_ms")
        if lat is not None:
            latencies.append(float(lat))

        nodes = metrics.get("nodes_explored", 0)
        nodes_explored.append(int(nodes))

        recall = trace_stats.get("recall_hit_rate")
        if recall is not None:
            recall_rates.append(float(recall))

        beam = metrics.get("beam_avg", 0)
        if beam:
            beam_avgs.append(float(beam))

        # Prune rate: avg across steps
        steps = trace.get("steps", [])
        if steps:
            total_cand = sum(s.get("candidates", 0) for s in steps)
            total_pruned = sum(s.get("pruned", 0) for s in steps)
            if total_cand > 0:
                prune_rates.append(total_pruned / total_cand)

        if failure:
            failure_types[failure.get("failure_type", "unknown")] += 1

        if anomalies.get("latency_spike"):
            anomaly_latency_count += 1
        if anomalies.get("exploration_explosion"):
            anomaly_explosion_count += 1
        if anomalies.get("recall_low"):
            anomaly_recall_low_count += 1

    # ── KPI output ───────────────────────────────────────────────────────────

    success_rate = success_count / total if total else 0.0
    avg_latency  = sum(latencies) / len(latencies) if latencies else 0.0
    p50_latency  = percentile(latencies, 50)
    p95_latency  = percentile(latencies, 95)
    avg_recall   = sum(recall_rates) / len(recall_rates) if recall_rates else 0.0
    avg_nodes    = sum(nodes_explored) / len(nodes_explored) if nodes_explored else 0.0
    avg_beam     = sum(beam_avgs) / len(beam_avgs) if beam_avgs else 0.0
    avg_prune    = sum(prune_rates) / len(prune_rates) if prune_rates else 0.0

    ok = lambda val, target, higher_better=True: (
        "✓" if (val >= target if higher_better else val <= target) else "✗"
    )

    print("=" * 56)
    print("  Operational Phase — KPI Report")
    print("=" * 56)
    print(f"  Runs total      : {total}")
    print(f"  Runs success    : {success_count}")
    print()
    print("  ── Core KPIs ──────────────────────────────────────")
    print(f"  success_rate    : {success_rate:.3f}  {ok(success_rate, KPI_SUCCESS_RATE_MIN)}  (target ≥ {KPI_SUCCESS_RATE_MIN})")
    print(f"  latency avg ms  : {avg_latency:7.1f}  {ok(avg_latency, KPI_LATENCY_MAX_MS, False)}  (target < {KPI_LATENCY_MAX_MS})")
    print(f"  latency p50     : {p50_latency:7.1f}")
    print(f"  latency p95     : {p95_latency:7.1f}")
    print(f"  recall_hit_rate : {avg_recall:.3f}  {ok(avg_recall, KPI_RECALL_RATE_MIN)}  (target ≥ {KPI_RECALL_RATE_MIN})")
    print()
    print("  ── Supplementary KPIs ──────────────────────────────")
    print(f"  nodes_explored  : {avg_nodes:7.1f}  avg")
    print(f"  beam_avg        : {avg_beam:7.2f}")
    print(f"  prune_rate      : {avg_prune:.3f}")
    print()
    print("  ── Anomalies ────────────────────────────────────────")
    print(f"  latency_spike   : {anomaly_latency_count:4d}  ({anomaly_latency_count/total*100:.1f}%)")
    print(f"  expl_explosion  : {anomaly_explosion_count:4d}  ({anomaly_explosion_count/total*100:.1f}%)")
    print(f"  recall_low      : {anomaly_recall_low_count:4d}  ({anomaly_recall_low_count/total*100:.1f}%)")
    print("=" * 56)

    return {
        "total": total,
        "success_rate": success_rate,
        "avg_latency": avg_latency,
        "avg_recall": avg_recall,
        "failure_types": dict(failure_types),
        "anomaly_latency_count": anomaly_latency_count,
        "anomaly_explosion_count": anomaly_explosion_count,
    }


def show_failures(logs: list[dict]) -> None:
    failures = [l for l in logs if l.get("failure")]
    if not failures:
        print("  No failures recorded.")
        return

    by_type: dict[str, list[dict]] = {}
    for log in failures:
        ft = log["failure"].get("failure_type", "unknown")
        by_type.setdefault(ft, []).append(log)

    print(f"\n  ── Failure Breakdown ({len(failures)} total) ──────────────────")
    for ftype, items in sorted(by_type.items()):
        print(f"  {ftype:25s} : {len(items):4d}")
    print()

    for ftype, items in sorted(by_type.items()):
        print(f"  [{ftype}]")
        for item in items[:3]:  # show first 3 examples
            inp = item.get("input", "")[:60]
            actual = item["failure"].get("actual", "")[:80]
            print(f"    input  : {inp}")
            print(f"    actual : {actual}")
            print()


def show_per_run(logs: list[dict]) -> None:
    print(f"\n  {'#':>5}  {'success':>8}  {'latency_ms':>10}  {'nodes':>6}  {'recall':>7}  request_id")
    print(f"  {'-'*5}  {'-'*8}  {'-'*10}  {'-'*6}  {'-'*7}  {'-'*20}")
    for i, log in enumerate(logs, 1):
        m = log.get("metrics", {})
        ts = log.get("trace_stats") or {}
        print(
            f"  {i:5d}  "
            f"{'yes' if m.get('success') else 'no':>8}  "
            f"{m.get('latency_ms', 0):10.0f}  "
            f"{m.get('nodes_explored', 0):6d}  "
            f"{ts.get('recall_hit_rate', 0):7.3f}  "
            f"{log.get('request_id', '')[:24]}"
        )


# ── Entry ─────────────────────────────────────────────────────────────────────

def main() -> None:
    args = parse_args()
    logs = load_logs(args.log_dir)
    aggregate(logs)

    if args.failures:
        show_failures(logs)

    if args.verbose:
        show_per_run(logs)


if __name__ == "__main__":
    main()
