# Tools

This folder contains scripts for detecting impacted targets and uploading them to Trunk.

## Scripts

### `detect_impacted_nx_targets.py`

Detects impacted Nx projects/targets based on git changes using Nx's affected command.

**Usage:**

```bash
# Check affected projects between a base commit and HEAD
python3 tools/detect_impacted_nx_targets.py --base=main

# Check affected projects for uncommitted changes
python3 tools/detect_impacted_nx_targets.py --uncommitted

# Check affected projects for specific files
python3 tools/detect_impacted_nx_targets.py --files="nx/alpha/alpha.txt,nx/bravo/bravo.txt"

# Custom output file
python3 tools/detect_impacted_nx_targets.py --base=HEAD~1 -o nx_targets.json
```

**Options:**

- `--base BASE`: Base commit/branch for comparison (e.g., 'main', 'HEAD~1')
- `--head HEAD`: Head commit (default: HEAD)
- `--files FILES`: Comma-separated list of specific files to check
- `--uncommitted`: Include uncommitted changes
- `--untracked`: Include untracked files
- `-o, --output OUTPUT`: Output file path (default: impacted_targets_json_tmp)
- `-q, --quiet`: Suppress verbose output

**Output:** Writes a JSON array of affected project names to the output file.

### `upload_glob_targets.py`

Uploads impacted targets to Trunk API.

### `glob_targets.sh`

Helper script for glob-based target detection.
