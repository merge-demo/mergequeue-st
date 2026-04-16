#!/usr/bin/env python3
"""
Quick shortcut for impacted-targets detection.

If every file changed in the PR lives outside the current build tool's
workspace folder, upload the set of first-level folder names of the changed
files as the impacted targets and exit. This lets the workflow skip the
expensive build-tool setup (Python deps, Node, npm install, nx/turbo/bazel
graph, etc.) when the PR only touches folders unrelated to the active build
system.

Uses only the Python stdlib so it can run on a bare runner without any
`pip install` step.

Exit codes:
  0 - Shortcut applied and targets uploaded. Skip the heavy detection.
  2 - Shortcut does not apply. Run the full tool-specific detection.
  1 - Shortcut applied but upload failed (or bad inputs).
"""

import argparse
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import PurePosixPath
from typing import List, Set

SHORTCUT_APPLIED = 0
SHORTCUT_FAILED = 1
SHORTCUT_SKIPPED = 2


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)


def get_changed_files(base: str, head: str) -> List[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", base, head],
        capture_output=True,
        text=True,
        check=True,
    )
    return [line for line in result.stdout.splitlines() if line.strip()]


def all_files_outside(files: List[str], mode_dir: str) -> bool:
    prefix = mode_dir.strip("/") + "/"
    return all(not f.startswith(prefix) for f in files)


def first_level_folders(files: List[str]) -> List[str]:
    folders: Set[str] = set()
    for f in files:
        parts = PurePosixPath(f).parts
        # Skip files at the repo root (no first-level folder).
        if len(parts) >= 2:
            folders.add(parts[0])
    return sorted(folders)


def upload_targets(
    targets: List[str],
    api_url: str,
    trunk_token: str,
    repository: str,
    pr_number: int,
    pr_sha: str,
    target_branch: str,
) -> None:
    owner, name = repository.split("/", 1)
    body = {
        "repo": {"host": "github.com", "owner": owner, "name": name},
        "pr": {"number": pr_number, "sha": pr_sha},
        "targetBranch": target_branch,
        "impactedTargets": targets,
    }
    req = urllib.request.Request(
        api_url,
        data=json.dumps(body).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "x-api-token": trunk_token,
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        if resp.status != 200:
            raise RuntimeError(f"HTTP {resp.status}: {resp.read().decode('utf-8', 'replace')}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Shortcut impacted-targets upload for PRs that don't touch the build tool's folder."
    )
    parser.add_argument(
        "--mode-dir",
        required=True,
        help="Build tool workspace folder (e.g. 'nx', 'turbo', 'bazel', 'uv').",
    )
    parser.add_argument("--base", required=True, help="Base SHA/ref for comparison.")
    parser.add_argument("--head", required=True, help="Head SHA/ref for comparison.")
    parser.add_argument(
        "--api-url",
        default=os.environ.get("API_URL", "https://api.trunk.io:443/v1/setImpactedTargets"),
        help="Trunk API URL.",
    )
    parser.add_argument(
        "--trunk-token",
        default=os.environ.get("TRUNK_TOKEN", ""),
        help="Trunk API token (or TRUNK_TOKEN env var).",
    )
    parser.add_argument(
        "--repository",
        default=os.environ.get("GITHUB_REPOSITORY", ""),
        help="Repository in 'owner/name' form (or GITHUB_REPOSITORY env var).",
    )
    parser.add_argument(
        "--pr-number",
        default=os.environ.get("PR_NUMBER") or os.environ.get("GITHUB_EVENT_NUMBER", ""),
        help="PR number (or PR_NUMBER/GITHUB_EVENT_NUMBER env var).",
    )
    parser.add_argument(
        "--pr-sha",
        default=os.environ.get("PR_SHA") or os.environ.get("GITHUB_SHA", ""),
        help="PR head SHA (or PR_SHA/GITHUB_SHA env var).",
    )
    parser.add_argument(
        "--target-branch",
        default=os.environ.get("TARGET_BRANCH") or os.environ.get("GITHUB_BASE_REF", ""),
        help="Target branch (or TARGET_BRANCH/GITHUB_BASE_REF env var).",
    )
    args = parser.parse_args()

    try:
        files = get_changed_files(args.base, args.head)
    except subprocess.CalledProcessError as e:
        eprint(f"git diff failed: {e.stderr or e}")
        return SHORTCUT_SKIPPED

    if not files:
        print("No changed files detected; falling through to full detection.")
        return SHORTCUT_SKIPPED

    if not all_files_outside(files, args.mode_dir):
        print(
            f"Changed files overlap with {args.mode_dir}/; falling through to full detection."
        )
        return SHORTCUT_SKIPPED

    targets = first_level_folders(files)
    print(
        f"Shortcut: all {len(files)} changed file(s) are outside {args.mode_dir}/. "
        f"Uploading {len(targets)} first-level folder target(s): {targets}"
    )

    missing = [
        name
        for name, value in [
            ("--trunk-token", args.trunk_token),
            ("--repository", args.repository),
            ("--pr-number", args.pr_number),
            ("--pr-sha", args.pr_sha),
            ("--target-branch", args.target_branch),
        ]
        if not value
    ]
    if missing:
        eprint(f"Missing required inputs: {', '.join(missing)}")
        return SHORTCUT_FAILED

    try:
        pr_number_int = int(args.pr_number)
    except (TypeError, ValueError):
        eprint(f"PR number must be an integer, got: {args.pr_number!r}")
        return SHORTCUT_FAILED

    try:
        upload_targets(
            targets=targets,
            api_url=args.api_url,
            trunk_token=args.trunk_token,
            repository=args.repository,
            pr_number=pr_number_int,
            pr_sha=args.pr_sha,
            target_branch=args.target_branch,
        )
    except (urllib.error.URLError, urllib.error.HTTPError, RuntimeError, ValueError) as e:
        eprint(f"Shortcut upload failed: {e}")
        return SHORTCUT_FAILED

    print(
        f"✨ Uploaded {len(targets)} impacted targets for PR #{pr_number_int} @ {args.pr_sha}"
    )
    return SHORTCUT_APPLIED


if __name__ == "__main__":
    sys.exit(main())
