use crate::process::try_gh;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct GitHub;

impl GitHub {
    pub fn comment(pr: &str, body: &str, token: &str) -> String {
        try_gh(&["pr", "comment", pr, "--body", body], token).expect("Failed to comment on PR")
    }

    pub fn close(pr: &str, token: &str) -> String {
        try_gh(&["pr", "close", pr], token).expect("Failed to close PR")
    }

    pub fn add_label(pr: &str, label: &str, token: &str) -> String {
        try_gh(&["pr", "edit", pr, "--add-label", label], token).expect("Failed to add label to PR")
    }

    pub fn get_pr_base_branch(pr: &str, gh_token: &str) -> String {
        let result = try_gh(&["pr", "view", pr, "--json", "baseRefName"], gh_token);
        if result.is_err() {
            // Log the error and fallback to "main" if we can't get the PR info
            eprintln!(
                "Warning: Failed to get base branch for PR {}: {:?}. Falling back to 'main'",
                pr,
                result.as_ref().err()
            );
            return "main".to_string();
        }
        let json_str = result.unwrap();
        let v: Value = match serde_json::from_str(&json_str) {
            Ok(val) => val,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to parse PR info JSON for PR {}: {}. Falling back to 'main'",
                    pr, e
                );
                return "main".to_string();
            }
        };
        v["baseRefName"]
            .as_str()
            .unwrap_or_else(|| {
                eprintln!(
                    "Warning: PR {} JSON does not contain 'baseRefName' field. Falling back to 'main'",
                    pr
                );
                "main"
            })
            .to_string()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GitHubAction {
    repository: String,
    #[serde(rename = "base_ref")]
    pub base_ref: Option<String>,
    pub event: Event,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    pub pull_request: PullRequest,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PullRequest {
    pub number: u32,
    pub head: Head,
    pub body: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Head {
    pub sha: String,
}

impl GitHubAction {
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap()
    }

    pub fn repo_owner(&self) -> &str {
        let repo_parts: Vec<&str> = self.repository.split('/').collect();
        repo_parts.first().expect("Invalid REPOSITORY format")
    }
    pub fn repo_name(&self) -> &str {
        let repo_parts: Vec<&str> = self.repository.split('/').collect();
        repo_parts.get(1).expect("Invalid REPOSITORY format")
    }

    pub fn base_branch(&self) -> &str {
        self.base_ref.as_deref().unwrap_or("main")
    }
}
