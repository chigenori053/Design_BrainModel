#!/usr/bin/env python3
"""Trace analysis for DesignBrainModel Phase 3.

Usage:
    python tools/trace_analysis.py trace.csv
"""

from __future__ import annotations

import argparse
from pathlib import Path

try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import numpy as np
    import pandas as pd
except ModuleNotFoundError as exc:
    missing = str(exc).split("'")[1] if "'" in str(exc) else str(exc)
    raise SystemExit(
        "Missing Python dependency: "
        f"{missing}. Install with: pip install pandas matplotlib numpy"
    ) from exc

REQUIRED_COLUMNS = [
    "depth",
    "lambda",
    "delta_lambda",
    "tau_prime",
    "conf_chm",
    "density",
    "k",
    "h_profile",
    "pareto_size",
    "diversity",
    "resonance_avg",
]

OPTIONAL_COLUMNS = ["pressure", "epsilon_effect"]


def load_trace(csv_path: Path) -> pd.DataFrame:
    df = pd.read_csv(csv_path)

    missing = [c for c in REQUIRED_COLUMNS if c not in df.columns]
    if missing:
        raise ValueError(f"Missing required columns: {missing}")

    df = df.sort_values("depth", kind="mergesort").reset_index(drop=True)
    df["lambda_ma5"] = df["lambda"].rolling(window=5, min_periods=1).mean()
    return df


def save_lambda_plot(df: pd.DataFrame, out_dir: Path) -> None:
    plt.figure(figsize=(9, 4.5))
    plt.plot(df["depth"], df["lambda"], label="lambda", linewidth=1.5)
    plt.plot(df["depth"], df["lambda_ma5"], label="lambda_ma5", linewidth=2.0)
    plt.xlabel("depth")
    plt.ylabel("lambda")
    plt.title("Lambda Transition")
    plt.grid(True, alpha=0.3)
    plt.legend()
    plt.tight_layout()
    plt.savefig(out_dir / "lambda_transition.png", dpi=140)
    plt.close()


def save_tau_plot(df: pd.DataFrame, out_dir: Path) -> None:
    plt.figure(figsize=(9, 4.5))
    plt.plot(df["depth"], df["tau_prime"], color="tab:orange", linewidth=1.8)
    plt.xlabel("depth")
    plt.ylabel("tau_prime")
    plt.title("Tau Prime Transition")
    plt.grid(True, alpha=0.3)
    plt.tight_layout()
    plt.savefig(out_dir / "tau_prime_transition.png", dpi=140)
    plt.close()


def save_diversity_plot(df: pd.DataFrame, out_dir: Path) -> None:
    plt.figure(figsize=(9, 4.5))
    plt.plot(df["depth"], df["diversity"], color="tab:green", linewidth=1.8)
    plt.xlabel("depth")
    plt.ylabel("diversity")
    plt.title("Diversity Transition")
    plt.grid(True, alpha=0.3)
    plt.tight_layout()
    plt.savefig(out_dir / "diversity_transition.png", dpi=140)
    plt.close()


def save_conf_density_scatter(df: pd.DataFrame, out_dir: Path) -> None:
    plt.figure(figsize=(6.5, 5.5))
    plt.scatter(df["density"], df["conf_chm"], s=18, alpha=0.75, color="tab:purple")
    plt.xlabel("density")
    plt.ylabel("conf_chm")
    plt.title("conf_chm vs density")
    plt.grid(True, alpha=0.3)
    plt.tight_layout()
    plt.savefig(out_dir / "conf_chm_vs_density.png", dpi=140)
    plt.close()


def save_pareto_plot(df: pd.DataFrame, out_dir: Path) -> None:
    plt.figure(figsize=(9, 4.5))
    plt.plot(df["depth"], df["pareto_size"], color="tab:red", linewidth=1.8)
    plt.xlabel("depth")
    plt.ylabel("pareto_size")
    plt.title("Pareto Size Transition")
    plt.grid(True, alpha=0.3)
    plt.tight_layout()
    plt.savefig(out_dir / "pareto_size_transition.png", dpi=140)
    plt.close()


def compute_metrics(df: pd.DataFrame) -> dict[str, float]:
    metrics = {
        "var_lambda": float(np.var(df["lambda"].to_numpy(dtype=np.float64))),
        "max_abs_delta_lambda": float(np.max(np.abs(df["delta_lambda"].to_numpy(dtype=np.float64)))),
        "diversity_min": float(np.min(df["diversity"].to_numpy(dtype=np.float64))),
        "tau_prime_avg": float(np.mean(df["tau_prime"].to_numpy(dtype=np.float64))),
    }
    if "pressure" in df.columns:
        metrics["pressure_avg"] = float(np.mean(df["pressure"].to_numpy(dtype=np.float64)))
    if "epsilon_effect" in df.columns:
        metrics["epsilon_effect_avg"] = float(np.mean(df["epsilon_effect"].to_numpy(dtype=np.float64)))
    return metrics


def print_diagnostics(metrics: dict[str, float]) -> None:
    print("=== Trace Analysis Metrics ===")
    print(f"Var(lambda): {metrics['var_lambda']:.9f}")
    print(f"max|Δlambda|: {metrics['max_abs_delta_lambda']:.9f}")
    print(f"diversity_min: {metrics['diversity_min']:.9f}")
    print(f"tau_prime_avg: {metrics['tau_prime_avg']:.9f}")
    if "pressure_avg" in metrics:
        print(f"pressure_avg: {metrics['pressure_avg']:.9f}")
    if "epsilon_effect_avg" in metrics:
        print(f"epsilon_effect_avg: {metrics['epsilon_effect_avg']:.9f}")

    print("=== Qualitative Checks ===")
    if metrics["max_abs_delta_lambda"] <= 0.05:
        print("lambda step-size: OK (<= 0.05)")
    else:
        print("lambda step-size: WARNING (> 0.05)")

    if metrics["diversity_min"] > 0.0:
        print("diversity collapse: OK (never reached 0)")
    else:
        print("diversity collapse: WARNING (reached 0)")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Analyze DesignBrainModel trace.csv")
    parser.add_argument("trace_csv", type=Path, help="Path to trace.csv")
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=None,
        help="Output directory for plots (default: <trace_dir>/trace_analysis)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    trace_csv: Path = args.trace_csv

    if not trace_csv.exists():
        raise FileNotFoundError(f"Trace CSV not found: {trace_csv}")

    out_dir = args.out_dir or (trace_csv.parent / "trace_analysis")
    out_dir.mkdir(parents=True, exist_ok=True)

    df = load_trace(trace_csv)

    save_lambda_plot(df, out_dir)
    save_tau_plot(df, out_dir)
    save_diversity_plot(df, out_dir)
    save_conf_density_scatter(df, out_dir)
    save_pareto_plot(df, out_dir)

    metrics = compute_metrics(df)
    print_diagnostics(metrics)

    print(f"plots_dir: {out_dir}")


if __name__ == "__main__":
    main()
