import csv
import math
import os
import shutil
import statistics
import subprocess
from collections import defaultdict

import numpy as np


DEPTH = 50
BEAMS = [5, 8, 12]
SEED = 42
NORM_ALPHA = 0.25
CATEGORY_ALPHA = 3.0
OUT_DIR = "report/phase6_collapse_fix"

MODES = [
    ("A_baseline_off", "off"),
    ("B_v6_0_contractive", "v6.0"),
    ("C_v6_1_repulsive", "v6.1"),
]


def run_one(mode_name: str, mode_env: str, beam: int):
    mode_dir = os.path.join(OUT_DIR, mode_name)
    os.makedirs(mode_dir, exist_ok=True)
    raw_csv = os.path.join(mode_dir, f"raw_objectives_beam{beam}.csv")
    trace_csv = os.path.join(mode_dir, f"trace_beam{beam}.csv")
    if os.path.exists(raw_csv):
        os.remove(raw_csv)
    if os.path.exists(trace_csv):
        os.remove(trace_csv)

    env = os.environ.copy()
    env["PHASE6_MEMORY_MODE"] = mode_env
    cmd = [
        "cargo",
        "run",
        "--release",
        "-p",
        "design_cli",
        "--",
        "--trace",
        "--trace-depth",
        str(DEPTH),
        "--trace-beam",
        str(beam),
        "--seed",
        str(SEED),
        "--norm-alpha",
        str(NORM_ALPHA),
        "--baseline-off",
        "--category-soft",
        "--category-alpha",
        str(CATEGORY_ALPHA),
        "--trace-output",
        trace_csv,
        "--raw-trace-output",
        raw_csv,
    ]
    print("Running:", " ".join(cmd), f"(PHASE6_MEMORY_MODE={mode_env})")
    result = subprocess.run(cmd, capture_output=True, text=True, env=env)
    if result.returncode != 0:
        print(result.stderr)
        raise RuntimeError(f"trace generation failed for mode={mode_name}, beam={beam}")
    return raw_csv, trace_csv


def rankdata(values):
    pairs = sorted((v, i) for i, v in enumerate(values))
    ranks = [0.0] * len(values)
    i = 0
    while i < len(pairs):
        j = i
        while j + 1 < len(pairs) and pairs[j + 1][0] == pairs[i][0]:
            j += 1
        avg_rank = (i + j + 2) / 2.0
        for k in range(i, j + 1):
            ranks[pairs[k][1]] = avg_rank
        i = j + 1
    return ranks


def pearson_corr(x, y):
    if len(x) < 2:
        return 0.0
    mx = statistics.mean(x)
    my = statistics.mean(y)
    dx = [v - mx for v in x]
    dy = [v - my for v in y]
    num = sum(a * b for a, b in zip(dx, dy))
    denx = math.sqrt(sum(a * a for a in dx))
    deny = math.sqrt(sum(b * b for b in dy))
    if denx == 0.0 or deny == 0.0:
        return 0.0
    return num / (denx * deny)


def spearman_corr(x, y):
    return pearson_corr(rankdata(x), rankdata(y))


def mad(values):
    if not values:
        return 0.0
    med = statistics.median(values)
    return statistics.median([abs(v - med) for v in values])


def load_depth_groups(path):
    grouped = defaultdict(list)
    with open(path, "r", newline="") as f:
        reader = csv.DictReader(f)
        o3_col = "objective_3_shape" if "objective_3_shape" in reader.fieldnames else "objective_3"
        for row in reader:
            depth = int(row["depth"])
            grouped[depth].append(
                [
                    float(row["objective_0"]),
                    float(row["objective_1"]),
                    float(row["objective_2"]),
                    float(row[o3_col]),
                ]
            )
    return grouped


def unique_distance_ratio(mat):
    n = mat.shape[0]
    pairs = n * (n - 1) // 2
    if pairs == 0:
        return 0.0
    dists = []
    for i in range(n):
        for j in range(i + 1, n):
            d = float(np.linalg.norm(mat[i] - mat[j]))
            dists.append(round(d, 12))
    return len(set(dists)) / pairs


def analyze_raw(path):
    grouped = load_depth_groups(path)
    depths = [d for d in range(1, DEPTH + 1) if d in grouped]
    u0_eq_u3 = []
    rho_list = []
    mad0_o1 = []
    mad0_o2 = []
    ranks = []
    unique_ratios = []
    n_list = []
    u0_levels = []
    tie_rate = []

    for d in depths:
        rows = grouped[d]
        mat = np.array(rows, dtype=float)
        n = mat.shape[0]
        n_list.append(n)

        vals0 = mat[:, 0].tolist()
        vals1 = mat[:, 1].tolist()
        vals2 = mat[:, 2].tolist()
        vals3 = mat[:, 3].tolist()

        u0_count = len(set(vals0))
        u3_count = len(set(vals3))
        u0_eq_u3.append(1 if u0_count == u3_count else 0)
        rho_list.append(spearman_corr(vals0, vals3))
        mad0_o1.append(1 if mad(vals1) == 0.0 else 0)
        mad0_o2.append(1 if mad(vals2) == 0.0 else 0)

        if n >= 2:
            cov = np.cov(mat, rowvar=False, bias=False)
            rank = int(np.linalg.matrix_rank(cov, tol=1e-12))
        else:
            rank = 0
        ranks.append(rank)

        unique_ratios.append(unique_distance_ratio(mat))
        u0_levels.append(u0_count)
        tie_rate.append(1.0 - (u0_count / n if n > 0 else 0.0))

    return {
        "depth_count": len(depths),
        "u0_eq_u3_ratio": statistics.mean(u0_eq_u3) if u0_eq_u3 else 0.0,
        "spearman_median": statistics.median(rho_list) if rho_list else 0.0,
        "spearman_mean": statistics.mean(rho_list) if rho_list else 0.0,
        "mad0_o1_ratio": statistics.mean(mad0_o1) if mad0_o1 else 0.0,
        "mad0_o2_ratio": statistics.mean(mad0_o2) if mad0_o2 else 0.0,
        "avg_rank": statistics.mean(ranks) if ranks else 0.0,
        "avg_unique_distance_ratio": statistics.mean(unique_ratios) if unique_ratios else 0.0,
        "avg_tie_rate": statistics.mean(tie_rate) if tie_rate else 0.0,
        "avg_n": statistics.mean(n_list) if n_list else 0.0,
    }


