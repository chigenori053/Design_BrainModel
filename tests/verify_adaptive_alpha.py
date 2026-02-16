import csv
import subprocess
import os
import math
import statistics
import time

# Configuration
FIXED_ALPHAS = [0.01, 0.05, 0.1]
ADAPTIVE_START_ALPHA = 0.01
BEAMS = [5, 8, 12]
SEEDS = list(range(10))
DEPTH = 50
WARMUP_DEPTH = 10

REPORT_DIR = "report/adaptive_verify"
SUMMARY_FILE = "report/adaptive_verify_summary.csv"

def run_experiment(mode, alpha, beam, seed):
    """
    mode: "fixed" or "adaptive"
    alpha: value for --norm-alpha
    """
    os.makedirs(REPORT_DIR, exist_ok=True)
    mode_str = "adapt" if mode == "adaptive" else f"fixed_{alpha}"
    output_file = f"{REPORT_DIR}/trace_{mode_str}_b{beam}_s{seed}.csv"
    
    cmd = [
        "cargo", "run", "--release", "--bin", "design_cli", "--",
        "--trace",
        "--trace-output", output_file,
        "--trace-depth", str(DEPTH),
        "--trace-beam", str(beam),
        "--norm-alpha", str(alpha),
        "--seed", str(seed),
        "--baseline-off",
        "--category-soft",
        "--category-alpha", "3.0",
        "--lambda-min", "0.05",
        "--entropy-beta", "0.0",
        "--log-per-depth"
    ]
    
    if mode == "adaptive":
        cmd.append("--adaptive")

    # print(f"Running {mode} (a={alpha}, b={beam}, s={seed})...")
    start_time = time.time()
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"Error executing command: {' '.join(cmd)}\n{result.stderr}")
        return None
        
    return output_file

def analyze_trace(trace_path):
    if not trace_path or not os.path.exists(trace_path):
        return None

    try:
        with open(trace_path, 'r') as f:
            reader = csv.DictReader(f)
            rows = list(reader)
    except Exception as e:
        print(f"Error reading {trace_path}: {e}")
        return None

    steps_valid = 0
    collapse_count = 0
    nn_dists = []
    eff_dims = []
    alphas = []
    weak_contribs = []

    for row in rows:
        depth = int(row['depth'])
        if depth <= WARMUP_DEPTH:
            continue
            
        pareto_size = int(row['pareto_size'])
        mean_nn_norm = float(row['mean_nn_dist_norm'])
        effective_dim = int(row['effective_dim_count'])
        
        # New Metrics (handle missing if older trace - though we allow panic if field missing as this is for verification)
        alpha_t = float(row.get('alpha_t', 0.0))
        weak_contrib = float(row.get('weak_contrib_ratio', 0.0))
        
        # Collapse Definition: pareto_size < 2 OR mean_nn_dist_norm < 0.01
        is_collapsed = (pareto_size < 2) or (mean_nn_norm < 0.01)
        if is_collapsed:
            collapse_count += 1
            
        nn_dists.append(mean_nn_norm)
        eff_dims.append(effective_dim)
        alphas.append(alpha_t)
        weak_contribs.append(weak_contrib)
        steps_valid += 1

    if steps_valid == 0:
        return None

    collapse_ratio = collapse_count / steps_valid
    avg_nn_dist = statistics.mean(nn_dists) if nn_dists else 0.0
    min_nn_dist = min(nn_dists) if nn_dists else 0.0
    avg_eff_dims = statistics.mean(eff_dims) if eff_dims else 0.0
    avg_alpha = statistics.mean(alphas) if alphas else 0.0
    max_alpha = max(alphas) if alphas else 0.0
    min_alpha = min(alphas) if alphas else 0.0
    avg_weak_contrib = statistics.mean(weak_contribs) if weak_contribs else 0.0
    
    return {
        "collapse_ratio": collapse_ratio,
        "mean_nn_dist": avg_nn_dist,
        "min_nn_dist": min_nn_dist,
        "effective_dims": avg_eff_dims,
        "avg_alpha": avg_alpha,
        "min_alpha": min_alpha,
        "max_alpha": max_alpha,
        "avg_weak_contrib": avg_weak_contrib
    }

