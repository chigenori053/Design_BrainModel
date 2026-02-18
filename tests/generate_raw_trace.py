
import subprocess
import sys
import os
import shutil

BEAM_SIZES = [5, 8, 12]

def run_extraction():
    os.makedirs("report", exist_ok=True)
    
    for beam in BEAM_SIZES:
        output_csv = f"report/raw_objectives_beam{beam}.csv"
        
        # Remove existing file if it exists
        if os.path.exists(output_csv):
            os.remove(output_csv)
            
        print(f"--- Running extraction for Beam Size: {beam} ---")

        cmd = [
            "cargo", "run", "--release", "-p", "design_cli", "--",
            "--trace",
            "--trace-depth", "50",
            "--trace-beam", str(beam),
            "--baseline-off",
            "--category-soft",
            "--raw-trace-output", output_csv
        ]
        
        print(f"Running command: {' '.join(cmd)}")
        
        try:
            result = subprocess.run(cmd, check=True, capture_output=True, text=True)
            print("Command execution completed.")
        except subprocess.CalledProcessError as e:
            print(f"Error executing command: {e}")
            print(f"Stderr: {e.stderr}")
            sys.exit(1)

        if not os.path.exists(output_csv):
            print(f"Error: Output file {output_csv} was not created.")
            sys.exit(1)
            
        # Validation
        with open(output_csv, 'r') as f:
            header = f.readline().strip()
            expected = "depth,candidate_id,objective_0,objective_1,objective_2,objective_3_shape"
            if header != expected:
                print(f"Error: Unexpected header. Got: {header}, Expected: {expected}")
                sys.exit(1)
                
            lines = f.readlines()
            print(f"Generated {len(lines)} data rows for beam {beam}.")
            min_expected = 50 * beam // 2 # very rough lower bound
            if len(lines) < min_expected: 
                 print(f"Warning: Row count seems low ({len(lines)}).")

        print(f"Success! Raw trace data saved to {output_csv}\n")

if __name__ == "__main__":
    run_extraction()