def analyze_trace_metrics(path):
    tau = []
    delta = []
    hit = []
    with open(path, "r", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            tau.append(float(row.get("avg_tau_mem", "0") or 0.0))
            delta.append(float(row.get("avg_delta_norm", "0") or 0.0))
            hit.append(float(row.get("memory_hit_rate", "0") or 0.0))
    return {
        "avg_tau_mem": statistics.mean(tau) if tau else 0.0,
        "avg_delta_norm": statistics.mean(delta) if delta else 0.0,
        "memory_hit_rate": statistics.mean(hit) if hit else 0.0,
    }


def gate_pass(metrics_by_beam):
    b5 = metrics_by_beam[5]
    b12 = metrics_by_beam[12]
    all_beams = list(metrics_by_beam.values())
    return (
        b5["avg_unique_distance_ratio"] >= 0.30
        and b12["avg_unique_distance_ratio"] >= 0.20
        and min(m["avg_rank"] for m in all_beams) >= 3.0
        and max(m["u0_eq_u3_ratio"] for m in all_beams) <= 0.20
    )


def main():
    if os.path.exists(OUT_DIR):
        shutil.rmtree(OUT_DIR)
    os.makedirs(OUT_DIR, exist_ok=True)

    summary_rows = []
    mode_aggregate = []

    for mode_name, mode_env in MODES:
        metrics_by_beam = {}
        mem_metrics_by_beam = {}
        for beam in BEAMS:
            raw_csv, trace_csv = run_one(mode_name, mode_env, beam)
            raw_metrics = analyze_raw(raw_csv)
            mem_metrics = analyze_trace_metrics(trace_csv)
            merged = dict(raw_metrics)
            merged.update(mem_metrics)
            metrics_by_beam[beam] = merged
            mem_metrics_by_beam[beam] = mem_metrics
            row = {"mode": mode_name, "beam": beam}
            row.update(merged)
            summary_rows.append(row)

        gate = gate_pass(metrics_by_beam) if mode_name == "C_v6_1_repulsive" else None
        mode_aggregate.append(
            {
                "mode": mode_name,
                "gate_pass": gate if gate is not None else "",
                "avg_unique_distance_ratio_beam5": metrics_by_beam[5]["avg_unique_distance_ratio"],
                "avg_unique_distance_ratio_beam12": metrics_by_beam[12]["avg_unique_distance_ratio"],
                "avg_rank_min": min(metrics_by_beam[b]["avg_rank"] for b in BEAMS),
                "mad0_o1_max": max(metrics_by_beam[b]["mad0_o1_ratio"] for b in BEAMS),
                "mad0_o2_max": max(metrics_by_beam[b]["mad0_o2_ratio"] for b in BEAMS),
                "u0_eq_u3_max": max(metrics_by_beam[b]["u0_eq_u3_ratio"] for b in BEAMS),
                "avg_tau_mem": statistics.mean(
                    [mem_metrics_by_beam[b]["avg_tau_mem"] for b in BEAMS]
                ),
                "avg_delta_norm": statistics.mean(
                    [mem_metrics_by_beam[b]["avg_delta_norm"] for b in BEAMS]
                ),
                "memory_hit_rate": statistics.mean(
                    [mem_metrics_by_beam[b]["memory_hit_rate"] for b in BEAMS]
                ),
            }
        )

    summary_csv = os.path.join(OUT_DIR, "phase6_collapse_fix_summary.csv")
    with open(summary_csv, "w", newline="") as f:
        fields = [
            "mode",
            "beam",
            "depth_count",
            "u0_eq_u3_ratio",
            "spearman_median",
            "spearman_mean",
            "mad0_o1_ratio",
            "mad0_o2_ratio",
            "avg_rank",
            "avg_unique_distance_ratio",
            "avg_tie_rate",
            "avg_n",
            "avg_tau_mem",
            "avg_delta_norm",
            "memory_hit_rate",
        ]
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        writer.writerows(summary_rows)

    gate_csv = os.path.join(OUT_DIR, "phase6_collapse_fix_gate.csv")
    with open(gate_csv, "w", newline="") as f:
        fields = [
            "mode",
            "gate_pass",
            "avg_unique_distance_ratio_beam5",
            "avg_unique_distance_ratio_beam12",
            "avg_rank_min",
            "mad0_o1_max",
            "mad0_o2_max",
            "u0_eq_u3_max",
            "avg_tau_mem",
            "avg_delta_norm",
            "memory_hit_rate",
        ]
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        writer.writerows(mode_aggregate)

    print(f"Saved summary CSV: {summary_csv}")
    print(f"Saved gate CSV   : {gate_csv}")


if __name__ == "__main__":
    main()