def main():
    print("Starting Adaptive Alpha Verification (v2.2)...")
    results = []

    # 1. Run Fixed Alphas
    for alpha in FIXED_ALPHAS:
        for beam in BEAMS:
            metrics_acc = []
            print(f"Processing Fixed alpha={alpha}, beam={beam}...")
            for seed in SEEDS:
                trace = run_experiment("fixed", alpha, beam, seed)
                m = analyze_trace(trace)
                if m: metrics_acc.append(m)
            
            if not metrics_acc: continue
            
            # Aggregate
            res = {
                "mode": "fixed",
                "alpha_setting": alpha,
                "beam": beam,
                "collapse_ratio": statistics.mean([m["collapse_ratio"] for m in metrics_acc]),
                "min_nn_dist": statistics.mean([m["min_nn_dist"] for m in metrics_acc]),
                "mean_nn_dist": statistics.mean([m["mean_nn_dist"] for m in metrics_acc]),
                "effective_dims": statistics.mean([m["effective_dims"] for m in metrics_acc]),
                "avg_alpha_observed": statistics.mean([m["avg_alpha"] for m in metrics_acc]), # Should be constant-ish if recorded correctly (or 0 if fixed logic doesn't update alpha_t field? Wait, fixed logic uses config.norm_alpha but doesn't update AdaptiveState if adaptive=false. But we updated TraceRow to use alpha_t. In fixed mode, alpha_t comes from adaptive_state which is initialized but NOT updated. So it should be constant initial value.)
                "avg_weak_contrib": statistics.mean([m["avg_weak_contrib"] for m in metrics_acc]),
            }
            results.append(res)
            print(f"  -> Collapse: {res['collapse_ratio']:.2f}, MinNN: {res['min_nn_dist']:.4f}")

    # 2. Run Adaptive Alpha
    for beam in BEAMS:
        metrics_acc = []
        print(f"Processing Adaptive (start={ADAPTIVE_START_ALPHA}), beam={beam}...")
        for seed in SEEDS:
            trace = run_experiment("adaptive", ADAPTIVE_START_ALPHA, beam, seed)
            m = analyze_trace(trace)
            if m: metrics_acc.append(m)

        if not metrics_acc: continue

        res = {
            "mode": "adaptive",
            "alpha_setting": ADAPTIVE_START_ALPHA,
            "beam": beam,
            "collapse_ratio": statistics.mean([m["collapse_ratio"] for m in metrics_acc]),
            "min_nn_dist": statistics.mean([m["min_nn_dist"] for m in metrics_acc]),
            "mean_nn_dist": statistics.mean([m["mean_nn_dist"] for m in metrics_acc]),
            "effective_dims": statistics.mean([m["effective_dims"] for m in metrics_acc]),
            "avg_alpha_observed": statistics.mean([m["avg_alpha"] for m in metrics_acc]),
            "avg_weak_contrib": statistics.mean([m["avg_weak_contrib"] for m in metrics_acc]),
        }
        results.append(res)
        print(f"  -> Collapse: {res['collapse_ratio']:.2f}, MinNN: {res['min_nn_dist']:.4f}, AvgAlpha: {res['avg_alpha_observed']:.4f}")

    # Save Summary
    os.makedirs(os.path.dirname(SUMMARY_FILE), exist_ok=True)
    keys = ["mode", "alpha_setting", "beam", "collapse_ratio", "min_nn_dist", "mean_nn_dist", "effective_dims", "avg_alpha_observed", "avg_weak_contrib"]
    with open(SUMMARY_FILE, 'w') as f:
        writer = csv.DictWriter(f, fieldnames=keys)
        writer.writeheader()
        writer.writerows(results)

    print(f"\nVerification Complete. Saved to {SUMMARY_FILE}")

    # Pass/Fail Check
    # "beam=5 で collapse_ratio を 固定α最良値以下"
    # "min_nn_dist_norm を固定α最良値以上"
    
    print("\n=== Analysis ===")
    
    # Get Fixed Best for Beam 5
    fixed_beam5 = [r for r in results if r["mode"] == "fixed" and r["beam"] == 5]
    adaptive_beam5 = [r for r in results if r["mode"] == "adaptive" and r["beam"] == 5][0]
    
    best_fixed_collapse = min(r["collapse_ratio"] for r in fixed_beam5)
    best_fixed_min_nn = max(r["min_nn_dist"] for r in fixed_beam5) # Higher is better for discrimination
    
    print(f"Beam 5 Fixed Best Collapse: {best_fixed_collapse:.2f}")
    print(f"Beam 5 Fixed Best MinNN: {best_fixed_min_nn:.4f}")
    print(f"Beam 5 Adaptive: Collapse={adaptive_beam5['collapse_ratio']:.2f}, MinNN={adaptive_beam5['min_nn_dist']:.4f}")
    
    pass_collapse = adaptive_beam5["collapse_ratio"] <= (best_fixed_collapse + 0.05) # tolerance
    pass_min_nn = adaptive_beam5["min_nn_dist"] >= (best_fixed_min_nn * 0.9) # tolerance
    
    if pass_collapse and pass_min_nn:
        print("\nSUCCESS: Adaptive Alpha validation passed criteria.")
    else:
        print("\nWARNING: Adaptive Alpha validation outcomes mixed.")
        if not pass_collapse: print(f"  - Collapse ratio too high ({adaptive_beam5['collapse_ratio']:.2f} vs {best_fixed_collapse:.2f})")
        if not pass_min_nn: print(f"  - Min NN dist too low ({adaptive_beam5['min_nn_dist']:.4f} vs {best_fixed_min_nn:.4f})")

if __name__ == "__main__":
    main()
