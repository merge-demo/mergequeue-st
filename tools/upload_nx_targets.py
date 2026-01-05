#!/usr/bin/env python3
"""
Upload impacted Nx targets to Trunk API.

This script reads impacted targets from a JSON file and uploads them to Trunk's API.
"""

import argparse
import json
import os
import sys

import requests


def eprint(*args, **kwargs):
    """Print to stderr."""
    print(*args, file=sys.stderr, **kwargs)


def main():
    parser = argparse.ArgumentParser(
        description="Upload impacted Nx targets to Trunk API"
    )
    parser.add_argument(
        "--targets-file",
        required=True,
        help="Path to JSON file containing impacted targets (array of strings)",
    )
    parser.add_argument(
        "--trunk-token",
        help="Trunk API token (or set TRUNK_TOKEN env var)",
    )
    parser.add_argument(
        "--api-url",
        default="https://api.trunk.io:443/v1/setImpactedTargets",
        help="Trunk API URL (default: https://api.trunk.io:443/v1/setImpactedTargets)",
    )
    parser.add_argument(
        "--repository",
        help="Repository in format 'owner/name' (or set GITHUB_REPOSITORY env var)",
    )
    parser.add_argument(
        "--pr-number",
        help="Pull request number (or set PR_NUMBER env var)",
    )
    parser.add_argument(
        "--pr-sha",
        help="Pull request head SHA (or set PR_SHA env var)",
    )
    parser.add_argument(
        "--target-branch",
        help="Target branch name (or set TARGET_BRANCH env var)",
    )

    args = parser.parse_args()

    # Get token from arg or env
    trunk_token = args.trunk_token or os.environ.get("TRUNK_TOKEN")
    if not trunk_token:
        eprint("Error: Trunk token required (--trunk-token or TRUNK_TOKEN env var)")
        sys.exit(1)

    # Read impacted targets
    try:
        with open(args.targets_file, "r") as f:
            impacted_targets = json.load(f)
        if not isinstance(impacted_targets, list):
            eprint(f"Error: Expected JSON array in {args.targets_file}")
            sys.exit(1)
    except FileNotFoundError:
        eprint(f"Error: Targets file not found: {args.targets_file}")
        sys.exit(1)
    except json.JSONDecodeError as e:
        eprint(f"Error: Invalid JSON in targets file: {e}")
        sys.exit(1)

    # Get repository and PR information from args or environment variables
    repository = args.repository or os.environ.get("GITHUB_REPOSITORY")
    pr_number = (
        args.pr_number
        or os.environ.get("PR_NUMBER")
        or os.environ.get("GITHUB_EVENT_NUMBER")
    )
    pr_sha = args.pr_sha or os.environ.get("PR_SHA") or os.environ.get("GITHUB_SHA")
    target_branch = (
        args.target_branch
        or os.environ.get("TARGET_BRANCH")
        or os.environ.get("GITHUB_BASE_REF")
    )

    # Parse repository owner and name
    if repository:
        try:
            repo_owner, repo_name = repository.split("/", 1)
        except ValueError:
            eprint(
                f"Error: Repository must be in format 'owner/name', got: {repository}"
            )
            sys.exit(1)
    else:
        repo_owner = None
        repo_name = None

    # Validate required fields
    if not repo_owner or not repo_name:
        eprint("Error: Repository required (--repository or GITHUB_REPOSITORY env var)")
        sys.exit(1)
    if not pr_number:
        eprint(
            "Error: PR number required (--pr-number, PR_NUMBER, or GITHUB_EVENT_NUMBER env var)"
        )
        sys.exit(1)
    if not pr_sha:
        eprint("Error: PR SHA required (--pr-sha, PR_SHA, or GITHUB_SHA env var)")
        sys.exit(1)
    if not target_branch:
        eprint(
            "Error: Target branch required (--target-branch, TARGET_BRANCH, or GITHUB_BASE_REF env var)"
        )
        sys.exit(1)

    # Convert PR number to int
    try:
        pr_number = int(pr_number)
    except (ValueError, TypeError):
        eprint(f"Error: PR number must be an integer, got: {pr_number}")
        sys.exit(1)

    # Build API request body
    post_body = {
        "repo": {"host": "github.com", "owner": repo_owner, "name": repo_name},
        "pr": {"number": pr_number, "sha": pr_sha},
        "targetBranch": target_branch,
        "impactedTargets": impacted_targets,
    }

    # Make API request
    headers = {"Content-Type": "application/json", "x-api-token": trunk_token}
    try:
        response = requests.post(args.api_url, headers=headers, json=post_body)
        http_status_code = response.status_code
    except Exception as e:
        eprint(f"HTTP request failed: {e}")
        sys.exit(1)

    # Handle response
    if http_status_code == 200:
        num_targets = len(impacted_targets)
        print(
            f"✨ Uploaded {num_targets} impacted targets for PR #{pr_number} @ {pr_sha}"
        )
        sys.exit(0)
    else:
        eprint(f"❌ Failed to upload impacted targets. HTTP {http_status_code}")
        try:
            error_body = response.json()
            eprint(f"Response: {json.dumps(error_body, indent=2)}")
        except:
            eprint(f"Response: {response.text}")
        sys.exit(1)


if __name__ == "__main__":
    main()
