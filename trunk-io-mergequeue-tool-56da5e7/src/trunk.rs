use crate::cli::Cli;
use crate::config::Conf;
use crate::github::{GitHub, GitHubAction};
use regex::Regex;
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use serde_json::json;
use std::fs;

/// Extracts dependency targets from a PR body string.
/// Looks for the pattern `deps=[target1,target2,target3]` and returns a vector of targets.
/// Returns an empty vector if no deps pattern is found.
pub fn get_targets(pr_body: &str) -> Vec<String> {
    let re = Regex::new(r".*deps=\[(.*?)\].*").unwrap();

    if let Some(caps) = re.captures(pr_body) {
        caps[1]
            .split(',')
            .map(|s| s.trim().to_owned())
            .collect::<Vec<String>>()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_targets_basic() {
        let body = "This is a test PR\ndeps=[a,b]\nMore content";
        let targets = get_targets(body);
        assert_eq!(targets, vec!["a", "b"]);
    }

    #[test]
    fn test_get_targets_with_spaces() {
        let body = "deps=[ a , b , c ]";
        let targets = get_targets(body);
        assert_eq!(targets, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_get_targets_single() {
        let body = "deps=[single-target]";
        let targets = get_targets(body);
        assert_eq!(targets, vec!["single-target"]);
    }

    #[test]
    fn test_get_targets_empty() {
        let body = "deps=[]";
        let targets = get_targets(body);
        assert_eq!(targets, vec![""]);
    }

    #[test]
    fn test_get_targets_no_match() {
        let body = "This PR has no deps information";
        let targets = get_targets(body);
        assert_eq!(targets, Vec::<String>::new());
    }
}

pub fn upload_targets(config: &Conf, cli: &Cli, github_json_path: &str) {
    // Check for TRUNK_TOKEN at runtime
    if cli.trunk_token.is_empty() {
        eprintln!("TRUNK_TOKEN is required for upload-targets subcommand");
        eprintln!("Provide it via --trunk-token flag or TRUNK_TOKEN environment variable");
        std::process::exit(1);
    }

    let github_json = fs::read_to_string(github_json_path).expect("Failed to read file");
    let ga = GitHubAction::from_json(&github_json);

    // Extract targets from PR body
    if !&ga.event.pull_request.body.is_some() {
        println!("No PR body content found - skipping target upload");
        println!("The PR body is required to extract dependency information using the format: deps=[target1,target2,target3]");
        return;
    }

    let body = ga.event.pull_request.body.clone().unwrap();
    let impacted_targets = get_targets(&body);

    if impacted_targets.is_empty() {
        println!("No deps listed in PR body like deps=[a,b,c]");
    }

    // Validate that we have targets to upload
    if impacted_targets.is_empty() {
        println!("No impacted targets found - skipping upload");
        return;
    }

    // Get the PR's base branch using GitHub API
    let github_tokens = cli.get_github_tokens();
    let gh_token = github_tokens.first().map(|t| t.as_str()).unwrap_or("");
    let pr_number_str = ga.event.pull_request.number.to_string();
    let target_branch = if !gh_token.is_empty() {
        GitHub::get_pr_base_branch(&pr_number_str, gh_token)
    } else {
        // Fallback to base_ref from JSON if no token available
        ga.base_branch().to_string()
    };
    println!(
        "Uploading {} impacted targets: {:?} (target branch: {})",
        impacted_targets.len(),
        impacted_targets,
        target_branch
    );

    let result = post_targets(
        ga.repo_owner(),
        ga.repo_name(),
        ga.event.pull_request.number,
        &ga.event.pull_request.head.sha,
        &target_branch,
        impacted_targets,
        &config.trunk.api,
        &cli.trunk_token,
    );

    match result {
        Ok(_) => println!("Successfully uploaded impacted targets"),
        Err(e) => {
            eprintln!("Failed to upload impacted targets: {:?}", e);
            // Exit with error code to fail the workflow
            std::process::exit(1);
        }
    }
}

pub fn post_targets(
    repo_owner: &str,
    repo_name: &str,
    pr_number: u32,
    pr_sha: &str,
    target_branch: &str,
    impacted_targets: Vec<String>,
    api: &str,
    api_token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert("x-api-token", api_token.parse().unwrap());

    let body = json!({
        "repo": {
            "host": "github.com",
            "owner": repo_owner,
            "name": repo_name,
        },
        "pr": {
            "number": pr_number,
            "sha": pr_sha,
        },
        "targetBranch": target_branch,
        "impactedTargets": impacted_targets,
    });

    let res = client
        .post(&format!("https://{}:443/v1/setImpactedTargets", api))
        .headers(headers)
        .body(body.to_string())
        .send()?;

    // Check HTTP status code
    if !res.status().is_success() {
        let status = res.status();
        let error_body = res
            .text()
            .unwrap_or_else(|_| "Unable to read error response".to_string());

        // Show debug info on errors
        println!("API request failed:");
        println!("  URL: https://{}:443/v1/setImpactedTargets", api);
        println!("  Repository: {}/{}", repo_owner, repo_name);
        println!("  PR Number: {}", pr_number);
        println!("  Target Branch: {}", target_branch);
        println!("  Impacted Targets: {:?}", impacted_targets);

        match status.as_u16() {
            400 => {
                return Err(format!(
                    "Bad Request (400): {}. Check request format and parameters.",
                    error_body
                )
                .into())
            }
            401 => {
                return Err(format!("API key rejected (401 Unauthorized): {}", error_body).into())
            }
            403 => return Err(format!("API key forbidden (403 Forbidden): {}", error_body).into()),
            404 => {
                return Err(
                    format!("Pull request not found (404 Not Found): {}", error_body).into(),
                )
            }
            429 => {
                return Err(format!("Rate limited (429 Too Many Requests): {}", error_body).into())
            }
            _ => return Err(format!("HTTP error {}: {}", status, error_body).into()),
        }
    }

    Ok(())
}

pub fn submit_pull_request(
    repo_owner: &str,
    repo_name: &str,
    pr_number: u32,
    target_branch: &str,
    priority: Option<&str>,
    api: &str,
    cli: &Cli,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check for TRUNK_TOKEN at runtime
    if cli.trunk_token.is_empty() {
        return Err("TRUNK_TOKEN is required when using API trigger. Provide it via --trunk-token flag or TRUNK_TOKEN environment variable".into());
    }

    // Handle dry-run mode
    if cli.dry_run {
        println!("dry-run: would submit to Trunk API:");
        println!("  - Repository: {}/{}", repo_owner, repo_name);
        println!("  - PR Number: {}", pr_number);
        println!("  - Target Branch: {}", target_branch);
        if let Some(priority_value) = priority {
            println!("  - Priority: {}", priority_value);
        }
        println!("  - API Endpoint: {}", api);
        println!(
            "  - Token: {}...",
            &cli.trunk_token[..std::cmp::min(8, cli.trunk_token.len())]
        );
        return Ok(());
    }
    let client = reqwest::blocking::Client::new();

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert("x-api-token", cli.trunk_token.parse().unwrap());

    let mut body = json!({
        "repo": {
            "host": "github.com",
            "owner": repo_owner,
            "name": repo_name,
        },
        "pr": {
            "number": pr_number,
        },
        "targetBranch": target_branch,
    });

    // Add priority - use provided value or default to "medium"
    let priority_value = priority.unwrap_or("medium");
    body["priority"] = json!(priority_value);

    let url = format!("https://{}/v1/submitPullRequest", api);
    let body_str = body.to_string();

    let res = client
        .post(&url)
        .headers(headers)
        .body(body_str.clone())
        .send()?;

    // Check HTTP status code
    if !res.status().is_success() {
        let status = res.status();
        let error_body = res
            .text()
            .unwrap_or_else(|_| "Unable to read error response".to_string());

        // Show debug info only on errors
        println!("API request failed:");
        println!("  URL: {}", url);
        println!("  Request body: {}", body_str);
        println!("  Repository: {}/{}", repo_owner, repo_name);
        println!("  PR Number: {}", pr_number);
        println!("  Target Branch: {}", target_branch);

        match status.as_u16() {
            400 => {
                return Err(format!(
                    "Bad Request (400): {}. Check request format and parameters.",
                    error_body
                )
                .into())
            }
            401 => {
                return Err(format!("API key rejected (401 Unauthorized): {}", error_body).into())
            }
            403 => return Err(format!("API key forbidden (403 Forbidden): {}", error_body).into()),
            404 => {
                return Err(
                    format!("Pull request not found (404 Not Found): {}", error_body).into(),
                )
            }
            429 => {
                return Err(format!("Rate limited (429 Too Many Requests): {}", error_body).into())
            }
            _ => return Err(format!("HTTP error {}: {}", status, error_body).into()),
        }
    }

    Ok(())
}
