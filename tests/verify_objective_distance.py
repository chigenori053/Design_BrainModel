import csv
import subprocess
import sys
import os

def run_experiment():
    print("Running Experiment 4 Set A...")
    cmd = [
        "cargo", "run", "--release", "--bin", "design_cli", "--",
        "--trace",
        "--trace-output", "report/verification_trace.csv",
        "--trace-depth", "100",
        "--trace-beam", "5",
        "--baseline-off",
        "--category-soft",
        "--category-alpha", "3.0",
        "--lambda-min", "0.05",
        "--entropy-beta", "0.0",
        "--log-per-depth"
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print("Experiment failed:")
        print(result.stderr)
        sys.exit(1)
    print("Experiment finished. Trace written to report/verification_trace.csv")

def analyze_trace():
    trace_path = "report/verification_trace.csv"
    if not os.path.exists(trace_path):
        print(f"Error: {trace_path} not found.")
        sys.exit(1)

    with open(trace_path, 'r') as f:
        reader = csv.DictReader(f)
        rows = list(reader)

    print(f"Loaded {len(rows)} rows from trace.")
    
    # Counters for 4 Cases
    count_case_a = 0 # Norm Degeneracy
    count_case_b = 0 # Disconnected
    count_case_c = 0 # NN Logic Bug
    count_case_d = 0 # MAD Failure
    
    steps_checked = 0

    for row in rows:
        depth = int(row['depth'])
        pareto_size = int(row['pareto_size'])
        unique_norm = int(row['unique_norm_vec_count'])
        mean_nn_norm = float(row['mean_nn_dist_norm'])
        distance_calls = int(row['distance_calls'])
        mad_zero = int(row['norm_dim_mad_zero_count'])
        
        # Collapse definition from spec: not explicitly defined in "判定ロジック" section but implies low diversity or size?
        # "collapse发生" usually means pareto_size became extraordinarily small or 1?
        # Or mean_nn_dist becomes 0.
        # Spec says Case D: "norm_dim_mad_zero_count > 0 AND collapse発生"
        # Let's assume collapse means pareto_size=1 OR mean_nn_norm=0
        is_collapse = (pareto_size == 1) or (mean_nn_norm == 0.0)

        if pareto_size < 2:
            # If pareto_size is 1, unique_norm is 1.
            # Does Case A apply? "pareto_size > 1 AND unique_norm_vec_count == 1".
            # So if pareto_size=1, Case A is false.
            pass

        # Case A: Normalization Degeneracy
        if pareto_size > 1 and unique_norm == 1:
            count_case_a += 1
            print(f"Depth {depth}: Case A Detected (Pareto={pareto_size}, UniqueNorm={unique_norm})")

        # Case B: Execution Path Disconnection
        if distance_calls == 0:
            count_case_b += 1
            print(f"Depth {depth}: Case B Detected (DistanceCalls=0)")

        # Case C: NN Logic Bug
        if unique_norm > 1 and mean_nn_norm == 0.0 and distance_calls > 0:
            count_case_c += 1
            print(f"Depth {depth}: Case C Detected (Unique={unique_norm}, NN=0, Calls={distance_calls})")

        # Case D: MAD Norm Failure
        if mad_zero > 0 and is_collapse:
            count_case_d += 1
            print(f"Depth {depth}: Case D Detected (MAD=0 count={mad_zero}, Collapse={is_collapse})")
            
        steps_checked += 1

    print("\n=== VERIFICATION REPORT ===")
    print(f"Total Steps Analyzed: {steps_checked}")
    print(f"Case A (Normalization Degeneracy): {count_case_a}")
    print(f"Case B (Path Disconnection):       {count_case_b}")
    print(f"Case C (NN Logic Bug):             {count_case_c}")
    print(f"Case D (MAD Norm Failure):         {count_case_d}")
    
    print("\n--- Conclusion ---")
    detected = []
    if count_case_a > 0: detected.append("Normalization Degeneracy (Case A)")
    if count_case_b > 0: detected.append("Execution Path Disconnection (Case B)")
    if count_case_c > 0: detected.append("NN Logic Implementation Bug (Case C)")
    if count_case_d > 0: detected.append("MAD Normalization Failure (Case D)")
    
    if len(detected) == 0:
        print("SUCCESS: No specification violations detected.")
        print("Note: This means none of the 4 failure cases were identified.")
    else:
        print("FAILURE DETECTED:")
        for reason in detected:
            print(f" - {reason}")

if __name__ == "__main__":
    run_experiment()
    analyze_trace()
