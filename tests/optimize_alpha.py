import csv
import subprocess
import sys
import os
import math
import statistics

# Parameters
ALPHAS = [0.0, 0.01, 0.05, 0.1, 0.2, 0.3, 0.5, 1.0]
BEAMS = [5, 8, 12]
SEEDS = list(range(10))
DEPTH = 50
WARMUP_DEPTH = 10

REPORT_DIR = "report/alpha_opt"
SUMMARY_FILE = "report/alpha_opt_summary.csv"

def run_experiment(alpha, beam, seed):
    os.makedirs(REPORT_DIR, exist_ok=True)
    output_file = f"{REPORT_DIR}/trace_a{alpha}_b{beam}_s{seed}.csv"
    
    # Skip if already exists (optional, but good for resuming)
    # if os.path.exists(output_file):
    #     return output_file

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
    
    # print(f"Running: alpha={alpha}, beam={beam}, seed={seed}...")
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

    for row in rows:
        depth = int(row['depth'])
        if depth <= WARMUP_DEPTH:
            continue
            
        pareto_size = int(row['pareto_size'])
        mean_nn_norm = float(row['mean_nn_dist_norm'])
        effective_dim = int(row['effective_dim_count'])
        
        # Collapse Definition: pareto_size < 2 OR mean_nn_dist_norm < 0.01
        is_collapsed = (pareto_size < 2) or (mean_nn_norm < 0.01)
        if is_collapsed:
            collapse_count += 1
            
        nn_dists.append(mean_nn_norm)
        eff_dims.append(effective_dim)
        steps_valid += 1

    if steps_valid == 0:
        return {
            "collapse_ratio": 1.0,
            "mean_nn_dist_norm": 0.0,
            "effective_dim_count": 0.0,
            "score": 0.0
        }

    collapse_ratio = collapse_count / steps_valid
    avg_nn_dist = statistics.mean(nn_dists) if nn_dists else 0.0
    min_nn_dist = min(nn_dists) if nn_dists else 0.0
    avg_eff_dims = statistics.mean(eff_dims) if eff_dims else 0.0
    
    # Score Calculation
    # S = (1 - collapse_ratio) * min(mean_nn_dist_norm, 0.5) * log(effective_dim_count + 1)
    score = (1.0 - collapse_ratio) * min(avg_nn_dist, 0.5) * math.log(avg_eff_dims + 1.0)
    
    return {
        "collapse_ratio": collapse_ratio,
        "mean_nn_dist_norm": avg_nn_dist,
        "min_nn_dist_norm": min_nn_dist,
        "effective_dim_count": avg_eff_dims,
        "score": score
    }

def main():
    print("Starting Alpha Optimization Protocol...")
    print(f"Alphas: {ALPHAS}")
    print(f"Beams: {BEAMS}")
    print(f"Seeds: {len(SEEDS)}")
    print("--------------------------------------------------")

    results = [] # List of dicts

    total_runs = len(ALPHAS) * len(BEAMS) * len(SEEDS)
    current_run = 0

    for alpha in ALPHAS:
        for beam in BEAMS:
            metrics_list = []
            print(f"Processing alpha={alpha}, beam={beam}...")
            
            for seed in SEEDS:
                current_run += 1
                # print(f"[{current_run}/{total_runs}] Run: a={alpha}, b={beam}, s={seed}")
                
                trace_file = run_experiment(alpha, beam, seed)
                metrics = analyze_trace(trace_file)
                
                if metrics:
                    metrics_list.append(metrics)
            
            # Aggregate for (alpha, beam)
            if not metrics_list:
                continue
                
            avg_collapse = statistics.mean([m["collapse_ratio"] for m in metrics_list])
            avg_nn_dist = statistics.mean([m["mean_nn_dist_norm"] for m in metrics_list])
            avg_min_nn_dist = statistics.mean([m["min_nn_dist_norm"] for m in metrics_list])
            avg_eff_dims = statistics.mean([m["effective_dim_count"] for m in metrics_list])
            
            # Recalculate score based on aggregates or average of scores?
            # Typically average of scores, or score of averages.
            # Plan says "Compute Score". Let's compute Score of Averages to be stable.
            # actually prompt implies score per run? No, optimize parameter.
            # "Metrics: Collapse Ratio (Avg), Mean NN Dist (Avg), Effective Dims (Avg)"
            # "Score S = ..." using the aggregated metrics.
            
            combined_score = (1.0 - avg_collapse) * min(avg_nn_dist, 0.5) * math.log(avg_eff_dims + 1.0)
            
            result_row = {
                "alpha": alpha,
                "beam": beam,
                "collapse_ratio": avg_collapse,
                "mean_nn_dist_norm": avg_nn_dist,
                "min_nn_dist_norm": avg_min_nn_dist,
                "effective_dim_count": avg_eff_dims,
                "score": combined_score
            }
            results.append(result_row)
            print(f"  Result: Score={combined_score:.4f} (Collapse={avg_collapse:.2f}, NN={avg_nn_dist:.4f}, MinNN={avg_min_nn_dist:.4f}, Dims={avg_eff_dims:.2f})")

    # Save Summary
    os.makedirs(os.path.dirname(SUMMARY_FILE), exist_ok=True)
    with open(SUMMARY_FILE, 'w') as f:
        writer = csv.DictWriter(f, fieldnames=["alpha", "beam", "collapse_ratio", "mean_nn_dist_norm", "min_nn_dist_norm", "effective_dim_count", "score"])
        writer.writeheader()
        writer.writerows(results)
    
    print(f"\nOptimization Complete. Summary saved to {SUMMARY_FILE}")
    
    # Determine Best Alpha (Marginalize over beam or pick best (alpha, beam) pair?)
    # "Determine optimal alpha". Usually we look for robust alpha across beams or best overall.
    # I'll print best alpha by max average score across beams? Or just best single config?
    # I'll aggregate by alpha.
    
    print("\nAlpha Performance Summary (Averaged over Beams):")
    alpha_scores = {}
    for r in results:
        a = r["alpha"]
        if a not in alpha_scores: alpha_scores[a] = []
        alpha_scores[a].append(r["score"])
        
    best_alpha = -1
    best_alpha_score = -1
    
    for a in sorted(alpha_scores.keys()):
        avg_s = statistics.mean(alpha_scores[a])
        print(f"Alpha {a}: Score = {avg_s:.4f}")
        if avg_s > best_alpha_score:
            best_alpha_score = avg_s
            best_alpha = a
            
    print(f"\nRecommended Optimal Alpha: {best_alpha}")

if __name__ == "__main__":
    main()
