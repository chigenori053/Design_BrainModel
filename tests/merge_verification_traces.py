
import csv
import glob
import os
import sys

OUTPUT_FILE = "report/combined_verification_trace.csv"

def merge_csvs():
    # Find all source CSVs
    files = []
    if os.path.exists("report/verification_trace.csv"):
        files.append("report/verification_trace.csv")
    
    adaptive_traces = glob.glob("report/adaptive_verify/trace_*.csv")
    files.extend(adaptive_traces)
    
    if not files:
        print("No trace files found to merge.")
        return

    print(f"Found {len(files)} trace files. Merging...")

    all_rows = []
    fieldnames = set()

    # First pass: read all files and collect fieldnames
    for file_path in files:
        try:
            with open(file_path, 'r') as f:
                reader = csv.DictReader(f)
                if not reader.fieldnames:
                    continue
                
                # Update fieldnames
                fieldnames.update(reader.fieldnames)
                
                # Read rows and add source_file + objective mapping
                for row in reader:
                    # Add source file
                    row['source_file'] = os.path.basename(file_path)
                    
                    # Map norm_median_X to objective_X
                    for i in range(4):
                        key_src = f"norm_median_{i}"
                        key_dst = f"objective_{i}"
                        if key_src in row:
                            row[key_dst] = row[key_src]
                        else:
                            # If missing, set to empty or 0? 
                            # If verify_objective_distance.py has it, verify_adaptive_alpha.py should too.
                            pass 

                    all_rows.append(row)
        except Exception as e:
            print(f"Error reading {file_path}: {e}")

    # Add new fields to fieldnames
    fieldnames.add('source_file')
    for i in range(4):
        fieldnames.add(f"objective_{i}")
    
    # Sort fieldnames for consistent output (optional but nice)
    sorted_fieldnames = sorted(list(fieldnames))
    
    # Ensure important columns are first
    priority_cols = ['source_file', 'depth', 'objective_0', 'objective_1', 'objective_2', 'objective_3']
    final_fieldnames = []
    for col in priority_cols:
        if col in sorted_fieldnames:
            final_fieldnames.append(col)
            sorted_fieldnames.remove(col)
    final_fieldnames.extend(sorted_fieldnames)

    # Write output
    os.makedirs(os.path.dirname(OUTPUT_FILE), exist_ok=True)
    with open(OUTPUT_FILE, 'w') as f:
        writer = csv.DictWriter(f, fieldnames=final_fieldnames)
        writer.writeheader()
        writer.writerows(all_rows)
    
    print(f"Successfully merged {len(all_rows)} rows into {OUTPUT_FILE}")

if __name__ == "__main__":
    merge_csvs()
