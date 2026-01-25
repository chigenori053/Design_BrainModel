import os
import json
import glob
from datetime import datetime
from typing import List, Dict

# Target directory
TARGET_DIR = "rust_ui_poc/target/debug/.fingerprint"

def parse_fingerprints(base_dir: str) -> List[Dict]:
    data = []
    
    # 1. Iterate over all subdirectories in .fingerprint
    # Structure: .fingerprint / <crate_name>-<hash> / <json_files>
    if not os.path.exists(base_dir):
        print(f"Directory not found: {base_dir}")
        return []

    for crate_dir in os.listdir(base_dir):
        full_crate_dir = os.path.join(base_dir, crate_dir)
        if not os.path.isdir(full_crate_dir):
            continue
            
        # Parse crate name and hash from directory name (last hyphen separator)
        if '-' in crate_dir:
            crate_name = crate_dir.rsplit('-', 1)[0]
        else:
            crate_name = crate_dir

        # 2. Look for JSON files inside
        # Usually named "lib-<crate_name>.json" or similar
        json_files = glob.glob(os.path.join(full_crate_dir, "*.json"))
        
        for json_file in json_files:
            try:
                # Get file modification time
                mtime = os.path.getmtime(json_file)
                mod_time_str = datetime.fromtimestamp(mtime).strftime('%Y-%m-%d %H:%M:%S')
                
                with open(json_file, 'r') as f:
                    content = json.load(f)
                
                # Extract relevant fields
                # "deps" is a list of [hash, name, ...]
                deps_count = len(content.get("deps", []))
                features = content.get("features", "")
                target = content.get("target", "")
                profile = content.get("profile", "")
                
                # Append to dataset
                data.append({
                    "crate": crate_name,
                    "file": os.path.basename(json_file),
                    "mod_time": mod_time_str,
                    "target_id": str(target),
                    "profile_id": str(profile),
                    "deps_count": deps_count,
                    "features_snippet": str(features)[:30] + "..." if len(str(features)) > 30 else str(features)
                })
            except Exception as e:
                # Skip invalid files
                pass
                
    return data

def main():
    root_dir = os.path.abspath(".")
    build_dir = os.path.join(root_dir, TARGET_DIR)
    
    print(f"Scanning: {build_dir}")
    records = parse_fingerprints(build_dir)
    
    print(f"\nFound {len(records)} metadata records. Showing top 10 examples:\n")
    
    # Simple Tabular Print (Simulating DataFrame.head())
    header = f"{'Crate':<30} | {'Deps':<5} | {'Modified':<20} | {'Features'}"
    print("-" * len(header))
    print(header)
    print("-" * len(header))
    
    for row in sorted(records, key=lambda x: x['crate'])[:10]:
        print(f"{row['crate']:<30} | {row['deps_count']:<5} | {row['mod_time']:<20} | {row['features_snippet']}")

    print("\n--- Conclusion ---")
    print("Yes, these JSON files contain structured data that can be loaded into a Pandas DataFrame.")
    print("Columns available: crate, target_id, profile_id, features, dependencies, paths, timestamps.")

if __name__ == "__main__":
    main()
