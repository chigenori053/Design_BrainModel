import os
import json
import glob
import csv
from datetime import datetime
from typing import List, Dict

# Config
TARGET_DIR = "rust_ui_poc/target/debug/.fingerprint"
OUTPUT_FILE = "build_stats.csv"

def parse_fingerprints(base_dir: str) -> List[Dict]:
    data = []
    
    if not os.path.exists(base_dir):
        print(f"Error: Directory not found: {base_dir}")
        return []

    for crate_dir in os.listdir(base_dir):
        full_crate_dir = os.path.join(base_dir, crate_dir)
        if not os.path.isdir(full_crate_dir):
            continue
            
        # Parse crate name
        if '-' in crate_dir:
            crate_name = crate_dir.rsplit('-', 1)[0]
        else:
            crate_name = crate_dir

        # Look for JSON files
        json_files = glob.glob(os.path.join(full_crate_dir, "*.json"))
        
        for json_file in json_files:
            try:
                mtime = os.path.getmtime(json_file)
                mod_time_str = datetime.fromtimestamp(mtime).strftime('%Y-%m-%d %H:%M:%S')
                
                with open(json_file, 'r') as f:
                    content = json.load(f)
                
                features = content.get("features", [])
                deps = content.get("deps", [])
                
                data.append({
                    "crate": crate_name,
                    "file": os.path.basename(json_file),
                    "modified": mod_time_str,
                    "deps_count": len(deps),
                    "features_count": len(features),
                    "target_id": str(content.get("target", "")),
                    "profile_id": str(content.get("profile", "")),
                })
            except Exception as e:
                pass
                
    return data

def save_to_csv(data: List[Dict], filename: str):
    if not data:
        print("No data to save.")
        return

    keys = ["crate", "file", "modified", "deps_count", "features_count", "target_id", "profile_id"]
    
    try:
        with open(filename, 'w', newline='', encoding='utf-8') as f:
            writer = csv.DictWriter(f, fieldnames=keys)
            writer.writeheader()
            writer.writerows(data)
        print(f"Successfully saved {len(data)} records to {filename}")
    except IOError as e:
        print(f"Failed to save CSV: {e}")

def main():
    root_dir = os.path.abspath(".")
    build_dir = os.path.join(root_dir, TARGET_DIR)
    
    print(f"Analyzing build artifacts in: {build_dir}")
    records = parse_fingerprints(build_dir)
    
    if records:
        output_path = os.path.join(root_dir, OUTPUT_FILE)
        save_to_csv(records, output_path)
    else:
        print("No records found to analyze.")

if __name__ == "__main__":
    main()
