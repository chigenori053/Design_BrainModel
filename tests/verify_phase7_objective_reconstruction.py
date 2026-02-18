import csv
import os
import shutil
import statistics
import subprocess
from collections import defaultdict

DEPTH = 50
BEAMS = [5, 8, 12]
SEED = 42
NORM_ALPHA = 0.25
CATEGORY_ALPHA = 3.0
OUT_DIR = "report/phase7_objective_reconstruction"

MODES = [
    ("objective_only", "off"),
    ("with_memory_v6_1", "v6.1"),
]


def run_trace(mode_name: str, mode_env: str, beam: int):
    mode_dir = os.path.join(OUT_DIR, mode_name)
    os.makedirs(mode_dir, exist_ok=True)
    raw_csv = os.path.join(mode_dir, f"raw_objectives_beam{beam}.csv")
    if os.path.exists(raw_csv):
        os.remove(raw_csv)

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
        "--raw-trace-output",
        raw_csv,
    ]
    print("Running:", " ".join(cmd), f"(PHASE6_MEMORY_MODE={mode_env})")
    result = subprocess.run(cmd, capture_output=True, text=True, env=env)
    if result.returncode != 0:
        print(result.stderr)
        raise RuntimeError(f"trace generation failed for mode={mode_name}, beam={beam}")
    return raw_csv


def mad(values):
    if not values:
        return 0.0
    med = statistics.median(values)
    return statistics.median([abs(v - med) for v in values])


def load_depth_values(raw_csv):
    grouped = defaultdict(lambda: {"o1": [], "o2": []})
    with open(raw_csv, "r", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            depth = int(row["depth"])
            grouped[depth]["o1"].append(float(row["objective_1"]))
            grouped[depth]["o2"].append(float(row["objective_2"]))
    return grouped


def analyze_depth_stats(raw_csv):
    grouped = load_depth_values(raw_csv)
    rows = []
    for depth in sorted(grouped.keys()):
        o1 = grouped[depth]["o1"]
        o2 = grouped[depth]["o2"]
        if len(o1) >= 2:
            var_o1 = statistics.pvariance(o1)
            var_o2 = statistics.pvariance(o2)
        else:
            var_o1 = 0.0
            var_o2 = 0.0
        unique_o1 = len(set(round(v, 12) for v in o1))
        unique_o2 = len(set(round(v, 12) for v in o2))
        rows.append(
            {
                "depth": depth,
                "n": len(o1),
                "var_o1": var_o1,
                "var_o2": var_o2,
                "unique_level_count_o1": unique_o1,
                "unique_level_count_o2": unique_o2,
                "mad0_o1": 1 if mad(o1) == 0.0 else 0,
                "mad0_o2": 1 if mad(o2) == 0.0 else 0,
            }
        )
    return rows


def write_csv(path, rows, fields):
    with open(path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)


def main():
    if os.path.exists(OUT_DIR):
        shutil.rmtree(OUT_DIR)
    os.makedirs(OUT_DIR, exist_ok=True)

    summary_rows = []
    histogram_rows = []

    for mode_name, mode_env in MODES:
        for beam in BEAMS:
            raw_csv = run_trace(mode_name, mode_env, beam)
            depth_rows = analyze_depth_stats(raw_csv)

            depth_csv = os.path.join(OUT_DIR, mode_name, f"depth_stats_beam{beam}.csv")
            write_csv(
                depth_csv,
                depth_rows,
                [
                    "depth",
                    "n",
                    "var_o1",
                    "var_o2",
                    "unique_level_count_o1",
                    "unique_level_count_o2",
                    "mad0_o1",
                    "mad0_o2",
                ],
            )

            u1_hist = defaultdict(int)
            u2_hist = defaultdict(int)
            for row in depth_rows:
                u1_hist[row["unique_level_count_o1"]] += 1
                u2_hist[row["unique_level_count_o2"]] += 1

            for level, count in sorted(u1_hist.items()):
                histogram_rows.append(
                    {
                        "mode": mode_name,
                        "beam": beam,
                        "objective": "o1",
                        "unique_level_count": level,
                        "depth_frequency": count,
                    }
                )
            for level, count in sorted(u2_hist.items()):
                histogram_rows.append(
                    {
                        "mode": mode_name,
                        "beam": beam,
                        "objective": "o2",
                        "unique_level_count": level,
                        "depth_frequency": count,
                    }
                )

            summary_rows.append(
                {
                    "mode": mode_name,
                    "beam": beam,
                    "depth_count": len(depth_rows),
                    "mean_var_o1": statistics.mean([r["var_o1"] for r in depth_rows]),
                    "mean_var_o2": statistics.mean([r["var_o2"] for r in depth_rows]),
                    "median_var_o1": statistics.median([r["var_o1"] for r in depth_rows]),
                    "median_var_o2": statistics.median([r["var_o2"] for r in depth_rows]),
                    "mean_unique_o1": statistics.mean(
                        [r["unique_level_count_o1"] for r in depth_rows]
                    ),
                    "mean_unique_o2": statistics.mean(
                        [r["unique_level_count_o2"] for r in depth_rows]
                    ),
                    "mad0_o1_ratio": statistics.mean([r["mad0_o1"] for r in depth_rows]),
                    "mad0_o2_ratio": statistics.mean([r["mad0_o2"] for r in depth_rows]),
                    "target_pass": (
                        statistics.mean([r["mad0_o1"] for r in depth_rows]) <= 0.50
                        and statistics.mean([r["mad0_o2"] for r in depth_rows]) <= 0.50
                    ),
                }
            )

    write_csv(
        os.path.join(OUT_DIR, "phase7_objective_summary.csv"),
        summary_rows,
        [
            "mode",
            "beam",
            "depth_count",
            "mean_var_o1",
            "mean_var_o2",
            "median_var_o1",
            "median_var_o2",
            "mean_unique_o1",
            "mean_unique_o2",
            "mad0_o1_ratio",
            "mad0_o2_ratio",
            "target_pass",
        ],
    )

    write_csv(
        os.path.join(OUT_DIR, "phase7_unique_level_histogram.csv"),
        histogram_rows,
        ["mode", "beam", "objective", "unique_level_count", "depth_frequency"],
    )

    print("Saved summary: report/phase7_objective_reconstruction/phase7_objective_summary.csv")
    print(
        "Saved histogram: report/phase7_objective_reconstruction/phase7_unique_level_histogram.csv"
    )


if __name__ == "__main__":
    main()
