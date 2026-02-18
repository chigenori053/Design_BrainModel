import csv
import math
import os
import statistics
import subprocess
import sys
from collections import defaultdict

import numpy as np


DEPTH = 50
BEAMS = [5, 8, 12]
SEED = 42
NORM_ALPHA = 0.25
CATEGORY_ALPHA = 3.0
OUT_DIR = "report/phase3_post_collapse_validation"


def run_raw_trace(beam: int, output_csv: str) -> None:
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
        "--raw-trace-output",
        output_csv,
    ]
    print("Running:", " ".join(cmd))
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(result.stderr)
        raise RuntimeError(f"trace generation failed for beam={beam}")


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
    rx = rankdata(x)
    ry = rankdata(y)
    return pearson_corr(rx, ry)


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


def analyze_one_beam(path):
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
    front_thickness = []

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
        front_thickness.append(float(np.max(mat[:, 0]) - np.min(mat[:, 0])) if n > 0 else 0.0)

    mean_rho = statistics.mean(rho_list) if rho_list else 0.0
    med_rho = statistics.median(rho_list) if rho_list else 0.0

    return {
        "depth_count": len(depths),
        "u0_eq_u3_ratio": statistics.mean(u0_eq_u3) if u0_eq_u3 else 0.0,
        "spearman_median": med_rho,
        "spearman_mean": mean_rho,
        "mad0_o1_ratio": statistics.mean(mad0_o1) if mad0_o1 else 0.0,
        "mad0_o2_ratio": statistics.mean(mad0_o2) if mad0_o2 else 0.0,
        "avg_rank": statistics.mean(ranks) if ranks else 0.0,
        "avg_unique_distance_ratio": statistics.mean(unique_ratios) if unique_ratios else 0.0,
        "avg_n": statistics.mean(n_list) if n_list else 0.0,
        "avg_u0_levels": statistics.mean(u0_levels) if u0_levels else 0.0,
        "avg_u0_over_n": statistics.mean(
            [(u / n if n > 0 else 0.0) for u, n in zip(u0_levels, n_list)]
        )
        if n_list
        else 0.0,
        "avg_tie_rate": statistics.mean(tie_rate) if tie_rate else 0.0,
        "avg_front_thickness": statistics.mean(front_thickness) if front_thickness else 0.0,
    }


def write_summary(metrics_by_beam):
    summary_csv = os.path.join(OUT_DIR, "phase3_post_collapse_summary.csv")
    with open(summary_csv, "w", newline="") as f:
        fields = [
            "beam",
            "depth_count",
            "u0_eq_u3_ratio",
            "spearman_median",
            "spearman_mean",
            "mad0_o1_ratio",
            "mad0_o2_ratio",
            "avg_rank",
            "avg_unique_distance_ratio",
            "avg_n",
            "avg_u0_levels",
            "avg_u0_over_n",
            "avg_tie_rate",
            "avg_front_thickness",
        ]
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        for beam in sorted(metrics_by_beam):
            row = {"beam": beam}
            row.update(metrics_by_beam[beam])
            writer.writerow(row)
    return summary_csv


def write_report(metrics_by_beam):
    b5 = metrics_by_beam[5]
    b8 = metrics_by_beam[8]
    b12 = metrics_by_beam[12]

    beam12_u0_increase = b12["avg_u0_levels"] > b8["avg_u0_levels"]
    unique_ratio_trend = (
        b5["avg_unique_distance_ratio"],
        b8["avg_unique_distance_ratio"],
        b12["avg_unique_distance_ratio"],
    )
    unique_ratio_down = unique_ratio_trend[2] < unique_ratio_trend[1] < unique_ratio_trend[0]

    collapse_flags = []
    for beam, m in metrics_by_beam.items():
        flag = (
            m["u0_eq_u3_ratio"] > 0.95
            and (m["mad0_o1_ratio"] > 0.20 or m["mad0_o2_ratio"] > 0.20)
        )
        collapse_flags.append((beam, flag))

    md_path = os.path.join(OUT_DIR, "phase3_post_collapse_report.md")
    with open(md_path, "w") as f:
        f.write("# Phase3 Post Validation (depth=50, same seed)\n\n")
        f.write("## Conditions\n")
        f.write(f"- depth: {DEPTH}\n")
        f.write(f"- beams: {BEAMS}\n")
        f.write(f"- seed (fixed): {SEED}\n")
        f.write(f"- norm-alpha (fixed): {NORM_ALPHA}\n")
        f.write(f"- category-alpha (fixed): {CATEGORY_ALPHA}\n")
        f.write("- filters: `--baseline-off --category-soft`\n\n")

        for beam in [5, 8, 12]:
            m = metrics_by_beam[beam]
            f.write(f"Beam{beam}:\n")
            f.write(f"  u0==u3 ratio: {m['u0_eq_u3_ratio']:.4f}\n")
            f.write(f"  median rho: {m['spearman_median']:.4f}\n")
            f.write(f"  mean rho: {m['spearman_mean']:.4f}\n")
            f.write(f"  MAD0_o1: {m['mad0_o1_ratio']:.4f}\n")
            f.write(f"  MAD0_o2: {m['mad0_o2_ratio']:.4f}\n")
            f.write(f"  avg rank: {m['avg_rank']:.4f}\n")
            f.write(f"  avg unique_distance_ratio: {m['avg_unique_distance_ratio']:.4f}\n")
            f.write("\n")

        f.write("## Stage Saturation (depth average)\n")
        f.write("| Beam | n | u0 | u0/n | tie_rate |\n")
        f.write("|---:|---:|---:|---:|---:|\n")
        for beam in [5, 8, 12]:
            m = metrics_by_beam[beam]
            f.write(
                f"| {beam} | {m['avg_n']:.4f} | {m['avg_u0_levels']:.4f} | "
                f"{m['avg_u0_over_n']:.4f} | {m['avg_tie_rate']:.4f} |\n"
            )

        f.write("\n## Front Thickness / Tie Rate\n")
        for beam in [5, 8, 12]:
            m = metrics_by_beam[beam]
            f.write(
                f"- Beam{beam}: avg_front_thickness={m['avg_front_thickness']:.4f}, "
                f"avg_tie_rate={m['avg_tie_rate']:.4f}\n"
            )

        f.write("\n## Criteria Check\n")
        f.write(f"- Beam8 -> Beam12 で u0増加: {'YES' if beam12_u0_increase else 'NO'}\n")
        f.write(
            f"- unique_distance_ratio 低下 (Beam5>Beam8>Beam12): "
            f"{'YES' if unique_ratio_down else 'NO'} "
            f"(values={unique_ratio_trend[0]:.4f}, {unique_ratio_trend[1]:.4f}, {unique_ratio_trend[2]:.4f})\n"
        )
        for beam, flag in collapse_flags:
            f.write(f"- Beam{beam} collapse継続条件(部分): {'YES' if flag else 'NO'}\n")

    return md_path


def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    metrics_by_beam = {}
    for beam in BEAMS:
        out_csv = os.path.join(OUT_DIR, f"raw_objectives_beam{beam}.csv")
        if os.path.exists(out_csv):
            os.remove(out_csv)
        run_raw_trace(beam, out_csv)
        metrics_by_beam[beam] = analyze_one_beam(out_csv)

    summary_csv = write_summary(metrics_by_beam)
    report_md = write_report(metrics_by_beam)
    print(f"Saved summary CSV: {summary_csv}")
    print(f"Saved report MD : {report_md}")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"ERROR: {e}")
        sys.exit(1)
